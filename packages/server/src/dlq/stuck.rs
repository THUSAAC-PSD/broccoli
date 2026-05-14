use std::time::Duration;

use chrono::Utc;
use common::{DlqConfig, DlqErrorCode, DlqMessageType, SubmissionDlqErrorCode, SubmissionStatus};
use sea_orm::sea_query::{Expr, LockType};
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect,
    TransactionTrait,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::consumers::{mark_code_run_system_error, mark_submission_system_error};
use crate::entity::{code_run, dead_letter_message, submission, submission_judgement};
use crate::state::AppState;

use super::DlqService;

const IN_PROGRESS_STATUSES: [SubmissionStatus; 3] = [
    SubmissionStatus::Pending,
    SubmissionStatus::Compiling,
    SubmissionStatus::Running,
];

pub async fn run_stuck_job_detector(state: AppState, config: DlqConfig) {
    let scan_interval = Duration::from_secs(config.stuck_job_scan_interval_secs);
    let max_dispatch_retries = state.config.server.max_dispatch_retries;

    info!(
        timeout_secs = config.stuck_job_timeout_secs,
        scan_interval_secs = config.stuck_job_scan_interval_secs,
        max_dispatch_retries,
        "Starting stuck job detector"
    );

    let mut interval = tokio::time::interval(scan_interval);

    loop {
        interval.tick().await;

        if let Err(e) = detect_and_handle_stuck_jobs(&state, &config, max_dispatch_retries).await {
            error!(error = %e, "Stuck job detection failed");
        }
    }
}

async fn detect_and_handle_stuck_jobs(
    state: &AppState,
    config: &DlqConfig,
    max_dispatch_retries: u32,
) -> anyhow::Result<()> {
    let db = &state.db;
    let timeout_threshold =
        Utc::now() - chrono::Duration::seconds(config.stuck_job_timeout_secs as i64);

    // Composite staleness predicate mirroring dispatcher/steal.rs:
    // unleased rows clock from creation; leased rows clock from the last
    // heartbeat (a leased row with NULL heartbeat is also suspect). A leased
    // submission whose worker is heartbeating every 10s never trips this,
    // even if its created_at is hours old.
    let stuck_submission_ids: Vec<i32> = submission::Entity::find()
        .select_only()
        .column(submission::Column::Id)
        .filter(submission::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .filter(
            Condition::any()
                .add(
                    Condition::all()
                        .add(submission::Column::OwnerServerId.is_null())
                        .add(submission::Column::CreatedAt.lt(timeout_threshold)),
                )
                .add(
                    Condition::all()
                        .add(submission::Column::OwnerServerId.is_not_null())
                        .add(
                            Condition::any()
                                .add(submission::Column::LeaseHeartbeatAt.is_null())
                                .add(submission::Column::LeaseHeartbeatAt.lt(timeout_threshold)),
                        ),
                ),
        )
        .into_tuple()
        .all(db)
        .await?;

    if !stuck_submission_ids.is_empty() {
        info!(
            count = stuck_submission_ids.len(),
            "Found stuck submissions, evaluating recovery"
        );

        for submission_id in stuck_submission_ids {
            if let Err(e) =
                handle_stuck_submission(state, submission_id, config, max_dispatch_retries).await
            {
                error!(
                    submission_id,
                    error = %e,
                    "Failed to handle stuck submission"
                );
            }
        }
    }

    let stuck_code_run_ids: Vec<i32> = code_run::Entity::find()
        .select_only()
        .column(code_run::Column::Id)
        .filter(code_run::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .filter(
            Condition::any()
                .add(
                    Condition::all()
                        .add(code_run::Column::OwnerServerId.is_null())
                        .add(code_run::Column::CreatedAt.lt(timeout_threshold)),
                )
                .add(
                    Condition::all()
                        .add(code_run::Column::OwnerServerId.is_not_null())
                        .add(
                            Condition::any()
                                .add(code_run::Column::LeaseHeartbeatAt.is_null())
                                .add(code_run::Column::LeaseHeartbeatAt.lt(timeout_threshold)),
                        ),
                ),
        )
        .into_tuple()
        .all(db)
        .await?;

    if !stuck_code_run_ids.is_empty() {
        info!(
            count = stuck_code_run_ids.len(),
            "Found stuck code runs, evaluating recovery"
        );
        for code_run_id in stuck_code_run_ids {
            if let Err(e) = handle_stuck_code_run(state, code_run_id, max_dispatch_retries).await {
                error!(code_run_id, error = %e, "Failed to handle stuck code run");
            }
        }
    }

    Ok(())
}

/// Outcome of a stuck-job recovery attempt within a transaction.
enum StuckRecovery<M> {
    /// Re-dispatch the (mutated) row via `tokio::spawn`.
    Redispatch { model: M, retry_count: i32 },
    /// Retry budget exhausted; row was terminally marked `SystemError`.
    Terminal,
    /// Row no longer needs handling (already terminal, vanished, or
    /// concurrently re-dispatched between SELECT and UPDATE).
    Skip,
}

async fn handle_stuck_submission(
    state: &AppState,
    submission_id: i32,
    config: &DlqConfig,
    max_dispatch_retries: u32,
) -> anyhow::Result<()> {
    let db = &state.db;
    let txn = db.begin().await?;

    let submission = submission::Entity::find_by_id(submission_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?;

    let Some(submission) = submission else {
        txn.rollback().await?;
        return Ok(());
    };

    if submission.status.is_terminal() {
        txn.rollback().await?;
        return Ok(());
    }

    let current_retry_count = submission.retry_count;
    let max = max_dispatch_retries as i32;

    let recovery: StuckRecovery<submission::Model> = if current_retry_count >= max {
        // Retry budget exhausted: keep the terminal SystemError path and
        // record an unresolved DLQ entry so operators see the failure.
        let existing = dead_letter_message::Entity::find()
            .filter(dead_letter_message::Column::SubmissionId.eq(submission_id))
            .filter(dead_letter_message::Column::Resolved.eq(false))
            .one(&txn)
            .await?;

        if existing.is_some() {
            warn!(
                submission_id,
                "Submission already has unresolved DLQ entry, skipping"
            );
            StuckRecovery::Skip
        } else {
            let payload = serde_json::json!({
                "submission_id": submission.id,
                "problem_id": submission.problem_id,
                "user_id": submission.user_id,
                "language": submission.language,
                "contest_id": submission.contest_id,
                "created_at": submission.created_at,
                "retry_count": current_retry_count,
            });

            let dlq = DlqService::new(&txn);
            dlq.create_entry(
                format!("stuck-submission-{}-{}", submission.id, Uuid::new_v4()),
                DlqMessageType::StuckSubmission,
                Some(submission.id),
                payload,
                DlqErrorCode::StuckJob,
                format!(
                    "Submission stuck in {} for over {} seconds (retry {}/{})",
                    submission.status,
                    config.stuck_job_timeout_secs,
                    current_retry_count,
                    max_dispatch_retries
                ),
            )
            .await?;

            mark_submission_system_error(
                &txn,
                submission.id,
                SubmissionDlqErrorCode::STUCK_JOB,
                "Job timed out waiting for worker after retry budget exhausted",
            )
            .await?;
            StuckRecovery::Terminal
        }
    } else {
        // Re-dispatch path. Reset the submission row to Pending, bump
        // retry_count + judge_epoch, clear intermediate result fields, and
        // open a fresh judgement so the in-flight version is closed off.
        let new_retry_count = current_retry_count.saturating_add(1);
        let new_epoch = submission.judge_epoch.saturating_add(1);

        let affected = submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                submission::Column::RetryCount,
                Expr::value(new_retry_count).into(),
            )
            .col_expr(
                submission::Column::JudgeEpoch,
                Expr::value(new_epoch).into(),
            )
            .col_expr(
                submission::Column::OwnerServerId,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::LeaseHeartbeatAt,
                Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .col_expr(
                submission::Column::Verdict,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::CompileOutput,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::ErrorCode,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::ErrorMessage,
                Expr::value(None::<String>).into(),
            )
            .col_expr(submission::Column::Score, Expr::value(None::<f64>).into())
            .col_expr(
                submission::Column::TimeUsed,
                Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission::Column::MemoryUsed,
                Expr::value(None::<i32>).into(),
            )
            .col_expr(
                submission::Column::JudgedAt,
                Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .filter(submission::Column::Id.eq(submission_id))
            .filter(submission::Column::JudgeEpoch.eq(submission.judge_epoch))
            .filter(submission::Column::RetryCount.lt(max))
            .filter(submission::Column::Status.is_in(IN_PROGRESS_STATUSES))
            .exec(&txn)
            .await?;

        if affected.rows_affected == 0 {
            // Lost a race with lease/steal or another stuck cycle: fall
            // through with Skip rather than blowing up the transaction.
            StuckRecovery::Skip
        } else {
            open_fresh_submission_judgement(&txn, &submission, new_epoch).await?;
            let mut redispatch_model = submission.clone();
            redispatch_model.status = SubmissionStatus::Pending;
            redispatch_model.retry_count = new_retry_count;
            redispatch_model.judge_epoch = new_epoch;
            redispatch_model.owner_server_id = None;
            redispatch_model.lease_heartbeat_at = None;
            redispatch_model.verdict = None;
            redispatch_model.compile_output = None;
            redispatch_model.error_code = None;
            redispatch_model.error_message = None;
            redispatch_model.score = None;
            redispatch_model.time_used = None;
            redispatch_model.memory_used = None;
            redispatch_model.judged_at = None;
            StuckRecovery::Redispatch {
                model: redispatch_model,
                retry_count: new_retry_count,
            }
        }
    };

    txn.commit().await?;

    match recovery {
        StuckRecovery::Redispatch { model, retry_count } => {
            info!(
                submission_id,
                retry = retry_count,
                max_retries = max_dispatch_retries,
                "Re-dispatching stuck submission"
            );
            let dispatch_state = state.clone();
            tokio::spawn(async move {
                crate::handlers::submission::dispatch_to_plugin(dispatch_state, model).await;
            });
        }
        StuckRecovery::Terminal => {
            info!(
                submission_id,
                retry = current_retry_count,
                max_retries = max_dispatch_retries,
                "Marking stuck submission as SystemError (retry budget exhausted)"
            );
        }
        StuckRecovery::Skip => {}
    }

    Ok(())
}

async fn handle_stuck_code_run(
    state: &AppState,
    code_run_id: i32,
    max_dispatch_retries: u32,
) -> anyhow::Result<()> {
    let db = &state.db;
    let txn = db.begin().await?;

    let run = code_run::Entity::find_by_id(code_run_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?;

    let Some(run) = run else {
        txn.rollback().await?;
        return Ok(());
    };

    if run.status.is_terminal() {
        txn.rollback().await?;
        return Ok(());
    }

    let current_retry_count = run.retry_count;
    let max = max_dispatch_retries as i32;

    let recovery: StuckRecovery<code_run::Model> = if current_retry_count >= max {
        mark_code_run_system_error(
            &txn,
            run.id,
            "STUCK_JOB",
            "Code run timed out waiting for worker after retry budget exhausted",
        )
        .await?;
        StuckRecovery::Terminal
    } else {
        let new_retry_count = current_retry_count.saturating_add(1);
        let new_epoch = run.judge_epoch.saturating_add(1);

        let affected = code_run::Entity::update_many()
            .col_expr(
                code_run::Column::Status,
                Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                code_run::Column::RetryCount,
                Expr::value(new_retry_count).into(),
            )
            .col_expr(code_run::Column::JudgeEpoch, Expr::value(new_epoch).into())
            .col_expr(
                code_run::Column::OwnerServerId,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::LeaseHeartbeatAt,
                Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .col_expr(
                code_run::Column::Verdict,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::CompileOutput,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::ErrorCode,
                Expr::value(None::<String>).into(),
            )
            .col_expr(
                code_run::Column::ErrorMessage,
                Expr::value(None::<String>).into(),
            )
            .col_expr(code_run::Column::Score, Expr::value(None::<f64>).into())
            .col_expr(code_run::Column::TimeUsed, Expr::value(None::<i32>).into())
            .col_expr(
                code_run::Column::MemoryUsed,
                Expr::value(None::<i32>).into(),
            )
            .col_expr(
                code_run::Column::JudgedAt,
                Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
            )
            .filter(code_run::Column::Id.eq(code_run_id))
            .filter(code_run::Column::JudgeEpoch.eq(run.judge_epoch))
            .filter(code_run::Column::RetryCount.lt(max))
            .filter(code_run::Column::Status.is_in(IN_PROGRESS_STATUSES))
            .exec(&txn)
            .await?;

        if affected.rows_affected == 0 {
            StuckRecovery::Skip
        } else {
            let mut redispatch_model = run.clone();
            redispatch_model.status = SubmissionStatus::Pending;
            redispatch_model.retry_count = new_retry_count;
            redispatch_model.judge_epoch = new_epoch;
            redispatch_model.owner_server_id = None;
            redispatch_model.lease_heartbeat_at = None;
            redispatch_model.verdict = None;
            redispatch_model.compile_output = None;
            redispatch_model.error_code = None;
            redispatch_model.error_message = None;
            redispatch_model.score = None;
            redispatch_model.time_used = None;
            redispatch_model.memory_used = None;
            redispatch_model.judged_at = None;
            StuckRecovery::Redispatch {
                model: redispatch_model,
                retry_count: new_retry_count,
            }
        }
    };

    txn.commit().await?;

    match recovery {
        StuckRecovery::Redispatch { model, retry_count } => {
            info!(
                code_run_id,
                retry = retry_count,
                max_retries = max_dispatch_retries,
                "Re-dispatching stuck code run"
            );
            let dispatch_state = state.clone();
            tokio::spawn(async move {
                crate::handlers::code_run::dispatch_to_plugin(dispatch_state, model).await;
            });
        }
        StuckRecovery::Terminal => {
            info!(
                code_run_id,
                retry = current_retry_count,
                max_retries = max_dispatch_retries,
                "Marking stuck code run as SystemError (retry budget exhausted)"
            );
        }
        StuckRecovery::Skip => {}
    }

    Ok(())
}

/// Close the in-flight `submission_judgement` (if any) and insert a fresh one
/// for the new epoch. Mirrors `steal::open_stolen_submission_judgements` but
/// for a single submission.
async fn open_fresh_submission_judgement(
    txn: &DatabaseTransaction,
    submission: &submission::Model,
    new_epoch: i32,
) -> anyhow::Result<()> {
    use sea_orm::{ActiveModelTrait, QueryOrder, Set};

    submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::IsCurrent,
            Expr::value(false).into(),
        )
        .filter(submission_judgement::Column::SubmissionId.eq(submission.id))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .exec(txn)
        .await?;

    let max_version: Option<i32> = submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(submission.id))
        .order_by_desc(submission_judgement::Column::Version)
        .one(txn)
        .await?
        .map(|j| j.version);

    submission_judgement::ActiveModel {
        submission_id: Set(submission.id),
        version: Set(max_version.unwrap_or(0).saturating_add(1)),
        is_current: Set(true),
        is_finalized: Set(false),
        triggered_by_user_id: Set(None),
        target_worker_id: Set(submission.target_worker_id.clone()),
        note: Set(None),
        status: Set(SubmissionStatus::Pending),
        verdict: Set(None),
        score: Set(None),
        time_used: Set(None),
        memory_used: Set(None),
        compile_output: Set(None),
        error_code: Set(None),
        error_message: Set(None),
        judge_epoch: Set(new_epoch),
        created_at: Set(chrono::Utc::now()),
        finalized_at: Set(None),
        ..Default::default()
    }
    .insert(txn)
    .await?;

    Ok(())
}
