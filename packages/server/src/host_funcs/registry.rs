use crate::registry::{
    CheckerFormatRegistry, ContestTypeHandlers, ContestTypeRegistry, EvaluatorRegistry,
    LanguageResolverEntry, LanguageResolverRegistry, PluginHandler,
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
    submission_handler: String,
    code_run_handler: String,
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

/// Input for register_language_resolver
#[derive(Deserialize)]
struct RegisterLanguageResolverInput {
    /// Language ID this resolver handles (e.g. "cpp", "python3").
    language_id: String,
    /// Function name in this plugin that implements resolution.
    function_name: String,
    /// Human-friendly display name (e.g. "C++", "Python 3").
    /// Defaults to `language_id` if not provided.
    #[serde(default)]
    display_name: String,
    /// Default source filename (e.g. "solution.cpp").
    /// Defaults to "solution.txt" if not provided.
    #[serde(default = "default_source_filename")]
    default_filename: String,
}

fn default_source_filename() -> String {
    "solution.txt".to_string()
}

/// Named context for registry host functions.
struct RegistryContext {
    plugin_id: String,
    contest_type_registry: ContestTypeRegistry,
    evaluator_registry: EvaluatorRegistry,
    checker_format_registry: CheckerFormatRegistry,
    language_resolver_registry: LanguageResolverRegistry,
}

type RegistryUserData = RegistryContext;

pub fn create_registry_functions(
    plugin_id: String,
    contest_type_registry: ContestTypeRegistry,
    evaluator_registry: EvaluatorRegistry,
    checker_format_registry: CheckerFormatRegistry,
    language_resolver_registry: LanguageResolverRegistry,
) -> Vec<Function> {
    let user_data: UserData<RegistryUserData> = UserData::new(RegistryContext {
        plugin_id: plugin_id.clone(),
        contest_type_registry: contest_type_registry.clone(),
        evaluator_registry: evaluator_registry.clone(),
        checker_format_registry: checker_format_registry.clone(),
        language_resolver_registry: language_resolver_registry.clone(),
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
            user_data.clone(),
            register_checker_format_fn,
        ),
        Function::new(
            "register_language_resolver",
            [ValType::I64],
            [],
            user_data,
            register_language_resolver_fn,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn register_handler<I: serde::de::DeserializeOwned>(
    plugin: &mut extism::CurrentPlugin,
    inputs: &[Val],
    _outputs: &mut [Val],
    plugin_id: &str,
    registry: &Arc<RwLock<HashMap<String, PluginHandler>>>,
    extract: impl FnOnce(&I) -> (&str, &str), // (registry_key, handler_name)
    validate: impl FnOnce(&I) -> Result<(), extism::Error>,
    label: &str,
) -> Result<(), extism::Error> {
    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: I = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    validate(&input)?;

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

    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: RegisterContestTypeInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let key = input.contest_type;
    validate_registry_id(&key, "contest_type")?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let mut registry = registry.write().await;
            registry.insert(
                key.clone(),
                ContestTypeHandlers {
                    plugin_id: plugin_id.to_string(),
                    submission_fn: input.submission_handler.clone(),
                    code_run_fn: input.code_run_handler.clone(),
                },
            );
            tracing::info!(
                plugin_id = %plugin_id,
                key = %key,
                submission_fn = %input.submission_handler,
                code_run_fn = %input.code_run_handler,
                "Contest type registered"
            );
        })
    });

    Ok(())
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
        |input| validate_registry_id(&input.problem_type, "problem_type"),
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
        |input| validate_registry_id(&input.checker_format, "checker_format"),
        "Checker format",
    )
}

/// Validates that an ID contains only alphanumeric characters, hyphens, and underscores,
/// and is between 1 and 128 characters. Used for registry keys (language IDs, etc.).
fn validate_registry_id(id: &str, field_name: &str) -> Result<(), extism::Error> {
    if id.is_empty() || id.len() > 128 {
        return Err(extism::Error::msg(format!(
            "{field_name} must be 1-128 characters"
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(extism::Error::msg(format!(
            "{field_name} must contain only letters, digits, hyphens, and underscores"
        )));
    }
    Ok(())
}

fn register_language_resolver_fn(
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
        (
            data.plugin_id.clone(),
            data.language_resolver_registry.clone(),
        )
    };

    let input_bytes: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
    let input: RegisterLanguageResolverInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    validate_registry_id(&input.language_id, "language_id")?;
    if input.function_name.is_empty() {
        return Err(extism::Error::msg("function_name must not be empty"));
    }

    let display_name = if input.display_name.is_empty() {
        input.language_id.clone()
    } else {
        input.display_name
    };

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let mut registry = registry.write().await;
            registry.insert(
                input.language_id.clone(),
                LanguageResolverEntry {
                    plugin_id: plugin_id.to_string(),
                    function_name: input.function_name.clone(),
                    display_name,
                    default_filename: input.default_filename,
                },
            );
            tracing::info!(
                plugin_id = %plugin_id,
                language_id = %input.language_id,
                handler = %input.function_name,
                "Language resolver registered"
            );
        })
    });

    Ok(())
}
