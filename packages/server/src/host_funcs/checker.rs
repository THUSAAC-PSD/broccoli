use crate::registry::CheckerStageRegistry;
use broccoli_server_sdk::types::{
    CheckerStage, CheckerVerdict, InterpretCheckerInput, ResolveCheckerInput,
};
use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginInvoker;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Checker fusion: resolve + interpret host functions. These dispatch to a
// plugin's resolve/interpret fns via CheckerStageRegistry, mirroring
// `language::resolve_language_fn`. The host stays generic - no checker logic.
// Unknown format / plugin error surface as a host-fn error (the SDK wrapper
// returns Err; the evaluator maps that to SystemError).
// ---------------------------------------------------------------------------

struct CheckerStageContext {
    caller_plugin_id: String,
    checker_stage_registry: CheckerStageRegistry,
    plugin_manager: Arc<dyn PluginInvoker>,
}

pub fn create_resolve_checker_function(
    plugin_id: String,
    checker_stage_registry: CheckerStageRegistry,
    plugin_manager: Arc<dyn PluginInvoker>,
) -> Function {
    let user_data: UserData<CheckerStageContext> = UserData::new(CheckerStageContext {
        caller_plugin_id: plugin_id,
        checker_stage_registry,
        plugin_manager,
    });
    Function::new(
        "resolve_checker",
        [ValType::I64],
        [ValType::I64],
        user_data,
        resolve_checker_fn,
    )
}

pub fn create_interpret_checker_function(
    plugin_id: String,
    checker_stage_registry: CheckerStageRegistry,
    plugin_manager: Arc<dyn PluginInvoker>,
) -> Function {
    let user_data: UserData<CheckerStageContext> = UserData::new(CheckerStageContext {
        caller_plugin_id: plugin_id,
        checker_stage_registry,
        plugin_manager,
    });
    Function::new(
        "interpret_checker_result",
        [ValType::I64],
        [ValType::I64],
        user_data,
        interpret_checker_fn,
    )
}

/// Look up the format's handler, call the selected plugin fn, and return its raw
/// output bytes. The caller validates + writes them. Unknown format / plugin
/// error -> `Err` (surfaced to the SDK as an error, mapped to SystemError).
fn checker_stage_call(
    user_data: &UserData<CheckerStageContext>,
    host_fn_name: &'static str,
    format: &str,
    select_handler_fn: impl FnOnce(&crate::registry::CheckerStageHandlers) -> String,
    request_bytes: Vec<u8>,
) -> Result<Vec<u8>, extism::Error> {
    let (caller, registry, pm) = {
        let guard = user_data.get()?;
        let ctx = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (
            ctx.caller_plugin_id.clone(),
            ctx.checker_stage_registry.clone(),
            ctx.plugin_manager.clone(),
        )
    };
    let span = super::host_fn_span(host_fn_name, &caller);
    let _enter = span.enter();

    let handler = tokio::runtime::Handle::current()
        .block_on(async { registry.read().await.get(format).cloned() })
        .ok_or_else(|| {
            extism::Error::msg(format!("No checker resolver registered for '{format}'"))
        })?;

    let function_name = select_handler_fn(&handler);
    tracing::debug!(
        caller = %caller,
        handler_plugin = %handler.plugin_id,
        handler_fn = %function_name,
        checker_format = %format,
        "Dispatching checker stage call"
    );

    tokio::runtime::Handle::current()
        .block_on(async {
            pm.call_raw(&handler.plugin_id, &function_name, request_bytes)
                .await
        })
        .map_err(|e| extism::Error::msg(format!("Checker stage plugin error: {e}")))
}

fn resolve_checker_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<CheckerStageContext>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: ResolveCheckerInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {e}")))?;
    let request_bytes = serde_json::to_vec(&input)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize resolve input: {e}")))?;

    let output_bytes = checker_stage_call(
        &user_data,
        "resolve_checker",
        &input.format,
        |h| h.resolve_fn.clone(),
        request_bytes,
    )?;

    // Validate the plugin returned a well-formed CheckerStage before handing it
    // back, so a malformed stage fails here rather than deep in op assembly.
    let _stage: CheckerStage = serde_json::from_slice(&output_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize checker stage: {e}")))?;

    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);
    Ok(())
}

fn interpret_checker_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<CheckerStageContext>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: InterpretCheckerInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {e}")))?;
    let request_bytes = serde_json::to_vec(&input)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize interpret input: {e}")))?;

    let output_bytes = checker_stage_call(
        &user_data,
        "interpret_checker_result",
        &input.format,
        |h| h.interpret_fn.clone(),
        request_bytes,
    )?;

    let _verdict: CheckerVerdict = serde_json::from_slice(&output_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize checker verdict: {e}")))?;

    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);
    Ok(())
}
