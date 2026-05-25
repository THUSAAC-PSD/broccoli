use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use broccoli_server_sdk::types::{
    DetachedOperationCallbackAction, DetachedOperationCallbackEvent,
    DetachedOperationCallbackInput, DetachedOperationCallbackOutput, DetachedOperationSession,
    OperationResult, OperationTask, SessionFile, StartDetachedWindowedOperationInput,
};
use common::storage::BlobStore;
use common::worker::{Task, TaskResult};
use futures::stream::{self, StreamExt, TryStreamExt};
use mq::{MqQueue, config::PublishConfig};
use opentelemetry::KeyValue;
use plugin_core::retry::{PoolRetryPolicy, call_raw_with_pool_retry};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::host_funcs::context::OperationHostDeps;
use crate::registry::{BatchState, OperationBatches, OperationWaiter, OperationWaiters};

const INLINE_FILE_BLOB_THRESHOLD_BYTES: usize = 1_048_576;

#[async_trait]
pub trait OperationTaskPublisher: Send + Sync {
    async fn publish_operation_task(
        &self,
        target_queue: &str,
        task: &Task,
        publish_config: Option<PublishConfig>,
    ) -> anyhow::Result<()>;
}

pub struct MqOperationTaskPublisher {
    mq: Arc<MqQueue>,
}

impl MqOperationTaskPublisher {
    pub fn new(mq: Arc<MqQueue>) -> Self {
        Self { mq }
    }
}

#[async_trait]
impl OperationTaskPublisher for MqOperationTaskPublisher {
    async fn publish_operation_task(
        &self,
        target_queue: &str,
        task: &Task,
        publish_config: Option<PublishConfig>,
    ) -> anyhow::Result<()> {
        self.mq
            .publish(target_queue, None, task, publish_config)
            .await
            .map(|_| ())
            .with_context(|| "MQ publish error")
    }
}

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
    deps.operation_publisher
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
    let evaluate_refs = operations
        .iter()
        .filter_map(|op| {
            op.evaluate_batch_id
                .clone()
                .zip(op.test_case_id)
                .map(|(evaluate_batch_id, test_case_id)| (evaluate_batch_id, test_case_id))
        })
        .collect::<Vec<_>>();

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

            let publisher = deps
                .operation_publisher
                .as_ref()
                .ok_or_else(|| anyhow!("MQ not available"))?;
            let publish_start = Instant::now();
            let publish_result = publisher
                .publish_operation_task(
                    &target_queue,
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
    .await
    .inspect_err(|_| {
        cleanup_failed_operation_batch(
            &deps,
            &batch_id,
            cleanup_keys.as_ref(),
            &evaluate_refs,
            plugin_id.as_str(),
        );
    })?;

    Ok(batch_id)
}

fn cleanup_failed_operation_batch(
    deps: &OperationHostDeps,
    batch_id: &str,
    cleanup_keys: &[String],
    evaluate_refs: &[(String, i32)],
    plugin_id: &str,
) {
    deps.operation_batches.remove(batch_id);
    for correlation_id in cleanup_keys {
        deps.operation_waiters.remove(correlation_id);
    }
    for (evaluate_batch_id, test_case_id) in evaluate_refs {
        deps.evaluate_ops_registry.remove_operation_batch(
            evaluate_batch_id,
            *test_case_id,
            batch_id,
        );
    }
    if let Some(metrics) = deps.metrics.as_ref() {
        metrics.batch_active.add(
            -1,
            &[
                KeyValue::new("batch.kind", "operation"),
                KeyValue::new("plugin.id", plugin_id.to_string()),
            ],
        );
    }
}

pub fn start_detached_windowed_operation(
    plugin_id: String,
    plugin_manager: Arc<dyn plugin_core::traits::PluginManager>,
    deps: OperationHostDeps,
    input: StartDetachedWindowedOperationInput,
) -> anyhow::Result<DetachedOperationSession> {
    if input.callback_fn.trim().is_empty() {
        return Err(anyhow!("Detached operation callback_fn cannot be empty"));
    }

    let session_id = Uuid::new_v4().to_string();
    let session = DetachedOperationSession {
        session_id: session_id.clone(),
    };
    tokio::spawn(run_detached_windowed_operation(
        plugin_id,
        plugin_manager,
        deps,
        session_id,
        input,
    ));
    Ok(session)
}

#[derive(Debug)]
struct DetachedOperationResult {
    operation_index: usize,
    batch_id: String,
    result: anyhow::Result<Option<TaskResult>>,
}

async fn run_detached_windowed_operation(
    plugin_id: String,
    plugin_manager: Arc<dyn plugin_core::traits::PluginManager>,
    deps: OperationHostDeps,
    session_id: String,
    input: StartDetachedWindowedOperationInput,
) {
    let total = input.operations.len();
    let concurrency = input.concurrency.max(1);
    let timeout = Duration::from_millis(input.result_timeout_ms);
    let mut pending = input.operations.into_iter().enumerate().collect::<Vec<_>>();
    pending.reverse();
    let mut active = Vec::<(usize, String)>::new();
    let (tx, mut rx) = mpsc::channel::<DetachedOperationResult>(concurrency * 2);
    let mut state = input.state;
    let mut completed = 0usize;

    while active.len() < concurrency {
        let Some((operation_index, operation)) = pending.pop() else {
            break;
        };
        match start_detached_operation_slot(
            &plugin_id,
            deps.clone(),
            tx.clone(),
            operation_index,
            operation,
            timeout,
        )
        .await
        {
            Ok(batch_id) => active.push((operation_index, batch_id)),
            Err(e) => {
                let callback = DetachedOperationCallbackInput {
                    session_id: session_id.clone(),
                    state: state.clone(),
                    event: DetachedOperationCallbackEvent::Timeout {
                        message: e.to_string(),
                    },
                    completed,
                    total,
                };
                let _ = call_detached_operation_callback(
                    plugin_manager.as_ref(),
                    &plugin_id,
                    &input.callback_fn,
                    callback,
                )
                .await;
                cancel_active_operation_slots(&plugin_id, &deps, &active);
                return;
            }
        }
    }

    if active.is_empty() {
        send_detached_operation_exhausted(
            plugin_manager.as_ref(),
            &plugin_id,
            &input.callback_fn,
            session_id,
            state,
            completed,
            total,
        )
        .await;
        return;
    }

    while let Some(item) = rx.recv().await {
        if !remove_active_operation_slot_by_batch_id(&mut active, &item.batch_id) {
            continue;
        }
        completed += 1;

        let event = match item.result {
            Ok(Some(task_result)) => match operation_result_from_task_result(task_result) {
                Ok(result) => DetachedOperationCallbackEvent::Result {
                    operation_index: item.operation_index,
                    result,
                },
                Err(e) => DetachedOperationCallbackEvent::Timeout {
                    message: e.to_string(),
                },
            },
            Ok(None) => DetachedOperationCallbackEvent::Timeout {
                message: format!("Operation {} timed out", item.operation_index),
            },
            Err(e) => DetachedOperationCallbackEvent::Timeout {
                message: e.to_string(),
            },
        };

        let callback = DetachedOperationCallbackInput {
            session_id: session_id.clone(),
            state: state.clone(),
            event,
            completed,
            total,
        };

        let output = match call_detached_operation_callback(
            plugin_manager.as_ref(),
            &plugin_id,
            &input.callback_fn,
            callback,
        )
        .await
        {
            Ok(output) => output,
            Err(e) => {
                tracing::error!(%plugin_id, %session_id, error = %e, "Detached operation callback failed");
                cancel_active_operation_slots(&plugin_id, &deps, &active);
                return;
            }
        };

        state = output.state;
        let refill_enabled = output.refill;

        if !output.cancel_operation_indices.is_empty() {
            let cancel_set: HashSet<usize> =
                output.cancel_operation_indices.iter().copied().collect();
            cancel_operation_indices(&plugin_id, &deps, &mut active, &cancel_set);
            pending.retain(|(idx, _)| !cancel_set.contains(idx));
        }

        match output.action {
            DetachedOperationCallbackAction::Continue => {}
            DetachedOperationCallbackAction::Finish | DetachedOperationCallbackAction::Cancel => {
                cancel_active_operation_slots(&plugin_id, &deps, &active);
                return;
            }
        }

        while refill_enabled && active.len() < concurrency {
            let Some((operation_index, operation)) = pending.pop() else {
                break;
            };
            match start_detached_operation_slot(
                &plugin_id,
                deps.clone(),
                tx.clone(),
                operation_index,
                operation,
                timeout,
            )
            .await
            {
                Ok(batch_id) => active.push((operation_index, batch_id)),
                Err(e) => {
                    tracing::error!(%plugin_id, %session_id, operation_index, error = %e, "Failed to refill detached operation slot");
                    let callback = DetachedOperationCallbackInput {
                        session_id: session_id.clone(),
                        state: state.clone(),
                        event: DetachedOperationCallbackEvent::Timeout {
                            message: e.to_string(),
                        },
                        completed,
                        total,
                    };
                    let output = match call_detached_operation_callback(
                        plugin_manager.as_ref(),
                        &plugin_id,
                        &input.callback_fn,
                        callback,
                    )
                    .await
                    {
                        Ok(output) => output,
                        Err(e) => {
                            tracing::error!(%plugin_id, %session_id, error = %e, "Detached operation callback failed after refill start failure");
                            cancel_active_operation_slots(&plugin_id, &deps, &active);
                            return;
                        }
                    };
                    state = output.state;
                    match output.action {
                        DetachedOperationCallbackAction::Continue => {}
                        DetachedOperationCallbackAction::Finish
                        | DetachedOperationCallbackAction::Cancel => {
                            cancel_active_operation_slots(&plugin_id, &deps, &active);
                            return;
                        }
                    }
                    break;
                }
            }
        }

        if active.is_empty() && (!refill_enabled || pending.is_empty()) {
            break;
        }
    }

    send_detached_operation_exhausted(
        plugin_manager.as_ref(),
        &plugin_id,
        &input.callback_fn,
        session_id,
        state,
        completed,
        total,
    )
    .await;
}

async fn send_detached_operation_exhausted(
    plugin_manager: &dyn plugin_core::traits::PluginManager,
    plugin_id: &str,
    callback_fn: &str,
    session_id: String,
    state: serde_json::Value,
    completed: usize,
    total: usize,
) {
    let callback = DetachedOperationCallbackInput {
        session_id,
        state,
        event: DetachedOperationCallbackEvent::Exhausted,
        completed,
        total,
    };
    let _ =
        call_detached_operation_callback(plugin_manager, plugin_id, callback_fn, callback).await;
}

async fn start_detached_operation_slot(
    plugin_id: &str,
    deps: OperationHostDeps,
    tx: mpsc::Sender<DetachedOperationResult>,
    operation_index: usize,
    operation: OperationTask,
    timeout: Duration,
) -> anyhow::Result<String> {
    let batch_id =
        start_operation_batch(plugin_id.to_string(), deps.clone(), vec![operation]).await?;
    let wait_batch_id = batch_id.clone();
    let wait_plugin_id = plugin_id.to_string();
    tokio::spawn(async move {
        let batches = deps.operation_batches.clone();
        let metrics = deps.metrics.clone();
        let batch_id_for_wait = wait_batch_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            next_operation_result(
                &wait_plugin_id,
                &batches,
                metrics.as_ref(),
                &batch_id_for_wait,
                timeout,
            )
        })
        .await
        .map_err(|e| anyhow!("Operation waiter task failed: {e}"))
        .and_then(|result| result);
        let _ = tx
            .send(DetachedOperationResult {
                operation_index,
                batch_id: wait_batch_id,
                result,
            })
            .await;
    });
    Ok(batch_id)
}

async fn call_detached_operation_callback(
    plugin_manager: &dyn plugin_core::traits::PluginManager,
    plugin_id: &str,
    callback_fn: &str,
    input: DetachedOperationCallbackInput,
) -> anyhow::Result<DetachedOperationCallbackOutput> {
    let input_bytes = serde_json::to_vec(&input)
        .with_context(|| "Failed to serialize detached operation callback input")?;
    let output_bytes = call_raw_with_pool_retry(
        plugin_manager,
        plugin_id,
        callback_fn,
        input_bytes,
        PoolRetryPolicy::default(),
    )
    .await
    .with_context(|| "Detached operation callback failed")?;
    serde_json::from_slice::<DetachedOperationCallbackOutput>(&output_bytes)
        .with_context(|| "Failed to deserialize detached operation callback output")
}

fn operation_result_from_task_result(task_result: TaskResult) -> anyhow::Result<OperationResult> {
    serde_json::from_value::<OperationResult>(task_result.output).map_err(|_| {
        anyhow!(
            "{}",
            task_result
                .error
                .unwrap_or_else(|| "Worker did not return an OperationResult".to_string())
        )
    })
}

fn cancel_active_operation_slots(
    plugin_id: &str,
    deps: &OperationHostDeps,
    active: &[(usize, String)],
) {
    for (_, batch_id) in active {
        cancel_operation_batch(
            plugin_id,
            &deps.operation_batches,
            &deps.operation_waiters,
            deps.metrics.as_ref(),
            batch_id,
        );
    }
}

fn cancel_operation_indices(
    plugin_id: &str,
    deps: &OperationHostDeps,
    active: &mut Vec<(usize, String)>,
    indices: &HashSet<usize>,
) {
    let mut cancelled = Vec::new();
    active.retain(|(idx, batch_id)| {
        if indices.contains(idx) {
            cancel_operation_batch(
                plugin_id,
                &deps.operation_batches,
                &deps.operation_waiters,
                deps.metrics.as_ref(),
                batch_id,
            );
            cancelled.push(*idx);
            false
        } else {
            true
        }
    });
    if !cancelled.is_empty() {
        tracing::debug!(?cancelled, "Detached operation slots cancelled by callback");
    }
}

fn remove_active_operation_slot_by_batch_id(
    active: &mut Vec<(usize, String)>,
    batch_id: &str,
) -> bool {
    let Some(index) = active
        .iter()
        .position(|(_, active_batch_id)| active_batch_id == batch_id)
    else {
        return false;
    };
    active.swap_remove(index);
    true
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
    fn detached_operation_ignores_results_for_cancelled_slots() {
        let mut active = vec![(0, "batch-1".to_string()), (1, "batch-2".to_string())];

        assert!(remove_active_operation_slot_by_batch_id(
            &mut active,
            "batch-1"
        ));
        assert!(!remove_active_operation_slot_by_batch_id(
            &mut active,
            "batch-1"
        ));
        assert_eq!(active, vec![(1, "batch-2".to_string())]);
    }

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
            plugin_manager: None,
            operation_publisher: None,
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

    #[tokio::test]
    async fn failed_operation_batch_cleanup_removes_batch_waiters_and_eval_refs() {
        let temp = tempfile::tempdir().unwrap();
        let blob_store = Arc::new(
            FilesystemBlobStore::new(temp.path().to_path_buf(), 2_000_000)
                .await
                .unwrap(),
        );
        let operation_batches = Arc::new(dashmap::DashMap::new());
        let operation_waiters = Arc::new(dashmap::DashMap::new());
        let evaluate_ops_registry =
            crate::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry::default();
        let cleanup_keys = vec!["op-1".to_string(), "op-2".to_string()];
        let (_batch_tx, batch_rx) = crossbeam::channel::unbounded();
        let (waiter_tx, _waiter_rx) = tokio::sync::oneshot::channel();

        operation_batches.insert(
            "batch-1".to_string(),
            BatchState {
                result_rx: batch_rx,
                pending_count: Arc::new(AtomicUsize::new(2)),
                created_at: Instant::now(),
                cleanup_keys: Arc::new(cleanup_keys.clone()),
                poisoned: AtomicBool::new(false),
            },
        );
        operation_waiters.insert("op-1".to_string(), OperationWaiter::new(waiter_tx));
        evaluate_ops_registry.record_ops("eval-1", 10, "batch-1", cleanup_keys.clone());

        let deps = OperationHostDeps {
            plugin_manager: None,
            operation_publisher: None,
            operation_batches: operation_batches.clone(),
            operation_waiters: operation_waiters.clone(),
            operation_queue_name: "ops".to_string(),
            operation_result_queue_name: "ops-results".to_string(),
            blob_store,
            metrics: None,
            evaluate_ops_registry: evaluate_ops_registry.clone(),
            operation_batch_publish_concurrency: 32,
        };

        cleanup_failed_operation_batch(
            &deps,
            "batch-1",
            &cleanup_keys,
            &[("eval-1".to_string(), 10)],
            "plugin",
        );

        assert!(operation_batches.get("batch-1").is_none());
        assert!(operation_waiters.get("op-1").is_none());
        assert!(
            evaluate_ops_registry
                .operation_task_ids_for_test_cases("eval-1", &[10])
                .is_empty()
        );
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
