pub mod error;
pub mod isolate;

use async_trait::async_trait;
use error::SandboxError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// The following structs and traits are based on isolate sandboxing tool concepts.

/// Directory binding rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryRule {
    pub inside_path: PathBuf,
    pub outside_path: Option<PathBuf>,
    pub options: DirectoryOptions,
}

/// Directory binding options
/// NOTE: Unless --no-default-dirs is specified, the default set of directory rules binds /bin, /dev (with devices allowed), /lib, /lib64 (if it exists), and /usr.
/// It also binds the working directory to /box (read-write), mounts the proc filesystem at /proc, and creates a temporary directory /tmp.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryOptions {
    pub read_write: bool,
    pub allow_devices: bool,
    pub no_exec: bool,
    pub is_filesystem: bool,
    pub is_tmp: bool,
    pub no_recursive: bool,
}

/// Environment variable rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvRule {
    Inherit(String),
    Set(String, String),
    FullEnv,
}

/// Resource limits for isolate sandbox
/// All size-related items are in kilobytes (kB), time-related items are in seconds (s).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub time_limit: Option<f64>,
    pub wall_time_limit: Option<f64>,
    pub extra_time: Option<f64>,
    pub memory_limit: Option<u32>,
    pub stack_limit: Option<u32>,
    pub open_files_limit: Option<u32>,
    pub file_size_limit: Option<u32>,
    pub process_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunOptions {
    pub resource_limits: ResourceLimits,
    pub wait: bool,
    pub as_uid: Option<u32>,
    pub as_gid: Option<u32>,
    pub stdin: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxOptions {
    pub directory_rules: Vec<DirectoryRule>,
    pub env_rules: Vec<EnvRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub time_used: f64,
    pub wall_time_used: f64,
    pub memory_used: Option<u32>,
    pub killed: bool,
    pub cg_oom_killed: bool,
    pub status: String,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            exit_code: None,
            signal: None,
            time_used: 0.0,
            wall_time_used: 0.0,
            memory_used: None,
            killed: false,
            cg_oom_killed: false,
            status: "UNKNOWN".to_string(),
            message: String::new(),
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

#[async_trait]
pub trait SandboxManager {
    async fn create_sandbox(
        &mut self,
        id: Option<&str>,
        options: &SandboxOptions,
    ) -> Result<PathBuf, SandboxError>;
    async fn remove_sandbox(&mut self, id: &str) -> Result<(), SandboxError>;
    async fn execute(
        &self,
        box_id: &str,
        argv: Vec<String>,
        run_options: &RunOptions,
    ) -> Result<ExecutionResult, SandboxError>;
}
