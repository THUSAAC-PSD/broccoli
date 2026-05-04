use crate::registry::OperationWaiters;
use common::worker::TaskResult;
use mq::MqQueue;
use std::sync::Arc;
use tracing::{error, info, warn};

pub async fn consume_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
) {
    consume_operation_results_inner(mq, waiters, queue_name, false).await;
}

/// Variant used by the rolling-upgrade compatibility consumer. Behaves
/// identically except every received message is warn-logged so operators
/// know a v0.2-era worker is still publishing to the un-suffixed queue.
/// Best-effort routing through the local waiters map only succeeds when this
/// replica is also the one that originated the task.
pub async fn consume_legacy_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
) {
    consume_operation_results_inner(mq, waiters, queue_name, true).await;
}

async fn consume_operation_results_inner(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
    legacy: bool,
) {
    info!(
        queue = %queue_name,
        legacy,
        "Starting operation result consumer"
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

                    if legacy {
                        warn!(
                            %task_id,
                            "Received operation result on legacy un-suffixed queue; \
                             a pre-upgrade worker is still publishing here"
                        );
                    }

                    if let Some((_, tx)) = waiters.remove(&task_id) {
                        if tx.send(result).is_err() {
                            error!(%task_id, "Failed to send operation result to waiter (receiver dropped)");
                        } else {
                            tracing::debug!(%task_id, "Operation result delivered to plugin");
                        }
                    } else if legacy {
                        warn!(
                            %task_id,
                            "Legacy operation result has no local waiter; \
                             originating replica is likely a different process"
                        );
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
