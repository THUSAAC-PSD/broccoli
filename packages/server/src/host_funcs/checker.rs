use crate::registry::CheckerFormatRegistry;
use common::submission_dispatch::{CheckerVerdict, RunCheckerInput};
use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginManager;
use std::sync::Arc;

/// (plugin_id, plugin_manager, checker_format_registry)
type CheckerUserData = (String, Arc<dyn PluginManager>, CheckerFormatRegistry);

pub fn create_checker_function(
    plugin_id: String,
    plugin_manager: Arc<dyn PluginManager>,
    checker_format_registry: CheckerFormatRegistry,
) -> Function {
    let user_data: UserData<CheckerUserData> =
        UserData::new((plugin_id, plugin_manager, checker_format_registry));

    Function::new(
        "run_checker",
        [ValType::I64],
        [ValType::I64],
        user_data,
        run_checker_fn,
    )
}

fn run_checker_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<CheckerUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: RunCheckerInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (caller_plugin_id, plugin_manager, checker_format_registry) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        let (id, pm, reg) = &*guard;
        (id.clone(), pm.clone(), reg.clone())
    };

    let handler = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let registry = checker_format_registry.read().await;
            registry.get(&input.format).cloned()
        })
    });

    let handler = handler.ok_or_else(|| {
        extism::Error::msg(format!(
            "No checker format handler registered for: {}",
            input.format
        ))
    })?;

    tracing::debug!(
        caller = %caller_plugin_id,
        checker_format = %input.format,
        handler_plugin = %handler.plugin_id,
        handler_fn = %handler.function_name,
        "Calling checker format handler"
    );

    let input_bytes = serde_json::to_vec(&input.input)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize checker input: {}", e)))?;

    let result_bytes = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_manager
                .call_raw(&handler.plugin_id, &handler.function_name, input_bytes)
                .await
        })
    })?;

    let verdict: CheckerVerdict = serde_json::from_slice(&result_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize checker verdict: {}", e)))?;

    tracing::debug!(
        caller = %caller_plugin_id,
        checker_format = %input.format,
        verdict = ?verdict.verdict,
        score = verdict.score,
        "Checker format handler returned verdict"
    );

    let output_bytes = serde_json::to_vec(&verdict)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}
