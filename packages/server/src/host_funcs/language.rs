use std::collections::HashMap;
use std::sync::Arc;

use common::language::{LanguageDefinition, resolve_language};
use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginManager;
use serde::Deserialize;

use crate::registry::LanguageResolverRegistry;

// TODO: Remove legacy get_language_config

#[derive(Deserialize)]
struct GetLanguageConfigInput {
    language: String,
    submitted_filename: String,
    #[serde(default)]
    extra_sources: Vec<String>,
}

type LegacyUserData = (String, HashMap<String, LanguageDefinition>);

pub fn create_language_function(
    plugin_id: String,
    languages: HashMap<String, LanguageDefinition>,
) -> Function {
    let user_data: UserData<LegacyUserData> = UserData::new((plugin_id, languages));

    Function::new(
        "get_language_config",
        [ValType::I64],
        [ValType::I64],
        user_data,
        get_language_config_fn,
    )
}

fn get_language_config_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    outputs: &mut [Val],
    user_data: UserData<LegacyUserData>,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: GetLanguageConfigInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard
        .lock()
        .map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, languages) = &*user_data;

    let resolved = resolve_language(
        &input.language,
        &input.submitted_filename,
        languages,
        &input.extra_sources,
    )
    .map_err(|e| extism::Error::msg(format!("Language config error: {}", e)))?;

    tracing::debug!(
        plugin_id = %plugin_id,
        language = %input.language,
        "Language config retrieved"
    );

    let output_bytes = serde_json::to_vec(&resolved)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize output: {}", e)))?;
    let offset = plugin.memory_new(&output_bytes)?;
    outputs[0] = Val::I64(offset.offset() as i64);

    Ok(())
}

use broccoli_server_sdk::types::{
    CompileSpec, OutputSpec, ResolveLanguageInput, ResolveLanguageOutput, RunSpec,
};

struct ResolveLanguageContext {
    caller_plugin_id: String,
    languages: HashMap<String, LanguageDefinition>,
    resolver_registry: LanguageResolverRegistry,
    plugin_manager: Arc<dyn PluginManager>,
}

pub fn create_resolve_language_function(
    plugin_id: String,
    languages: HashMap<String, LanguageDefinition>,
    resolver_registry: LanguageResolverRegistry,
    plugin_manager: Arc<dyn PluginManager>,
) -> Function {
    let user_data: UserData<ResolveLanguageContext> = UserData::new(ResolveLanguageContext {
        caller_plugin_id: plugin_id,
        languages,
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

    let result = if let Some(handler) = resolver {
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

        let input_bytes = serde_json::to_vec(&input).map_err(|e| {
            extism::Error::msg(format!("Failed to serialize resolver input: {}", e))
        })?;

        let output_bytes = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                ctx.plugin_manager
                    .call_raw(&handler.plugin_id, &handler.function_name, input_bytes)
                    .await
            })
        })
        .map_err(|e| extism::Error::msg(format!("Language resolver plugin error: {}", e)))?;

        serde_json::from_slice::<ResolveLanguageOutput>(&output_bytes).map_err(|e| {
            extism::Error::msg(format!("Failed to deserialize resolver output: {}", e))
        })?
    } else {
        // TODO: Remove this fallback
        let all_files: Vec<String> = input
            .submitted_files
            .iter()
            .chain(input.additional_files.iter())
            .cloned()
            .collect();

        let probe = resolve_language(&input.language_id, "", &ctx.languages, &[])
            .map_err(|e| extism::Error::msg(format!("Language config error: {}", e)))?;

        let primary = all_files
            .iter()
            .find(|f| **f == probe.source_filename)
            .or(all_files.first())
            .map(|s| s.as_str())
            .unwrap_or_default();

        let extras: Vec<String> = all_files
            .iter()
            .filter(|f| f.as_str() != primary)
            .cloned()
            .collect();

        let resolved = resolve_language(&input.language_id, primary, &ctx.languages, &extras)
            .map_err(|e| extism::Error::msg(format!("Language config error: {}", e)))?;

        tracing::debug!(
            caller = %ctx.caller_plugin_id,
            language = %input.language_id,
            "Language resolved via template fallback"
        );

        ResolveLanguageOutput {
            compile: resolved.compile_cmd.as_ref().map(|cmd| CompileSpec {
                command: cmd.clone(),
                cache_inputs: all_files.clone(),
                outputs: vec![OutputSpec::File(resolved.binary_name.clone())],
            }),
            run: RunSpec {
                command: resolved.run_cmd.clone(),
                extra_files: if resolved.compile_cmd.is_none() {
                    all_files
                } else {
                    vec![]
                },
            },
        }
    };

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
