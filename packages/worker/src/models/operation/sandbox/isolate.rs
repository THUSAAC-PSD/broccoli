use super::error::SandboxError;
use super::{DirectoryRule, EnvRule, ExecutionResult, ResourceLimits, RunOptions, SandboxManager};
use crate::config::WorkerAppConfig;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use tokio::process::Command;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct IsolateSandboxManager {
    isolate_bin: String,
    enable_cgroups: bool,
    /// Maps box_id -> working directory path (the `/box` subdirectory inside isolate's sandbox root).
    sandboxes: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl IsolateSandboxManager {
    pub fn new(isolate_bin: String, enable_cgroups: bool) -> Self {
        Self {
            isolate_bin,
            enable_cgroups,
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for IsolateSandboxManager {
    fn default() -> Self {
        let cfg = WorkerAppConfig::load().ok();
        Self {
            isolate_bin: cfg
                .as_ref()
                .map(|c| c.worker.isolate_bin.clone())
                .unwrap_or_else(|| "isolate".to_string()),
            enable_cgroups: cfg.map(|c| c.worker.enable_cgroups).unwrap_or(false),
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

fn is_fifo(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path)
            .map(|m| m.file_type().is_fifo())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
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
    cgroups_enabled: bool,
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
        if cgroups_enabled {
            command.arg(format!("--cg-mem={memory_limit}"));
        } else {
            command.arg(format!("--mem={memory_limit}"));
        }
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

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            raw.insert(key.trim().to_string(), value.trim().to_string());
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
        cg_oom_killed: parse_i32("cg-oom-killed").map(|v| v != 0).unwrap_or(false),
        killed: parse_i32("killed").map(|v| v != 0).unwrap_or(false),
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
    async fn create_sandbox(&self, id: Option<&str>) -> Result<PathBuf, SandboxError> {
        let mut command = Command::new(&self.isolate_bin);
        if let Some(box_id) = id {
            command.arg(format!("--box-id={box_id}"));
        }
        if self.enable_cgroups {
            command.arg("--cg");
        }
        command.arg("--init");

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

        let working_dir = PathBuf::from(&path_text).join("box");
        let box_id = id.unwrap_or("0").to_string();
        self.sandboxes
            .write()
            .await
            .insert(box_id, working_dir.clone());

        Ok(working_dir)
    }

    async fn remove_sandbox(&self, id: &str) -> Result<(), SandboxError> {
        let box_id = parse_box_id(Some(id))?;
        self.sandboxes.write().await.remove(&box_id);

        let mut command = Command::new(&self.isolate_bin);
        command.arg(format!("--box-id={box_id}"));
        if self.enable_cgroups {
            command.arg("--cg");
        }
        command.arg("--cleanup");

        let output = command.output().await.map_err(|err| {
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

    /// NOTE: by default, the process limit is set to 1 by isolate default, which means the executed program cannot spawn child processes. If you encounter errors related to process spawning (like compiling), please check if the process limit is the cause and set it to a larger value or 0 (no limit) in RunOptions.
    async fn execute(
        &self,
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
        let meta_path = std::env::temp_dir().join(format!(
            "broccoli-isolate-{box_id}-{}.meta",
            uuid::Uuid::new_v4()
        ));

        let mut command = Command::new(&self.isolate_bin);
        command.arg(format!("--box-id={box_id}"));
        if self.enable_cgroups {
            command.arg("--cg");
        }
        command.arg(format!("--meta={}", meta_path.to_string_lossy()));

        if run_options.wait {
            command.arg("--wait");
        }
        if let Some(uid) = run_options.as_uid {
            command.arg(format!("--as-uid={uid}"));
        }
        if let Some(gid) = run_options.as_gid {
            command.arg(format!("--as-gid={gid}"));
        }

        add_resource_limit_args(
            &mut command,
            &run_options.resource_limits,
            self.enable_cgroups,
        )?;

        if let Some(stdin) = &run_options.stdin {
            command.arg(format!("--stdin={}", stdin.to_string_lossy()));
        }
        if let Some(stdout) = &run_options.stdout {
            command.arg(format!("--stdout={}", stdout.to_string_lossy()));
        }
        if let Some(stderr) = &run_options.stderr {
            command.arg(format!("--stderr={}", stderr.to_string_lossy()));
        }
        if run_options.env_rules.is_empty() {
            // Default to inheriting the full environment so that tools like
            // g++ can locate sub-programs (ld, as, etc.) via PATH.
            command.arg("--full-env");
        } else {
            for rule in &run_options.env_rules {
                add_env_rule_args(&mut command, rule);
            }
        }
        for rule in &run_options.directory_rules {
            add_directory_rule_args(&mut command, rule);
        }

        // Isolate resolves relative paths from `/box/` (the working directory
        // inside the chroot).  Directory mounts via `--dir` are placed at the
        // sandbox root, e.g. `--dir=/channels=<host>:rw` mounts at `/channels`,
        // NOT at `/box/channels`.  Plugins produce relative argv paths like
        // `channels/c0_to_m` (which resolves to `/box/channels/c0_to_m` inside
        // the chroot — not the mount point).
        //
        // To bridge this gap, rewrite argv elements whose leading segment
        // matches an absolute directory-rule inside_path.  For example, if a
        // rule has inside_path `/channels` and an argv element is
        // `channels/c0_to_m`, rewrite it to `/channels/c0_to_m`.
        let rewritten_argv: Vec<String> = argv
            .into_iter()
            .map(|arg| {
                for rule in &run_options.directory_rules {
                    let inside = rule.inside_path.to_string_lossy();
                    if let Some(rel) = inside.strip_prefix('/')
                        && (arg == rel || arg.starts_with(&format!("{rel}/")))
                    {
                        return format!("/{arg}");
                    }
                }
                arg
            })
            .collect();

        command.arg("--run").arg("--").args(&rewritten_argv);

        let output = command.output().await.map_err(|err| {
            SandboxError::Execution(format!("failed to execute isolate --run: {err}"))
        })?;

        match output.status.code() {
            Some(0) | Some(1) => {
                let mut result = parse_meta_file(&meta_path).await?;
                let _ = fs::remove_file(&meta_path).await;
                let box_dir = self
                    .sandboxes
                    .read()
                    .await
                    .get(&box_id)
                    .cloned()
                    .ok_or_else(|| {
                        SandboxError::Execution(format!(
                            "sandbox working directory not found for box id: {box_id}"
                        ))
                    })?;
                result.stdout = if let Some(stdout_path) = &run_options.stdout {
                    let resolved = box_dir.join(stdout_path);
                    // FIFOs must not be re-read after process exit: the write
                    // end is closed, so open(O_RDONLY) would block forever.
                    if is_fifo(&resolved) {
                        String::new()
                    } else {
                        match tokio::fs::read_to_string(&resolved).await {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!(
                                    path = %resolved.display(),
                                    error = %e,
                                    "Failed to read stdout file after execution"
                                );
                                String::new()
                            }
                        }
                    }
                } else {
                    String::from_utf8_lossy(&output.stdout).to_string()
                };
                result.stderr = if let Some(stderr_path) = &run_options.stderr {
                    let resolved = box_dir.join(stderr_path);
                    if is_fifo(&resolved) {
                        String::new()
                    } else {
                        match tokio::fs::read_to_string(&resolved).await {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!(
                                    path = %resolved.display(),
                                    error = %e,
                                    "Failed to read stderr file after execution"
                                );
                                String::new()
                            }
                        }
                    }
                } else {
                    String::from_utf8_lossy(&output.stderr).to_string()
                };
                Ok(result)
            }
            _ => {
                let _ = fs::remove_file(&meta_path).await;
                Err(SandboxError::Unknown(format!(
                    "isolate internal error: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
        }
    }
}
