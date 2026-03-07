use super::error::SandboxError;
use super::{DirectoryRule, ExecutionResult, RunOptions, SandboxManager};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
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

    fn open_stdin(path: &PathBuf) -> Result<std::fs::File, SandboxError> {
        if Self::is_fifo(path) {
            return OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to open stdin pipe {}: {err}",
                        path.display()
                    ))
                });
        }

        std::fs::File::open(path).map_err(|err| {
            SandboxError::Execution(format!(
                "failed to open stdin file {}: {err}",
                path.display()
            ))
        })
    }

    fn open_stdout_stderr(
        path: &PathBuf,
        stream_name: &str,
    ) -> Result<std::fs::File, SandboxError> {
        if Self::is_fifo(path) {
            return OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to open {stream_name} pipe {}: {err}",
                        path.display()
                    ))
                });
        }

        std::fs::File::create(path).map_err(|err| {
            SandboxError::Execution(format!(
                "failed to create {stream_name} file {}: {err}",
                path.display()
            ))
        })
    }

    fn is_fifo(path: &PathBuf) -> bool {
        std::fs::metadata(path)
            .map(|m| m.file_type().is_fifo())
            .unwrap_or(false)
    }

    fn resolve_inside_path(base: &Path, inside: &Path) -> Result<PathBuf, SandboxError> {
        let mut resolved = base.to_path_buf();
        for component in inside.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(part) => resolved.push(part),
                Component::ParentDir => {
                    return Err(SandboxError::Initialization(format!(
                        "invalid inside_path with parent traversal: {}",
                        inside.display()
                    )));
                }
                Component::Prefix(_) => {
                    return Err(SandboxError::Initialization(format!(
                        "unsupported inside_path prefix: {}",
                        inside.display()
                    )));
                }
            }
        }
        Ok(resolved)
    }

    async fn remove_existing_target(path: &Path) -> Result<(), SandboxError> {
        if !fs::try_exists(path).await.map_err(|err| {
            SandboxError::Initialization(format!("failed to inspect mapped path: {err}"))
        })? {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(path).await.map_err(|err| {
            SandboxError::Initialization(format!("failed to inspect mapped metadata: {err}"))
        })?;

        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path).await.map_err(|err| {
                SandboxError::Initialization(format!("failed to remove mapped directory: {err}"))
            })
        } else {
            fs::remove_file(path).await.map_err(|err| {
                SandboxError::Initialization(format!("failed to remove mapped file: {err}"))
            })
        }
    }

    async fn apply_directory_rule(
        sandbox_path: &Path,
        rule: &DirectoryRule,
    ) -> Result<(), SandboxError> {
        let inside_path = Self::resolve_inside_path(sandbox_path, &rule.inside_path)?;

        if let Some(parent) = inside_path.parent() {
            fs::create_dir_all(parent).await.map_err(|err| {
                SandboxError::Initialization(format!(
                    "failed to create parent directory for mapping {}: {err}",
                    parent.display()
                ))
            })?;
        }

        match &rule.outside_path {
            Some(outside_path) => {
                if !fs::try_exists(outside_path).await.map_err(|err| {
                    SandboxError::Initialization(format!(
                        "failed to inspect outside_path {}: {err}",
                        outside_path.display()
                    ))
                })? {
                    fs::create_dir_all(outside_path).await.map_err(|err| {
                        SandboxError::Initialization(format!(
                            "failed to create outside_path {}: {err}",
                            outside_path.display()
                        ))
                    })?;
                }

                Self::remove_existing_target(&inside_path).await?;

                std::os::unix::fs::symlink(outside_path, &inside_path).map_err(|err| {
                    SandboxError::Initialization(format!(
                        "failed to create mock directory mapping {} -> {}: {err}",
                        inside_path.display(),
                        outside_path.display()
                    ))
                })?;
            }
            None => {
                fs::create_dir_all(&inside_path).await.map_err(|err| {
                    SandboxError::Initialization(format!(
                        "failed to create inside_path {}: {err}",
                        inside_path.display()
                    ))
                })?;
            }
        }

        Ok(())
    }

    async fn apply_directory_rules(
        sandbox_path: &Path,
        directory_rules: &[DirectoryRule],
    ) -> Result<(), SandboxError> {
        for rule in directory_rules {
            Self::apply_directory_rule(sandbox_path, rule).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SandboxManager for MockSandboxManager {
    async fn create_sandbox(&self, id: Option<&str>) -> Result<PathBuf, SandboxError> {
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

    async fn remove_sandbox(&self, id: &str) -> Result<(), SandboxError> {
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

        // Rewrite /box/ paths in argv to use the actual sandbox directory.
        // Plugins designed for isolate use /box/ as the sandbox root; the
        // mock sandbox replicates this by mapping /box/X → sandbox_path/X.
        let rewritten_argv: Vec<String> = argv
            .iter()
            .map(|arg| {
                if let Some(rest) = arg.strip_prefix("/box/") {
                    sandbox_path.join(rest).to_string_lossy().into_owned()
                } else if arg == "/box" {
                    sandbox_path.to_string_lossy().into_owned()
                } else {
                    arg.clone()
                }
            })
            .collect();

        let mut command = Command::new(&rewritten_argv[0]);
        command.args(rewritten_argv.iter().skip(1));
        command.current_dir(&sandbox_path);

        if let Some(stdin_path) = &run_options.stdin {
            let stdin_path = sandbox_path.join(stdin_path);
            let stdin_file = Self::open_stdin(&stdin_path)?;
            command.stdin(Stdio::from(stdin_file));
        }

        if let Some(stdout_path) = &run_options.stdout {
            let stdout_path = sandbox_path.join(stdout_path);
            if let Some(parent) = stdout_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to create stdout directory {}: {err}",
                        parent.display()
                    ))
                })?;
            }
            let stdout_file = Self::open_stdout_stderr(&stdout_path, "stdout")?;
            command.stdout(Stdio::from(stdout_file));
        } else {
            command.stdout(Stdio::piped());
        }

        if let Some(stderr_path) = &run_options.stderr {
            let stderr_path = sandbox_path.join(stderr_path);
            if let Some(parent) = stderr_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|err| {
                    SandboxError::Execution(format!(
                        "failed to create stderr directory {}: {err}",
                        parent.display()
                    ))
                })?;
            }
            let stderr_file = Self::open_stdout_stderr(&stderr_path, "stderr")?;
            command.stderr(Stdio::from(stderr_file));
        } else {
            command.stderr(Stdio::piped());
        }

        Self::apply_directory_rules(&sandbox_path, &run_options.directory_rules).await?;
        let start = Instant::now();
        let mut child = command.spawn().map_err(|err| {
            SandboxError::Execution(format!("failed to spawn mock sandbox process: {err}"))
        })?;

        // If stdout/stderr are piped (not redirected to file), we must drain them
        // concurrently with wait() to prevent the child from blocking on a full pipe.
        use tokio::io::AsyncReadExt;
        let piped_stdout_handle = child.stdout.take();
        let piped_stderr_handle = child.stderr.take();

        let stdout_task = piped_stdout_handle.map(|mut s| {
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let _ = s.read_to_end(&mut buf).await;
                buf
            })
        });
        let stderr_task = piped_stderr_handle.map(|mut s| {
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let _ = s.read_to_end(&mut buf).await;
                buf
            })
        });

        // Enforce wall-time limit. The mock sandbox doesn't run inside a cgroup,
        // so we apply an explicit timeout using the wall_time_limit if provided,
        // or the time_limit with a 1.5x multiplier as a fallback.
        let time_limit_secs = run_options.resource_limits.wall_time_limit.or_else(|| {
            run_options
                .resource_limits
                .time_limit
                .map(|t| (t * 1.5).max(t + 5.0))
        });

        let (timed_out, exit_status) = if let Some(limit) = time_limit_secs {
            let timeout_dur = Duration::from_secs_f64(limit);
            tokio::select! {
                result = child.wait() => {
                    let status = result.map_err(|err| {
                        SandboxError::Execution(format!("failed to wait mock sandbox process: {err}"))
                    })?;
                    (false, status)
                }
                _ = tokio::time::sleep(timeout_dur) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    (true, std::process::ExitStatus::default())
                }
            }
        } else {
            let status = child.wait().await.map_err(|err| {
                SandboxError::Execution(format!("failed to wait mock sandbox process: {err}"))
            })?;
            (false, status)
        };

        let elapsed = start.elapsed().as_secs_f64();

        // Collect piped output (empty if timed out — tasks aborted or finished)
        let piped_stdout_bytes = if let Some(task) = stdout_task {
            task.await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let piped_stderr_bytes = if let Some(task) = stderr_task {
            task.await.unwrap_or_default()
        } else {
            Vec::new()
        };

        let stdout = if let Some(path) = &run_options.stdout {
            let resolved = sandbox_path.join(path);
            if Self::is_fifo(&resolved) {
                String::new()
            } else {
                tokio::fs::read_to_string(&resolved)
                    .await
                    .unwrap_or_else(|_| String::new())
            }
        } else {
            String::from_utf8_lossy(&piped_stdout_bytes).to_string()
        };

        let stderr = if let Some(path) = &run_options.stderr {
            let resolved = sandbox_path.join(path);
            if Self::is_fifo(&resolved) {
                String::new()
            } else {
                tokio::fs::read_to_string(&resolved)
                    .await
                    .unwrap_or_else(|_| String::new())
            }
        } else {
            String::from_utf8_lossy(&piped_stderr_bytes).to_string()
        };

        let exit_code = if timed_out { None } else { exit_status.code() };
        let signal = if timed_out {
            None
        } else {
            exit_status.signal()
        };
        let success = !timed_out && exit_status.success();

        if timed_out {
            warn!(box_id = %box_id, time_used = elapsed, "Mock sandbox process killed (wall-time limit exceeded)");
        } else if success {
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
            killed: timed_out || signal.is_some(),
            cg_oom_killed: false,
            status: if timed_out {
                "TO".to_string()
            } else if success {
                "OK".to_string()
            } else if signal.is_some() {
                "SG".to_string()
            } else {
                "RE".to_string()
            },
            message: if timed_out {
                "wall-time limit exceeded".to_string()
            } else if success {
                String::new()
            } else {
                format!("mock sandbox process exited with status: {}", exit_status)
            },
            stdout,
            stderr,
        })
    }
}
