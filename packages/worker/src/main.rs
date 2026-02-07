mod config;
mod error;
mod handlers;
mod models;

use anyhow::Context;
use common::judge_job::JudgeJob;
use common::judge_result::JudgeResult;
use common::retry::{
    RetryCleanupGuard, RetryDecision, RetryTracker, calculate_backoff, spawn_cleanup_task,
};
use common::worker::Task;
use common::{DlqConfig, DlqEnvelope, DlqErrorCode, DlqMessageType};
use handlers::judge::handle_judge_job;
use mq::{BroccoliError, BrokerMessage, MqConfig, init_mq};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let config = config::WorkerAppConfig::load().context("Failed to load config")?;
    info!("Worker starting: {}", config.worker.id);

    let mq = Arc::new(
        init_mq(MqConfig {
            url: config.mq.url.clone(),
            pool_size: config.mq.pool_size,
        })
        .await
        .context("Failed to initialize MQ")?,
    );

    info!(
        queue_name = %config.mq.queue_name,
        result_queue_name = %config.mq.result_queue_name,
        dlq_queue_name = %config.mq.dlq_queue_name,
        max_retries = config.mq.dlq.max_retries,
        "MQ connected"
    );

    let result_queue = config.mq.result_queue_name.clone();
    let dlq_queue = config.mq.dlq_queue_name.clone();
    let dlq_config = config.mq.dlq.clone();
    let mq_for_handler = Arc::clone(&mq);

    let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));

    // TODO: Store handle for graceful shutdown. Currently the task runs until process exit.
    let _cleanup_handle = spawn_cleanup_task(
        retry_tracker.clone(),
        Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
        Duration::from_secs(dlq_config.retry_max_age_secs),
    );

    let result = mq
        .process_messages(
            &config.mq.queue_name,
            Some(config.worker.batch_size), // concurrent workers
            None,
            move |message: BrokerMessage<Task>| {
                let mq = Arc::clone(&mq_for_handler);
                let result_queue = result_queue.clone();
                let dlq_queue = dlq_queue.clone();
                let dlq_config = dlq_config.clone();
                let retry_tracker = Arc::clone(&retry_tracker);
                async move {
                    process_message(
                        message,
                        &mq,
                        &result_queue,
                        &dlq_queue,
                        &dlq_config,
                        &retry_tracker,
                    )
                    .await
                }
            },
        )
        .await;

    if let Err(e) = result {
        error!(error = %e, "Worker stopped unexpectedly");
    }

    Ok(())
}

async fn process_message(
    message: BrokerMessage<Task>,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
    dlq_queue: &str,
    dlq_config: &DlqConfig,
    retry_tracker: &Arc<Mutex<RetryTracker>>,
) -> Result<(), BroccoliError> {
    let task = message.payload;
    let job_id = task.id.clone();

    if task.task_type != "judge" {
        warn!(task_type = %task.task_type, "Unknown task type, skipping");
        return Ok(());
    }

    let job: JudgeJob = match serde_json::from_value(task.payload.clone()) {
        Ok(j) => j,
        Err(e) => {
            error!(job_id = %job_id, error = %e, "Failed to parse JudgeJob");

            let submission_id = task
                .payload
                .get("submission_id")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);

            let envelope = DlqEnvelope {
                message_id: job_id.clone(),
                message_type: DlqMessageType::JudgeJob,
                submission_id,
                payload: task.payload,
                error_code: DlqErrorCode::DeserializationError,
                error_message: format!("Failed to parse JudgeJob: {}", e),
                retry_history: vec![],
            };

            if let Err(pub_err) = mq.publish(dlq_queue, None, &envelope, None).await {
                error!(error = %pub_err, "Failed to publish to DLQ");
            }

            return Ok(());
        }
    };

    let submission_id = job.submission_id;

    let mut cleanup_guard = RetryCleanupGuard::new(retry_tracker, &job_id);

    loop {
        match process_job(&job, mq, result_queue).await {
            Ok(()) => {
                retry_tracker.lock().await.clear(&job_id);
                cleanup_guard.defuse();
                return Ok(());
            }
            Err(e) => {
                let error_str = e.to_string();
                let decision = retry_tracker
                    .lock()
                    .await
                    .record_failure(&job_id, &error_str);

                match decision {
                    RetryDecision::Retry { attempt, .. } => {
                        let delay = calculate_backoff(
                            attempt,
                            dlq_config.base_delay_ms,
                            dlq_config.max_delay_ms,
                        );
                        warn!(
                            submission_id,
                            job_id = %job_id,
                            attempt,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "Retrying job processing"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    RetryDecision::Exhausted { history } => {
                        error!(
                            submission_id,
                            job_id = %job_id,
                            retry_count = history.len(),
                            error = %e,
                            "Max retries exhausted, sending to DLQ"
                        );

                        let envelope = DlqEnvelope {
                            message_id: job_id.clone(),
                            message_type: DlqMessageType::JudgeJob,
                            submission_id: Some(submission_id),
                            payload: serde_json::to_value(&job).unwrap_or_default(),
                            error_code: DlqErrorCode::MaxRetriesExceeded,
                            error_message: error_str,
                            retry_history: history,
                        };

                        if let Err(pub_err) = mq.publish(dlq_queue, None, &envelope, None).await {
                            error!(error = %pub_err, "Failed to publish to DLQ queue");
                            return Err(BroccoliError::Publish(format!(
                                "Failed to publish to DLQ: {}",
                                pub_err
                            )));
                        }

                        cleanup_guard.defuse();
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn process_job(
    job: &JudgeJob,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
) -> Result<(), BroccoliError> {
    info!(
        submission_id = job.submission_id,
        job_id = %job.job_id,
        test_cases = job.test_cases.len(),
        "Processing judge job"
    );

    let result: JudgeResult = handle_judge_job(job.clone());

    mq.publish(result_queue, None, &result, None)
        .await
        .map_err(|e| BroccoliError::Publish(format!("Failed to publish JudgeResult: {e}")))?;

    info!(
        submission_id = result.submission_id,
        status = ?result.status,
        verdict = ?result.verdict,
        score = ?result.score,
        "Published result to queue"
    );

    Ok(())
}
