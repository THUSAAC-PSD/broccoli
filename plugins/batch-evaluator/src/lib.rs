#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};

pub mod batch;

#[cfg(target_arch = "wasm32")]
fn load_sandbox_config(host: &Host) -> batch::SandboxConfig {
    match host.config.get_global("sandbox") {
        Ok(r) => serde_json::from_value(r.config).unwrap_or_default(),
        Err(_) => batch::SandboxConfig::default(),
    }
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    host.registry
        .register_evaluator("batch", "evaluate_batch")?;
    host.log.info("Batch evaluator registered")?;
    Ok("ok".to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn evaluate_batch(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: BuildEvalOpsInput = serde_json::from_str(&input)?;
    let tc_id = req.test_case_id;

    let sandbox_config = load_sandbox_config(&host);

    let default_lang = host
        .language
        .get_config(&req.solution_language, "", &[])
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;
    let primary_source = req
        .solution_source
        .iter()
        .find(|file| file.filename == default_lang.source_filename)
        .or_else(|| req.solution_source.first())
        .ok_or_else(|| extism_pdk::Error::msg("No source file provided"))?;
    let extra_sources: Vec<String> = req
        .solution_source
        .iter()
        .filter(|f| f.filename != primary_source.filename)
        .map(|f| f.filename.clone())
        .collect();
    let lang = host
        .language
        .get_config(
            &req.solution_language,
            &primary_source.filename,
            &extra_sources,
        )
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let operations = batch::build_operation(&req, &lang, &sandbox_config)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let batch_id = host
        .operations
        .start_batch(&operations)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let result = host
        .operations
        .next_result(&batch_id, sandbox_config.result_timeout_ms)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let checker_format = req.checker_format.as_deref().unwrap_or("exact");
    let checker_input = CheckerParseInput {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
        expected_output: req.expected_output.clone(),
        test_input: req.test_input.clone(),
        checker_source: req.checker_source.clone(),
        config: req.checker_config.clone(),
    };
    let verdict = evaluator::interpret_sandbox_result(
        &host.checker,
        tc_id,
        &result,
        checker_format,
        &checker_input,
    )
    .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    Ok(serde_json::to_string(&verdict)?)
}
