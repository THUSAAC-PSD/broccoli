use std::time::Duration;

use common::{DlqErrorCode, DlqMessageType, SubmissionDlqErrorCode, SubmissionStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    ExprTrait, QueryFilter, QueryOrder, QueryResult, QuerySelect, Set, Statement, TransactionTrait,
};
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::dispatcher::permits::DispatcherReservation;
use crate::dlq::DlqService;
use crate::entity::{code_run, submission, submission_judgement, test_case_result};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClaimedRow {
    id: i32,
    judge_epoch: i32,
    retry_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaimPlan {
    redispatch_ids: Vec<i32>,
    exhausted_rows: Vec<ClaimedRow>,
}

impl ClaimPlan {
    fn from_rows(rows: Vec<ClaimedRow>, max_dispatch_retries: u32) -> Self {
        let max = std::cmp::min(max_dispatch_retries, i32::MAX as u32) as i32;
        let mut redispatch_ids = Vec::new();
        let mut exhausted_rows = Vec::new();

        for row in rows {
            if row.retry_count.saturating_add(1) > max {
                exhausted_rows.push(row);
            } else {
                redispatch_ids.push(row.id);
            }
        }

        Self {
            redispatch_ids,
            exhausted_rows,
        }
    }
}

pub async fn run(
    state: AppState,
    server_id: String,
    lease_ttl_secs: u64,
    interval_secs: u64,
    batch_size: u32,
    max_dispatch_retries: u32,
    mut cancel: watch::Receiver<bool>,
) {
    if let Err(e) = scan_once(
        state.clone(),
        &server_id,
        lease_ttl_secs,
        batch_size,
        max_dispatch_retries,
    )
    .await
    {
        error!(server_id = %server_id, error = %e, "Initial steal scan failed");
    }

    let mut interval = tokio::time::interval(Duration::from_secs(std::cmp::max(interval_secs, 1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let jitter_ms = rand::random::<u64>() % 3001;
                tokio::time::sleep(Duration::from_millis(jitter_ms)).await;
                if let Err(e) = scan_once(
                    state.clone(),
                    &server_id,
                    lease_ttl_secs,
                    batch_size,
                    max_dispatch_retries,
                ).await {
                    error!(server_id = %server_id, error = %e, "Steal scan failed");
                }
            }
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return;
                }
            }
        }
    }
}

async fn scan_once(
    state: AppState,
    server_id: &str,
    lease_ttl_secs: u64,
    batch_size: u32,
    max_dispatch_retries: u32,
) -> anyhow::Result<()> {
    let submission_slots = reserve_redispatch_slots(&state, batch_size, "submission");
    if !submission_slots.is_empty() {
        let submissions = claim_submissions(
            &state,
            server_id,
            lease_ttl_secs,
            submission_slots.len() as u32,
            max_dispatch_retries,
        )
        .await?;
        for (sub, dispatch_slot) in submissions.into_iter().zip(submission_slots) {
            let state = state.clone();
            dispatch_slot.spawn("steal_submission", async move {
                crate::handlers::submission::dispatch_to_plugin(state, sub).await;
            });
        }
    }

    let deferred_slots = reserve_redispatch_slots(&state, batch_size, "deferred_judgement");
    if !deferred_slots.is_empty() {
        let deferred_judgements = claim_deferred_judgements(
            &state,
            server_id,
            lease_ttl_secs,
            deferred_slots.len() as u32,
            max_dispatch_retries,
        )
        .await?;
        for ((sub, judgement_id), dispatch_slot) in
            deferred_judgements.into_iter().zip(deferred_slots)
        {
            let state = state.clone();
            dispatch_slot.spawn("steal_deferred_judgement", async move {
                crate::handlers::submission::dispatch_to_plugin_with_judgement(
                    state,
                    sub,
                    Some(judgement_id),
                    false,
                )
                .await;
            });
        }
    }

    let code_run_slots = reserve_redispatch_slots(&state, batch_size, "code_run");
    if !code_run_slots.is_empty() {
        let code_runs = claim_code_runs(
            &state,
            server_id,
            lease_ttl_secs,
            code_run_slots.len() as u32,
            max_dispatch_retries,
        )
        .await?;
        for (code_run, dispatch_slot) in code_runs.into_iter().zip(code_run_slots) {
            let state = state.clone();
            dispatch_slot.spawn("steal_code_run", async move {
                crate::handlers::code_run::dispatch_to_plugin(state, code_run).await;
            });
        }
    }

    Ok(())
}

fn reserve_redispatch_slots(
    state: &AppState,
    batch_size: u32,
    work_kind: &'static str,
) -> Vec<DispatcherReservation> {
    let mut slots = Vec::with_capacity(batch_size as usize);
    for _ in 0..batch_size {
        match state.dispatcher_permits.reserve() {
            Ok(slot) => slots.push(slot),
            Err(e) => {
                warn!(
                    work_kind = work_kind,
                    reserved = slots.len(),
                    retry_after_secs = e.retry_after_secs,
                    "Steal scanner claim skipped because dispatcher queue is full"
                );
                break;
            }
        }
    }
    slots
}

async fn claim_submissions(
    state: &AppState,
    server_id: &str,
    lease_ttl_secs: u64,
    batch_size: u32,
    max_dispatch_retries: u32,
) -> anyhow::Result<Vec<submission::Model>> {
    let txn = state.db.begin().await?;
    let rows = claim_rows(&txn, "submission", lease_ttl_secs, batch_size).await?;
    let plan = ClaimPlan::from_rows(rows, max_dispatch_retries);

    if !plan.redispatch_ids.is_empty() {
        open_stolen_submission_judgements(&txn, &plan.redispatch_ids).await?;
        clear_current_submission_results(&txn, &plan.redispatch_ids).await?;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::OwnerServerId,
                sea_orm::sea_query::Expr::value(Some(server_id.to_string())).into(),
            )
            .col_expr(
                submission::Column::LeaseHeartbeatAt,
                sea_orm::sea_query::Expr::cust("NOW()").into(),
            )
            .col_expr(
                submission::Column::RetryCount,
                sea_orm::sea_query::Expr::col(submission::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .col_expr(
                submission::Column::JudgeEpoch,
                sea_orm::sea_query::Expr::col(submission::Column::JudgeEpoch)
                    .add(1)
                    .into(),
            )
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                submission::Column::Verdict,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::CompileOutput,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::ErrorCode,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::Score,
                sea_orm::sea_query::Expr::value(None::<f64>).into(),
            )
            .col_expr(
                submission::Column::TimeUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission::Column::MemoryUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission::Column::JudgedAt,
                sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .filter(submission::Column::Id.is_in(plan.redispatch_ids.clone()))
            .exec(&txn)
            .await?;
    }

    for row in &plan.exhausted_rows {
        let message = format!(
            "Submission dispatch retry limit exhausted after {max_dispatch_retries} attempts"
        );
        let payload = serde_json::json!({
            "submission_id": row.id,
            "judge_epoch": row.judge_epoch,
            "server_id": server_id
        });
        DlqService::new(&txn)
            .create_entry(
                format!(
                    "dispatch-retry-exhausted-submission-{}-{}",
                    row.id,
                    Uuid::new_v4()
                ),
                DlqMessageType::StuckSubmission,
                Some(row.id),
                payload,
                DlqErrorCode::DispatchRetryExhausted,
                message.clone(),
            )
            .await?;
        crate::consumers::mark_submission_system_error_with_epoch(
            &txn,
            row.id,
            SubmissionDlqErrorCode::DISPATCH_RETRY_EXHAUSTED,
            &message,
            Some(row.judge_epoch),
        )
        .await?;
        submission::Entity::update_many()
            .col_expr(
                submission::Column::RetryCount,
                sea_orm::sea_query::Expr::col(submission::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .filter(submission::Column::Id.eq(row.id))
            .filter(submission::Column::JudgeEpoch.eq(row.judge_epoch))
            .exec(&txn)
            .await?;
    }

    let models = if plan.redispatch_ids.is_empty() {
        Vec::new()
    } else {
        submission::Entity::find()
            .filter(submission::Column::Id.is_in(plan.redispatch_ids.clone()))
            .all(&txn)
            .await?
    };

    txn.commit().await?;

    if !models.is_empty() || !plan.exhausted_rows.is_empty() {
        info!(
            server_id,
            redispatch = models.len(),
            exhausted = plan.exhausted_rows.len(),
            "Stole stale submission leases"
        );
    }

    Ok(models)
}

async fn open_stolen_submission_judgements(
    txn: &DatabaseTransaction,
    submission_ids: &[i32],
) -> anyhow::Result<()> {
    if submission_ids.is_empty() {
        return Ok(());
    }

    let submissions = submission::Entity::find()
        .filter(submission::Column::Id.is_in(submission_ids.to_vec()))
        .all(txn)
        .await?;

    submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::IsCurrent,
            sea_orm::sea_query::Expr::value(false).into(),
        )
        .filter(submission_judgement::Column::SubmissionId.is_in(submission_ids.to_vec()))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .exec(txn)
        .await?;

    for sub in submissions {
        let max_version: Option<i32> = submission_judgement::Entity::find()
            .filter(submission_judgement::Column::SubmissionId.eq(sub.id))
            .order_by_desc(submission_judgement::Column::Version)
            .one(txn)
            .await?
            .map(|j| j.version);

        submission_judgement::ActiveModel {
            submission_id: Set(sub.id),
            version: Set(max_version.unwrap_or(0).saturating_add(1)),
            is_current: Set(true),
            is_finalized: Set(false),
            triggered_by_user_id: Set(None),
            target_worker_id: Set(sub.target_worker_id),
            note: Set(None),
            status: Set(SubmissionStatus::Pending),
            verdict: Set(None),
            score: Set(None),
            time_used: Set(None),
            memory_used: Set(None),
            compile_output: Set(None),
            error_code: Set(None),
            error_message: Set(None),
            judge_epoch: Set(sub.judge_epoch.saturating_add(1)),
            created_at: Set(chrono::Utc::now()),
            finalized_at: Set(None),
            ..Default::default()
        }
        .insert(txn)
        .await?;
    }

    Ok(())
}

async fn clear_current_submission_results(
    txn: &DatabaseTransaction,
    submission_ids: &[i32],
) -> anyhow::Result<()> {
    if submission_ids.is_empty() {
        return Ok(());
    }

    let current_unfinalized_judgement_ids: Vec<i32> = submission_judgement::Entity::find()
        .select_only()
        .column(submission_judgement::Column::Id)
        .filter(submission_judgement::Column::SubmissionId.is_in(submission_ids.to_vec()))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .into_tuple()
        .all(txn)
        .await?;

    if current_unfinalized_judgement_ids.is_empty() {
        return Ok(());
    }

    test_case_result::Entity::delete_many()
        .filter(test_case_result::Column::JudgementId.is_in(current_unfinalized_judgement_ids))
        .exec(txn)
        .await?;

    Ok(())
}

async fn claim_deferred_judgements(
    state: &AppState,
    server_id: &str,
    lease_ttl_secs: u64,
    batch_size: u32,
    max_dispatch_retries: u32,
) -> anyhow::Result<Vec<(submission::Model, i32)>> {
    let txn = state.db.begin().await?;
    let rows = claim_deferred_judgement_rows(&txn, lease_ttl_secs, batch_size).await?;
    let plan = ClaimPlan::from_rows(rows, max_dispatch_retries);

    if !plan.redispatch_ids.is_empty() {
        submission_judgement::Entity::update_many()
            .col_expr(
                submission_judgement::Column::OwnerServerId,
                sea_orm::sea_query::Expr::value(Some(server_id.to_string())).into(),
            )
            .col_expr(
                submission_judgement::Column::LeaseHeartbeatAt,
                sea_orm::sea_query::Expr::cust("NOW()").into(),
            )
            .col_expr(
                submission_judgement::Column::RetryCount,
                sea_orm::sea_query::Expr::col(submission_judgement::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .col_expr(
                submission_judgement::Column::JudgeEpoch,
                sea_orm::sea_query::Expr::col(submission_judgement::Column::JudgeEpoch)
                    .add(1)
                    .into(),
            )
            .col_expr(
                submission_judgement::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                submission_judgement::Column::Verdict,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission_judgement::Column::CompileOutput,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorCode,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission_judgement::Column::Score,
                sea_orm::sea_query::Expr::value(None::<f64>).into(),
            )
            .col_expr(
                submission_judgement::Column::TimeUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission_judgement::Column::MemoryUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission_judgement::Column::FinalizedAt,
                sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .filter(submission_judgement::Column::Id.is_in(plan.redispatch_ids.clone()))
            .exec(&txn)
            .await?;
    }

    for row in &plan.exhausted_rows {
        let message = format!(
            "Deferred judgement dispatch retry limit exhausted after {max_dispatch_retries} attempts"
        );
        submission_judgement::Entity::update_many()
            .col_expr(
                submission_judgement::Column::RetryCount,
                sea_orm::sea_query::Expr::col(submission_judgement::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .col_expr(
                submission_judgement::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::SystemError.to_string()).into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorCode,
                sea_orm::sea_query::Expr::value(Some(
                    SubmissionDlqErrorCode::DISPATCH_RETRY_EXHAUSTED.to_string(),
                ))
                .into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(Some(message.clone())).into(),
            )
            .col_expr(
                submission_judgement::Column::IsFinalized,
                sea_orm::sea_query::Expr::value(true).into(),
            )
            .col_expr(
                submission_judgement::Column::FinalizedAt,
                sea_orm::sea_query::Expr::cust("NOW()").into(),
            )
            .filter(submission_judgement::Column::Id.eq(row.id))
            .filter(submission_judgement::Column::JudgeEpoch.eq(row.judge_epoch))
            .exec(&txn)
            .await?;
    }

    let judgements = if plan.redispatch_ids.is_empty() {
        Vec::new()
    } else {
        submission_judgement::Entity::find()
            .filter(submission_judgement::Column::Id.is_in(plan.redispatch_ids.clone()))
            .all(&txn)
            .await?
    };

    let submission_ids: Vec<i32> = judgements.iter().map(|j| j.submission_id).collect();
    let submissions = if submission_ids.is_empty() {
        Vec::new()
    } else {
        submission::Entity::find()
            .filter(submission::Column::Id.is_in(submission_ids))
            .all(&txn)
            .await?
    };
    let submissions_by_id = submissions
        .into_iter()
        .map(|sub| (sub.id, sub))
        .collect::<std::collections::HashMap<_, _>>();

    txn.commit().await?;

    let mut dispatches = Vec::new();
    for judgement in judgements {
        let Some(mut sub) = submissions_by_id.get(&judgement.submission_id).cloned() else {
            error!(
                judgement_id = judgement.id,
                submission_id = judgement.submission_id,
                "Deferred judgement owner submission disappeared during steal"
            );
            continue;
        };
        sub.status = SubmissionStatus::Pending;
        sub.judge_epoch = judgement.judge_epoch;
        if let Some(target) = judgement.target_worker_id.clone() {
            sub.target_worker_id = Some(target);
        }
        sub.owner_server_id = Some(server_id.to_string());
        sub.lease_heartbeat_at = judgement.lease_heartbeat_at;
        sub.retry_count = judgement.retry_count;
        dispatches.push((sub, judgement.id));
    }

    if !dispatches.is_empty() || !plan.exhausted_rows.is_empty() {
        info!(
            server_id,
            redispatch = dispatches.len(),
            exhausted = plan.exhausted_rows.len(),
            "Stole stale deferred submission judgements"
        );
    }

    Ok(dispatches)
}

async fn claim_code_runs(
    state: &AppState,
    server_id: &str,
    lease_ttl_secs: u64,
    batch_size: u32,
    max_dispatch_retries: u32,
) -> anyhow::Result<Vec<code_run::Model>> {
    let txn = state.db.begin().await?;
    let rows = claim_rows(&txn, "code_run", lease_ttl_secs, batch_size).await?;
    let plan = ClaimPlan::from_rows(rows, max_dispatch_retries);

    if !plan.redispatch_ids.is_empty() {
        code_run::Entity::update_many()
            .col_expr(
                code_run::Column::OwnerServerId,
                sea_orm::sea_query::Expr::value(Some(server_id.to_string())).into(),
            )
            .col_expr(
                code_run::Column::LeaseHeartbeatAt,
                sea_orm::sea_query::Expr::cust("NOW()").into(),
            )
            .col_expr(
                code_run::Column::RetryCount,
                sea_orm::sea_query::Expr::col(code_run::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .col_expr(
                code_run::Column::JudgeEpoch,
                sea_orm::sea_query::Expr::col(code_run::Column::JudgeEpoch)
                    .add(1)
                    .into(),
            )
            .col_expr(
                code_run::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                code_run::Column::Verdict,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::CompileOutput,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::ErrorCode,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::Score,
                sea_orm::sea_query::Expr::value(None::<f64>).into(),
            )
            .col_expr(
                code_run::Column::TimeUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                code_run::Column::MemoryUsed,
                sea_orm::sea_query::Expr::value(None::<i32>).into(),
            )
            .col_expr(
                code_run::Column::JudgedAt,
                sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .filter(code_run::Column::Id.is_in(plan.redispatch_ids.clone()))
            .exec(&txn)
            .await?;
    }

    for row in &plan.exhausted_rows {
        let message = format!(
            "Code run dispatch retry limit exhausted after {max_dispatch_retries} attempts"
        );
        crate::consumers::mark_code_run_system_error_with_epoch(
            &txn,
            row.id,
            SubmissionDlqErrorCode::DISPATCH_RETRY_EXHAUSTED,
            &message,
            Some(row.judge_epoch),
        )
        .await?;
        code_run::Entity::update_many()
            .col_expr(
                code_run::Column::RetryCount,
                sea_orm::sea_query::Expr::col(code_run::Column::RetryCount)
                    .add(1)
                    .into(),
            )
            .filter(code_run::Column::Id.eq(row.id))
            .filter(code_run::Column::JudgeEpoch.eq(row.judge_epoch))
            .exec(&txn)
            .await?;
    }

    let models = if plan.redispatch_ids.is_empty() {
        Vec::new()
    } else {
        code_run::Entity::find()
            .filter(code_run::Column::Id.is_in(plan.redispatch_ids.clone()))
            .all(&txn)
            .await?
    };

    txn.commit().await?;

    if !models.is_empty() || !plan.exhausted_rows.is_empty() {
        info!(
            server_id,
            redispatch = models.len(),
            exhausted = plan.exhausted_rows.len(),
            "Stole stale code-run leases"
        );
    }

    Ok(models)
}

async fn claim_rows(
    txn: &DatabaseTransaction,
    table: &str,
    lease_ttl_secs: u64,
    batch_size: u32,
) -> Result<Vec<ClaimedRow>, sea_orm::DbErr> {
    let threshold = chrono::Utc::now() - chrono::Duration::seconds(lease_ttl_secs as i64);
    let sql = format!(
        r#"SELECT id, judge_epoch, retry_count
           FROM {table}
           WHERE status IN ('Pending', 'Compiling', 'Running')
             AND (
               (owner_server_id IS NULL AND created_at < $1)
               OR (owner_server_id IS NOT NULL AND (lease_heartbeat_at IS NULL OR lease_heartbeat_at < $1))
             )
           ORDER BY created_at
           LIMIT $2
           FOR UPDATE SKIP LOCKED"#
    );
    let rows = txn
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [threshold.into(), i64::from(batch_size).into()],
        ))
        .await?;

    rows.into_iter().map(row_to_claimed).collect()
}

async fn claim_deferred_judgement_rows(
    txn: &DatabaseTransaction,
    lease_ttl_secs: u64,
    batch_size: u32,
) -> Result<Vec<ClaimedRow>, sea_orm::DbErr> {
    let threshold = chrono::Utc::now() - chrono::Duration::seconds(lease_ttl_secs as i64);
    let sql = r#"SELECT id, judge_epoch, retry_count
                 FROM submission_judgement
                 WHERE is_current = TRUE
                   AND is_finalized = FALSE
                   AND status IN ('Pending', 'Compiling', 'Running')
                   AND (
                     (owner_server_id IS NULL AND created_at < $1)
                     OR (owner_server_id IS NOT NULL AND (lease_heartbeat_at IS NULL OR lease_heartbeat_at < $1))
                   )
                 ORDER BY created_at
                 LIMIT $2
                 FOR UPDATE SKIP LOCKED"#;
    let rows = txn
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [threshold.into(), i64::from(batch_size).into()],
        ))
        .await?;

    rows.into_iter().map(row_to_claimed).collect()
}

fn row_to_claimed(row: QueryResult) -> Result<ClaimedRow, sea_orm::DbErr> {
    let id = row
        .try_get::<i32>("", "id")
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    let retry_count = row
        .try_get::<i32>("", "retry_count")
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    let judge_epoch = row
        .try_get::<i32>("", "judge_epoch")
        .map_err(|e| sea_orm::DbErr::Custom(e.to_string()))?;
    Ok(ClaimedRow {
        id,
        judge_epoch,
        retry_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    use common::storage::filesystem::FilesystemBlobStore;
    use plugin_core::config::PluginConfig;
    use plugin_core::error::PluginError;
    use plugin_core::host::HostFunctionRegistry;
    use plugin_core::i18n::I18nRegistry;
    use plugin_core::manifest::PluginManifest;
    use plugin_core::registry::PluginRegistry;
    use plugin_core::traits::PluginManager;
    use sea_orm::{DatabaseBackend, MockDatabase};

    use crate::config::{
        AppConfig, AuthConfig, BlobStoreConfig, BootstrapConfig, CorsConfig, DatabaseConfig,
        MqAppConfig, ServerConfig, SubmissionConfig,
    };
    use crate::dispatcher::permits::DispatcherSemaphore;
    use crate::registry::{
        CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
        LanguageResolverRegistry, OperationBatches, OperationWaiters,
    };
    use crate::state::{AppState, RegistryState};

    struct NoopPluginManager {
        registry: PluginRegistry,
        config: PluginConfig,
        host_functions: HostFunctionRegistry,
        i18n: I18nRegistry,
    }

    #[async_trait::async_trait]
    impl PluginManager for NoopPluginManager {
        fn get_registry(&self) -> &PluginRegistry {
            &self.registry
        }

        fn get_config(&self) -> &PluginConfig {
            &self.config
        }

        fn get_host_functions(&self) -> &HostFunctionRegistry {
            &self.host_functions
        }

        fn get_i18n_registry(&self) -> &I18nRegistry {
            &self.i18n
        }

        fn resolve(&self, _manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
            None
        }

        async fn call_raw(
            &self,
            _plugin_id: &str,
            _func_name: &str,
            _input: Vec<u8>,
        ) -> Result<Vec<u8>, PluginError> {
            Err(PluginError::Internal(
                "NoopPluginManager cannot call plugins".into(),
            ))
        }
    }

    #[test]
    fn claim_plan_splits_retry_cap_before_increment() {
        let plan = ClaimPlan::from_rows(
            vec![
                ClaimedRow {
                    id: 1,
                    judge_epoch: 11,
                    retry_count: 0,
                },
                ClaimedRow {
                    id: 2,
                    judge_epoch: 12,
                    retry_count: 4,
                },
                ClaimedRow {
                    id: 3,
                    judge_epoch: 13,
                    retry_count: 5,
                },
            ],
            5,
        );

        assert_eq!(plan.redispatch_ids, vec![1, 2]);
        assert_eq!(
            plan.exhausted_rows,
            vec![ClaimedRow {
                id: 3,
                judge_epoch: 13,
                retry_count: 5,
            }]
        );
    }

    #[tokio::test]
    async fn scan_once_skips_claims_when_dispatcher_queue_is_full() {
        let dispatcher_permits = DispatcherSemaphore::new(true, 1, 0);
        let held_slot = dispatcher_permits
            .reserve()
            .expect("initial dispatcher slot should be available");

        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let blob_dir = tempfile::tempdir().expect("create blob tempdir");
        let blob_store = Arc::new(
            FilesystemBlobStore::new(blob_dir.path().join("blobs"), 1024 * 1024)
                .await
                .expect("create blob store"),
        );
        let (metrics, prometheus_registry) =
            common::observability::init_metrics("broccoli-steal-test");

        let operation_batches: OperationBatches = Arc::new(dashmap::DashMap::new());
        let operation_waiters: OperationWaiters = Arc::new(dashmap::DashMap::new());
        let evaluate_batches: EvaluateBatches = Arc::new(dashmap::DashMap::new());
        let contest_type_registry: ContestTypeRegistry =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let evaluator_registry: EvaluatorRegistry =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let checker_format_registry: CheckerFormatRegistry =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let language_resolver_registry: LanguageResolverRegistry =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        let state = AppState {
            plugins: Arc::new(NoopPluginManager {
                registry: Arc::new(RwLock::new(HashMap::new())),
                config: PluginConfig::default(),
                host_functions: HostFunctionRegistry::new(),
                i18n: I18nRegistry::new(),
            }),
            db,
            config: AppConfig {
                server: ServerConfig {
                    host: "127.0.0.1".to_string(),
                    port: 0,
                    cors: CorsConfig {
                        allow_origins: vec![],
                        max_age: 3600,
                    },
                    frontend_dist: PathBuf::from("/srv/dist"),
                    trusted_proxies: vec![],
                    rate_limit_auth: false,
                    id: String::new(),
                    expects_multi_replica: false,
                    dispatcher_lease_steal_enabled: true,
                    dispatcher_semaphore_enabled: true,
                    dispatcher_concurrency: 1,
                    max_queued_submissions: 0,
                    lease_ttl_secs: 1,
                    lease_refresh_interval_secs: 10,
                    steal_scan_interval_secs: 15,
                    steal_batch_size: 8,
                    sweep_interval_secs: 300,
                    max_dispatch_retries: 5,
                    sweeper_dry_run: true,
                    cancel_primitive_enabled: false,
                    fleet_aware_admission_enabled: false,
                    fleet_capacity_poll_interval_secs: 5,
                    max_blocking_threads: None,
                },
                database: DatabaseConfig {
                    url: "mock://steal-test".to_string(),
                    max_connections: 1,
                },
                auth: AuthConfig {
                    jwt_secret: "test-secret".to_string(),
                    secure_cookies: false,
                },
                plugin: PluginConfig::default(),
                submission: SubmissionConfig::default(),
                storage: BlobStoreConfig::default(),
                mq: MqAppConfig {
                    enabled: false,
                    ..Default::default()
                },
                observability: common::config::ObservabilityConfig::default(),
                batch_max_age_secs: 600,
                bootstrap: BootstrapConfig::default(),
            },
            mq: None,
            redis_client: None,
            blob_store,
            registries: RegistryState {
                contest_type_registry,
                evaluator_registry,
                checker_format_registry,
                language_resolver_registry,
                operation_batches,
                operation_waiters,
                evaluate_batches,
                hook_registry: crate::hooks::new_shared_registry(),
            },
            device_codes: Arc::new(dashmap::DashMap::new()),
            metrics,
            prometheus_registry,
            dispatcher_permits,
        };

        scan_once(state, "server-b", 1, 8, 5)
            .await
            .expect("full dispatcher queue should skip steal claims");

        drop(held_slot);
    }
}
