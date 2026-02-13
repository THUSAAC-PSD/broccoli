use super::error::SandboxError;
use super::{
    DirectoryRule, EnvRule, ExecutionResult, ResourceLimits, RunOptions, SandboxManager,
    SandboxOptions,
};
use crate::config::WorkerAppConfig;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Default)]
pub struct IsolateSandboxManager;

fn isolate_bin() -> String {
    WorkerAppConfig::load()
        .map(|cfg| cfg.worker.isolate_bin)
        .unwrap_or_else(|_| "isolate".to_string())
}

fn parse_box_id(id: Option<&str>) -> Result<String, SandboxError> {
    let raw = id.unwrap_or("0");
    raw.parse::<u32>()
        .map(|n| n.to_string())
        .map_err(|_| SandboxError::Initialization(format!("invalid isolate box id: {raw}")))
}

fn add_directory_rule_args(command: &mut Command, rule: &DirectoryRule) {
    let inside = rule.inside_path.to_string_lossy();
    let mut option_parts = Vec::new();

    if rule.options.read_write {
        option_parts.push("rw");
    }
    if rule.options.allow_devices {
        option_parts.push("dev");
    }
    if rule.options.no_exec {
        option_parts.push("noexec");
    }
    if rule.options.is_filesystem {
        option_parts.push("fs");
    }
    if rule.options.is_tmp {
        option_parts.push("tmp");
    }
    if rule.options.no_recursive {
        option_parts.push("norec");
    }

    let options = if option_parts.is_empty() {
        String::new()
    } else {
        format!(":{}", option_parts.join(","))
    };

    let rule_value = match &rule.outside_path {
        Some(outside) => format!("{}={}{}", inside, outside.to_string_lossy(), options),
        None => {
            if option_parts.is_empty() {
                format!("{}=", inside)
            } else {
                format!("{}{}", inside, options)
            }
        }
    };

    command.arg(format!("--dir={rule_value}"));
}

fn add_env_rule_args(command: &mut Command, rule: &EnvRule) {
    match rule {
        EnvRule::Inherit(var) => {
            command.arg(format!("--env={var}"));
        }
        EnvRule::Set(var, value) => {
            command.arg(format!("--env={var}={value}"));
        }
        EnvRule::FullEnv => {
            command.arg("--full-env");
        }
    }
}

fn add_resource_limit_args(
    command: &mut Command,
    limits: &ResourceLimits,
) -> Result<(), SandboxError> {
    if let Some(time_limit) = limits.time_limit {
        command.arg(format!("--time={time_limit}"));
    }
    if let Some(wall_time_limit) = limits.wall_time_limit {
        command.arg(format!("--wall-time={wall_time_limit}"));
    }
    if let Some(extra_time) = limits.extra_time {
        command.arg(format!("--extra-time={extra_time}"));
    }
    if let Some(memory_limit) = limits.memory_limit {
        command.arg(format!("--cg-mem={memory_limit}"));
    }
    if let Some(stack_limit) = limits.stack_limit {
        command.arg(format!("--stack={stack_limit}"));
    }
    if let Some(open_files_limit) = limits.open_files_limit {
        command.arg(format!("--open-files={open_files_limit}"));
    }
    if let Some(file_size_limit) = limits.file_size_limit {
        command.arg(format!("--fsize={file_size_limit}"));
    }
    if let Some(process_limit) = limits.process_limit {
        if process_limit == 0 {
            command.arg("--processes");
        } else {
            command.arg(format!("--processes={process_limit}"));
        }
    }

    Ok(())
}

async fn parse_meta_file(meta_path: &Path) -> Result<ExecutionResult, SandboxError> {
    let content = fs::read_to_string(meta_path).await.map_err(|err| {
        SandboxError::Execution(format!("failed to read isolate meta file: {err}"))
    })?;

    let mut raw = HashMap::<String, String>::new();
    let mut oom_killed = false;

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            raw.insert(key.trim().to_string(), value.trim().to_string());
        } else if line.trim() == "cg-oom-killed" {
            oom_killed = true;
        }
    }

    let parse_i32 = |key: &str| raw.get(key).and_then(|v| v.parse::<i32>().ok());
    let parse_u32 = |key: &str| raw.get(key).and_then(|v| v.parse::<u32>().ok());
    let parse_f64 = |key: &str| {
        raw.get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };

    Ok(ExecutionResult {
        exit_code: parse_i32("exitcode"),
        signal: parse_i32("exitsig"),
        time_used: parse_f64("time"),
        wall_time_used: parse_f64("time-wall"),
        memory_used: parse_u32("cg-mem").or(parse_u32("max-rss")),
        oom_killed,
        status: raw
            .get("status")
            .cloned()
            .unwrap_or_else(|| "OK".to_string()),
        message: raw.get("message").cloned().unwrap_or_default(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

#[async_trait]
impl SandboxManager for IsolateSandboxManager {
    async fn create_sandbox(
        id: Option<&str>,
        options: &SandboxOptions,
    ) -> Result<PathBuf, SandboxError> {
        let mut command = Command::new(isolate_bin());
        if let Some(box_id) = id {
            command.arg(format!("--box-id={box_id}"));
        }
        command.arg("--cg").arg("--init");

        for rule in &options.directory_rules {
            add_directory_rule_args(&mut command, rule);
        }
        for rule in &options.env_rules {
            add_env_rule_args(&mut command, rule);
        }

        let output = command.output().await.map_err(|err| {
            SandboxError::Initialization(format!("failed to execute isolate --init: {err}"))
        })?;

        if !output.status.success() {
            return Err(SandboxError::Initialization(format!(
                "isolate --init failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let path_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path_text.is_empty() {
            return Err(SandboxError::Initialization(
                "isolate --init did not return sandbox path".to_string(),
            ));
        }

        Ok(PathBuf::from(path_text))
    }

    async fn remove_sandbox(id: &str) -> Result<(), SandboxError> {
        let box_id = parse_box_id(Some(id))?;

        let output = Command::new(isolate_bin())
            .arg(format!("--box-id={box_id}"))
            .arg("--cg")
            .arg("--cleanup")
            .output()
            .await
            .map_err(|err| {
                SandboxError::Execution(format!("failed to execute isolate --cleanup: {err}"))
            })?;

        if !output.status.success() {
            return Err(SandboxError::Execution(format!(
                "isolate --cleanup failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        Ok(())
    }

    async fn execute(
        box_id: &str,
        argv: Vec<String>,
        run_options: &RunOptions,
    ) -> Result<ExecutionResult, SandboxError> {
        if argv.is_empty() {
            return Err(SandboxError::Execution(
                "isolate --run requires at least one program argument".to_string(),
            ));
        }

        let box_id = parse_box_id(Some(box_id))?;
        let meta_path = std::env::temp_dir().join(format!("broccoli-isolate-{box_id}.meta"));

        let mut command = Command::new(isolate_bin());
        command
            .arg(format!("--box-id={box_id}"))
            .arg("--cg")
            .arg(format!("--meta={}", meta_path.to_string_lossy()));

        if run_options.wait {
            command.arg("--wait");
        }
        if let Some(uid) = run_options.as_uid {
            command.arg(format!("--as-uid={uid}"));
        }
        if let Some(gid) = run_options.as_gid {
            command.arg(format!("--as-gid={gid}"));
        }

        add_resource_limit_args(&mut command, &run_options.resource_limits)?;

        if let Some(stdin) = &run_options.stdin {
            command.arg(format!("--stdin={}", stdin.to_string_lossy()));
        }
        if let Some(stdout) = &run_options.stdout {
            command.arg(format!("--stdout={}", stdout.to_string_lossy()));
        }
        if let Some(stderr) = &run_options.stderr {
            command.arg(format!("--stderr={}", stderr.to_string_lossy()));
        }

        command.arg("--run").arg("--").args(argv);

        let output = command.output().await.map_err(|err| {
            SandboxError::Execution(format!("failed to execute isolate --run: {err}"))
        })?;

        match output.status.code() {
            Some(0) | Some(1) => {
                let mut result = parse_meta_file(&meta_path).await?;
                result.stdout = String::from_utf8_lossy(&output.stdout).to_string();
                result.stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(result)
            }
            _ => Err(SandboxError::Unknown(format!(
                "isolate internal error: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }
}
