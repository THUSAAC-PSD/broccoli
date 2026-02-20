mod config;
mod error;
mod models;

use anyhow::Context;
use common::DlqConfig;
use common::retry::{RetryTracker, spawn_cleanup_task};
use common::worker::Task;
use mq::{BroccoliError, BrokerMessage, MqConfig, init_mq};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info};

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
    let worker = Arc::new(Worker::new());

    let retry_tracker = Arc::new(Mutex::new(RetryTracker::new(dlq_config.max_retries)));

    // TODO: Store handle for graceful shutdown. Currently the task runs until process exit.
    let _cleanup_handle = spawn_cleanup_task(
        retry_tracker.clone(),
        Duration::from_secs(dlq_config.retry_cleanup_interval_secs),
        Duration::from_secs(dlq_config.retry_max_age_secs),
    );

    // TODO: consider use an infinite loop
    let result = mq
        .process_messages(
            &config.mq.queue_name,
            None,
            None,
            move |message: BrokerMessage<Task>| {
                let mq = Arc::clone(&mq_for_handler);
                let worker = Arc::clone(&worker);
                let result_queue = result_queue.clone();
                let dlq_queue = dlq_queue.clone();
                let dlq_config = dlq_config.clone();
                let retry_tracker = Arc::clone(&retry_tracker);
                async move {
                    process_message(
                        message,
                        &worker,
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
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
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

    if let Err(e) = process_task(&task, worker, mq, result_queue).await {
        error!(
            job_id = %task_id,
            error = %e,
            dlq_queue = %dlq_queue,
            max_retries = dlq_config.max_retries,
            "Task execution failed (DLQ flow is not implemented yet)"
        );
    }

    let mut tracker = retry_tracker.lock().await;
    tracker.clear(&task_id);

    Ok(())
}

async fn process_task(
    task: &Task,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
    result_queue: &str,
) -> Result<(), WorkerError> {
    info!(
        job_id = %task.id,
        task_type = %task.task_type,
        "Processing task"
    );

    let result = worker.execute_task(task.clone()).await?;

    // Judge tasks: publish the inner JudgeResult directly (server consumer expects it)
    // Other tasks: publish the full TaskResult wrapper
    if task.task_type == "judge" {
        mq.publish(result_queue, None, &result.output, None)
            .await
            .map_err(|e| WorkerError::Mq(e.to_string()))?;
    } else {
        mq.publish(result_queue, None, &result, None)
            .await
            .map_err(|e| WorkerError::Mq(e.to_string()))?;
    }

    info!(
        job_id = %task.id,
        task_result_id = %result.task_id,
        success = result.success,
        result_queue = %result_queue,
        "Task finished"
    );

    Ok(())
}
