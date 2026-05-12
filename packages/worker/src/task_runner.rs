use std::sync::Arc;

use common::metrics::Metrics;
use common::worker::{Task, TaskResult};
use tracing::info;

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
    match mq
        .publish(task.reply_queue_name(), None, &result, None)
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
