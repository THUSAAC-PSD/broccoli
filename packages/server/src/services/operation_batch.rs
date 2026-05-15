use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use broccoli_server_sdk::types::{OperationTask, SessionFile};
use common::storage::BlobStore;
use common::worker::{Task, TaskResult};
use futures::stream::{self, StreamExt, TryStreamExt};
use mq::config::PublishConfig;
use opentelemetry::KeyValue;
use uuid::Uuid;

use crate::host_funcs::context::OperationHostDeps;
use crate::registry::{BatchState, OperationBatches, OperationWaiter, OperationWaiters};

const INLINE_FILE_BLOB_THRESHOLD_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperationResultE2eLabels {
    task_type: String,
    operation: String,
    worker_id: String,
    outcome: &'static str,
}

fn operation_result_e2e_labels(
    task_result: &TaskResult,
    outcome: &'static str,
) -> OperationResultE2eLabels {
    OperationResultE2eLabels {
        task_type: task_result
            .task_type
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        operation: task_result
            .operation
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        worker_id: task_result
            .worker_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        outcome,
    }
}

fn record_operation_result_e2e(
    metrics: Option<&common::metrics::Metrics>,
    task_result: &TaskResult,
    outcome: &'static str,
) {
    let Some(metrics) = metrics else {
        return;
    };
    let Some(enqueued_at_unix_ms) = task_result.enqueued_at_unix_ms else {
        return;
    };

    let labels = operation_result_e2e_labels(task_result, outcome);
    let now = chrono::Utc::now().timestamp_millis();
    let duration_seconds = now.saturating_sub(enqueued_at_unix_ms) as f64 / 1_000.0;
    let attrs = [
        KeyValue::new("task_type", labels.task_type),
        KeyValue::new("operation", labels.operation),
        KeyValue::new("worker.id", labels.worker_id),
        KeyValue::new("outcome", labels.outcome),
        KeyValue::new("task_success", task_result.success.to_string()),
    ];
    metrics
        .operation_result_e2e_duration
        .record(duration_seconds, &attrs);
}

pub async fn start_operation_batch(
    plugin_id: String,
    deps: OperationHostDeps,
    operations: Vec<OperationTask>,
) -> anyhow::Result<String> {
    // Fast-fail before allocating batch state if MQ isn't configured. Each
    // per-op future also re-checks (since they clone `deps`), but rejecting
    // here preserves the early-return semantic (and avoids registering a
    // BatchState whose ops can never publish).
    deps.mq
        .as_ref()
        .ok_or_else(|| anyhow!("MQ not available"))?;

    let batch_id = Uuid::new_v4().to_string();

    let (batch_tx, batch_rx) = crossbeam::channel::unbounded();
    let pending_count = Arc::new(AtomicUsize::new(operations.len()));
    let cleanup_keys = Arc::new(
        operations
            .iter()
            .map(|_| Uuid::new_v4().to_string())
            .collect::<Vec<_>>(),
    );

    deps.operation_batches.insert(
        batch_id.clone(),
        BatchState {
            result_rx: batch_rx,
            pending_count: pending_count.clone(),
            created_at: Instant::now(),
            cleanup_keys: cleanup_keys.clone(),
            poisoned: AtomicBool::new(false),
        },
    );

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        operation_count = operations.len(),
        "Starting operation batch"
    );

    if let Some(metrics) = deps.metrics.as_ref() {
        metrics.batch_started_total.add(
            1,
            &[
                KeyValue::new("batch.kind", "operation"),
                KeyValue::new("plugin.id", plugin_id.clone()),
            ],
        );
        metrics
            .batch_active
            .add(1, &[KeyValue::new("batch.kind", "operation")]);
    }

    // UP#14c: parallelize per-op blob-externalize + mq.publish via
    // buffer_unordered. Each per-op future preserves the waiter-insert →
    // publish ordering invariant (UP#14d) by sequential await inside one
    // closure; cross-op ordering is intentionally unordered. First error
    // short-circuits via try_collect; in-flight peers are dropped.
    let publish_concurrency = deps.operation_batch_publish_concurrency.max(1);
    let per_op_inputs: Vec<(String, OperationTask)> =
        cleanup_keys.iter().cloned().zip(operations).collect();

    stream::iter(per_op_inputs.into_iter().map(|(correlation_id, op)| {
        let deps = deps.clone();
        let batch_tx = batch_tx.clone();
        let pending_count = pending_count.clone();
        let plugin_id = plugin_id.clone();
        let batch_id = batch_id.clone();
        async move {
            let (op_tx, op_rx) = tokio::sync::oneshot::channel();

            // Waiter insert must precede publish for THIS op (UP#14d). These
            // two sync calls happen before the awaits below.
            deps.operation_waiters.insert(
                correlation_id.clone(),
                OperationWaiter::new(op_tx),
            );
            spawn_waiter_forwarder(
                correlation_id.clone(),
                op_rx,
                batch_tx,
                pending_count,
                deps.metrics.clone(),
            );

            let target_queue =
                target_operation_queue(&deps.operation_queue_name, op.target_worker_id.as_deref());
            if op.target_worker_id.is_some() && target_queue == deps.operation_queue_name {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    target = ?op.target_worker_id,
                    "Rejecting operation with invalid target_worker_id; falling back to shared queue"
                );
            }

            if let (Some(eval_batch_id), Some(test_case_id)) =
                (op.evaluate_batch_id.clone(), op.test_case_id)
            {
                deps.evaluate_ops_registry.record_ops(
                    &eval_batch_id,
                    test_case_id,
                    &batch_id,
                    std::iter::once(correlation_id.clone()),
                );
            }

            let op = externalize_large_inline_files(op, deps.blob_store.clone())
                .await
                .with_context(|| "Blob store error")?;

            let task = Task {
                id: correlation_id.clone(),
                task_type: "operation".to_string(),
                executor_name: "operation".to_string(),
                payload: serde_json::to_value(&op)
                    .with_context(|| "Failed to serialize operation")?,
                result_queue: deps.operation_result_queue_name.clone(),
                operation_batch_id: None,
                reply_queue: Some(deps.operation_result_queue_name.clone()),
                priority: op.priority,
                trace_context: common::observability::inject_trace_context(),
                enqueued_at_unix_ms: Some(chrono::Utc::now().timestamp_millis()),
            };

            let mq = deps
                .mq
                .as_ref()
                .ok_or_else(|| anyhow!("MQ not available"))?;
            let publish_start = Instant::now();
            let publish_result = mq
                .publish(
                    &target_queue,
                    None,
                    &task,
                    task.priority
                        .map(|p| PublishConfig::builder().priority(p).build()),
                )
                .await;
            record_mq_publish(
                deps.metrics.as_ref(),
                &target_queue,
                "operation_task",
                if publish_result.is_ok() {
                    "success"
                } else {
                    "error"
                },
                publish_start,
            );
            publish_result.with_context(|| "MQ publish error")?;

            tracing::debug!(
                batch_id = %batch_id,
                correlation_id = %correlation_id,
                "Operation dispatched"
            );

            Ok::<(), anyhow::Error>(())
        }
    }))
    .buffer_unordered(publish_concurrency)
    .try_collect::<Vec<()>>()
    .await?;

    Ok(batch_id)
}

pub fn next_operation_result(
    plugin_id: &str,
    batches: &OperationBatches,
    metrics: Option<&common::metrics::Metrics>,
    batch_id: &str,
    timeout: Duration,
) -> anyhow::Result<Option<TaskResult>> {
    let (result_rx, pending_count) = {
        let batch = batches
            .get(batch_id)
            .ok_or_else(|| anyhow!("Batch not found: {}", batch_id))?;
        (batch.result_rx.clone(), batch.pending_count.clone())
    };

    let wait_start = Instant::now();
    let result = result_rx.recv_timeout(timeout);

    match result {
        Ok(task_result) => {
            record_operation_result_e2e(metrics, &task_result, "delivered");
            if let Some(metrics) = metrics {
                let attrs = [
                    KeyValue::new("batch.kind", "operation"),
                    KeyValue::new("plugin.id", plugin_id.to_string()),
                    KeyValue::new("outcome", "result"),
                    KeyValue::new("task_success", task_result.success.to_string()),
                ];
                metrics
                    .batch_wait_duration
                    .record(wait_start.elapsed().as_secs_f64(), &attrs);
                metrics.batch_results_total.add(1, &attrs);
            }
            tracing::debug!(
                plugin_id = %plugin_id,
                batch_id = %batch_id,
                task_id = %task_result.task_id,
                "Operation result received"
            );

            if pending_count.load(Ordering::SeqCst) == 0
                && result_rx.is_empty()
                && batches.remove(batch_id).is_some()
                && let Some(metrics) = metrics
            {
                metrics
                    .batch_active
                    .add(-1, &[KeyValue::new("batch.kind", "operation")]);
            }

            Ok(Some(task_result))
        }
        Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
            if let Some(metrics) = metrics {
                let attrs = [
                    KeyValue::new("batch.kind", "operation"),
                    KeyValue::new("plugin.id", plugin_id.to_string()),
                    KeyValue::new("outcome", "timeout"),
                ];
                metrics
                    .batch_wait_duration
                    .record(wait_start.elapsed().as_secs_f64(), &attrs);
                metrics.batch_results_total.add(1, &attrs);
            }
            Ok(None)
        }
        Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
            if let Some(metrics) = metrics {
                let attrs = [
                    KeyValue::new("batch.kind", "operation"),
                    KeyValue::new("plugin.id", plugin_id.to_string()),
                    KeyValue::new("outcome", "disconnected"),
                ];
                metrics
                    .batch_wait_duration
                    .record(wait_start.elapsed().as_secs_f64(), &attrs);
                metrics.batch_results_total.add(1, &attrs);
            }
            Err(anyhow!("Batch channel disconnected"))
        }
    }
}

pub fn cancel_operation_batch(
    plugin_id: &str,
    batches: &OperationBatches,
    waiters: &OperationWaiters,
    metrics: Option<&common::metrics::Metrics>,
    batch_id: &str,
) {
    if let Some((_, batch)) = batches.remove(batch_id) {
        for key in batch.cleanup_keys.iter() {
            waiters.remove(key);
        }
        if let Some(metrics) = metrics {
            let attrs = [
                KeyValue::new("batch.kind", "operation"),
                KeyValue::new("plugin.id", plugin_id.to_string()),
            ];
            metrics.batch_cancelled_total.add(1, &attrs);
            metrics
                .batch_active
                .add(-1, &[KeyValue::new("batch.kind", "operation")]);
        }
    }

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        "Operation batch cancelled"
    );
}

fn spawn_waiter_forwarder(
    correlation_id: String,
    op_rx: tokio::sync::oneshot::Receiver<TaskResult>,
    batch_tx: crossbeam::channel::Sender<TaskResult>,
    pending_count: Arc<AtomicUsize>,
    metrics: Option<common::metrics::Metrics>,
) {
    if let Some(metrics) = metrics.as_ref() {
        metrics
            .batch_pending_items
            .add(1, &[KeyValue::new("batch.kind", "operation")]);
    }

    tokio::spawn(async move {
        match op_rx.await {
            Ok(result) => {
                let _ = batch_tx.send(result);
                pending_count.fetch_sub(1, Ordering::SeqCst);
                if let Some(metrics) = metrics.as_ref() {
                    metrics
                        .batch_pending_items
                        .add(-1, &[KeyValue::new("batch.kind", "operation")]);
                }
            }
            Err(_) => {
                let error_result = TaskResult {
                    task_id: correlation_id,
                    success: false,
                    output: serde_json::json!({}),
                    error: Some("Operation cancelled or timed out".into()),
                    task_type: Some("operation".to_string()),
                    operation: Some("operation".to_string()),
                    worker_id: None,
                    enqueued_at_unix_ms: None,
                };
                let _ = batch_tx.send(error_result);
                pending_count.fetch_sub(1, Ordering::SeqCst);
                if let Some(metrics) = metrics.as_ref() {
                    metrics
                        .batch_pending_items
                        .add(-1, &[KeyValue::new("batch.kind", "operation")]);
                }
            }
        }
    });
}

fn target_operation_queue(shared_queue: &str, target_worker_id: Option<&str>) -> String {
    match target_worker_id {
        Some(worker_id) if crate::config::is_valid_server_id(worker_id) => {
            format!("{}:worker:{}", shared_queue, worker_id)
        }
        _ => shared_queue.to_string(),
    }
}

async fn externalize_large_inline_files(
    mut op: OperationTask,
    blob_store: Arc<dyn BlobStore>,
) -> Result<OperationTask, common::storage::StorageError> {
    let mut replaced = 0usize;
    let mut replaced_bytes = 0usize;

    for env in &mut op.environments {
        for (_path, file) in &mut env.files_in {
            let SessionFile::Content { content } = file else {
                continue;
            };
            if content.len() < INLINE_FILE_BLOB_THRESHOLD_BYTES {
                continue;
            }

            let hash = blob_store.put(content.as_bytes()).await?;
            replaced += 1;
            replaced_bytes += content.len();
            *file = SessionFile::Blob {
                hash: hash.to_hex(),
            };
        }
    }

    if replaced > 0 {
        tracing::info!(
            replaced,
            replaced_bytes,
            "Externalized large inline operation files to blob storage"
        );
    }

    Ok(op)
}

fn record_mq_publish(
    metrics: Option<&common::metrics::Metrics>,
    queue: &str,
    message_type: &'static str,
    outcome: &'static str,
    start: Instant,
) {
    let Some(metrics) = metrics else {
        return;
    };
    let attrs = [
        KeyValue::new("queue", queue.to_string()),
        KeyValue::new("message.type", message_type),
        KeyValue::new("outcome", outcome),
    ];
    metrics
        .mq_publish_duration
        .record(start.elapsed().as_secs_f64(), &attrs);
    metrics.mq_publish_messages_total.add(1, &attrs);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use broccoli_server_sdk::types::{Environment, OperationTask, SessionFile};
    use common::storage::BlobStore;
    use common::storage::filesystem::FilesystemBlobStore;

    use super::*;

    #[test]
    fn operation_result_e2e_labels_prefer_result_metadata() {
        let result = TaskResult {
            task_id: "op-1".to_string(),
            success: true,
            output: serde_json::json!({}),
            error: None,
            task_type: Some("operation".to_string()),
            operation: Some("compile".to_string()),
            worker_id: Some("worker-a".to_string()),
            enqueued_at_unix_ms: Some(1_234),
        };

        let labels = operation_result_e2e_labels(&result, "delivered");

        assert_eq!(labels.task_type, "operation");
        assert_eq!(labels.operation, "compile");
        assert_eq!(labels.worker_id, "worker-a");
        assert_eq!(labels.outcome, "delivered");
    }

    fn operation_with_file(file: SessionFile) -> OperationTask {
        OperationTask {
            environments: vec![Environment {
                id: "env".to_string(),
                files_in: vec![("input.txt".to_string(), file)],
            }],
            tasks: vec![],
            channels: vec![],
            priority: None,
            target_worker_id: None,
            evaluate_batch_id: None,
            test_case_id: None,
        }
    }

    #[test]
    fn invalid_target_worker_falls_back_to_shared_queue() {
        assert_eq!(target_operation_queue("ops", Some("../bad")), "ops");
        assert_eq!(target_operation_queue("ops", None), "ops");
    }

    #[test]
    fn valid_target_worker_uses_private_queue() {
        assert_eq!(
            target_operation_queue("ops", Some("worker-1")),
            "ops:worker:worker-1"
        );
    }

    #[tokio::test]
    async fn externalizes_large_inline_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            FilesystemBlobStore::new(temp.path().to_path_buf(), 2_000_000)
                .await
                .unwrap(),
        );
        let content = "x".repeat(INLINE_FILE_BLOB_THRESHOLD_BYTES);

        let op = externalize_large_inline_files(
            operation_with_file(SessionFile::Content {
                content: content.clone(),
            }),
            store.clone(),
        )
        .await
        .unwrap();

        let SessionFile::Blob { hash } = &op.environments[0].files_in[0].1 else {
            panic!("large inline file should be replaced with a blob reference");
        };
        assert!(
            store
                .exists(&common::storage::ContentHash::compute(content.as_bytes()))
                .await
                .unwrap()
        );
        assert!(!hash.is_empty());
    }

    #[tokio::test]
    async fn start_operation_batch_fails_when_mq_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let blob_store = Arc::new(
            FilesystemBlobStore::new(temp.path().to_path_buf(), 2_000_000)
                .await
                .unwrap(),
        );
        let deps = OperationHostDeps {
            mq: None,
            operation_batches: Arc::new(dashmap::DashMap::new()),
            operation_waiters: Arc::new(dashmap::DashMap::new()),
            operation_queue_name: "ops".to_string(),
            operation_result_queue_name: "ops-results".to_string(),
            blob_store,
            metrics: None,
            evaluate_ops_registry:
                crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default(),
            operation_batch_publish_concurrency: 32,
        };

        let err = start_operation_batch("plugin".to_string(), deps, vec![])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("MQ not available"));
    }

    #[test]
    fn cancel_operation_batch_removes_waiters() {
        let batches = Arc::new(dashmap::DashMap::new());
        let waiters = Arc::new(dashmap::DashMap::new());
        let (batch_tx, batch_rx) = crossbeam::channel::unbounded();
        drop(batch_tx);
        let (waiter_tx, _waiter_rx) = tokio::sync::oneshot::channel();
        let cleanup_keys = Arc::new(vec!["task-1".to_string()]);
        batches.insert(
            "batch-1".to_string(),
            BatchState {
                result_rx: batch_rx,
                pending_count: Arc::new(AtomicUsize::new(1)),
                created_at: Instant::now(),
                cleanup_keys: cleanup_keys.clone(),
                poisoned: AtomicBool::new(false),
            },
        );
        waiters.insert("task-1".to_string(), OperationWaiter::new(waiter_tx));

        cancel_operation_batch("plugin", &batches, &waiters, None, "batch-1");

        assert!(batches.get("batch-1").is_none());
        assert!(waiters.get("task-1").is_none());
    }
}
