use super::cache_leader::{CacheLeaderElector, LeaderRole, NoopCacheLeaderElector};
use super::file_cacher::FileCacher;
use super::models::*;
use super::sandbox::{
    DirectoryOptions, DirectoryRule, ExecutionResult, RunOptions, SandboxManager,
};
use super::task_cache::{TaskCachePutOutcome, TaskCacheStore, compute_cache_key};
use anyhow::{Context, Result, anyhow};
use futures::future::join_all;
use opentelemetry::KeyValue;
use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tracing::{Instrument, debug, error, info, instrument, warn};

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let mut resolved = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            _ => {
                return Err(anyhow!(
                    "Unsafe path component in '{}': {:?}",
                    relative,
                    component
                ));
            }
        }
    }
    Ok(resolved)
}

static NEXT_BOX_ID: AtomicU32 = AtomicU32::new(0);

struct StepMetricRecord {
    start: std::time::Instant,
    step_kind: &'static str,
    outcome: &'static str,
    sandbox_status: String,
    exit_kind: &'static str,
    killed: bool,
    cg_oom_killed: bool,
}

fn validate_pipe_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("Pipe/channel name cannot be empty"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
        return Err(anyhow!(
            "Pipe/channel name contains unsafe characters: '{name}'"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(anyhow!(
            "Pipe/channel name must be alphanumeric, underscore, or hyphen: '{name}'"
        ));
    }
    Ok(())
}

fn sandbox_status_label(result: &ExecutionResult) -> String {
    if result.status.trim().is_empty() {
        "UNKNOWN".to_string()
    } else {
        result.status.clone()
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        ExecutionResult, cached_output_materialization_path_kind, materialization_path_kind,
        sandbox_exit_kind, sandbox_status_label, step_kind,
    };
    use crate::models::operation::models::{IOConfig, RunOptions, Step, StepKind};
    use broccoli_server_sdk::types::SessionFile;

    #[test]
    fn classifies_sandbox_exit_kind_for_metrics() {
        let mut result = ExecutionResult {
            exit_code: Some(0),
            ..ExecutionResult::default()
        };
        assert_eq!(sandbox_exit_kind(&result), "zero");

        result.exit_code = Some(17);
        assert_eq!(sandbox_exit_kind(&result), "nonzero");

        result.signal = Some(9);
        assert_eq!(sandbox_exit_kind(&result), "signal");

        result.signal = None;
        result.exit_code = None;
        assert_eq!(sandbox_exit_kind(&result), "none");
    }

    #[test]
    fn normalizes_empty_sandbox_status_for_metrics() {
        let mut result = ExecutionResult {
            status: "  ".to_string(),
            ..ExecutionResult::default()
        };
        assert_eq!(sandbox_status_label(&result), "UNKNOWN");

        result.status = "TO".to_string();
        assert_eq!(sandbox_status_label(&result), "TO");
    }

    #[test]
    fn classifies_materialization_path_kind_for_metrics() {
        assert_eq!(
            materialization_path_kind(
                "main.cpp",
                &SessionFile::Content {
                    content: "int main() {}".to_string(),
                }
            ),
            "source"
        );
        assert_eq!(
            materialization_path_kind(
                "input.txt",
                &SessionFile::Blob {
                    hash: "a".repeat(64),
                }
            ),
            "input"
        );
        assert_eq!(cached_output_materialization_path_kind(), "output");
    }

    #[test]
    fn classifies_step_kind_only_from_canonical_step_ids() {
        let mut step = Step {
            id: "custom-build".to_string(),
            kind: StepKind::Compile,
            env_ref: "sandbox".to_string(),
            argv: vec!["custom-build-tool".to_string()],
            conf: RunOptions::default(),
            io: IOConfig::default(),
            collect: vec![],
            depends_on: vec![],
            cache: None,
        };
        assert_eq!(step_kind(&step), "compile");

        step.id = "run-main".to_string();
        step.kind = StepKind::Testcase;
        step.argv = vec!["./solution".to_string()];
        assert_eq!(step_kind(&step), "testcase");

        step.id = "build-main".to_string();
        step.kind = StepKind::Generic;
        step.argv = vec!["g++".to_string(), "main.cpp".to_string()];
        assert_eq!(step_kind(&step), "step");

        step.id = "build-checker".to_string();
        step.kind = StepKind::CheckerCompile;
        step.argv = vec!["custom-build-tool".to_string()];
        assert_eq!(step_kind(&step), "checker_compile");

        step.id = "run-custom-checker".to_string();
        step.kind = StepKind::Checker;
        step.argv = vec!["custom-checker".to_string()];
        assert_eq!(step_kind(&step), "checker");
    }
}

fn sandbox_exit_kind(result: &ExecutionResult) -> &'static str {
    if result.signal.is_some() {
        "signal"
    } else if result.exit_code == Some(0) {
        "zero"
    } else if result.exit_code.is_some() {
        "nonzero"
    } else {
        "none"
    }
}

fn allocate_box_id() -> String {
    let id = NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed) % 1000;
    id.to_string()
}

struct EnvironmentList {
    id: String,
    box_id: String,
    working_dir: PathBuf,
}

impl EnvironmentList {
    fn new(id: String, box_id: String, working_dir: PathBuf) -> Self {
        Self {
            id,
            box_id,
            working_dir,
        }
    }
}

fn session_file_kind(source: &SessionFile) -> &'static str {
    match source {
        SessionFile::Path { .. } => "path",
        SessionFile::Content { .. } => "content",
        SessionFile::Blob { .. } => "blob",
    }
}

fn materialization_path_kind(target_path: &str, source: &SessionFile) -> &'static str {
    match source {
        SessionFile::Blob { .. } => "input",
        SessionFile::Content { .. } | SessionFile::Path { .. } => {
            let lower = target_path.to_ascii_lowercase();
            if lower.contains("input") || lower.ends_with(".in") {
                "input"
            } else {
                "source"
            }
        }
    }
}

fn cached_output_materialization_path_kind() -> &'static str {
    "output"
}

fn step_kind(step: &Step) -> &'static str {
    match step.kind {
        StepKind::Compile => "compile",
        StepKind::Testcase => "testcase",
        StepKind::CheckerCompile => "checker_compile",
        StepKind::Checker => "checker",
        StepKind::Generic => "step",
    }
}

pub struct OperationHandler {
    sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
    file_cacher: Box<dyn FileCacher>,
    task_cache: Arc<dyn TaskCacheStore>,
    cache_leader: Arc<dyn CacheLeaderElector>,
    follower_poll_interval: Duration,
    follower_max_wait: Duration,
    toolchain_fingerprint: String,
    metrics: common::metrics::Metrics,
}

impl OperationHandler {
    pub fn new(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
        file_cacher: Box<dyn FileCacher>,
        task_cache: Box<dyn TaskCacheStore>,
        toolchain_fingerprint: String,
        metrics: common::metrics::Metrics,
    ) -> Self {
        Self::with_cache_leader(
            sandbox_manager,
            file_cacher,
            Arc::from(task_cache),
            Arc::new(NoopCacheLeaderElector),
            Duration::from_millis(250),
            Duration::from_secs(30),
            toolchain_fingerprint,
            metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_cache_leader(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
        file_cacher: Box<dyn FileCacher>,
        task_cache: Arc<dyn TaskCacheStore>,
        cache_leader: Arc<dyn CacheLeaderElector>,
        follower_poll_interval: Duration,
        follower_max_wait: Duration,
        toolchain_fingerprint: String,
        metrics: common::metrics::Metrics,
    ) -> Self {
        Self {
            sandbox_manager,
            file_cacher,
            task_cache,
            cache_leader,
            follower_poll_interval,
            follower_max_wait,
            toolchain_fingerprint,
            metrics,
        }
    }

    #[instrument(skip(self, operation))]
    pub async fn execute(&self, operation: &OperationTask) -> Result<OperationResult> {
        info!(
            "Starting operation execution with {} environments and {} tasks",
            operation.environments.len(),
            operation.tasks.len()
        );

        let mut environments = HashMap::new();
        for env_config in operation.environments.iter() {
            let box_id = allocate_box_id();
            debug!(env_id = %env_config.id, box_id = %box_id, "Initializing environment");

            let working_dir = match self.create_sandbox(&box_id).await {
                Ok(dir) => dir,
                Err(e) => {
                    self.cleanup_environments(&environments).await.ok();
                    return Err(e.context("Failed to create sandbox"));
                }
            };

            if let Err(e) = self
                .load_environment_files(&working_dir, &env_config.files_in)
                .await
            {
                if let Err(cleanup_err) = self.sandbox_manager.remove_sandbox(&box_id).await {
                    error!(box_id = %box_id, error = %cleanup_err, "Failed to clean up sandbox after file loading failure");
                }
                self.cleanup_environments(&environments).await?;
                return Err(e.context("Failed to load environment files"));
            }

            environments.insert(
                env_config.id.clone(),
                EnvironmentList::new(env_config.id.clone(), box_id, working_dir),
            );
        }

        let channel_names: HashSet<String> =
            operation.channels.iter().map(|c| c.name.clone()).collect();
        for name in &channel_names {
            if let Err(e) = validate_pipe_name(name) {
                self.cleanup_environments(&environments).await.ok();
                return Err(e);
            }
        }
        let shared_channels_dir = if !channel_names.is_empty() {
            let dir = std::env::temp_dir().join(format!(
                "broccoli-channels-{}-{}-{}",
                std::process::id(),
                NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            ));
            if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                self.cleanup_environments(&environments).await.ok();
                return Err(
                    anyhow::Error::new(e).context("Failed to create shared channels directory")
                );
            }

            for channel in &operation.channels {
                let fifo_path = dir.join(&channel.name);
                let output = tokio::process::Command::new("mkfifo")
                    .arg(&fifo_path)
                    .output()
                    .await
                    .context("Failed to execute mkfifo for channel")?;
                if !output.status.success() {
                    if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
                        warn!(error = %e, "Failed to clean up shared channels directory after mkfifo failure");
                    }
                    self.cleanup_environments(&environments).await.ok();
                    return Err(anyhow!(
                        "mkfifo failed for channel {}: {}",
                        channel.name,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(e) =
                        std::fs::set_permissions(&fifo_path, std::fs::Permissions::from_mode(0o666))
                    {
                        warn!(path = %fifo_path.display(), error = %e, "Failed to set permissions on channel FIFO");
                    }
                }
                debug!(channel = %channel.name, "Created shared channel FIFO");
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) =
                    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777))
                {
                    warn!(path = %dir.display(), error = %e, "Failed to set permissions on channels directory");
                }
            }

            Some(dir)
        } else {
            None
        };

        let execution_layers = match self.get_execution_order(operation) {
            Ok(layers) => layers,
            Err(e) => {
                if let Some(ref dir) = shared_channels_dir {
                    let _ = tokio::fs::remove_dir_all(dir).await;
                }
                self.cleanup_environments(&environments).await.ok();
                return Err(e);
            }
        };
        debug!(layers = ?execution_layers, "Task execution layers determined");

        let mut task_results = HashMap::new();
        let mut global_success = true;

        for layer in execution_layers {
            let mut futures = Vec::new();
            for task_id in &layer {
                let task = operation
                    .tasks
                    .iter()
                    .find(|t| t.id == *task_id)
                    .ok_or_else(|| {
                        anyhow!(
                            "Task '{}' not found — dependency graph inconsistency",
                            task_id
                        )
                    })?;

                let deps_ok = task.depends_on.iter().all(|dep_id| {
                    task_results
                        .get(dep_id)
                        .map(|r: &TaskExecutionResult| r.success)
                        .unwrap_or(false)
                });

                futures.push(self.execute_step_with_deps(
                    task,
                    &environments,
                    deps_ok,
                    shared_channels_dir.as_deref(),
                    &channel_names,
                ));
            }

            let results = join_all(futures).await;
            for result in results {
                if !result.success {
                    global_success = false;
                }
                task_results.insert(result.task_id.clone(), result);
            }
        }

        if let Some(dir) = &shared_channels_dir
            && let Err(e) = tokio::fs::remove_dir_all(dir).await
        {
            error!(error = %e, "Failed to clean up shared channels directory");
        }
        self.cleanup_environments(&environments).await.ok();

        info!(
            success = global_success,
            tasks_count = task_results.len(),
            "Operation execution completed"
        );

        Ok(OperationResult {
            success: global_success,
            task_results,
            error: None,
        })
    }

    async fn create_sandbox(&self, box_id: &str) -> Result<PathBuf> {
        let sandbox_path = self
            .sandbox_manager
            .create_sandbox(Some(box_id))
            .await
            .context("Sandbox creation failed")?;
        Ok(sandbox_path)
    }

    #[instrument(skip(self, files), fields(file_count = files.len()))]
    async fn load_environment_files(
        &self,
        working_dir: &Path,
        files: &[(String, SessionFile)],
    ) -> Result<()> {
        for (target_path, source) in files {
            let start = std::time::Instant::now();
            let source_kind = session_file_kind(source);
            let path_kind = materialization_path_kind(target_path, source);
            let dest = safe_join(working_dir, target_path)?;
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create parent directory")?;
            }
            match source {
                SessionFile::Path { path: src } => {
                    let bytes = tokio::fs::copy(src, &dest).await.with_context(|| {
                        format!("Failed to copy file {} -> {}", src, dest.display())
                    })?;
                    self.record_file_materialization(
                        start,
                        source_kind,
                        path_kind,
                        "success",
                        bytes,
                    );
                }
                SessionFile::Content { content } => {
                    tokio::fs::write(&dest, content).await.with_context(|| {
                        format!("Failed to write content to {}", dest.display())
                    })?;
                    self.record_file_materialization(
                        start,
                        source_kind,
                        path_kind,
                        "success",
                        content.len() as u64,
                    );
                }
                SessionFile::Blob { hash: content_hash } => {
                    self.file_cacher
                        .fetch_to_path(content_hash, &dest)
                        .await
                        .map_err(|e| anyhow!("Failed to fetch blob {}: {}", content_hash, e))?;
                    let bytes = tokio::fs::metadata(&dest)
                        .await
                        .map(|m| m.len())
                        .unwrap_or(0);
                    self.record_file_materialization(
                        start,
                        source_kind,
                        path_kind,
                        "success",
                        bytes,
                    );
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Err(e) =
                            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700))
                        {
                            warn!(path = %dest.display(), error = %e, "Failed to set permissions on blob file");
                        }
                    }
                }
            }
            debug!(target = %target_path, dest = %dest.display(), "Loaded environment file");
        }
        Ok(())
    }

    fn record_file_materialization(
        &self,
        start: std::time::Instant,
        source_kind: &'static str,
        path_kind: &'static str,
        outcome: &'static str,
        bytes: u64,
    ) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("source.kind", source_kind),
            KeyValue::new("path_kind", path_kind),
            KeyValue::new("outcome", outcome),
        ];
        self.metrics
            .operation_file_materialization_duration
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics
            .file_materialization_copy_seconds
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics
            .operation_file_materialization_bytes
            .add(bytes, &attrs);
    }

    fn build_dependency_graph(&self, operation: &OperationTask) -> HashMap<String, Vec<String>> {
        let mut graph = HashMap::new();

        for task in &operation.tasks {
            graph.insert(task.id.clone(), task.depends_on.clone());
        }

        graph
    }

    fn get_execution_order(&self, operation: &OperationTask) -> Result<Vec<Vec<String>>> {
        let graph = self.build_dependency_graph(operation);
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();

        for task in &operation.tasks {
            in_degree.insert(task.id.clone(), 0);
            adj_list.insert(task.id.clone(), Vec::new());
        }

        for (task_id, deps) in &graph {
            let mut degree = 0;
            for dep in deps {
                if !operation.tasks.iter().any(|t| &t.id == dep) {
                    return Err(anyhow!("Dependency task not found: {}", dep));
                }
                degree += 1;
                if let Some(adj) = adj_list.get_mut(dep) {
                    adj.push(task_id.clone());
                }
            }
            in_degree.insert(task_id.clone(), degree);
        }

        let mut current_layer: Vec<_> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut layers = Vec::new();
        let mut total = 0;

        while !current_layer.is_empty() {
            let mut next_layer = Vec::new();

            for task_id in &current_layer {
                if let Some(neighbors) = adj_list.get(task_id) {
                    for neighbor in neighbors {
                        if let Some(degree) = in_degree.get_mut(neighbor) {
                            *degree -= 1;
                            if *degree == 0 {
                                next_layer.push(neighbor.clone());
                            }
                        }
                    }
                }
            }

            total += current_layer.len();
            layers.push(current_layer);
            current_layer = next_layer;
        }

        if total != operation.tasks.len() {
            return Err(anyhow!(
                "Circular dependency detected or task missing in dependency graph"
            ));
        }

        Ok(layers)
    }

    async fn execute_step_with_deps(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        deps_ok: bool,
        shared_channels_dir: Option<&Path>,
        channel_names: &HashSet<String>,
    ) -> TaskExecutionResult {
        let start = std::time::Instant::now();

        if !deps_ok {
            warn!(task_id = %step.id, "Skipping task due to dependency failure");
            self.record_step_metrics(StepMetricRecord {
                start,
                step_kind: step_kind(step),
                outcome: "skipped",
                sandbox_status: "dependency_failed".to_string(),
                exit_kind: "none",
                killed: false,
                cg_oom_killed: false,
            });
            return TaskExecutionResult {
                task_id: step.id.clone(),
                success: false,
                sandbox_result: ExecutionResult::default(),
                collected_outputs: HashMap::new(),
            };
        }

        // Cached cache_key (computed at most once per process_step call) so
        // try_cache_hit, leader-acquire and store_in_cache all see the same key.
        let mut cache_key_cache: Option<String> = None;

        if let Some(cache_spec) = &step.cache
            && let Some(cached) = self
                .try_cache_hit(step, environments, cache_spec, &mut cache_key_cache)
                .await
        {
            self.record_step_metrics(StepMetricRecord {
                start,
                step_kind: step_kind(step),
                outcome: "cache_hit",
                sandbox_status: "cache_hit".to_string(),
                exit_kind: "none",
                killed: false,
                cg_oom_killed: false,
            });
            return cached;
        }

        // Leader-election around the cache miss: if another worker is already
        // computing this same key, become a follower and poll task_cache until
        // they store the result (or our max_wait elapses).
        //
        // The `_lease` binding keeps the leader's Redis lock alive (with
        // heartbeat) through `execute_step` + `store_in_cache`. Drop happens
        // at the end of this function and fires a CAS-release.
        let _lease;
        if let Some(cache_spec) = &step.cache {
            // None here means we couldn't compute a key (e.g. env missing or
            // input file read error); fall through with an empty key so we
            // skip leader-election but still attempt execution.
            let cache_key = self
                .ensure_cache_key(step, environments, cache_spec, &mut cache_key_cache)
                .await
                .unwrap_or_default();

            if !cache_key.is_empty() {
                match self.cache_leader.acquire(&cache_key).await {
                    Ok(LeaderRole::Leader(lease)) => {
                        _lease = Some(lease);
                        debug!(step_id = %step.id, cache_key = %cache_key, "cache-leader: leading");
                    }
                    Ok(LeaderRole::Follower) => {
                        _lease = None;
                        debug!(step_id = %step.id, cache_key = %cache_key, "cache-leader: following — polling task_cache");
                        if let Some(restored) = self
                            .follower_poll_loop(step, environments, &cache_key)
                            .await
                        {
                            self.record_step_metrics(StepMetricRecord {
                                start,
                                step_kind: step_kind(step),
                                outcome: "cache_hit",
                                sandbox_status: "cache_follower_hit".to_string(),
                                exit_kind: "none",
                                killed: false,
                                cg_oom_killed: false,
                            });
                            return restored;
                        }
                        warn!(step_id = %step.id, cache_key = %cache_key, "cache-leader: follower timed out, falling back to execution");
                    }
                    Err(e) => {
                        _lease = None;
                        warn!(step_id = %step.id, cache_key = %cache_key, error = %e, "cache-leader: acquire failed, executing normally");
                    }
                }
            } else {
                _lease = None;
            }
        } else {
            _lease = None;
        }

        let result = match self
            .execute_step(step, environments, shared_channels_dir, channel_names)
            .instrument(tracing::info_span!(
                "operation_step",
                step_id = %step.id,
                step_kind = %step_kind(step),
                env_ref = %step.env_ref,
            ))
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!(task_id = %step.id, error = %e, "Task execution error");
                self.record_step_metrics(StepMetricRecord {
                    start,
                    step_kind: step_kind(step),
                    outcome: "failure",
                    sandbox_status: "execution_error".to_string(),
                    exit_kind: "none",
                    killed: false,
                    cg_oom_killed: false,
                });
                return TaskExecutionResult {
                    task_id: step.id.clone(),
                    success: false,
                    sandbox_result: ExecutionResult::default(),
                    collected_outputs: HashMap::new(),
                };
            }
        };

        if result.success
            && let Some(cache_spec) = &step.cache
        {
            self.store_in_cache(
                step,
                environments,
                cache_spec,
                &result.collected_outputs,
                &mut cache_key_cache,
            )
            .await;
        }
        drop(_lease);

        self.record_sandbox_metrics(&result.sandbox_result, result.success);
        self.record_step_metrics(StepMetricRecord {
            start,
            step_kind: step_kind(step),
            outcome: if result.success { "success" } else { "failure" },
            sandbox_status: sandbox_status_label(&result.sandbox_result),
            exit_kind: sandbox_exit_kind(&result.sandbox_result),
            killed: result.sandbox_result.killed,
            cg_oom_killed: result.sandbox_result.cg_oom_killed,
        });
        result
    }

    fn record_step_metrics(&self, record: StepMetricRecord) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("step.kind", record.step_kind),
            KeyValue::new("outcome", record.outcome),
            KeyValue::new("sandbox.status", record.sandbox_status),
            KeyValue::new("exit.kind", record.exit_kind),
            KeyValue::new("killed", record.killed.to_string()),
            KeyValue::new("cg_oom_killed", record.cg_oom_killed.to_string()),
        ];
        self.metrics
            .step_duration
            .record(record.start.elapsed().as_secs_f64(), &attrs);
        self.metrics.step_results_total.add(1, &attrs);
    }

    fn record_sandbox_metrics(&self, result: &ExecutionResult, success: bool) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("outcome", if success { "success" } else { "failure" }),
            KeyValue::new("sandbox.status", sandbox_status_label(result)),
            KeyValue::new("exit.kind", sandbox_exit_kind(result)),
            KeyValue::new("killed", result.killed.to_string()),
            KeyValue::new("cg_oom_killed", result.cg_oom_killed.to_string()),
        ];
        self.metrics.sandbox_executions_total.add(1, &attrs);
        self.metrics
            .sandbox_time_used
            .record(result.time_used.max(0.0), &attrs);
        self.metrics
            .sandbox_wall_time_used
            .record(result.wall_time_used.max(0.0), &attrs);
        if let Some(memory_used) = result.memory_used {
            self.metrics
                .sandbox_memory_used
                .record(memory_used as f64, &attrs);
        }
    }

    async fn ensure_cache_key(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_spec: &StepCacheConfig,
        cache_key_cache: &mut Option<String>,
    ) -> Option<String> {
        if let Some(k) = cache_key_cache {
            return Some(k.clone());
        }
        let env = environments.get(&step.env_ref)?;
        match self
            .build_cache_key(&env.working_dir, &step.argv, &cache_spec.key_inputs)
            .await
        {
            Ok(key) => {
                *cache_key_cache = Some(key.clone());
                Some(key)
            }
            Err(e) => {
                debug!(step_id = %step.id, error = %e, "Failed to compute cache key");
                None
            }
        }
    }

    /// Restore cached outputs from `task_cache` outputs map into the env's
    /// working directory. Returns `Some(TaskExecutionResult)` on success, or
    /// `None` if any file restore failed (in which case the caller should
    /// fall through to normal execution).
    async fn restore_cached_outputs(
        &self,
        step: &Step,
        env: &EnvironmentList,
        cached_outputs: HashMap<String, String>,
        cache_key: &str,
    ) -> Option<TaskExecutionResult> {
        for (filename, content_hash) in &cached_outputs {
            let dest = match safe_join(&env.working_dir, filename) {
                Ok(p) => p,
                Err(e) => {
                    warn!(step_id = %step.id, error = %e, "Unsafe cached output path");
                    return None;
                }
            };
            if let Some(parent) = dest.parent()
                && let Err(e) = tokio::fs::create_dir_all(parent).await
            {
                warn!(step_id = %step.id, error = %e, "Failed to create parent dir for cached output");
                return None;
            }
            let materialization_start = std::time::Instant::now();
            if let Err(e) = self.file_cacher.fetch_to_path(content_hash, &dest).await {
                warn!(
                    step_id = %step.id,
                    file = %filename,
                    error = %e,
                    "Failed to restore cached output, falling back to execution"
                );
                return None;
            }
            self.metrics.file_materialization_copy_seconds.record(
                materialization_start.elapsed().as_secs_f64(),
                &[
                    KeyValue::new("source.kind", "blob"),
                    KeyValue::new("path_kind", cached_output_materialization_path_kind()),
                    KeyValue::new("outcome", "success"),
                ],
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) =
                    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700))
                {
                    warn!(path = %dest.display(), error = %e, "Failed to set permissions on cached output file");
                }
            }
        }

        info!(step_id = %step.id, cache_key = %cache_key, "Cache hit — skipped execution");
        Some(TaskExecutionResult {
            task_id: step.id.clone(),
            success: true,
            sandbox_result: ExecutionResult::default(),
            collected_outputs: cached_outputs,
        })
    }

    async fn try_cache_hit(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_spec: &StepCacheConfig,
        cache_key_cache: &mut Option<String>,
    ) -> Option<TaskExecutionResult> {
        let env = environments.get(&step.env_ref)?;
        let cache_key = self
            .ensure_cache_key(step, environments, cache_spec, cache_key_cache)
            .await?;

        let cache_start = std::time::Instant::now();
        let cached_outputs = match self.task_cache.get(&cache_key).await {
            Ok(Some(outputs)) => {
                self.record_task_cache_metric(cache_start, "get", "hit");
                outputs
            }
            Ok(None) => {
                self.record_task_cache_metric(cache_start, "get", "miss");
                debug!(step_id = %step.id, cache_key = %cache_key, "Cache miss");
                return None;
            }
            Err(e) => {
                self.record_task_cache_metric(cache_start, "get", "error");
                warn!(step_id = %step.id, error = %e, "Cache lookup failed, executing normally");
                return None;
            }
        };

        self.restore_cached_outputs(step, env, cached_outputs, &cache_key)
            .await
    }

    /// As a follower, repeatedly poll task_cache until we see the leader's
    /// stored entry. Returns `Some(TaskExecutionResult)` if we successfully
    /// restored the cached outputs; `None` on timeout or any failure (caller
    /// then falls back to executing the step itself).
    async fn follower_poll_loop(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_key: &str,
    ) -> Option<TaskExecutionResult> {
        let env = environments.get(&step.env_ref)?;
        let deadline = tokio::time::Instant::now() + self.follower_max_wait;
        loop {
            tokio::time::sleep(self.follower_poll_interval).await;
            let cache_start = std::time::Instant::now();
            match self.task_cache.get(cache_key).await {
                Ok(Some(outputs)) => {
                    self.record_task_cache_metric(cache_start, "get", "follower_hit");
                    return self
                        .restore_cached_outputs(step, env, outputs, cache_key)
                        .await;
                }
                Ok(None) => {
                    self.record_task_cache_metric(cache_start, "get", "follower_miss");
                }
                Err(e) => {
                    self.record_task_cache_metric(cache_start, "get", "follower_error");
                    warn!(step_id = %step.id, error = %e, "Follower cache poll failed");
                    return None;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
        }
    }

    async fn store_in_cache(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_spec: &StepCacheConfig,
        existing_hashes: &HashMap<String, String>,
        cache_key_cache: &mut Option<String>,
    ) {
        let env = match environments.get(&step.env_ref) {
            Some(e) => e,
            None => return,
        };

        let cache_key = match self
            .ensure_cache_key(step, environments, cache_spec, cache_key_cache)
            .await
        {
            Some(k) => k,
            None => {
                warn!(step_id = %step.id, "Failed to compute cache key for storage");
                return;
            }
        };

        let mut output_hashes = HashMap::new();
        for filename in &cache_spec.outputs {
            if let Some(hash) = existing_hashes.get(filename) {
                output_hashes.insert(filename.clone(), hash.clone());
                continue;
            }
            let src = match safe_join(&env.working_dir, filename) {
                Ok(p) => p,
                Err(e) => {
                    warn!(step_id = %step.id, error = %e, "Unsafe cache output path");
                    return;
                }
            };
            if !tokio::fs::try_exists(&src).await.unwrap_or(false) {
                debug!(step_id = %step.id, file = %filename, "Cache output file not found, skipping cache store");
                return;
            }
            match self.file_cacher.upload_from_path(&src).await {
                Ok(hash) => {
                    output_hashes.insert(filename.clone(), hash);
                }
                Err(e) => {
                    warn!(step_id = %step.id, file = %filename, error = %e, "Failed to upload output for cache");
                    return;
                }
            }
        }

        let cache_start = std::time::Instant::now();
        match self.task_cache.put(&cache_key, output_hashes).await {
            Ok(TaskCachePutOutcome::Inserted) => {
                self.record_task_cache_metric(cache_start, "put", "success");
                info!(step_id = %step.id, cache_key = %cache_key, "Stored step outputs in task cache");
            }
            Ok(TaskCachePutOutcome::AlreadyExists) => {
                self.record_task_cache_metric(cache_start, "put", "already_exists");
                self.metrics
                    .worker_compile_cache_redundancy_total
                    .add(1, &[KeyValue::new("step.kind", step_kind(step))]);
                info!(step_id = %step.id, cache_key = %cache_key, "Skipped redundant compile-cache store");
            }
            Err(e) => {
                self.record_task_cache_metric(cache_start, "put", "error");
                warn!(step_id = %step.id, error = %e, "Failed to store task cache entry");
            }
        }
    }

    fn record_task_cache_metric(
        &self,
        start: std::time::Instant,
        operation: &'static str,
        outcome: &'static str,
    ) {
        use opentelemetry::KeyValue;

        let attrs = [
            KeyValue::new("operation", operation),
            KeyValue::new("outcome", outcome),
        ];
        self.metrics
            .task_cache_operation_duration
            .record(start.elapsed().as_secs_f64(), &attrs);
        self.metrics.task_cache_operations_total.add(1, &attrs);
    }

    async fn build_cache_key(
        &self,
        working_dir: &Path,
        argv: &[String],
        key_inputs: &[String],
    ) -> Result<String> {
        let mut input_files = Vec::new();
        for filename in key_inputs {
            let path = safe_join(working_dir, filename)?;
            let content = tokio::fs::read(&path)
                .await
                .with_context(|| format!("Failed to read cache key input: {}", path.display()))?;
            input_files.push((filename.clone(), content));
        }
        Ok(compute_cache_key(
            &self.toolchain_fingerprint,
            argv,
            &input_files,
        ))
    }

    #[instrument(skip(self, step, environments, shared_channels_dir, channel_names))]
    async fn execute_step(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        shared_channels_dir: Option<&Path>,
        channel_names: &HashSet<String>,
    ) -> Result<TaskExecutionResult> {
        debug!(step_id = %step.id, "Executing step");

        let env = environments
            .get(&step.env_ref)
            .ok_or_else(|| anyhow!("Environment not found: {}", step.env_ref))?;

        let (stdin_path, stdout_path, stderr_path) = self
            .prepare_io(
                &env.working_dir,
                &step.io,
                shared_channels_dir,
                channel_names,
            )
            .await?;

        let mut directory_rules = step.conf.directory_rules.clone();
        if let Some(channels_dir) = shared_channels_dir {
            directory_rules.push(DirectoryRule {
                inside_path: PathBuf::from("/channels"),
                outside_path: Some(channels_dir.to_path_buf()),
                options: DirectoryOptions {
                    read_write: true,
                    ..Default::default()
                },
            });
        }

        let run_opts = RunOptions {
            resource_limits: step.conf.resource_limits.clone(),
            wait: true,
            as_uid: None,
            as_gid: None,
            stdin: stdin_path,
            stdout: stdout_path,
            stderr: stderr_path,
            env_rules: step.conf.env_rules.clone(),
            directory_rules,
        };

        let exec_result = self
            .sandbox_manager
            .execute(&env.box_id, step.argv.clone(), &run_opts)
            .await
            .map_err(|e| {
                error!(step_id = %step.id, error = %e, "Step execution failed");
                anyhow!("Sandbox execution failed: {}", e)
            })?;

        let success = exec_result.exit_code == Some(0);
        let collected_outputs = self.collect_output(&env.working_dir, &step.collect).await?;

        Ok(TaskExecutionResult {
            task_id: step.id.clone(),
            success,
            sandbox_result: exec_result,
            collected_outputs,
        })
    }

    async fn prepare_io(
        &self,
        working_dir: &Path,
        io_config: &IOConfig,
        shared_channels_dir: Option<&Path>,
        channel_names: &HashSet<String>,
    ) -> Result<(Option<PathBuf>, Option<PathBuf>, Option<PathBuf>)> {
        let stdin = self
            .prepare_io_target(
                working_dir,
                &io_config.stdin,
                shared_channels_dir,
                channel_names,
            )
            .await?;
        let stdout = self
            .prepare_io_target(
                working_dir,
                &io_config.stdout,
                shared_channels_dir,
                channel_names,
            )
            .await?;
        let stderr = self
            .prepare_io_target(
                working_dir,
                &io_config.stderr,
                shared_channels_dir,
                channel_names,
            )
            .await?;

        Ok((stdin, stdout, stderr))
    }

    async fn prepare_io_target(
        &self,
        working_dir: &Path,
        target: &IOTarget,
        shared_channels_dir: Option<&Path>,
        channel_names: &HashSet<String>,
    ) -> Result<Option<PathBuf>> {
        match target {
            IOTarget::Null | IOTarget::Inherit => Ok(None),
            IOTarget::File { path } => {
                let p = Path::new(path);
                Ok(Some(p.to_path_buf()))
            }
            IOTarget::Pipe { name } => {
                validate_pipe_name(name)?;

                if channel_names.contains(name) {
                    shared_channels_dir.ok_or_else(|| {
                        anyhow!(
                            "Pipe '{}' references a channel but no channels directory exists",
                            name
                        )
                    })?;
                    let fifo_path = PathBuf::from("/channels").join(name);
                    return Ok(Some(fifo_path));
                }

                let pipes_dir = working_dir.join("pipes");
                tokio::fs::create_dir_all(&pipes_dir)
                    .await
                    .context("Failed to create pipes directory")?;

                let pipe_path = pipes_dir.join(name);

                if let Ok(_meta) = tokio::fs::metadata(&pipe_path).await {
                    #[cfg(unix)]
                    if !_meta.file_type().is_fifo() {
                        return Err(anyhow!(
                            "Pipe target exists but is not a FIFO: {}",
                            pipe_path.display()
                        ));
                    }
                    return Ok(Some(pipe_path));
                }

                let output = tokio::process::Command::new("mkfifo")
                    .arg(&pipe_path)
                    .output()
                    .await
                    .context("Failed to execute mkfifo")?;

                if !output.status.success() {
                    if let Ok(meta) = tokio::fs::metadata(&pipe_path).await
                        && meta.file_type().is_fifo()
                    {
                        return Ok(Some(pipe_path));
                    }
                    return Err(anyhow!(
                        "mkfifo failed for {}: {}",
                        pipe_path.display(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }

                Ok(Some(pipe_path))
            }
        }
    }

    async fn collect_output(
        &self,
        working_dir: &Path,
        collect_files: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut collected = HashMap::new();
        for file_path in collect_files {
            let src = safe_join(working_dir, file_path)?;
            if tokio::fs::try_exists(&src).await.unwrap_or(false) {
                let hash = self.file_cacher.upload_from_path(&src).await.map_err(|e| {
                    anyhow!("Failed to upload output file {}: {}", src.display(), e)
                })?;
                info!(
                    file = %file_path,
                    hash = %hash,
                    "Collected output file"
                );
                collected.insert(file_path.clone(), hash);
            } else {
                warn!(path = %src.display(), "Collect target not found, skipping");
            }
        }
        Ok(collected)
    }

    async fn cleanup_environments(
        &self,
        environments: &HashMap<String, EnvironmentList>,
    ) -> Result<()> {
        for env in environments.values() {
            if let Err(e) = self.sandbox_manager.remove_sandbox(&env.box_id).await {
                error!(env_id = %env.id, error = %e, "Failed to cleanup sandbox");
            }
        }
        Ok(())
    }
}
