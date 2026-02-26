use super::error::SandboxError;
use super::{ExecutionResult, RunOptions, SandboxManager, SandboxOptions};
use async_trait::async_trait;
use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct MockSandboxManager {
    base_dir: PathBuf,
    sandboxes: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl Default for MockSandboxManager {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("broccoli-mock-sandbox"))
    }
}

impl MockSandboxManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_sandbox_path(&self, box_id: &str) -> Result<PathBuf, SandboxError> {
        if let Some(path) = self.sandboxes.read().await.get(box_id).cloned() {
            return Ok(path);
        }

        let fallback = self.base_dir.join(box_id);
        if tokio::fs::try_exists(&fallback).await.map_err(|err| {
            SandboxError::Execution(format!("failed to inspect sandbox path: {err}"))
        })? {
            return Ok(fallback);
        }

        Err(SandboxError::Execution(format!(
            "mock sandbox not found for box id: {box_id}"
        )))
    }
}

#[async_trait]
impl SandboxManager for MockSandboxManager {
    async fn create_sandbox(
        &mut self,
        id: Option<&str>,
        _options: &SandboxOptions,
    ) -> Result<PathBuf, SandboxError> {
        let box_id = id.unwrap_or("0").to_string();
        if box_id.is_empty() {
            return Err(SandboxError::Initialization(
                "mock sandbox box id cannot be empty".to_string(),
            ));
        }

        let sandbox_path = self.base_dir.join(&box_id);
        debug!(box_id = %box_id, path = %sandbox_path.display(), "Creating mock sandbox");

        if tokio::fs::try_exists(&sandbox_path).await.map_err(|err| {
            SandboxError::Initialization(format!("failed to inspect mock sandbox path: {err}"))
        })? {
            tokio::fs::remove_dir_all(&sandbox_path)
                .await
                .map_err(|err| {
                    SandboxError::Initialization(format!(
                        "failed to reset mock sandbox directory: {err}"
                    ))
                })?;
        }

        tokio::fs::create_dir_all(&sandbox_path)
            .await
            .map_err(|err| {
                SandboxError::Initialization(format!(
                    "failed to create mock sandbox directory: {err}"
                ))
            })?;

        self.sandboxes
            .write()
            .await
            .insert(box_id, sandbox_path.clone());

        Ok(sandbox_path)
    }

    async fn remove_sandbox(&mut self, id: &str) -> Result<(), SandboxError> {
        let known_path = self.sandboxes.write().await.remove(id);
        let sandbox_path = known_path.unwrap_or_else(|| self.base_dir.join(id));
        debug!(box_id = %id, path = %sandbox_path.display(), "Removing mock sandbox");

        if tokio::fs::try_exists(&sandbox_path).await.map_err(|err| {
            SandboxError::Execution(format!("failed to inspect mock sandbox path: {err}"))
        })? {
            tokio::fs::remove_dir_all(&sandbox_path)
                .await
                .map_err(|err| {
                    SandboxError::Execution(format!("failed to remove mock sandbox: {err}"))
                })?;
        }

        Ok(())
    }

    async fn execute(
        &self,
        box_id: &str,
        argv: Vec<String>,
        run_options: &RunOptions,
    ) -> Result<ExecutionResult, SandboxError> {
        if argv.is_empty() {
            return Err(SandboxError::Execution(
                "mock sandbox requires at least one argv element".to_string(),
            ));
        }

        let sandbox_path = self.get_sandbox_path(box_id).await?;
        debug!(box_id = %box_id, argv = ?argv, cwd = %sandbox_path.display(), "Executing command in mock sandbox");

        let mut command = Command::new(&argv[0]);
        command.args(argv.iter().skip(1));
        command.current_dir(&sandbox_path);

        if let Some(stdin_path) = &run_options.stdin {
            let stdin_file = std::fs::File::open(stdin_path).map_err(|err| {
                SandboxError::Execution(format!(
                    "failed to open stdin file {}: {err}",
                    stdin_path.display()
                ))
            })?;
            command.stdin(Stdio::from(stdin_file));
        }

        if let Some(stdout_path) = &run_options.stdout {
            if let Some(parent) = stdout_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to create stdout directory {}: {err}",
                        parent.display()
                    ))
                })?;
            }
            let stdout_file = std::fs::File::create(stdout_path).map_err(|err| {
                SandboxError::Execution(format!(
                    "failed to create stdout file {}: {err}",
                    stdout_path.display()
                ))
            })?;
            command.stdout(Stdio::from(stdout_file));
        } else {
            command.stdout(Stdio::piped());
        }

        if let Some(stderr_path) = &run_options.stderr {
            if let Some(parent) = stderr_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to create stderr directory {}: {err}",
                        parent.display()
                    ))
                })?;
            }
            let stderr_file = std::fs::File::create(stderr_path).map_err(|err| {
                SandboxError::Execution(format!(
                    "failed to create stderr file {}: {err}",
                    stderr_path.display()
                ))
            })?;
            command.stderr(Stdio::from(stderr_file));
        } else {
            command.stderr(Stdio::piped());
        }

        let start = Instant::now();
        let output = command.output().await.map_err(|err| {
            SandboxError::Execution(format!("failed to execute in mock sandbox: {err}"))
        })?;
        let elapsed = start.elapsed().as_secs_f64();

        let stdout = if let Some(path) = &run_options.stdout {
            tokio::fs::read_to_string(path)
                .await
                .unwrap_or_else(|_| String::new())
        } else {
            String::from_utf8_lossy(&output.stdout).to_string()
        };

        let stderr = if let Some(path) = &run_options.stderr {
            tokio::fs::read_to_string(path)
                .await
                .unwrap_or_else(|_| String::new())
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };

        let exit_code = output.status.code();
        let signal = output.status.signal();
        let success = output.status.success();

        if success {
            debug!(box_id = %box_id, exit_code = ?exit_code, time_used = elapsed, "Mock sandbox command finished");
        } else {
            warn!(box_id = %box_id, exit_code = ?exit_code, signal = ?signal, time_used = elapsed, "Mock sandbox command failed");
        }

        Ok(ExecutionResult {
            exit_code,
            signal,
            time_used: elapsed,
            wall_time_used: elapsed,
            memory_used: None,
            killed: signal.is_some(),
            cg_oom_killed: false,
            status: if success {
                "OK".to_string()
            } else if signal.is_some() {
                "SG".to_string()
            } else {
                "RE".to_string()
            },
            message: if success {
                String::new()
            } else {
                format!("mock sandbox process exited with status: {}", output.status)
            },
            stdout,
            stderr,
        })
    }
}
