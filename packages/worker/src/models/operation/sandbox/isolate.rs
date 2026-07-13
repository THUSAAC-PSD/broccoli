use super::error::SandboxError;
use super::{DirectoryRule, EnvRule, ExecutionResult, ResourceLimits, RunOptions, SandboxManager};
use crate::config::WorkerAppConfig;
use async_trait::async_trait;
use common::metrics::Metrics;
use opentelemetry::KeyValue;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::fs;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::RwLock;

const INLINE_OUTPUT_PREVIEW_BYTES: usize = 64 * 1024;

pub struct IsolateSandboxManager {
    isolate_bin: String,
    enable_cgroups: bool,
    sandboxes: Arc<RwLock<HashMap<String, PathBuf>>>,
    metrics: Option<Metrics>,
    /// Per-box cumulative CPU seconds consumed by steps that actually ran in the
    /// box (cache-hit steps never reach `execute`, so they do not contribute).
    ///
    /// isolate runs every step of an environment in ONE box (one `--init`), and
    /// with `--cg` the reported `time` and the `--time` limit are the box
    /// cgroup's CPU usage cumulative *since `--init`*. So a later step (e.g.
    /// `exec`) inherits an earlier step's CPU (e.g. `compile`) — charging compile
    /// time against the run-time limit and false-TLEing legitimate solutions.
    /// We offset each step's `--time` by this prior cumulative and report only
    /// the step's own delta. Reset on `create_sandbox`, dropped on
    /// `remove_sandbox`.
    box_cpu_secs: Arc<Mutex<HashMap<String, f64>>>,
}

impl std::fmt::Debug for IsolateSandboxManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsolateSandboxManager")
            .field("isolate_bin", &self.isolate_bin)
            .field("enable_cgroups", &self.enable_cgroups)
            .finish_non_exhaustive()
    }
}

impl IsolateSandboxManager {
    pub fn new(isolate_bin: String, enable_cgroups: bool) -> Self {
        Self::new_with_metrics(isolate_bin, enable_cgroups, None)
    }

    pub fn new_with_metrics(
        isolate_bin: String,
        enable_cgroups: bool,
        metrics: Option<Metrics>,
    ) -> Self {
        Self {
            isolate_bin,
            enable_cgroups,
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            metrics,
            box_cpu_secs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn metric_attrs(&self, outcome: &'static str) -> [KeyValue; 2] {
        [
            KeyValue::new("enable_cgroups", self.enable_cgroups.to_string()),
            KeyValue::new("outcome", outcome),
        ]
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
            metrics: None,
            box_cpu_secs: Arc::new(Mutex::new(HashMap::new())),
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

fn text_preview_from_bytes(mut bytes: Vec<u8>, truncated: bool) -> String {
    let was_truncated = truncated || bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES;
    if bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES {
        bytes.truncate(INLINE_OUTPUT_PREVIEW_BYTES);
    }

    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    // `from_utf8_lossy` preserves NUL (0x00) bytes — they are valid UTF-8
    // (U+0000) — but PostgreSQL TEXT columns reject 0x00. Sandbox stdout/stderr
    // and isolate `meta` can carry stray NUL (truncated multibyte sequences,
    // partially-flushed buffers, control bytes), so strip them at the capture
    // origin rather than relying on every downstream persistence layer. Replace
    // with U+FFFD to match the SDK/host sanitizers.
    if text.contains('\0') {
        text = text.replace('\0', "\u{FFFD}");
    }
    if was_truncated {
        text.push_str("\n... (truncated)");
    }
    text
}

async fn read_text_preview(path: &Path) -> Result<String, std::io::Error> {
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity(INLINE_OUTPUT_PREVIEW_BYTES + 1);
    let mut limited = file.take((INLINE_OUTPUT_PREVIEW_BYTES + 1) as u64);
    limited.read_to_end(&mut bytes).await?;
    let truncated = bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES;
    Ok(text_preview_from_bytes(bytes, truncated))
}

async fn read_capped_child_pipe<R>(mut reader: R) -> Result<Vec<u8>, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut preview = Vec::with_capacity(INLINE_OUTPUT_PREVIEW_BYTES + 1);
    let mut chunk = [0_u8; 8192];

    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }

        if preview.len() <= INLINE_OUTPUT_PREVIEW_BYTES {
            let remaining = (INLINE_OUTPUT_PREVIEW_BYTES + 1).saturating_sub(preview.len());
            let keep = remaining.min(read);
            preview.extend_from_slice(&chunk[..keep]);
        }
    }

    Ok(preview)
}

async fn join_pipe_capture(
    handle: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>,
    stream_name: &str,
) -> Result<Vec<u8>, SandboxError> {
    handle
        .await
        .map_err(|err| {
            SandboxError::Execution(format!(
                "failed to join isolate {stream_name} reader: {err}"
            ))
        })?
        .map_err(|err| {
            SandboxError::Execution(format!("failed to read isolate {stream_name}: {err}"))
        })
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
    #[tracing::instrument(
        name = "isolate_init",
        skip(self),
        fields(box_id = id.unwrap_or("0"), enable_cgroups = self.enable_cgroups)
    )]
    async fn create_sandbox(&self, id: Option<&str>) -> Result<PathBuf, SandboxError> {
        let start = std::time::Instant::now();
        let mut command = Command::new(&self.isolate_bin);
        if let Some(box_id) = id {
            command.arg(format!("--box-id={box_id}"));
        }
        if self.enable_cgroups {
            command.arg("--cg");
        }
        command.arg("--init");

        let output = match command.output().await {
            Ok(output) => output,
            Err(err) => {
                if let Some(metrics) = &self.metrics {
                    metrics
                        .sandbox_init_duration
                        .record(start.elapsed().as_secs_f64(), &self.metric_attrs("error"));
                }
                return Err(SandboxError::Initialization(format!(
                    "failed to execute isolate --init: {err}"
                )));
            }
        };

        if !output.status.success() {
            if let Some(metrics) = &self.metrics {
                metrics
                    .sandbox_init_duration
                    .record(start.elapsed().as_secs_f64(), &self.metric_attrs("error"));
            }
            return Err(SandboxError::Initialization(format!(
                "isolate --init failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let path_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path_text.is_empty() {
            if let Some(metrics) = &self.metrics {
                metrics
                    .sandbox_init_duration
                    .record(start.elapsed().as_secs_f64(), &self.metric_attrs("error"));
            }
            return Err(SandboxError::Initialization(
                "isolate --init did not return sandbox path".to_string(),
            ));
        }

        let working_dir = PathBuf::from(&path_text).join("box");
        let box_id = id.unwrap_or("0").to_string();
        self.sandboxes
            .write()
            .await
            .insert(box_id.clone(), working_dir.clone());
        // Fresh box (--init resets the cgroup) -> zero prior CPU for this box.
        if let Ok(mut m) = self.box_cpu_secs.lock() {
            m.insert(box_id, 0.0);
        }

        if let Some(metrics) = &self.metrics {
            metrics
                .sandbox_init_duration
                .record(start.elapsed().as_secs_f64(), &self.metric_attrs("success"));
        }

        Ok(working_dir)
    }

    #[tracing::instrument(
        name = "isolate_cleanup",
        skip(self),
        fields(box_id = %id, enable_cgroups = self.enable_cgroups)
    )]
    async fn remove_sandbox(&self, id: &str) -> Result<(), SandboxError> {
        let start = std::time::Instant::now();
        let box_id = parse_box_id(Some(id))?;
        self.sandboxes.write().await.remove(&box_id);
        if let Ok(mut m) = self.box_cpu_secs.lock() {
            m.remove(&box_id);
        }

        let mut command = Command::new(&self.isolate_bin);
        command.arg(format!("--box-id={box_id}"));
        if self.enable_cgroups {
            command.arg("--cg");
        }
        command.arg("--cleanup");

        let output = match command.output().await {
            Ok(output) => output,
            Err(err) => {
                if let Some(metrics) = &self.metrics {
                    metrics
                        .sandbox_cleanup_duration
                        .record(start.elapsed().as_secs_f64(), &self.metric_attrs("error"));
                }
                return Err(SandboxError::Execution(format!(
                    "failed to execute isolate --cleanup: {err}"
                )));
            }
        };

        if !output.status.success() {
            if let Some(metrics) = &self.metrics {
                metrics
                    .sandbox_cleanup_duration
                    .record(start.elapsed().as_secs_f64(), &self.metric_attrs("error"));
            }
            return Err(SandboxError::Execution(format!(
                "isolate --cleanup failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        if let Some(metrics) = &self.metrics {
            metrics
                .sandbox_cleanup_duration
                .record(start.elapsed().as_secs_f64(), &self.metric_attrs("success"));
        }

        Ok(())
    }

    #[tracing::instrument(
        name = "isolate_execute",
        skip(self, argv, run_options),
        fields(
            box_id = %box_id,
            argv_len = argv.len(),
            wait = run_options.wait,
            env_rule_count = run_options.env_rules.len(),
            directory_rule_count = run_options.directory_rules.len(),
            enable_cgroups = self.enable_cgroups
        )
    )]
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

        // Per-box cumulative-CPU offset (see `box_cpu_secs`). isolate's `--cg`
        // `time` and `--time` limit are the box cgroup's CPU usage cumulative
        // since `--init`, so a step inherits prior same-box steps' CPU. Add the
        // prior cumulative to this step's CPU `--time` so isolate enforces the
        // step's OWN limit, then report only the step's own delta. `time_limit`
        // is CPU seconds; wall time (`--wall-time`) is measured per `--run` and
        // is not cumulative, so it is left untouched.
        let box_key = parse_box_id(Some(box_id))?;
        let prior_cpu = self
            .box_cpu_secs
            .lock()
            .ok()
            .and_then(|m| m.get(&box_key).copied())
            .unwrap_or(0.0);
        let adjusted_opts =
            if prior_cpu > 0.0 && run_options.resource_limits.time_limit.is_some() {
                let mut ro = run_options.clone();
                ro.resource_limits.time_limit =
                    ro.resource_limits.time_limit.map(|t| t + prior_cpu);
                Some(ro)
            } else {
                None
            };
        let effective_opts = adjusted_opts.as_ref().unwrap_or(run_options);

        const MAX_TRANSIENT_RETRIES: usize = 3;
        for attempt in 0..=MAX_TRANSIENT_RETRIES {
            let result = self.execute_once(box_id, argv.clone(), effective_opts).await;
            match result {
                // Retryable isolate setup error: under concurrent judging a
                // box's input file can momentarily carry restrictive perms
                // (shared cache inode being re-chmodded by another op), making
                // isolate's open() of stdin/input fail. Re-running picks up the
                // now-readable file. Bounded by MAX_TRANSIENT_RETRIES.
                Err(SandboxError::Unknown(msg))
                    if attempt < MAX_TRANSIENT_RETRIES
                        && msg.contains("Permission denied") =>
                {
                    let backoff_ms = 25u64 << attempt;
                    tracing::warn!(
                        box_id = %box_id,
                        attempt = attempt + 1,
                        backoff_ms,
                        error = %msg,
                        "Transient isolate setup failure (EACCES), retrying after backoff",
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    continue;
                }
                Err(e) => return Err(e),
                Ok(mut exec) => {
                    if attempt < MAX_TRANSIENT_RETRIES && is_transient_exec_failure(&exec) {
                        let backoff_ms = 25u64 << attempt;
                        tracing::warn!(
                            box_id = %box_id,
                            attempt = attempt + 1,
                            backoff_ms,
                            stderr_preview = %exec.stderr.chars().take(120).collect::<String>(),
                            "Transient exec failure (EAGAIN), retrying after backoff",
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                    // `exec.time_used` is the box cgroup's cumulative CPU since
                    // --init. Persist it as the new prior, then report only this
                    // step's own CPU (the delta above the prior cumulative).
                    let cumulative = exec.time_used;
                    if let Ok(mut m) = self.box_cpu_secs.lock() {
                        m.insert(box_key.clone(), cumulative.max(prior_cpu));
                    }
                    exec.time_used = (cumulative - prior_cpu).max(0.0);
                    return Ok(exec);
                }
            }
        }
        unreachable!("retry loop exits via return")
    }
}

fn is_transient_exec_failure(result: &ExecutionResult) -> bool {
    result.exit_code == Some(127)
        && result.stderr.contains("execve(")
        && result.stderr.contains("Resource temporarily unavailable")
}

impl IsolateSandboxManager {
    #[tracing::instrument(
        name = "isolate_execute_once",
        skip(self, argv, run_options),
        fields(
            box_id = %box_id,
            argv_len = argv.len(),
            wait = run_options.wait,
            env_rule_count = run_options.env_rules.len(),
            directory_rule_count = run_options.directory_rules.len(),
            enable_cgroups = self.enable_cgroups
        )
    )]
    async fn execute_once(
        &self,
        box_id: &str,
        argv: Vec<String>,
        run_options: &RunOptions,
    ) -> Result<ExecutionResult, SandboxError> {
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
            // Never inherit the worker's full environment into a sandboxed
            // (potentially contestant-controlled) program: `--full-env` would
            // leak the DB/Redis/S3 credentials, the JWT secret, and every other
            // BROCCOLI__* value the worker process holds. Provide a minimal,
            // secret-free default environment instead. Callers that need extra
            // variables must request them explicitly via `env_rules`.
            command.arg("--env=PATH=/usr/local/bin:/usr/bin:/bin");
        } else {
            for rule in &run_options.env_rules {
                add_env_rule_args(&mut command, rule);
            }
        }
        for rule in &run_options.directory_rules {
            add_directory_rule_args(&mut command, rule);
        }

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

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|err| {
            SandboxError::Execution(format!("failed to spawn isolate --run: {err}"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            SandboxError::Execution("failed to capture isolate stdout".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            SandboxError::Execution("failed to capture isolate stderr".to_string())
        })?;
        let stdout_task = tokio::spawn(read_capped_child_pipe(stdout));
        let stderr_task = tokio::spawn(read_capped_child_pipe(stderr));
        let status = child.wait().await.map_err(|err| {
            SandboxError::Execution(format!("failed to wait for isolate --run: {err}"))
        })?;
        let output_stdout = join_pipe_capture(stdout_task, "stdout").await?;
        let output_stderr = join_pipe_capture(stderr_task, "stderr").await?;

        match status.code() {
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
                    if is_fifo(&resolved) {
                        String::new()
                    } else {
                        match read_text_preview(&resolved).await {
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
                    text_preview_from_bytes(output_stdout, false)
                };
                result.stderr = if let Some(stderr_path) = &run_options.stderr {
                    let resolved = box_dir.join(stderr_path);
                    if is_fifo(&resolved) {
                        String::new()
                    } else {
                        match read_text_preview(&resolved).await {
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
                    text_preview_from_bytes(output_stderr, false)
                };
                Ok(result)
            }
            _ => {
                let _ = fs::remove_file(&meta_path).await;
                Err(SandboxError::Unknown(format!(
                    "isolate internal error: {}",
                    String::from_utf8_lossy(&output_stderr).trim()
                )))
            }
        }
    }
}

#[cfg(test)]
mod text_preview_tests {
    use super::text_preview_from_bytes;

    #[test]
    fn strips_nul_bytes_from_captured_text() {
        // NUL in sandbox stdout/stderr/meta must not survive to a Postgres TEXT column.
        let out = text_preview_from_bytes(b"abc\0def\0\0xyz".to_vec(), false);
        assert_eq!(out, "abc\u{FFFD}def\u{FFFD}\u{FFFD}xyz");
        assert!(!out.contains('\0'));
    }

    #[test]
    fn clean_text_passes_through_unchanged() {
        let out = text_preview_from_bytes(b"999 998 997\n".to_vec(), false);
        assert_eq!(out, "999 998 997\n");
    }

    #[test]
    fn nul_strip_applies_before_truncation_marker() {
        let out = text_preview_from_bytes(b"a\0b".to_vec(), true);
        assert_eq!(out, "a\u{FFFD}b\n... (truncated)");
    }
}
