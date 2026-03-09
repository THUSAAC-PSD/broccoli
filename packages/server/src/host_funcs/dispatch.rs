use crate::registry::{BatchState, OperationBatches, OperationWaiters};
use common::worker::{Task, TaskResult};
use extism::{Function, UserData, Val, ValType};
use mq::MqQueue;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;
use worker::models::operation::models::OperationTask;

/// Input for get_next_operation_result
#[derive(Deserialize)]
struct GetNextResultInput {
    batch_id: String,
    timeout_ms: u64,
}

/// Input for cancel_operation_batch
#[derive(Deserialize)]
struct CancelBatchInput {
    batch_id: String,
}

/// Response wrapper for operation results (used by both Ok and Timeout arms).
#[derive(Serialize)]
struct ResultResponse {
    result: Option<TaskResult>,
}

/// Named context for dispatch host functions (replaces opaque 6-tuple).
struct DispatchContext {
    plugin_id: String,
    mq: Option<Arc<MqQueue>>,
    batches: OperationBatches,
    waiters: OperationWaiters,
    operation_queue_name: String,
    result_queue_name: String,
}

type DispatchUserData = DispatchContext;

pub fn create_dispatch_functions(
    plugin_id: String,
    mq: Option<Arc<MqQueue>>,
    operation_batches: OperationBatches,
    operation_waiters: OperationWaiters,
    operation_queue_name: String,
    operation_result_queue_name: String,
) -> Vec<Function> {
    let user_data: UserData<DispatchUserData> = UserData::new(DispatchContext {
        plugin_id,
        mq,
        batches: operation_batches,
        waiters: operation_waiters,
        operation_queue_name,
        result_queue_name: operation_result_queue_name,
    });

    vec![
        Function::new(
            "start_operation_batch",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            start_operation_batch_fn,
        ),
        Function::new(
            "get_next_operation_result",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            get_next_operation_result_fn,
        ),
        Function::new(
            "cancel_operation_batch",
            [ValType::I64],
            [],
            user_data,
            cancel_operation_batch_fn,
        ),
    ]
}

fn start_operation_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<DispatchUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let operations: Vec<OperationTask> = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize operations: {}", e)))?;

    let (plugin_id, mq, batches, waiters, queue_name, result_queue_name) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (
            guard.plugin_id.clone(),
            guard.mq.clone(),
            guard.batches.clone(),
            guard.waiters.clone(),
            guard.operation_queue_name.clone(),
            guard.result_queue_name.clone(),
        )
    };

    let mq = mq
        .as_ref()
        .ok_or_else(|| extism::Error::msg("MQ not available"))?;

    let batch_id = Uuid::new_v4().to_string();

    let (batch_tx, batch_rx) = crossbeam::channel::unbounded();
    let pending_count = Arc::new(AtomicUsize::new(operations.len()));

    batches.insert(
        batch_id.clone(),
        BatchState {
            result_rx: batch_rx,
            pending_count: pending_count.clone(),
            created_at: Instant::now(),
        },
    );

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        operation_count = operations.len(),
        "Starting operation batch"
    );

    for op in operations {
        let correlation_id = Uuid::new_v4().to_string();
        let (op_tx, op_rx) = tokio::sync::oneshot::channel();

        waiters.insert(correlation_id.clone(), op_tx);

        let batch_tx = batch_tx.clone();
        let pending_count = pending_count.clone();
        let correlation_id_clone = correlation_id.clone();

        tokio::spawn(async move {
            match op_rx.await {
                Ok(result) => {
                    let _ = batch_tx.send(result);
                    pending_count.fetch_sub(1, Ordering::SeqCst);
                }
                Err(_) => {
                    // Oneshot sender dropped (batch cancelled)
                    tracing::debug!(correlation_id = %correlation_id_clone, "Operation waiter dropped");
                    pending_count.fetch_sub(1, Ordering::SeqCst);
                }
            }
        });

        let task = Task {
            id: correlation_id.clone(),
            task_type: "operation".to_string(),
            executor_name: "operation".to_string(),
            payload: serde_json::to_value(&op)
                .map_err(|e| extism::Error::msg(format!("Failed to serialize operation: {}", e)))?,
            result_queue: result_queue_name.clone(),
        };

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { mq.publish(&queue_name, None, &task, None).await })
        })
        .map_err(|e| extism::Error::msg(format!("MQ publish error: {}", e)))?;

        tracing::debug!(
            batch_id = %batch_id,
            correlation_id = %correlation_id,
            "Operation dispatched"
        );
    }

    #[derive(Serialize)]
    struct BatchIdResponse {
        batch_id: String,
    }

    let response = BatchIdResponse { batch_id };
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize batch_id: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}

fn get_next_operation_result_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<DispatchUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: GetNextResultInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, batches) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.batches.clone())
    };

    let (result_rx, pending_count) = {
        let batch = batches
            .get(&input.batch_id)
            .ok_or_else(|| extism::Error::msg(format!("Batch not found: {}", input.batch_id)))?;
        (batch.result_rx.clone(), batch.pending_count.clone())
    };

    let result = result_rx.recv_timeout(Duration::from_millis(input.timeout_ms));

    match result {
        Ok(task_result) => {
            tracing::debug!(
                plugin_id = %plugin_id,
                batch_id = %input.batch_id,
                task_id = %task_result.task_id,
                "Operation result received"
            );

            if pending_count.load(Ordering::SeqCst) == 0 && result_rx.is_empty() {
                batches.remove(&input.batch_id);
            }

            let response = ResultResponse {
                result: Some(task_result),
            };
            let output_bytes = serde_json::to_vec(&response)
                .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
            let offset = plugin.memory_new(&output_bytes)?;
            outputs[0] = Val::I64(offset.offset() as i64);

            Ok(())
        }
        Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
            let response = ResultResponse { result: None };
            let output_bytes = serde_json::to_vec(&response)
                .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
            let offset = plugin.memory_new(&output_bytes)?;
            outputs[0] = Val::I64(offset.offset() as i64);

            Ok(())
        }
        Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
            Err(extism::Error::msg("Batch channel disconnected"))
        }
    }
}

fn cancel_operation_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<DispatchUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: CancelBatchInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, batches) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.batches.clone())
    };

    batches.remove(&input.batch_id);

    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %input.batch_id,
        "Operation batch cancelled"
    );

    Ok(())
}
