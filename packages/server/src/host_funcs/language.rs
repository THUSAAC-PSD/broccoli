use std::collections::HashMap;

use common::language::{LanguageDefinition, resolve_language};
use extism::{Function, UserData, Val, ValType};
use serde::Deserialize;

#[derive(Deserialize)]
struct GetLanguageConfigInput {
    language: String,
    submitted_filename: String,
    #[serde(default)]
    extra_sources: Vec<String>,
}

type LanguageUserData = (String, HashMap<String, LanguageDefinition>);

pub fn create_language_function(
    plugin_id: String,
    languages: HashMap<String, LanguageDefinition>,
) -> Function {
    let user_data: UserData<LanguageUserData> = UserData::new((plugin_id, languages));

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
    user_data: UserData<LanguageUserData>,
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
