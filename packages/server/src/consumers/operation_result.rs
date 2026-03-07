use crate::registry::OperationWaiters;
use common::worker::TaskResult;
use mq::MqQueue;
use std::sync::Arc;
use tracing::{error, info};

pub async fn consume_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
) {
    info!(
        "Starting operation result consumer on queue: {}",
        queue_name
    );

    if let Err(e) = mq
        .process_messages(
            &queue_name,
            None,
            None,
            move |message: mq::BrokerMessage<TaskResult>| {
                let waiters = waiters.clone();
                async move {
                    let result = message.payload;
                    let task_id = result.task_id.clone();

                    if let Some((_, tx)) = waiters.remove(&task_id) {
                        if tx.send(result).is_err() {
                            error!(%task_id, "Failed to send operation result to waiter (receiver dropped)");
                        } else {
                            tracing::debug!(%task_id, "Operation result delivered to plugin");
                        }
                    } else {
                        tracing::warn!(%task_id, "Operation result received but no waiter found (batch may have been cancelled)");
                    }

                    Ok(())
                }
            },
        )
        .await
    {
        error!(error = %e, "Operation result consumer exited with error");
    }
}
