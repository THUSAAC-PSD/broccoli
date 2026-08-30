use crate::host_funcs::context::OperationHostDeps;
use crate::services::operation_batch;
use broccoli_server_sdk::types::{
    DetachedOperationSession, OperationTask, StartDetachedWindowedOperationInput,
};
use common::worker::TaskResult;
use extism::{Function, UserData, Val, ValType};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
struct GetNextResultInput {
    batch_id: String,
    timeout_ms: u64,
}

#[derive(Deserialize)]
struct CancelBatchInput {
    batch_id: String,
}

#[derive(Serialize)]
struct ResultResponse {
    result: Option<TaskResult>,
}

struct DispatchContext {
    plugin_id: String,
    deps: OperationHostDeps,
}

type DispatchUserData = DispatchContext;

pub fn create_dispatch_functions(plugin_id: String, deps: OperationHostDeps) -> Vec<Function> {
    let user_data: UserData<DispatchUserData> = UserData::new(DispatchContext { plugin_id, deps });

    vec![
        Function::new(
            "start_operation_batch",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            start_operation_batch_fn,
        ),
        Function::new(
            "start_detached_windowed_operation",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            start_detached_windowed_operation_fn,
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

fn start_detached_windowed_operation_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<DispatchUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: StartDetachedWindowedOperationInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };
    let span = super::host_fn_span("start_detached_windowed_operation", &plugin_id);
    let _enter = span.enter();

    let manager = deps
        .plugin_manager
        .clone()
        .ok_or_else(|| extism::Error::msg("Plugin manager not available for detached operation"))?;
    let session =
        operation_batch::start_detached_windowed_operation(plugin_id, manager, deps, input)
            .map_err(|e| extism::Error::msg(e.to_string()))?;

    write_session(plugin, outputs, session)
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

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };
    let span = super::host_fn_span("start_operation_batch", &plugin_id);
    let _enter = span.enter();

    let batch_id = tokio::runtime::Handle::current()
        .block_on(operation_batch::start_operation_batch(
            plugin_id, deps, operations,
        ))
        .map_err(|e| extism::Error::msg(e.to_string()))?;

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

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };
    let span = super::host_fn_span("get_next_operation_result", &plugin_id);
    let _enter = span.enter();

    let result = operation_batch::next_operation_result(
        &plugin_id,
        &deps.operation_batches,
        deps.metrics.as_ref(),
        &input.batch_id,
        Duration::from_millis(input.timeout_ms),
    )
    .map_err(|e| extism::Error::msg(e.to_string()))?;

    let response = ResultResponse { result };
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
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

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };
    let span = super::host_fn_span("cancel_operation_batch", &plugin_id);
    let _enter = span.enter();

    operation_batch::cancel_operation_batch(
        &plugin_id,
        &deps.operation_batches,
        &deps.operation_waiters,
        deps.metrics.as_ref(),
        &input.batch_id,
    );

    Ok(())
}

fn write_session(
    plugin: &mut extism::CurrentPlugin,
    outputs: &mut [Val],
    session: DetachedOperationSession,
) -> Result<(), extism::Error> {
    let output_bytes = serde_json::to_vec(&session)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize session: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);
    Ok(())
}
