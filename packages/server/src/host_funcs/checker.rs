use crate::registry::CheckerFormatRegistry;
use common::submission_dispatch::{CheckerParseInput, CheckerVerdict, JudgeFile, RunCheckerInput};
use extism::{Function, UserData, Val, ValType};
use plugin_core::traits::PluginManager;
use std::sync::Arc;

use super::storage::{BlobReadGrants, issue_blob_read_token};

type CheckerUserData = (
    String,
    Arc<dyn PluginManager>,
    CheckerFormatRegistry,
    BlobReadGrants,
);

pub fn create_checker_function(
    plugin_id: String,
    plugin_manager: Arc<dyn PluginManager>,
    checker_format_registry: CheckerFormatRegistry,
    blob_read_grants: BlobReadGrants,
) -> Function {
    let user_data: UserData<CheckerUserData> = UserData::new((
        plugin_id,
        plugin_manager,
        checker_format_registry,
        blob_read_grants,
    ));

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
    let mut input: RunCheckerInput = serde_json::from_slice(&input_bytes)
        .map_err(|e| extism::Error::msg(format!("Failed to deserialize input: {}", e)))?;

    let (caller_plugin_id, plugin_manager, checker_format_registry, blob_read_grants) = {
        let user_data_guard = user_data.get()?;
        let guard = user_data_guard
            .lock()
            .map_err(|_| extism::Error::msg("Lock poisoned"))?;
        let (id, pm, reg, grants) = &*guard;
        (id.clone(), pm.clone(), reg.clone(), grants.clone())
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

    let issued_blob_tokens =
        grant_checker_blob_reads(&mut input.input, &handler.plugin_id, &blob_read_grants);

    let input_bytes = serde_json::to_vec(&input.input)
        .map_err(|e| extism::Error::msg(format!("Failed to serialize checker input: {}", e)))?;

    let result_bytes = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_manager
                .call_raw(&handler.plugin_id, &handler.function_name, input_bytes)
                .await
        })
    });
    for token in issued_blob_tokens {
        blob_read_grants.remove(&token);
    }
    let result_bytes = result_bytes?;

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

fn grant_checker_blob_reads(
    input: &mut CheckerParseInput,
    checker_plugin_id: &str,
    grants: &BlobReadGrants,
) -> Vec<String> {
    let mut tokens = Vec::new();
    grant_judge_file_read(&mut input.stdout, checker_plugin_id, grants, &mut tokens);
    grant_judge_file_read(
        &mut input.expected_output,
        checker_plugin_id,
        grants,
        &mut tokens,
    );
    grant_judge_file_read(
        &mut input.test_input,
        checker_plugin_id,
        grants,
        &mut tokens,
    );
    tokens
}

fn grant_judge_file_read(
    file: &mut JudgeFile,
    checker_plugin_id: &str,
    grants: &BlobReadGrants,
    tokens: &mut Vec<String>,
) {
    let JudgeFile::Blob { file } = file else {
        return;
    };
    let token = issue_blob_read_token(grants, checker_plugin_id, &file.blob_hash);
    file.read_token = Some(token.clone());
    tokens.push(token);
}
