use broccoli_server_sdk::types::{StartEvaluateBatchInput, TestCaseVerdict};
use extism::{Function, UserData, Val, ValType};
use serde::Deserialize;
use std::time::Duration;

use crate::host_funcs::context::EvaluateHostDeps;
use crate::services::evaluate_batch;

#[derive(Deserialize)]
struct GetNextEvaluateResultInput {
    batch_id: String,
    timeout_ms: u64,
}

#[derive(Deserialize)]
struct CancelEvaluateBatchInput {
    batch_id: String,
}

struct EvaluateContext {
    plugin_id: String,
    deps: EvaluateHostDeps,
}

type EvaluateUserData = EvaluateContext;

pub fn create_evaluate_functions(plugin_id: String, deps: EvaluateHostDeps) -> Vec<Function> {
    let user_data: UserData<EvaluateUserData> = UserData::new(EvaluateContext { plugin_id, deps });

    vec![
        Function::new(
            "start_evaluate_batch",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            start_evaluate_batch_fn,
        ),
        Function::new(
            "get_next_evaluate_result",
            [ValType::I64],
            [ValType::I64],
            user_data.clone(),
            get_next_evaluate_result_fn,
        ),
        Function::new(
            "cancel_evaluate_batch",
            [ValType::I64],
            [],
            user_data,
            cancel_evaluate_batch_fn,
        ),
    ]
}

fn start_evaluate_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: StartEvaluateBatchInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (caller_plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };

    let batch_id = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(evaluate_batch::start_evaluate_batch(
            caller_plugin_id,
            deps,
            input,
        ))
    })
    .map_err(|e| extism::Error::msg(e.to_string()))?;

    let response = serde_json::json!({ "batch_id": batch_id });
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize batch_id: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}

fn get_next_evaluate_result_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: GetNextEvaluateResultInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };

    let result = evaluate_batch::next_evaluate_result(
        &plugin_id,
        &deps.evaluate_batches,
        deps.metrics.as_ref(),
        &input.batch_id,
        Duration::from_millis(input.timeout_ms),
    )
    .map_err(|e| extism::Error::msg(e.to_string()))?;
    write_result(plugin, outputs, result)
}

fn cancel_evaluate_batch_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: CancelEvaluateBatchInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (plugin_id, deps) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (guard.plugin_id.clone(), guard.deps.clone())
    };

    evaluate_batch::cancel_evaluate_batch(
        &plugin_id,
        &deps.evaluate_batches,
        deps.metrics.as_ref(),
        &input.batch_id,
    );

    Ok(())
}

fn write_result(
    plugin: &mut extism::CurrentPlugin,
    outputs: &mut [Val],
    result: Option<TestCaseVerdict>,
) -> Result<(), extism::Error> {
    let response = serde_json::json!({ "result": result });
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize result: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);
    Ok(())
}
