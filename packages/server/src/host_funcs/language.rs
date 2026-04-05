use std::sync::Arc;

use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginManager;

use crate::registry::LanguageResolverRegistry;

use broccoli_server_sdk::types::{ResolveLanguageInput, ResolveLanguageOutput};

struct ResolveLanguageContext {
    caller_plugin_id: String,
    resolver_registry: LanguageResolverRegistry,
    plugin_manager: Arc<dyn PluginManager>,
}

pub fn create_resolve_language_function(
    plugin_id: String,
    resolver_registry: LanguageResolverRegistry,
    plugin_manager: Arc<dyn PluginManager>,
) -> Function {
    let user_data: UserData<ResolveLanguageContext> = UserData::new(ResolveLanguageContext {
        caller_plugin_id: plugin_id,
        resolver_registry,
        plugin_manager,
    });

    Function::new(
        "resolve_language",
        [ValType::I64],
        [ValType::I64],
        user_data,
        resolve_language_fn,
    )
}

fn resolve_language_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<ResolveLanguageContext>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: ResolveLanguageInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let guard = user_data.get()?;
    let ctx = guard
        .lock()
        .map_err(|_| extism::Error::msg("Lock poisoned"))?;

    let resolver = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let registry = ctx.resolver_registry.read().await;
            registry.get(&input.language_id).cloned()
        })
    });

    let handler = resolver.ok_or_else(|| {
        extism::Error::msg(format!(
            "No language resolver registered for '{}'",
            input.language_id
        ))
    })?;

    if handler.plugin_id == ctx.caller_plugin_id {
        return Err(extism::Error::msg(
            "Language resolver cannot be called by the plugin that registered it",
        ));
    }

    tracing::debug!(
        caller = %ctx.caller_plugin_id,
        resolver_plugin = %handler.plugin_id,
        language = %input.language_id,
        "Dispatching to plugin language resolver"
    );

    let input_bytes = serde_json::to_vec(&input)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize resolver input: {}", e)))?;

    let output_bytes = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            ctx.plugin_manager
                .call_raw(&handler.plugin_id, &handler.function_name, input_bytes)
                .await
        })
    })
    .map_err(|e| extism::Error::msg(format!("Language resolver plugin error: {}", e)))?;

    let result: ResolveLanguageOutput = serde_json::from_slice(&output_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize resolver output: {}", e)))?;

    if let Some(compile) = &result.compile {
        for output in &compile.outputs {
            output
                .validate()
                .map_err(|e| extism::Error::msg(format!("Invalid compile output: {}", e)))?;
        }
    }

    let output_bytes = serde_json::to_vec(&result)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}
