use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use common::{
    DlqConfig, DlqEnvelope, DlqErrorCode, DlqMessageType, SubmissionDlqErrorCode,
    judge_result::JudgeResult,
    retry::{
        RetryCleanupGuard, RetryDecision, RetryTracker, calculate_backoff, spawn_cleanup_task,
    },
};
use mq::{BrokerMessage, Mq};
use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use super::mark_submission_system_error;
use crate::dlq::DlqService;
use crate::entity::{submission, test_case_result};

/// Consume judge results from the result queue.
pub async fn consume_judge_results(
    db: DatabaseConnection,
    mq: Arc<Mq>,
    queue_name: String,
    dlq_config: DlqConfig,
) {
    info!(
        queue = %queue_name,
        max_retries = dlq_config.max_retries,
        "Starting judge result consumer with retry logic"
    );

    let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));
    let base_delay_ms = dlq_config.base_delay_ms;
    let max_delay_ms = dlq_config.max_delay_ms;

    // TODO: Store handle for graceful shutdown. Currently the task runs until process exit.
    let _cleanup_handle = spawn_cleanup_task(
        retry_tracker.clone(),
        Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
        Duration::from_secs(dlq_config.retry_max_age_secs),
    );

    let result = mq
        .process_messages(
            &queue_name,
            None, // single-threaded for sequential DB writes
            None,
            move |message: BrokerMessage<JudgeResult>| {
                let db = db.clone();
                let retry_tracker = retry_tracker.clone();

                async move {
                    let result = message.payload;
                    let submission_id = result.submission_id;
                    let job_id = result.job_id.clone();

                    let mut cleanup_guard = RetryCleanupGuard::new(&retry_tracker, &job_id);

                    loop {
                        match process_judge_result(&db, &result).await {
                            Ok(()) => {
                                retry_tracker.lock().await.clear(&job_id);
                                cleanup_guard.defuse();
                                return Ok(());
                            }
                            Err(e) => {
                                let error_str = e.to_string();
                                let decision = retry_tracker.lock().await.record_failure(&job_id, &error_str);

                                match decision {
                                    RetryDecision::Retry { attempt, .. } => {
                                        let delay = calculate_backoff(attempt, base_delay_ms, max_delay_ms);
                                        warn!(
                                            submission_id,
                                            job_id = %job_id,
                                            attempt,
                                            delay_ms = delay.as_millis() as u64,
                                            error = %e,
                                            "Retrying judge result processing"
                                        );
                                        tokio::time::sleep(delay).await;
                                    }
                                    RetryDecision::Exhausted { history } => {
                                        error!(
                                            submission_id,
                                            job_id = %job_id,
                                            retry_count = history.len(),
                                            error = %e,
                                            "Max retries exhausted, moving to DLQ"
                                        );

                                        let payload = match serde_json::to_value(&result) {
                                            Ok(v) => v,
                                            Err(ser_err) => {
                                                error!(error = %ser_err, "Failed to serialize result for DLQ");
                                                serde_json::json!({ "submission_id": submission_id })
                                            }
                                        };

                                        let envelope = DlqEnvelope {
                                            message_id: job_id.clone(),
                                            message_type: DlqMessageType::JudgeResult,
                                            submission_id: Some(submission_id),
                                            payload,
                                            error_code: DlqErrorCode::MaxRetriesExceeded,
                                            error_message: error_str,
                                            retry_history: history,
                                        };

                                        {
                                            let txn = match db.begin().await {
                                                Ok(txn) => txn,
                                                Err(txn_err) => {
                                                    error!(
                                                        error = %txn_err,
                                                        submission_id,
                                                        job_id = %job_id,
                                                        payload = %serde_json::to_string(&result).unwrap_or_default(),
                                                        "CRITICAL: Failed to begin DLQ transaction, message will be lost"
                                                    );
                                                    return Err(mq::BroccoliError::Job(format!(
                                                        "DLQ transaction failed: {}",
                                                        txn_err
                                                    )));
                                                }
                                            };

                                            let dlq = DlqService::new(&txn);
                                            if let Err(dlq_err) = dlq.send_to_dlq(&envelope).await {
                                                error!(
                                                    error = %dlq_err,
                                                    submission_id,
                                                    job_id = %job_id,
                                                    payload = %serde_json::to_string(&result).unwrap_or_default(),
                                                    "CRITICAL: Failed to persist exhausted message to DLQ, message will be lost"
                                                );
                                                return Err(mq::BroccoliError::Job(format!(
                                                    "DLQ persistence failed: {}",
                                                    dlq_err
                                                )));
                                            }

                                            if let Err(commit_err) = txn.commit().await {
                                                error!(
                                                    submission_id,
                                                    job_id = %job_id,
                                                    error = %commit_err,
                                                    "CRITICAL: Failed to commit DLQ entry"
                                                );
                                                return Err(mq::BroccoliError::Job(format!(
                                                    "DLQ commit failed: {}",
                                                    commit_err
                                                )));
                                            }
                                        }

                                        if let Err(update_err) = mark_submission_system_error(
                                            &db,
                                            submission_id,
                                            SubmissionDlqErrorCode::RESULT_PROCESSING_FAILED,
                                            "Failed to process judge result after max retries",
                                        )
                                        .await
                                        {
                                            warn!(
                                                submission_id,
                                                job_id = %job_id,
                                                error = %update_err,
                                                "Failed to mark submission as SystemError \
                                                 (DLQ entry persisted, submission may need manual review)"
                                            );
                                        }

                                        cleanup_guard.defuse();
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Judge result consumer stopped unexpectedly");
    }
}

/// Process a single judge result.
async fn process_judge_result(db: &DatabaseConnection, result: &JudgeResult) -> anyhow::Result<()> {
    let txn = db.begin().await?;

    let _ = submission::Entity::find_by_id(result.submission_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Submission {} not found", result.submission_id))?;

    let existing_count = test_case_result::Entity::find()
        .filter(test_case_result::Column::SubmissionId.eq(result.submission_id))
        .count(&txn)
        .await?;

    if existing_count > 0 {
        info!(
            submission_id = result.submission_id,
            existing_count, "Submission already processed, skipping"
        );

        txn.commit().await?;
        return Ok(());
    }

    let (error_code, error_message) = result
        .error_info
        .as_ref()
        .map(|info| (Some(info.code.to_string()), Some(info.message.clone())))
        .unwrap_or((None, None));

    let submission_update = submission::ActiveModel {
        id: Set(result.submission_id),
        status: Set(result.status),
        verdict: Set(result.verdict),
        score: Set(result.score),
        time_used: Set(result.time_used),
        memory_used: Set(result.memory_used),
        compile_output: Set(result.compile_output.clone()),
        error_code: Set(error_code),
        error_message: Set(error_message),
        judged_at: Set(Some(Utc::now())),
        ..Default::default()
    };
    submission_update.update(&txn).await?;

    let now = Utc::now();

    for tc_result in result.test_case_results.iter() {
        let model = test_case_result::ActiveModel {
            submission_id: Set(result.submission_id),
            test_case_id: Set(tc_result.test_case_id),
            verdict: Set(tc_result.verdict),
            score: Set(tc_result.score),
            time_used: Set(tc_result.time_used),
            memory_used: Set(tc_result.memory_used),
            stdout: Set(tc_result.stdout.clone()),
            stderr: Set(tc_result.stderr.clone()),
            checker_output: Set(tc_result.checker_output.clone()),
            created_at: Set(now),
            ..Default::default()
        };
        model.insert(&txn).await?;
    }

    txn.commit().await?;

    info!(
        submission_id = result.submission_id,
        status = ?result.status,
        verdict = ?result.verdict,
        score = ?result.score,
        "Processed judge result"
    );

    Ok(())
}
