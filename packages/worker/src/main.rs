mod config;
mod error;
mod models;

use anyhow::Context;
use common::retry::{
    RetryCleanupGuard, RetryDecision, RetryTracker, calculate_backoff, spawn_cleanup_task,
};
use common::worker::Task;
use common::{DlqConfig, DlqEnvelope, DlqErrorCode, DlqMessageType};
use mq::{BroccoliError, BrokerMessage, MqConfig, init_mq};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::error::WorkerError;
use crate::models::worker::Worker;

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
        operation_queue_name = %config.mq.operation_queue_name,
        operation_dlq_queue_name = %config.mq.operation_dlq_queue_name,
        max_retries = config.mq.dlq.max_retries,
        "MQ connected"
    );

    let op_dlq_queue = config.mq.operation_dlq_queue_name.clone();
    let dlq_config = config.mq.dlq.clone();
    let mq_for_handler = Arc::clone(&mq);
    let worker = Arc::new(Worker::new().await);

    let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));

    let _cleanup_handle = spawn_cleanup_task(
        retry_tracker.clone(),
        Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
        Duration::from_secs(dlq_config.retry_max_age_secs),
    );

    loop {
        let mq_for_handler = Arc::clone(&mq_for_handler);
        let worker = Arc::clone(&worker);
        let op_dlq_queue = op_dlq_queue.clone();
        let dlq_config = dlq_config.clone();
        let retry_tracker = Arc::clone(&retry_tracker);

        let op_fut = mq.process_messages(
            &config.mq.operation_queue_name,
            None,
            None,
            move |message: BrokerMessage<Task>| {
                let mq = Arc::clone(&mq_for_handler);
                let worker = Arc::clone(&worker);
                let dlq_queue = op_dlq_queue.clone();
                let dlq_config = dlq_config.clone();
                let retry_tracker = Arc::clone(&retry_tracker);
                async move {
                    process_message(
                        message,
                        &worker,
                        &mq,
                        &dlq_queue,
                        &dlq_config,
                        &retry_tracker,
                    )
                    .await
                }
            },
        );

        let should_break = tokio::select! {
            result = op_fut => {
                match result {
                    Ok(()) => {
                        info!("Operation consumer exited normally");
                        true
                    }
                    Err(e) => {
                        error!(error = %e, "Operation MQ error, reconnecting in 5s...");
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                            _ = tokio::signal::ctrl_c() => {
                                info!("Shutdown signal received during reconnect wait");
                                return Ok(());
                            }
                        }
                        false
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received, exiting gracefully...");
                true
            }
        };

        if should_break {
            break;
        }
    }

    info!("Worker stopped");
    Ok(())
}

async fn process_message(
    message: BrokerMessage<Task>,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
    dlq_queue: &str,
    dlq_config: &DlqConfig,
    retry_tracker: &Arc<Mutex<RetryTracker>>,
) -> Result<(), BroccoliError> {
    let task = message.payload;
    let task_id = task.id.clone();

    info!(
        job_id = %task_id,
        task_type = %task.task_type,
        executor_name = %task.executor_name,
        "Received task"
    );

    let mut cleanup_guard = RetryCleanupGuard::new(retry_tracker, &task_id);

    loop {
        match process_task(&task, worker, mq).await {
            Ok(()) => {
                retry_tracker.lock().await.clear(&task_id);
                cleanup_guard.defuse();
                return Ok(());
            }
            Err(e) => {
                let error_str = e.to_string();
                let decision = retry_tracker
                    .lock()
                    .await
                    .record_failure(&task_id, &error_str);

                match decision {
                    RetryDecision::Retry { attempt, .. } => {
                        let delay = calculate_backoff(
                            attempt,
                            dlq_config.base_delay_ms,
                            dlq_config.max_delay_ms,
                        );
                        warn!(
                            job_id = %task_id,
                            attempt,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "Task failed, retrying"
                        );
                        tokio::time::sleep(delay).await;
                    }
                    RetryDecision::Exhausted { history } => {
                        error!(
                            job_id = %task_id,
                            retry_count = history.len(),
                            error = %e,
                            "Max retries exhausted, sending to DLQ"
                        );

                        // Publish an error TaskResult so the waiting plugin receives
                        // a failure notification instead of hanging until timeout.
                        let error_result = common::worker::TaskResult {
                            task_id: task_id.clone(),
                            success: false,
                            output: serde_json::json!({}),
                            error: Some(format!(
                                "Operation failed after {} retries: {}",
                                history.len(),
                                error_str
                            )),
                        };
                        if let Err(e) = mq
                            .publish(&task.result_queue, None, &error_result, None)
                            .await
                        {
                            error!(job_id = %task_id, error = %e, "Failed to publish error result for operation task");
                        }

                        let payload = serde_json::to_value(&task).unwrap_or_else(|ser_err| {
                            error!(error = %ser_err, "Failed to serialize task for DLQ");
                            serde_json::json!({ "task_id": task_id })
                        });

                        let envelope = DlqEnvelope {
                            message_id: task_id.clone(),
                            message_type: DlqMessageType::OperationTask,
                            submission_id: None,
                            payload,
                            error_code: DlqErrorCode::MaxRetriesExceeded,
                            error_message: error_str,
                            retry_history: history,
                        };

                        if let Err(dlq_err) = mq.publish(dlq_queue, None, &envelope, None).await {
                            error!(
                                job_id = %task_id,
                                error = %dlq_err,
                                "CRITICAL: Failed to publish to DLQ, message may be lost"
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

async fn process_task(
    task: &Task,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
) -> Result<(), WorkerError> {
    info!(
        job_id = %task.id,
        task_type = %task.task_type,
        "Processing task"
    );

    let worker = Arc::clone(worker);
    let task_clone = task.clone();
    let result = tokio::spawn(async move { worker.execute_task(task_clone).await })
        .await
        .map_err(|e| {
            if e.is_panic() {
                WorkerError::TaskPanic(format!("{e}"))
            } else {
                WorkerError::Internal(format!("Task join error: {e}"))
            }
        })??;

    mq.publish(&task.result_queue, None, &result, None)
        .await
        .map_err(|e| WorkerError::Mq(e.to_string()))?;

    info!(
        job_id = %task.id,
        task_result_id = %result.task_id,
        success = result.success,
        result_queue = %task.result_queue,
        "Task finished"
    );

    Ok(())
}
