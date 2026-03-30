#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};

pub mod config;
pub mod interpret;
pub mod operation;

#[cfg(target_arch = "wasm32")]
fn load_sandbox_config(host: &Host) -> config::SandboxConfig {
    match host.config.get_global("sandbox") {
        Ok(r) => serde_json::from_value(r.config).unwrap_or_default(),
        Err(_) => config::SandboxConfig::default(),
    }
}

#[cfg(target_arch = "wasm32")]
fn load_comm_config(host: &Host, problem_id: i32) -> config::CommConfig {
    match host.config.get_problem(problem_id, "communication") {
        Ok(r) => serde_json::from_value(r.config).unwrap_or_default(),
        Err(_) => config::CommConfig::default(),
    }
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    host.registry
        .register_evaluator("communication", "evaluate_communication")?;
    host.log.info("Communication evaluator registered")?;
    Ok("ok".to_string())
}

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn evaluate_communication(input: String) -> FnResult<String> {
    let host = Host::new();
    let req: BuildEvalOpsInput = serde_json::from_str(&input)?;
    let tc_id = req.test_case_id;
    let problem_id = req.problem_id;

    let sandbox_config = load_sandbox_config(&host);
    let comm_config = load_comm_config(&host, problem_id);

    let default_lang = host
        .language
        .get_config(&req.solution_language, "", &[])
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;
    let primary_source = req
        .solution_source
        .iter()
        .find(|f| f.filename == default_lang.source_filename)
        .or_else(|| req.solution_source.first())
        .ok_or_else(|| extism_pdk::Error::msg("No contestant source file provided"))?;
    let contestant_extra: Vec<String> = req
        .solution_source
        .iter()
        .filter(|f| f.filename != primary_source.filename)
        .map(|f| f.filename.clone())
        .collect();
    let contestant_lang = host
        .language
        .get_config(
            &req.solution_language,
            &primary_source.filename,
            &contestant_extra,
        )
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    if comm_config.manager_sources.is_empty() {
        return Err(extism_pdk::Error::msg(
            "Communication config has no manager_sources. \
             Upload manager source files and set them in the problem's communication config.",
        )
        .into());
    }

    // Primary file = first entry, used for language resolution
    let mgr_filename = &comm_config.manager_sources[0].filename;
    let mgr_extra: Vec<String> = comm_config
        .manager_sources
        .iter()
        .skip(1)
        .map(|s| s.filename.clone())
        .collect();
    let manager_lang = host
        .language
        .get_config(&comm_config.manager_language, mgr_filename, &mgr_extra)
        .map_err(|e| extism_pdk::Error::msg(format!("Manager language config: {e}")))?;

    let operations = operation::build_operation(
        &req,
        &contestant_lang,
        &manager_lang,
        &comm_config.manager_sources,
        &comm_config,
        &sandbox_config,
    )
    .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let batch_id = host
        .operations
        .start_batch(&operations)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let result = host
        .operations
        .next_result(&batch_id, sandbox_config.result_timeout_ms)
        .map_err(|e| extism_pdk::Error::msg(format!("{e}")))?;

    let memory_limit_kb = u32::try_from(req.memory_limit_kb).unwrap_or(u32::MAX);
    let verdict = interpret::interpret_result(
        tc_id,
        &result,
        comm_config.num_processes as usize,
        memory_limit_kb,
    );

    Ok(serde_json::to_string(&verdict)?)
}
