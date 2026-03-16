use super::file_cacher::FileCacher;
use super::models::*;
use super::sandbox::{
    DirectoryOptions, DirectoryRule, ExecutionResult, RunOptions, SandboxManager,
};
use super::task_cache::{TaskCacheStore, compute_cache_key};
use anyhow::{Context, Result, anyhow};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{debug, error, info, instrument, warn};

/// Resolve a relative path safely within a base directory.
/// Rejects absolute paths, `..` components, and other unsafe path elements
/// to prevent path traversal attacks from plugin-supplied file paths.
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

/// Global counter for unique isolate box IDs (0–999, wraps around).
static NEXT_BOX_ID: AtomicU32 = AtomicU32::new(0);

/// Validate that a channel or pipe name is safe for use as a filename.
/// Rejects empty names, path separators, `..` traversals, and unsafe characters.
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

fn allocate_box_id() -> String {
    let id = NEXT_BOX_ID.fetch_add(1, Ordering::Relaxed) % 1000;
    id.to_string()
}

/// Environment instance with sandbox and file management
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

pub struct OperationHandler {
    sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
    file_cacher: Box<dyn FileCacher>,
    task_cache: Box<dyn TaskCacheStore>,
    toolchain_fingerprint: String,
}

impl OperationHandler {
    pub fn new(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
        file_cacher: Box<dyn FileCacher>,
        task_cache: Box<dyn TaskCacheStore>,
        toolchain_fingerprint: String,
    ) -> Self {
        Self {
            sandbox_manager,
            file_cacher,
            task_cache,
            toolchain_fingerprint,
        }
    }

    /// Execute an operation
    #[instrument(skip(self, operation))]
    pub async fn execute(&self, operation: &OperationTask) -> Result<OperationResult> {
        info!(
            "Starting operation execution with {} environments and {} tasks",
            operation.environments.len(),
            operation.tasks.len()
        );

        // Initialize environments
        let mut environments = HashMap::new();
        for env_config in operation.environments.iter() {
            let box_id = allocate_box_id();
            debug!(env_id = %env_config.id, box_id = %box_id, "Initializing environment");

            // Create sandbox
            let working_dir = match self.create_sandbox(&box_id).await {
                Ok(dir) => dir,
                Err(e) => {
                    self.cleanup_environments(&environments).await.ok();
                    return Err(e.context("Failed to create sandbox"));
                }
            };

            // Load initial files — clean up sandbox on failure since it's not yet
            // tracked in `environments` and would be orphaned.
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
                    // Clean up on failure
                    let _ = tokio::fs::remove_dir_all(&dir).await;
                    self.cleanup_environments(&environments).await.ok();
                    return Err(anyhow!(
                        "mkfifo failed for channel {}: {}",
                        channel.name,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                debug!(channel = %channel.name, "Created shared channel FIFO");
            }

            Some(dir)
        } else {
            None
        };

        // Get execution order
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

        // Execute layers sequentially; tasks within each layer run in parallel
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

        // Cleanup
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

    /// Create a sandbox instance
    async fn create_sandbox(&self, box_id: &str) -> Result<PathBuf> {
        let sandbox_path = self
            .sandbox_manager
            .create_sandbox(Some(box_id))
            .await
            .context("Sandbox creation failed")?;
        Ok(sandbox_path)
    }

    /// Load files into sandbox environment
    async fn load_environment_files(
        &self,
        working_dir: &Path,
        files: &[(String, SessionFile)],
    ) -> Result<()> {
        for (target_path, source) in files {
            let dest = safe_join(working_dir, target_path)?;
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create parent directory")?;
            }
            match source {
                SessionFile::Path { path: src } => {
                    tokio::fs::copy(src, &dest).await.with_context(|| {
                        format!("Failed to copy file {} -> {}", src, dest.display())
                    })?;
                }
                SessionFile::Content { content } => {
                    tokio::fs::write(&dest, content).await.with_context(|| {
                        format!("Failed to write content to {}", dest.display())
                    })?;
                }
                SessionFile::Blob { hash: content_hash } => {
                    self.file_cacher
                        .fetch_to_path(content_hash, &dest)
                        .await
                        .map_err(|e| anyhow!("Failed to fetch blob {}: {}", content_hash, e))?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700));
                    }
                }
            }
            debug!(target = %target_path, dest = %dest.display(), "Loaded environment file");
        }
        Ok(())
    }

    /// Build task dependency graph
    fn build_dependency_graph(&self, operation: &OperationTask) -> HashMap<String, Vec<String>> {
        let mut graph = HashMap::new();

        for task in &operation.tasks {
            graph.insert(task.id.clone(), task.depends_on.clone());
        }

        graph
    }

    /// Get execution order as layers using topological sort (Kahn's algorithm).
    fn get_execution_order(&self, operation: &OperationTask) -> Result<Vec<Vec<String>>> {
        let graph = self.build_dependency_graph(operation);
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize
        for task in &operation.tasks {
            in_degree.insert(task.id.clone(), 0);
            adj_list.insert(task.id.clone(), Vec::new());
        }

        // Build adjacency list and compute in-degrees
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

        // Kahn's algorithm
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

    /// Execute a task, skipping if dependencies failed.
    async fn execute_step_with_deps(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        deps_ok: bool,
        shared_channels_dir: Option<&Path>,
        channel_names: &HashSet<String>,
    ) -> TaskExecutionResult {
        if !deps_ok {
            warn!(task_id = %step.id, "Skipping task due to dependency failure");
            return TaskExecutionResult {
                task_id: step.id.clone(),
                success: false,
                sandbox_result: ExecutionResult::default(),
                collected_outputs: HashMap::new(),
            };
        }

        if let Some(cache_spec) = &step.cache
            && let Some(cached) = self.try_cache_hit(step, environments, cache_spec).await
        {
            return cached;
        }

        let result = match self
            .execute_step(step, environments, shared_channels_dir, channel_names)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!(task_id = %step.id, error = %e, "Task execution error");
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
            self.store_in_cache(step, environments, cache_spec, &result.collected_outputs)
                .await;
        }

        result
    }

    /// Attempt to restore a step's outputs from the task cache.
    ///
    /// Returns `Some(TaskExecutionResult)` on cache hit, `None` on miss.
    async fn try_cache_hit(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_spec: &StepCacheConfig,
    ) -> Option<TaskExecutionResult> {
        let env = environments.get(&step.env_ref)?;
        let cache_key = match self
            .build_cache_key(&env.working_dir, &step.argv, &cache_spec.key_inputs)
            .await
        {
            Ok(key) => key,
            Err(e) => {
                debug!(step_id = %step.id, error = %e, "Failed to compute cache key, skipping cache");
                return None;
            }
        };

        let cached_outputs = match self.task_cache.get(&cache_key).await {
            Ok(Some(outputs)) => outputs,
            Ok(None) => {
                debug!(step_id = %step.id, cache_key = %cache_key, "Cache miss");
                return None;
            }
            Err(e) => {
                warn!(step_id = %step.id, error = %e, "Cache lookup failed, executing normally");
                return None;
            }
        };

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
            if let Err(e) = self.file_cacher.fetch_to_path(content_hash, &dest).await {
                warn!(
                    step_id = %step.id,
                    file = %filename,
                    error = %e,
                    "Failed to restore cached output, falling back to execution"
                );
                return None;
            }
            // Set execute permission for binaries
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700));
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

    /// Store step outputs in the task cache after successful execution.
    ///
    /// `existing_hashes` maps filenames to already-uploaded content hashes from
    /// `collect_output`; files present there are reused without re-uploading.
    async fn store_in_cache(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
        cache_spec: &StepCacheConfig,
        existing_hashes: &HashMap<String, String>,
    ) {
        let env = match environments.get(&step.env_ref) {
            Some(e) => e,
            None => return,
        };

        let cache_key = match self
            .build_cache_key(&env.working_dir, &step.argv, &cache_spec.key_inputs)
            .await
        {
            Ok(key) => key,
            Err(e) => {
                warn!(step_id = %step.id, error = %e, "Failed to compute cache key for storage");
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

        if let Err(e) = self.task_cache.put(&cache_key, output_hashes).await {
            warn!(step_id = %step.id, error = %e, "Failed to store task cache entry");
        } else {
            info!(step_id = %step.id, cache_key = %cache_key, "Stored step outputs in task cache");
        }
    }

    /// Build a deterministic cache key from argv + input file contents.
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

    /// Execute a single step
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
                inside_path: PathBuf::from("channels"),
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

    /// Prepare IO for task execution
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

    /// Prepare single IO target.
    ///
    /// For channel pipes (name in `channel_names`), returns the absolute host
    /// path to the pre-created FIFO in the shared channels directory. Both mock
    /// and isolate sandboxes open stdin/stdout/stderr on the host before
    /// entering the sandbox, so absolute host paths work correctly.
    ///
    /// For regular (non-channel) pipes, creates a per-environment FIFO at
    /// `working_dir/pipes/{name}` as before.
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
                    let channels_dir = shared_channels_dir.ok_or_else(|| {
                        anyhow!(
                            "Pipe '{}' references a channel but no channels directory exists",
                            name
                        )
                    })?;
                    let fifo_path = channels_dir.join(name);
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
                    // Concurrent steps in the same layer may race to create
                    // the same per-environment FIFO. If mkfifo fails because
                    // the file already exists as a FIFO, treat it as success.
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

    /// Cleanup all sandboxes
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
