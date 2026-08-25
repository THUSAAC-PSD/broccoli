use super::super::cache_leader::{CacheLeaderElector, LeaderRole, NoopCacheLeaderElector};
use super::super::file_cacher::FileCacher;
use super::super::models::*;
use super::super::sandbox::{
    DirectoryOptions, DirectoryRule, ExecutionResult, RunOptions, SandboxManager,
};
use super::super::task_cache::TaskCacheStore;
use super::box_id::{allocate_box_id, next_channel_seq};
use super::metrics::{sandbox_exit_kind, sandbox_status_label, step_kind};
use super::paths::{
    platform_tool_directory_rule, resolve_step_output_src, stage_step_output_file,
    validate_pipe_name,
};
use super::{EnvironmentList, OperationHandler, StepMetricRecord};
use anyhow::{Context, Result, anyhow};
use futures::future::{FutureExt, join_all};
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{Instrument, debug, error, info, instrument, warn};

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
            tools_dir: None,
        }
    }

    /// Set the worker-local platform tools directory (builder style so existing
    /// constructor call sites are unaffected).
    pub fn with_tools_dir(mut self, tools_dir: Option<PathBuf>) -> Self {
        self.tools_dir = tools_dir;
        self
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

            let working_dir = match self.create_sandbox(box_id.as_str()).await {
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
                if let Err(cleanup_err) = self.sandbox_manager.remove_sandbox(box_id.as_str()).await
                {
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
                next_channel_seq(),
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

        // Map each step id to its environment's working dir so a dependent step
        // can mount a prior step's captured output file directly
        // (MountSource::StepOutput), no blob round-trip.
        let step_working_dirs: HashMap<String, PathBuf> = operation
            .tasks
            .iter()
            .filter_map(|t| {
                environments
                    .get(&t.env_ref)
                    .map(|env| (t.id.clone(), env.working_dir.clone()))
            })
            .collect();

        let mut task_results = HashMap::new();
        let mut global_success = true;

        // Channel keep-alive write fds (root prevention for the fused-checker FIFO
        // deadlock). Each channel is a named FIFO the producer step writes and the
        // consumer step reads via a concurrent `open(O_RDONLY)`, which blocks in
        // the kernel until *some* writer opens the O_WRONLY end. If the producer
        // never opens it (its exec fork/spawn EAGAIN-exhausts, artifact missing),
        // the consumer blocks forever -> the whole operation hangs and the queue
        // message is orphaned. We hold an O_RDWR fd on each producer's FIFO here:
        // O_RDWR never blocks on open, so the consumer's rendezvous always
        // succeeds, and dropping the fd the instant the producer's future resolves
        // (success OR failure) delivers EOF -- a finished producer's own writes are
        // already flushed, a failed producer yields empty input. The fd MUST close
        // on producer-resolution rather than after the layer: the consumer runs
        // concurrently in the SAME layer and needs EOF to finish, so a writer held
        // past `join_all` would starve every normal solution into a deadlock. A
        // failed producer that resolves before the consumer even opens is the one
        // residual race; the worker-side isolate `--wait` hard timeout is the
        // backstop that turns it into a self-healing SystemError, never a hang.
        let mut channel_keepalives: HashMap<String, Vec<std::fs::File>> = HashMap::new();
        if let Some(ref dir) = shared_channels_dir {
            let mut producer_of: HashMap<String, String> = HashMap::new();
            for task in &operation.tasks {
                for ch in step_produced_channels(task, &channel_names) {
                    // First writer wins; a channel has a single logical producer.
                    producer_of.entry(ch).or_insert_with(|| task.id.clone());
                }
            }
            for channel in &operation.channels {
                // Only manage a keep-alive for channels with an identifiable
                // producer step. An orphan channel (consumed, never produced) keeps
                // its pre-existing behavior; there is no safe moment to close a
                // writer we can't tie to a producer's completion.
                if let Some(producer_id) = producer_of.get(&channel.name)
                    && let Some(file) = open_channel_keepalive(&dir.join(&channel.name))
                {
                    channel_keepalives
                        .entry(producer_id.clone())
                        .or_default()
                        .push(file);
                }
            }
        }

        // Run the execution layers under a panic guard so the sandbox + channels
        // cleanup below ALWAYS runs. A panicking step future - or the `?` on a
        // dependency-graph inconsistency - would otherwise unwind straight past
        // the cleanup and orphan the isolate boxes and the shared channels dir.
        let layer_outcome: std::thread::Result<Result<()>> = AssertUnwindSafe(async {
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

                    // Hand this step the keep-alive fds for any channels it
                    // produces; they drop -- delivering EOF to the concurrent
                    // consumer -- the moment its future resolves, below.
                    let produced_keepalives = channel_keepalives.remove(task_id);
                    let step_future = self.execute_step_with_deps(
                        task,
                        &environments,
                        &step_working_dirs,
                        deps_ok,
                        shared_channels_dir.as_deref(),
                        &channel_names,
                    );
                    futures.push(async move {
                        let result = step_future.await;
                        drop(produced_keepalives);
                        result
                    });
                }

                let results = join_all(futures).await;
                for result in results {
                    if !result.success {
                        global_success = false;
                    }
                    task_results.insert(result.task_id.clone(), result);
                }
            }
            Ok(())
        })
        .catch_unwind()
        .await;

        // Always clean up, whatever the layers did (completed, errored, panicked).
        if let Some(dir) = &shared_channels_dir
            && let Err(e) = tokio::fs::remove_dir_all(dir).await
        {
            error!(error = %e, "Failed to clean up shared channels directory");
        }
        self.cleanup_environments(&environments).await.ok();

        match layer_outcome {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(panic) => {
                error!("A step future panicked; sandboxes cleaned up, re-raising the panic");
                std::panic::resume_unwind(panic);
            }
        }

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
        step_working_dirs: &HashMap<String, PathBuf>,
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
            .execute_step(
                step,
                environments,
                step_working_dirs,
                shared_channels_dir,
                channel_names,
            )
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

    #[instrument(skip(self, step, environments, shared_channels_dir, channel_names))]
    async fn execute_step(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        step_working_dirs: &HashMap<String, PathBuf>,
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
        for mount in &step.mounts {
            match &mount.source {
                // StepOutput is a single-file handoff: copy it into this box's
                // working dir (isolate cannot bind-mount a file, and the
                // producer dir holds the contestant's source - never mount it).
                MountSource::StepOutput { from_step, file } => {
                    let src = resolve_step_output_src(
                        from_step,
                        file,
                        step_working_dirs,
                        &step.depends_on,
                    )
                    .await?;
                    stage_step_output_file(&src, &mount.inside_path, &env.working_dir).await?;
                }
                // PlatformTool is a read-only directory mount (the tools dir at
                // the parent of inside_path) - see platform_tool_directory_rule.
                MountSource::PlatformTool { name } => {
                    let tools_dir = self.tools_dir.as_deref().ok_or_else(|| {
                        anyhow!(
                            "step requests platform tool '{name}' but no [worker.tools] dir is configured"
                        )
                    })?;
                    directory_rules.push(platform_tool_directory_rule(
                        &mount.inside_path,
                        name,
                        tools_dir,
                    )?);
                }
            }
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
            .execute(env.box_id.as_str(), step.argv.clone(), &run_opts)
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
}

/// Channels this step *produces* (writes): any output redirect (stdout/stderr)
/// bound to a `Pipe` whose name is a declared operation channel. The consumer
/// side (a `Pipe` on `stdin`) is deliberately excluded -- only a producer's
/// write end needs a keep-alive.
fn step_produced_channels(step: &Step, channel_names: &HashSet<String>) -> Vec<String> {
    let mut produced = Vec::new();
    for target in [&step.io.stdout, &step.io.stderr] {
        if let IOTarget::Pipe { name } = target
            && channel_names.contains(name)
        {
            produced.push(name.clone());
        }
    }
    produced
}

/// Open a keep-alive fd on a channel FIFO. `O_RDWR` is deliberate: unlike
/// `O_RDONLY`/`O_WRONLY`, opening a FIFO read-write never blocks on the
/// open-rendezvous, so holding this fd guarantees a concurrent consumer's
/// `open(O_RDONLY)` returns immediately. Dropping it later delivers EOF.
/// A failure here is non-fatal: the consumer simply reverts to relying on the
/// worker hard-timeout backstop, so we warn and continue rather than abort the
/// whole operation.
fn open_channel_keepalive(fifo_path: &Path) -> Option<std::fs::File> {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(fifo_path)
    {
        Ok(file) => Some(file),
        Err(e) => {
            warn!(
                path = %fifo_path.display(),
                error = %e,
                "Failed to open channel keep-alive fd; consumer relies on the worker hard timeout"
            );
            None
        }
    }
}

#[cfg(test)]
mod channel_keepalive_tests {
    use super::*;
    use broccoli_types::types::{IOConfig, StepKind};

    fn step_with_io(id: &str, io: IOConfig) -> Step {
        Step {
            id: id.to_string(),
            kind: StepKind::default(),
            env_ref: "env".to_string(),
            argv: vec![],
            conf: RunOptions::default(),
            io,
            collect: vec![],
            depends_on: vec![],
            cache: None,
            mounts: vec![],
        }
    }

    #[test]
    fn produced_channels_counts_stdout_pipe_only() {
        let channels: HashSet<String> = ["sol_out".to_string()].into_iter().collect();

        // stdout Pipe on a declared channel -> produced.
        let producer = step_with_io(
            "exec",
            IOConfig {
                stdout: IOTarget::Pipe {
                    name: "sol_out".to_string(),
                },
                ..Default::default()
            },
        );
        assert_eq!(
            step_produced_channels(&producer, &channels),
            vec!["sol_out".to_string()]
        );

        // stdin Pipe (the consumer side) is NOT a producer.
        let consumer = step_with_io(
            "check",
            IOConfig {
                stdin: IOTarget::Pipe {
                    name: "sol_out".to_string(),
                },
                ..Default::default()
            },
        );
        assert!(step_produced_channels(&consumer, &channels).is_empty());

        // A Pipe that is not a declared channel is a box-local pipe, not a channel.
        let box_pipe = step_with_io(
            "other",
            IOConfig {
                stdout: IOTarget::Pipe {
                    name: "not_a_channel".to_string(),
                },
                ..Default::default()
            },
        );
        assert!(step_produced_channels(&box_pipe, &channels).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn keepalive_unblocks_reader_open_and_missing_path_is_none() {
        // A path that does not exist -> None (non-fatal).
        assert!(
            open_channel_keepalive(Path::new("/nonexistent/broccoli/keepalive/fifo")).is_none()
        );

        // Create a real FIFO and prove the keep-alive writer lets an O_RDONLY
        // open return immediately instead of blocking on the FIFO rendezvous.
        let dir = std::env::temp_dir().join(format!("broccoli-ka-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("sol_out");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo runs");
        assert!(status.success(), "mkfifo failed");

        let keepalive = open_channel_keepalive(&fifo).expect("keep-alive opens O_RDWR");

        let reader_path = fifo.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            // Without a writer present this open would block forever.
            let opened = std::fs::OpenOptions::new().read(true).open(&reader_path);
            let _ = tx.send(opened.is_ok());
        });

        let unblocked = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("O_RDONLY open blocked despite keep-alive writer");
        assert!(unblocked, "reader failed to open the FIFO");

        drop(keepalive);
        reader.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
