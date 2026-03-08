#[cfg(feature = "wasm")]
use broccoli_server_sdk::prelude::*;

#[cfg(feature = "wasm")]
use extism_pdk::{plugin_fn, FnResult};

pub mod batch;

#[cfg(feature = "wasm")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    host::registry::register_evaluator("batch", "evaluate_batch")?;
    host::logger::log_info("Batch evaluator registered")?;
    Ok("ok".to_string())
}

#[cfg(feature = "wasm")]
#[plugin_fn]
pub fn evaluate_batch(input: String) -> FnResult<String> {
    let req: BuildEvalOpsInput = serde_json::from_str(&input)?;
    let tc_id = req.test_case_id;

    // 1. Get language config for compile/run commands
    let filename = req
        .solution_source
        .first()
        .map(|f| f.filename.as_str())
        .unwrap_or("solution");
    let host_lang = host::language::get_language_config(&req.solution_language, filename)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;
    let lang = batch::ResolvedLanguage {
        compile_cmd: host_lang.compile_cmd,
        run_cmd: host_lang.run_cmd,
        source_filename: host_lang.source_filename,
        binary_name: host_lang.binary_name,
    };

    // 2. Build sandbox operation (pure logic, no host calls)
    let ops_json =
        batch::build_operation(&req, &lang).map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    // 3. Dispatch to worker sandbox
    let batch_id = host::operations::start_batch(&ops_json)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    // 4. Wait for sandbox result (30s timeout)
    let result = host::operations::wait_for_result(&batch_id, 30_000)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    // 5. Interpret sandbox result + run checker (opaque passthrough)
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
    let verdict = evaluator::interpret_sandbox_result(tc_id, &result, checker_format, &checker_input)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    Ok(serde_json::to_string(&verdict)?)
}
