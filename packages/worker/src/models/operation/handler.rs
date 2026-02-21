use super::models::*;
use super::sandbox::{ExecutionResult, RunOptions, SandboxManager, SandboxOptions};
use anyhow::{Context, Result, anyhow};
use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, instrument, warn};

/// Environment instance with sandbox and file management
struct EnvironmentList {
    id: String,
    box_id: String,
    sandbox_path: PathBuf,
}

impl EnvironmentList {
    fn new(id: String, box_id: String, sandbox_path: PathBuf) -> Self {
        Self {
            id,
            box_id,
            sandbox_path,
        }
    }
}

pub struct OperationHandler<M: SandboxManager> {
    sandbox_manager: M,
}

impl<M: SandboxManager> OperationHandler<M> {
    pub fn new(sandbox_manager: M) -> Self {
        Self { sandbox_manager }
    }

    /// Execute an operation
    #[instrument(skip(self, operation))]
    pub async fn execute(&mut self, operation: &OperationTask) -> Result<OperationResult> {
        info!(
            "Starting operation execution with {} environments and {} tasks",
            operation.environments.len(),
            operation.tasks.len()
        );

        // Initialize environments
        let mut environments = HashMap::new();
        for (idx, env_config) in operation.environments.iter().enumerate() {
            let box_id = idx.to_string();
            debug!(env_id = %env_config.id, box_id = %box_id, "Initializing environment");

            // Create sandbox
            let sandbox_options = SandboxOptions::default();
            let sandbox_path = self
                .create_sandbox(&box_id, &sandbox_options)
                .await
                .context("Failed to create sandbox")?;

            // Load initial files
            self.load_environment_files(&sandbox_path, &env_config.files_in)
                .await
                .context("Failed to load environment files")?;

            environments.insert(
                env_config.id.clone(),
                EnvironmentList::new(env_config.id.clone(), box_id, sandbox_path),
            );
        }

        // Get execution order
        let execution_order = self.get_execution_order(operation)?;
        debug!(order = ?execution_order, "Task execution order determined");

        let mut task_results = HashMap::new();
        let mut global_success = true;

        // Execute tasks in order
        // TODO: Consider parallel execution of independent tasks
        for task_id in execution_order {
            let task = operation
                .tasks
                .iter()
                .find(|t| t.id == task_id)
                .expect("Task should exist");

            // Check if all dependencies succeeded
            let deps_ok = task.depends_on.iter().all(|dep_id| {
                task_results
                    .get(dep_id)
                    .map(|r: &TaskExecutionResult| r.success)
                    .unwrap_or(false)
            });

            let result = if deps_ok {
                match self.execute_step(task, &environments).await {
                    Ok(result) => {
                        if !result.success {
                            global_success = false;
                        }
                        result
                    }
                    Err(e) => {
                        error!(task_id = %task.id, error = %e, "Task execution error");
                        global_success = false;
                        TaskExecutionResult {
                            task_id: task.id.clone(),
                            success: false,
                            sandbox_result: ExecutionResult::default(),
                        }
                    }
                }
            } else {
                warn!(task_id = %task.id, "Skipping task due to dependency failure");
                TaskExecutionResult {
                    task_id: task.id.clone(),
                    success: false,
                    sandbox_result: ExecutionResult::default(),
                }
            };

            task_results.insert(task.id.clone(), result);
        }

        // Cleanup
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
    async fn create_sandbox(&mut self, box_id: &str, options: &SandboxOptions) -> Result<PathBuf> {
        let sandbox_path = self
            .sandbox_manager
            .create_sandbox(Some(box_id), options)
            .await
            .context("Sandbox creation failed")?;
        Ok(sandbox_path)
    }

    /// Load files into sandbox environment
    async fn load_environment_files(
        &self,
        _sandbox_path: &PathBuf,
        _files: &[(String, SessionFile)],
    ) -> Result<()> {
        // TODO
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

    /// Get execution order using topological sort
    fn get_execution_order(&self, operation: &OperationTask) -> Result<Vec<String>> {
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
        let mut queue: VecDeque<_> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut order = Vec::new();

        while let Some(task_id) = queue.pop_front() {
            order.push(task_id.clone());

            if let Some(neighbors) = adj_list.get(&task_id) {
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }
        }

        if order.len() != operation.tasks.len() {
            return Err(anyhow!(
                "Circular dependency detected or task missing in dependency graph"
            ));
        }

        Ok(order)
    }

    /// Execute a single step
    #[instrument(skip(self, step, environments))]
    async fn execute_step(
        &self,
        step: &Step,
        environments: &HashMap<String, EnvironmentList>,
    ) -> Result<TaskExecutionResult> {
        debug!(step_id = %step.id, "Executing step");

        let env = environments
            .get(&step.env_ref)
            .ok_or_else(|| anyhow!("Environment not found: {}", step.env_ref))?;

        let (stdin_path, stdout_path, stderr_path) =
            self.prepare_io(&env.sandbox_path, &step.io).await?;

        let run_opts = RunOptions {
            resource_limits: step.conf.resource_limits.clone(),
            wait: true,
            as_uid: None,
            as_gid: None,
            stdin: stdin_path,
            stdout: stdout_path,
            stderr: stderr_path,
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
        if success {
            self.collect_output(&env.sandbox_path, &step.collect)
                .await?;
        }

        Ok(TaskExecutionResult {
            task_id: step.id.clone(),
            success,
            sandbox_result: exec_result,
        })
    }

    /// Prepare IO for task execution
    async fn prepare_io(
        &self,
        sandbox_path: &Path,
        io_config: &IOConfig,
    ) -> Result<(Option<PathBuf>, Option<PathBuf>, Option<PathBuf>)> {
        let stdin = self
            .prepare_io_target(sandbox_path, &io_config.stdin)
            .await?;
        let stdout = self
            .prepare_io_target(sandbox_path, &io_config.stdout)
            .await?;
        let stderr = self
            .prepare_io_target(sandbox_path, &io_config.stderr)
            .await?;

        Ok((stdin, stdout, stderr))
    }

    /// Prepare single IO target
    async fn prepare_io_target(
        &self,
        sandbox_path: &Path,
        target: &IOTarget,
    ) -> Result<Option<PathBuf>> {
        match target {
            IOTarget::Null | IOTarget::Inherit => Ok(None),
            IOTarget::File(path) => Ok(Some(sandbox_path.join(path))),
            IOTarget::Pipe(name) => {
                if name.is_empty() {
                    return Err(anyhow!("Pipe name cannot be empty"));
                }

                let pipes_dir = sandbox_path.join("pipes");
                tokio::fs::create_dir_all(&pipes_dir)
                    .await
                    .context("Failed to create pipes directory")?;

                let pipe_path = pipes_dir.join(name);

                if let Ok(meta) = tokio::fs::metadata(&pipe_path).await {
                    if !meta.file_type().is_fifo() {
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
        _sandbox_path: &PathBuf,
        _collect_files: &[String],
    ) -> Result<()> {
        // TODO
        Ok(())
    }

    /// Cleanup all sandboxes
    async fn cleanup_environments(
        &mut self,
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
