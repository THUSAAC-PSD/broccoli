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

#[derive(Deserialize)]
struct CancelEvaluateTestCasesInput {
    evaluate_batch_id: String,
    test_case_ids: Vec<i32>,
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
            user_data.clone(),
            cancel_evaluate_batch_fn,
        ),
        Function::new(
            "cancel_evaluate_test_cases",
            [ValType::I64],
            [ValType::I64],
            user_data,
            cancel_evaluate_test_cases_fn,
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
        &deps.evaluate_ops_registry,
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

    if deps.cancel_primitive_enabled
        && let Some(client) = deps.redis_client.as_ref()
    {
        let op_task_ids = deps
            .evaluate_ops_registry
            .operation_task_ids_for_batch(&input.batch_id);
        let client = client.clone();
        let batch_id = input.batch_id.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                if let Err(e) = crate::host_funcs::cancel::set_cancel_batch_key(&client, &batch_id)
                    .await
                {
                    tracing::warn!(error = %e, batch_id = %batch_id, "Failed to set Redis batch cancel key");
                }
                if !op_task_ids.is_empty()
                    && let Err(e) =
                        crate::host_funcs::cancel::set_cancel_op_keys(&client, &op_task_ids).await
                {
                    tracing::warn!(error = %e, batch_id = %batch_id, "Failed to set Redis op cancel keys");
                }
            })
        });
    }

    deps.evaluate_ops_registry
        .mark_batch_cancelled(&input.batch_id);

    evaluate_batch::cancel_evaluate_batch(
        &plugin_id,
        &deps.evaluate_batches,
        deps.metrics.as_ref(),
        &deps.evaluate_ops_registry,
        &input.batch_id,
    );

    Ok(())
}

fn cancel_evaluate_test_cases_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<EvaluateUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: CancelEvaluateTestCasesInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let deps = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        guard.deps.clone()
    };

    let count = deps
        .evaluate_ops_registry
        .mark_cancelled(&input.evaluate_batch_id, &input.test_case_ids);

    if deps.cancel_primitive_enabled
        && let Some(client) = deps.redis_client.as_ref()
    {
        let op_task_ids = deps
            .evaluate_ops_registry
            .operation_task_ids_for_test_cases(&input.evaluate_batch_id, &input.test_case_ids);
        if !op_task_ids.is_empty() {
            let client = client.clone();
            let batch_id = input.evaluate_batch_id.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    if let Err(e) =
                        crate::host_funcs::cancel::set_cancel_op_keys(&client, &op_task_ids).await
                    {
                        tracing::warn!(error = %e, batch_id = %batch_id, "Failed to set Redis op cancel keys");
                    }
                })
            });
        }
    }

    let response = serde_json::json!({ "count": count });
    let output_bytes = serde_json::to_vec(&response)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize count: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);
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
