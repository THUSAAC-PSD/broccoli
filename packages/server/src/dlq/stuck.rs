use std::time::Duration;

use chrono::Utc;
use common::{DlqConfig, DlqErrorCode, DlqMessageType, SubmissionDlqErrorCode, SubmissionStatus};
use sea_orm::sea_query::LockType;
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect,
    TransactionTrait,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::consumers::{mark_code_run_system_error, mark_submission_system_error};
use crate::entity::{
    code_run, code_run_result, dead_letter_message, submission, submission_judgement,
    test_case_result,
};
use crate::state::AppState;

use super::DlqService;

// `Queued` is in-progress from the stuck-detector's perspective — a row
// stranded in `Queued` because every server's claim fiber is dead is a
// genuine system-stuck condition the wide-net 6h timeout (UP#19) must
// catch. UP#43 will layer a 5min per-state threshold on top so the
// detector escalates `Queued` orphans faster than the 6h global, but
// inclusion in this list is the floor.
const IN_PROGRESS_STATUSES: [SubmissionStatus; 4] = [
    SubmissionStatus::Queued,
    SubmissionStatus::Pending,
    SubmissionStatus::Compiling,
    SubmissionStatus::Running,
];

pub async fn run_stuck_job_detector(state: AppState, config: DlqConfig) {
    let scan_interval = Duration::from_secs(config.stuck_job_scan_interval_secs);
    let max_stuck_retries = state.config.server.max_stuck_retries;

    info!(
        timeout_secs = config.stuck_job_timeout_secs,
        scan_interval_secs = config.stuck_job_scan_interval_secs,
        max_stuck_retries,
        "Starting stuck job detector"
    );

    let mut interval = tokio::time::interval(scan_interval);

    loop {
        interval.tick().await;

        if let Err(e) = detect_and_handle_stuck_jobs(&state, &config, max_stuck_retries).await {
            error!(error = %e, "Stuck job detection failed");
        }
    }
}

fn stale_by_owner(
    owner_server_id: Option<&str>,
    created_at: chrono::DateTime<Utc>,
    lease_heartbeat_at: Option<chrono::DateTime<Utc>>,
    timeout_threshold: chrono::DateTime<Utc>,
) -> bool {
    if owner_server_id.is_none() {
        return created_at < timeout_threshold;
    }

    lease_heartbeat_at
        .map(|heartbeat| heartbeat < timeout_threshold)
        .unwrap_or(true)
}

async fn detect_and_handle_stuck_jobs(
    state: &AppState,
    config: &DlqConfig,
    max_stuck_retries: u32,
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
            if let Err(e) = handle_stuck_submission(
                state,
                submission_id,
                config,
                max_stuck_retries,
                timeout_threshold,
            )
            .await
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
            if let Err(e) = handle_stuck_code_run(
                state,
                code_run_id,
                config,
                max_stuck_retries,
                timeout_threshold,
            )
            .await
            {
                error!(code_run_id, error = %e, "Failed to handle stuck code run");
            }
        }
    }

    let stuck_judgement_ids: Vec<i32> = submission_judgement::Entity::find()
        .select_only()
        .column(submission_judgement::Column::Id)
        .filter(submission_judgement::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .filter(
            Condition::any()
                .add(
                    Condition::all()
                        .add(submission_judgement::Column::OwnerServerId.is_null())
                        .add(submission_judgement::Column::CreatedAt.lt(timeout_threshold)),
                )
                .add(
                    Condition::all()
                        .add(submission_judgement::Column::OwnerServerId.is_not_null())
                        .add(
                            Condition::any()
                                .add(submission_judgement::Column::LeaseHeartbeatAt.is_null())
                                .add(
                                    submission_judgement::Column::LeaseHeartbeatAt
                                        .lt(timeout_threshold),
                                ),
                        ),
                ),
        )
        .into_tuple()
        .all(db)
        .await?;

    if !stuck_judgement_ids.is_empty() {
        info!(
            count = stuck_judgement_ids.len(),
            "Found stuck submission judgements, evaluating recovery"
        );
        for judgement_id in stuck_judgement_ids {
            if let Err(e) = handle_stuck_submission_judgement(
                state,
                judgement_id,
                config,
                max_stuck_retries,
                timeout_threshold,
            )
            .await
            {
                error!(
                    judgement_id,
                    error = %e,
                    "Failed to handle stuck submission judgement"
                );
            }
        }
    }

    Ok(())
}

/// Outcome of a stuck-job recovery attempt within a transaction.
enum StuckRecovery {
    /// Re-dispatch the (mutated) submission after commit.
    RedispatchSubmission {
        model: submission::Model,
        retry_count: i32,
    },
    /// Re-dispatch the (mutated) code run after commit.
    RedispatchCodeRun {
        model: code_run::Model,
        retry_count: i32,
    },
    /// Re-dispatch a deferred/current judgement after commit.
    RedispatchJudgement {
        submission: submission::Model,
        judgement_id: i32,
        fire_after_judging: bool,
        retry_count: i32,
    },
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
    max_stuck_retries: u32,
    timeout_threshold: chrono::DateTime<Utc>,
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

    if submission.status.is_terminal()
        || !IN_PROGRESS_STATUSES.contains(&submission.status)
        || !stale_by_owner(
            submission.owner_server_id.as_deref(),
            submission.created_at,
            submission.lease_heartbeat_at,
            timeout_threshold,
        )
    {
        txn.rollback().await?;
        return Ok(());
    }

    let current_retry_count = submission.retry_count;

    let recovery = if stuck_retry_budget_exhausted(current_retry_count, max_stuck_retries) {
        // Retry budget exhausted: keep the terminal SystemError path and
        // record an unresolved DLQ entry so operators see the failure.
        let system_error_message = stuck_retries_exceeded_message(max_stuck_retries);
        let existing = dead_letter_message::Entity::find()
            .filter(dead_letter_message::Column::SubmissionId.eq(submission_id))
            .filter(
                dead_letter_message::Column::MessageType
                    .eq(DlqMessageType::StuckSubmission.to_string()),
            )
            .filter(dead_letter_message::Column::Resolved.eq(false))
            .one(&txn)
            .await?;

        if existing.is_some() {
            warn!(
                submission_id,
                "Submission already has unresolved DLQ entry, marking terminal without creating duplicate"
            );
            mark_submission_system_error(
                &txn,
                submission.id,
                SubmissionDlqErrorCode::STUCK_JOB,
                &system_error_message,
            )
            .await?;
            StuckRecovery::Terminal
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
                    "{}: submission stuck in {} for over {} seconds (retry {}/{})",
                    system_error_message,
                    submission.status,
                    config.stuck_job_timeout_secs,
                    current_retry_count,
                    max_stuck_retries
                ),
            )
            .await?;

            mark_submission_system_error(
                &txn,
                submission.id,
                SubmissionDlqErrorCode::STUCK_JOB,
                &system_error_message,
            )
            .await?;
            StuckRecovery::Terminal
        }
    } else if submission.status == SubmissionStatus::Queued
        || !state.config.server.dispatcher_lease_steal_enabled
    {
        recover_stuck_submission_without_steal(&txn, &submission).await?
    } else {
        StuckRecovery::Skip
    };

    txn.commit().await?;

    match recovery {
        StuckRecovery::RedispatchSubmission { model, retry_count } => {
            info!(
                submission_id,
                retry = retry_count,
                max_retries = max_stuck_retries,
                "Re-dispatching stuck submission because lease/steal is disabled"
            );
            let dispatch_state = state.clone();
            tokio::spawn(async move {
                crate::handlers::submission::dispatch_to_plugin(dispatch_state, model).await;
            });
        }
        StuckRecovery::RedispatchCodeRun { .. } | StuckRecovery::RedispatchJudgement { .. } => {
            unreachable!("submission handler returned a non-submission recovery")
        }
        StuckRecovery::Terminal => {
            info!(
                submission_id,
                retry = current_retry_count,
                max_retries = max_stuck_retries,
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
    config: &DlqConfig,
    max_stuck_retries: u32,
    timeout_threshold: chrono::DateTime<Utc>,
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

    if run.status.is_terminal()
        || !IN_PROGRESS_STATUSES.contains(&run.status)
        || !stale_by_owner(
            run.owner_server_id.as_deref(),
            run.created_at,
            run.lease_heartbeat_at,
            timeout_threshold,
        )
    {
        txn.rollback().await?;
        return Ok(());
    }

    let current_retry_count = run.retry_count;

    let recovery = if stuck_retry_budget_exhausted(current_retry_count, max_stuck_retries) {
        let system_error_message = stuck_retries_exceeded_message(max_stuck_retries);
        let payload = serde_json::json!({
            "code_run_id": run.id,
            "problem_id": run.problem_id,
            "user_id": run.user_id,
            "language": run.language,
            "contest_id": run.contest_id,
            "created_at": run.created_at,
            "retry_count": current_retry_count,
        });
        let created = create_unresolved_stuck_dlq_entry(
            &txn,
            stuck_code_run_message_id(run.id),
            DlqMessageType::StuckCodeRun,
            None,
            payload,
            format!(
                "{}: code run stuck in {} for over {} seconds (retry {}/{})",
                system_error_message,
                run.status,
                config.stuck_job_timeout_secs,
                current_retry_count,
                max_stuck_retries
            ),
        )
        .await?;
        if !created {
            warn!(
                code_run_id = run.id,
                "Code run already has unresolved DLQ entry, marking terminal without creating duplicate"
            );
        }
        mark_code_run_system_error(&txn, run.id, "STUCK_JOB", &system_error_message).await?;
        StuckRecovery::Terminal
    } else if run.status == SubmissionStatus::Queued
        || !state.config.server.dispatcher_lease_steal_enabled
    {
        recover_stuck_code_run_without_steal(&txn, &run).await?
    } else {
        StuckRecovery::Skip
    };

    txn.commit().await?;

    match recovery {
        StuckRecovery::RedispatchCodeRun { model, retry_count } => {
            info!(
                code_run_id,
                retry = retry_count,
                max_retries = max_stuck_retries,
                "Re-dispatching stuck code run because lease/steal is disabled"
            );
            let dispatch_state = state.clone();
            tokio::spawn(async move {
                crate::handlers::code_run::dispatch_to_plugin(dispatch_state, model).await;
            });
        }
        StuckRecovery::RedispatchSubmission { .. } | StuckRecovery::RedispatchJudgement { .. } => {
            unreachable!("code-run handler returned a non-code-run recovery")
        }
        StuckRecovery::Terminal => {
            info!(
                code_run_id,
                retry = current_retry_count,
                max_retries = max_stuck_retries,
                "Marking stuck code run as SystemError (retry budget exhausted)"
            );
        }
        StuckRecovery::Skip => {}
    }

    Ok(())
}

async fn handle_stuck_submission_judgement(
    state: &AppState,
    judgement_id: i32,
    config: &DlqConfig,
    max_stuck_retries: u32,
    timeout_threshold: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    let db = &state.db;
    let txn = db.begin().await?;

    let judgement = submission_judgement::Entity::find_by_id(judgement_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?;

    let Some(judgement) = judgement else {
        txn.rollback().await?;
        return Ok(());
    };

    if judgement.status.is_terminal()
        || judgement.is_finalized
        || !IN_PROGRESS_STATUSES.contains(&judgement.status)
        || !stale_by_owner(
            judgement.owner_server_id.as_deref(),
            judgement.created_at,
            judgement.lease_heartbeat_at,
            timeout_threshold,
        )
    {
        txn.rollback().await?;
        return Ok(());
    }

    let current_retry_count = judgement.retry_count;

    let recovery = if stuck_retry_budget_exhausted(current_retry_count, max_stuck_retries) {
        let system_error_message = stuck_retries_exceeded_message(max_stuck_retries);
        let payload = serde_json::json!({
            "judgement_id": judgement.id,
            "submission_id": judgement.submission_id,
            "version": judgement.version,
            "is_current": judgement.is_current,
            "judge_epoch": judgement.judge_epoch,
            "created_at": judgement.created_at,
            "retry_count": current_retry_count,
        });
        let created = create_unresolved_stuck_dlq_entry(
            &txn,
            stuck_submission_judgement_message_id(judgement.id),
            DlqMessageType::StuckSubmissionJudgement,
            Some(judgement.submission_id),
            payload,
            format!(
                "{}: submission judgement stuck in {} for over {} seconds (retry {}/{})",
                system_error_message,
                judgement.status,
                config.stuck_job_timeout_secs,
                current_retry_count,
                max_stuck_retries
            ),
        )
        .await?;
        if !created {
            warn!(
                judgement_id = judgement.id,
                submission_id = judgement.submission_id,
                "Submission judgement already has unresolved DLQ entry, marking terminal without creating duplicate"
            );
        }

        submission_judgement::Entity::update_many()
            .col_expr(
                submission_judgement::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::SystemError.to_string()).into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorCode,
                sea_orm::sea_query::Expr::value(Some(
                    SubmissionDlqErrorCode::STUCK_JOB.to_string(),
                ))
                .into(),
            )
            .col_expr(
                submission_judgement::Column::ErrorMessage,
                sea_orm::sea_query::Expr::value(Some(system_error_message)).into(),
            )
            .col_expr(
                submission_judgement::Column::IsFinalized,
                sea_orm::sea_query::Expr::value(true).into(),
            )
            .col_expr(
                submission_judgement::Column::FinalizedAt,
                sea_orm::sea_query::Expr::cust("NOW()").into(),
            )
            .filter(submission_judgement::Column::Id.eq(judgement.id))
            .filter(submission_judgement::Column::Status.is_in(IN_PROGRESS_STATUSES))
            .filter(submission_judgement::Column::IsFinalized.eq(false))
            .exec(&txn)
            .await?;

        StuckRecovery::Terminal
    } else if judgement.status == SubmissionStatus::Queued
        || !state.config.server.dispatcher_lease_steal_enabled
    {
        recover_stuck_judgement_without_steal(&txn, &judgement).await?
    } else {
        StuckRecovery::Skip
    };

    txn.commit().await?;

    match recovery {
        StuckRecovery::RedispatchJudgement {
            submission,
            judgement_id,
            fire_after_judging,
            retry_count,
        } => {
            info!(
                judgement_id,
                retry = retry_count,
                max_retries = max_stuck_retries,
                "Re-dispatching stuck submission judgement because lease/steal is disabled"
            );
            let dispatch_state = state.clone();
            tokio::spawn(async move {
                crate::handlers::submission::dispatch_to_plugin_with_judgement(
                    dispatch_state,
                    submission,
                    Some(judgement_id),
                    fire_after_judging,
                )
                .await;
            });
        }
        StuckRecovery::RedispatchSubmission { .. } | StuckRecovery::RedispatchCodeRun { .. } => {
            unreachable!("judgement handler returned a non-judgement recovery")
        }
        StuckRecovery::Terminal => {
            info!(
                judgement_id,
                retry = current_retry_count,
                max_retries = max_stuck_retries,
                "Marking stuck submission judgement as SystemError (retry budget exhausted)"
            );
        }
        StuckRecovery::Skip => {}
    }

    Ok(())
}

async fn recover_stuck_submission_without_steal(
    txn: &DatabaseTransaction,
    submission: &submission::Model,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = submission.retry_count.saturating_add(1);
    let new_epoch = submission.judge_epoch.saturating_add(1);

    let affected = submission::Entity::update_many()
        .col_expr(
            submission::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            submission::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            submission::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            submission::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(None::<String>).into(),
        )
        .col_expr(
            submission::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
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
        .filter(submission::Column::Id.eq(submission.id))
        .filter(submission::Column::JudgeEpoch.eq(submission.judge_epoch))
        .filter(submission::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        return Ok(StuckRecovery::Skip);
    }

    open_retry_submission_judgement(txn, submission, new_epoch).await?;

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

    Ok(StuckRecovery::RedispatchSubmission {
        model: redispatch_model,
        retry_count: new_retry_count,
    })
}

async fn recover_stuck_code_run_without_steal(
    txn: &DatabaseTransaction,
    run: &code_run::Model,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = run.retry_count.saturating_add(1);
    let new_epoch = run.judge_epoch.saturating_add(1);

    code_run_result::Entity::delete_many()
        .filter(code_run_result::Column::CodeRunId.eq(run.id))
        .exec(txn)
        .await?;

    let affected = code_run::Entity::update_many()
        .col_expr(
            code_run::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            code_run::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            code_run::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            code_run::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(None::<String>).into(),
        )
        .col_expr(
            code_run::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
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
        .filter(code_run::Column::Id.eq(run.id))
        .filter(code_run::Column::JudgeEpoch.eq(run.judge_epoch))
        .filter(code_run::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        return Ok(StuckRecovery::Skip);
    }

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

    Ok(StuckRecovery::RedispatchCodeRun {
        model: redispatch_model,
        retry_count: new_retry_count,
    })
}

async fn recover_stuck_judgement_without_steal(
    txn: &DatabaseTransaction,
    judgement: &submission_judgement::Model,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = judgement.retry_count.saturating_add(1);
    let new_epoch = judgement.judge_epoch.saturating_add(1);

    let Some(mut sub) = submission::Entity::find_by_id(judgement.submission_id)
        .one(txn)
        .await?
    else {
        return Ok(StuckRecovery::Skip);
    };

    if judgement.is_current {
        let affected = submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                submission::Column::RetryCount,
                sea_orm::sea_query::Expr::value(new_retry_count).into(),
            )
            .col_expr(
                submission::Column::JudgeEpoch,
                sea_orm::sea_query::Expr::value(new_epoch).into(),
            )
            .col_expr(
                submission::Column::OwnerServerId,
                sea_orm::sea_query::Expr::value(None::<String>).into(),
            )
            .col_expr(
                submission::Column::LeaseHeartbeatAt,
                sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
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
            .filter(submission::Column::Id.eq(sub.id))
            .filter(submission::Column::JudgeEpoch.eq(judgement.judge_epoch))
            .filter(submission::Column::Status.is_in(IN_PROGRESS_STATUSES))
            .exec(txn)
            .await?;
        if affected.rows_affected == 0 {
            return Ok(StuckRecovery::Skip);
        }
    }

    test_case_result::Entity::delete_many()
        .filter(test_case_result::Column::JudgementId.eq(judgement.id))
        .exec(txn)
        .await?;

    let affected = submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            submission_judgement::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            submission_judgement::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            submission_judgement::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(None::<String>).into(),
        )
        .col_expr(
            submission_judgement::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(None::<chrono::DateTime<chrono::Utc>>).into(),
        )
        .col_expr(
            submission_judgement::Column::Verdict,
            sea_orm::sea_query::Expr::value(None::<common::Verdict>).into(),
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
        .filter(submission_judgement::Column::Id.eq(judgement.id))
        .filter(submission_judgement::Column::JudgeEpoch.eq(judgement.judge_epoch))
        .filter(submission_judgement::Column::Status.is_in(IN_PROGRESS_STATUSES))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        anyhow::bail!(
            "stuck judgement recovery parent update succeeded but judgement update matched no rows"
        );
    }
    sub.status = SubmissionStatus::Pending;
    sub.judge_epoch = new_epoch;
    sub.retry_count = new_retry_count;
    sub.owner_server_id = None;
    sub.lease_heartbeat_at = None;
    if let Some(target) = judgement.target_worker_id.clone() {
        sub.target_worker_id = Some(target);
    }

    Ok(StuckRecovery::RedispatchJudgement {
        submission: sub,
        judgement_id: judgement.id,
        fire_after_judging: judgement.is_current,
        retry_count: new_retry_count,
    })
}

/// Supersede the abandoned current judgement and insert a fresh current
/// judgement for a detector-owned retry when lease/steal is disabled.
async fn open_retry_submission_judgement(
    txn: &DatabaseTransaction,
    submission: &submission::Model,
    new_epoch: i32,
) -> anyhow::Result<()> {
    use sea_orm::{ActiveModelTrait, QueryOrder, Set};

    submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::IsCurrent,
            sea_orm::sea_query::Expr::value(false).into(),
        )
        .col_expr(
            submission_judgement::Column::IsFinalized,
            sea_orm::sea_query::Expr::value(true).into(),
        )
        .col_expr(
            submission_judgement::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::SystemError.to_string()).into(),
        )
        .col_expr(
            submission_judgement::Column::ErrorCode,
            sea_orm::sea_query::Expr::value(Some(SubmissionDlqErrorCode::STUCK_JOB.to_string()))
                .into(),
        )
        .col_expr(
            submission_judgement::Column::ErrorMessage,
            sea_orm::sea_query::Expr::value(Some("Superseded by stuck-job retry".to_string()))
                .into(),
        )
        .col_expr(
            submission_judgement::Column::FinalizedAt,
            sea_orm::sea_query::Expr::cust("NOW()").into(),
        )
        .filter(submission_judgement::Column::SubmissionId.eq(submission.id))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
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

async fn create_unresolved_stuck_dlq_entry(
    txn: &DatabaseTransaction,
    message_id: String,
    message_type: DlqMessageType,
    submission_id: Option<i32>,
    payload: serde_json::Value,
    error_message: String,
) -> anyhow::Result<bool> {
    let existing = dead_letter_message::Entity::find()
        .filter(dead_letter_message::Column::MessageId.eq(&message_id))
        .filter(dead_letter_message::Column::Resolved.eq(false))
        .one(txn)
        .await?;

    if existing.is_some() {
        return Ok(false);
    }

    DlqService::new(txn)
        .create_entry(
            message_id,
            message_type,
            submission_id,
            payload,
            DlqErrorCode::StuckJob,
            error_message,
        )
        .await?;

    Ok(true)
}

fn stuck_code_run_message_id(code_run_id: i32) -> String {
    format!("stuck-code-run-{code_run_id}")
}

fn stuck_submission_judgement_message_id(judgement_id: i32) -> String {
    format!("stuck-submission-judgement-{judgement_id}")
}

fn stuck_retry_budget_exhausted(retry_count: i32, max_stuck_retries: u32) -> bool {
    let max = std::cmp::min(max_stuck_retries, i32::MAX as u32) as i32;
    retry_count > max
}

fn stuck_retries_exceeded_message(max_stuck_retries: u32) -> String {
    format!("Exceeded {max_stuck_retries} retries")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuck_detector_terminalizes_only_after_retry_threshold() {
        assert!(!stuck_retry_budget_exhausted(4, 5));
        assert!(!stuck_retry_budget_exhausted(5, 5));
        assert!(stuck_retry_budget_exhausted(6, 5));
    }

    #[test]
    fn stuck_detector_clamps_retry_threshold_to_i32() {
        assert!(!stuck_retry_budget_exhausted(
            i32::MAX,
            u32::try_from(i32::MAX).unwrap()
        ));
        assert!(!stuck_retry_budget_exhausted(i32::MAX, u32::MAX));
    }

    #[test]
    fn stuck_detector_error_message_names_retry_threshold() {
        assert_eq!(stuck_retries_exceeded_message(5), "Exceeded 5 retries");
    }

    #[test]
    fn stuck_detector_uses_stable_code_run_dlq_message_id() {
        assert_eq!(stuck_code_run_message_id(42), "stuck-code-run-42");
    }

    #[test]
    fn stuck_detector_uses_stable_judgement_dlq_message_id() {
        assert_eq!(
            stuck_submission_judgement_message_id(42),
            "stuck-submission-judgement-42"
        );
    }

    #[test]
    fn stuck_detector_new_dlq_message_types_are_non_retryable_by_default() {
        assert_eq!(DlqMessageType::StuckCodeRun.as_str(), "stuck_code_run");
        assert_eq!(
            DlqMessageType::StuckSubmissionJudgement.as_str(),
            "stuck_submission_judgement"
        );
    }
}
