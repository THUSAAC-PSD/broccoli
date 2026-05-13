use crate::registry::OperationWaiters;
use common::metrics::Metrics;
use common::worker::TaskResult;
use mq::MqQueue;
use opentelemetry::KeyValue;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub async fn consume_operation_results(
    mq: Arc<MqQueue>,
    waiters: OperationWaiters,
    queue_name: String,
    metrics: Metrics,
) {
    info!(
        queue = %queue_name,
        "Starting operation result consumer"
    );

    // Concurrency=8: each handler invocation is short (DashMap lookup +
    // channel send), but under burst load (1000-submission window) the queue
    // can briefly buffer thousands of result messages. Single-threaded
    // consumption serializes the waker fan-out and adds head-of-line blocking
    // on whichever evaluator is currently slow. 8 concurrent handlers give
    // enough parallelism that the result queue never becomes the bottleneck,
    // without flooding the runtime — each handler holds no locks across .await.
    if let Err(e) = mq
        .process_messages(
            &queue_name,
            Some(8),
            None,
            move |message: mq::BrokerMessage<TaskResult>| {
                let waiters = waiters.clone();
                let metrics = metrics.clone();
                async move {
                    let started = std::time::Instant::now();
                    let result = message.payload;
                    let task_id = result.task_id.clone();
                    let task_success = result.success;

                    let outcome = if let Some((_, tx)) = waiters.remove(&task_id) {
                        if tx.send(result).is_err() {
                            error!(%task_id, "Failed to send operation result to waiter (receiver dropped)");
                            "receiver_dropped"
                        } else {
                            debug!(%task_id, "Operation result delivered to plugin");
                            "delivered"
                        }
                    } else {
                        warn!(%task_id, "Operation result received but no waiter found (batch may have been cancelled)");
                        "no_waiter"
                    };

                    let attrs = [
                        KeyValue::new("outcome", outcome),
                        KeyValue::new("task_success", task_success.to_string()),
                    ];
                    metrics.operation_result_messages_total.add(1, &attrs);
                    metrics
                        .operation_result_consume_duration
                        .record(started.elapsed().as_secs_f64(), &attrs);

                    Ok(())
                }
            },
        )
        .await
    {
        error!(error = %e, "Operation result consumer exited with error");
    }
}
