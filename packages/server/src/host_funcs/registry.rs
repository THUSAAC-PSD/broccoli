use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluatorRegistry, PluginHandler,
};
use extism::{Function, UserData, Val, ValType};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Input for register_contest_type
#[derive(Deserialize)]
struct RegisterContestTypeInput {
    #[serde(rename = "type")]
    contest_type: String,
    handler: String,
}

/// Input for register_evaluator
#[derive(Deserialize)]
struct RegisterEvaluatorInput {
    #[serde(rename = "type")]
    problem_type: String,
    handler: String,
}

/// Input for register_checker_format
#[derive(Deserialize)]
struct RegisterCheckerFormatInput {
    #[serde(rename = "format")]
    checker_format: String,
    handler: String,
}

/// Named context for registry host functions.
struct RegistryContext {
    plugin_id: String,
    contest_type_registry: ContestTypeRegistry,
    evaluator_registry: EvaluatorRegistry,
    checker_format_registry: CheckerFormatRegistry,
}

type RegistryUserData = RegistryContext;

pub fn create_registry_functions(
    plugin_id: String,
    contest_type_registry: ContestTypeRegistry,
    evaluator_registry: EvaluatorRegistry,
    checker_format_registry: CheckerFormatRegistry,
) -> Vec<Function> {
    let user_data: UserData<RegistryUserData> = UserData::new(RegistryContext {
        plugin_id: plugin_id.clone(),
        contest_type_registry: contest_type_registry.clone(),
        evaluator_registry: evaluator_registry.clone(),
        checker_format_registry: checker_format_registry.clone(),
    });

    vec![
        Function::new(
            "register_contest_type",
            [ValType::I64],
            [],
            user_data.clone(),
            register_contest_type_fn,
        ),
        Function::new(
            "register_evaluator",
            [ValType::I64],
            [],
            user_data.clone(),
            register_evaluator_fn,
        ),
        Function::new(
            "register_checker_format",
            [ValType::I64],
            [],
            user_data,
            register_checker_format_fn,
        ),
    ]
}

fn register_handler<I: serde::de::DeserializeOwned>(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    plugin_id: &str,
    registry: &Arc<RwLock<HashMap<String, PluginHandler>>>,
    extract: impl FnOnce(&I) -> (&str, &str), // (registry_key, handler_name)
    label: &str,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: I = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (key, handler_name) = extract(&input);
    let key = key.to_string();
    let handler_name = handler_name.to_string();

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let mut registry = registry.write().await;
            registry.insert(
                key.clone(),
                PluginHandler {
                    plugin_id: plugin_id.to_string(),
                    function_name: handler_name.clone(),
                },
            );
            tracing::info!(
                plugin_id = %plugin_id,
                key = %key,
                handler = %handler_name,
                "{label} registered"
            );
        })
    });

    Ok(())
}

fn register_contest_type_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<RegistryUserData>,
) -> Result<(), extism::Error> {
    let (plugin_id, registry) = {
        let guard = user_data.get()?;
        let data = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (data.plugin_id.clone(), data.contest_type_registry.clone())
    };
    register_handler::<RegisterContestTypeInput>(
        plugin,
        inputs,
        _outputs,
        &plugin_id,
        &registry,
        |input| (&input.contest_type, &input.handler),
        "Contest type",
    )
}

fn register_evaluator_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<RegistryUserData>,
) -> Result<(), extism::Error> {
    let (plugin_id, registry) = {
        let guard = user_data.get()?;
        let data = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (data.plugin_id.clone(), data.evaluator_registry.clone())
    };
    register_handler::<RegisterEvaluatorInput>(
        plugin,
        inputs,
        _outputs,
        &plugin_id,
        &registry,
        |input| (&input.problem_type, &input.handler),
        "Evaluator",
    )
}

fn register_checker_format_fn(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    user_data: UserData<RegistryUserData>,
) -> Result<(), extism::Error> {
    let (plugin_id, registry) = {
        let guard = user_data.get()?;
        let data = guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (data.plugin_id.clone(), data.checker_format_registry.clone())
    };
    register_handler::<RegisterCheckerFormatInput>(
        plugin,
        inputs,
        _outputs,
        &plugin_id,
        &registry,
        |input| (&input.checker_format, &input.handler),
        "Checker format",
    )
}
