use std::sync::Arc;

use chrono::Utc;
use common::metrics::Metrics;
use common::worker::{Task, TaskReply, TaskResult, TaskStartedNotification};
use tracing::{info, warn};

use crate::error::WorkerError;
use crate::metrics::record_mq_publish;
use crate::models::worker::Worker;

pub async fn process_task(
    task: &Task,
    worker: &Arc<Worker>,
    mq: &Arc<mq::Mq>,
    metrics: &Metrics,
) -> Result<TaskResult, WorkerError> {
    info!(
        job_id = %task.id,
        task_type = %task.task_type,
        "Processing task"
    );

    if task.task_type == "operation" {
        publish_started(
            TaskStartedNotification {
                task_id: task.id.clone(),
                started_at_unix_ms: Utc::now().timestamp_millis(),
            },
            task.reply_queue_name(),
            mq,
            metrics,
        );
    }

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

    let publish_start = std::time::Instant::now();
    let reply = TaskReply::Completed {
        result: result.clone(),
    };
    match mq
        .publish(task.reply_queue_name(), None, &reply, None)
        .await
    {
        Ok(_) => record_mq_publish(
            metrics,
            task.reply_queue_name(),
            "task_result",
            "success",
            publish_start,
        ),
        Err(e) => {
            record_mq_publish(
                metrics,
                task.reply_queue_name(),
                "task_result",
                "error",
                publish_start,
            );
            return Err(WorkerError::Mq(e.to_string()));
        }
    }

    info!(
        job_id = %task.id,
        task_result_id = %result.task_id,
        success = result.success,
        result_queue = %task.reply_queue_name(),
        "Task finished"
    );

    Ok(result)
}

fn publish_started(
    notification: TaskStartedNotification,
    reply_queue: &str,
    mq: &Arc<mq::Mq>,
    metrics: &Metrics,
) {
    let task_id = notification.task_id.clone();
    let reply_queue = reply_queue.to_string();
    let mq = Arc::clone(mq);
    let metrics = metrics.clone();

    tokio::spawn(async move {
        let publish_start = std::time::Instant::now();
        let reply = TaskReply::Started { notification };
        match mq.publish(&reply_queue, None, &reply, None).await {
            Ok(_) => record_mq_publish(
                &metrics,
                &reply_queue,
                "task_started",
                "success",
                publish_start,
            ),
            Err(e) => {
                record_mq_publish(
                    &metrics,
                    &reply_queue,
                    "task_started",
                    "error",
                    publish_start,
                );
                warn!(
                    job_id = %task_id,
                    result_queue = %reply_queue,
                    error = %e,
                    "Failed to publish task started notification"
                );
            }
        }
    });
}
