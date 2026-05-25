#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::types::{
    BuildEvalOpsInput, EvaluateOperationResultsInput, OperationResult, PreparedEvaluateCase,
    ResolveLanguageInput, TestCaseVerdict, Verdict,
};
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
    let prepared = prepare_case(&host, &req)?;

    let mut results = host
        .operations
        .windowed(&prepared.operations)
        .collect(prepared.result_timeout_ms)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;
    if results.len() != 1 {
        return Err(extism_pdk::Error::msg(format!(
            "expected exactly one operation result, got {}",
            results.len()
        ))
        .into());
    }
    let result = results.remove(0);

    let verdict = interpret_case_result(&host, &req, &result)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    Ok(serde_json::to_string(&verdict)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn prepare_evaluate_case(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: BuildEvalOpsInput = serde_json::from_str(&input)?;
    let prepared = prepare_case(&host, &req)?;
    Ok(serde_json::to_string(&prepared)?)
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn on_operation_results(input: String) -> FnResult<String> {
    let host = Host::new();
    let input: EvaluateOperationResultsInput = serde_json::from_str(&input)?;
    let verdicts = input
        .results
        .into_iter()
        .map(|item| match item.operation_results.first() {
            Some(result) => interpret_case_result(&host, &item.case, result)
                .unwrap_or_else(|e| system_error(item.case.test_case_id, e.to_string())),
            None => system_error(item.case.test_case_id, "Operation produced no result"),
        })
        .collect::<Vec<_>>();

    Ok(serde_json::to_string(&verdicts)?)
}

#[cfg(target_arch = "wasm32")]
fn prepare_case(
    host: &Host,
    req: &BuildEvalOpsInput,
) -> Result<PreparedEvaluateCase, extism_pdk::Error> {
    let sandbox_config = load_sandbox_config(host);

    let additional_filenames: std::collections::HashSet<&str> = req
        .additional_file_refs
        .iter()
        .map(|f| f.filename.as_str())
        .collect();
    let submitted_files: Vec<String> = req
        .solution_source
        .iter()
        .filter(|f| !additional_filenames.contains(f.filename.as_str()))
        .map(|f| f.filename.clone())
        .collect();
    let resolved = host
        .language
        .resolve(&ResolveLanguageInput {
            language_id: req.solution_language.clone(),
            submitted_files,
            additional_files: req.additional_file_refs.clone(),
            problem_id: Some(req.problem_id),
            contest_id: req.contest_id,
            overrides: None,
        })
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let operations = batch::build_operation(req, &resolved, &sandbox_config)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;
    let result_timeout_ms = sandbox_config
        .result_timeout_ms_for(req.time_limit_ms, u32::from(resolved.compile.is_some()));

    Ok(PreparedEvaluateCase {
        operations,
        result_timeout_ms,
    })
}

#[cfg(target_arch = "wasm32")]
fn interpret_case_result(
    host: &Host,
    req: &BuildEvalOpsInput,
    result: &OperationResult,
) -> Result<TestCaseVerdict, extism_pdk::Error> {
    let tc_id = req.test_case_id;
    let checker_format = req.checker_format.as_deref().unwrap_or("exact");
    let checker_input = CheckerParseInput {
        stdout: JudgeFile::Missing,
        stderr: String::new(),
        exit_code: 0,
        expected_output: req.expected_output.clone(),
        test_input: req.test_input.clone(),
        checker_source: req.checker_source.clone(),
        config: req.checker_config.clone(),
    };
    evaluator::interpret_sandbox_result(
        &host.checker,
        tc_id,
        &result,
        checker_format,
        &checker_input,
    )
    .map_err(|e| extism_pdk::Error::msg(format!("{e}")))
}

#[cfg(target_arch = "wasm32")]
fn system_error(test_case_id: i32, message: impl Into<String>) -> TestCaseVerdict {
    TestCaseVerdict {
        test_case_id,
        verdict: Verdict::SystemError,
        score: 0.0,
        time_used_ms: None,
        memory_used_kb: None,
        message: Some(message.into()),
        stdout: None,
        stderr: None,
    }
}
