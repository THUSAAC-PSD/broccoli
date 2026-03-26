use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// File source for initial environment setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionFile {
    #[serde(rename = "path")]
    Path { path: String },
    #[serde(rename = "content")]
    Content { content: String },
    #[serde(rename = "blob")]
    Blob { hash: String },
}

/// Environment configuration for an operation batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub files_in: Vec<(String, SessionFile)>,
}

/// IO target for task stdin/stdout/stderr.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum IOTarget {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "inherit")]
    #[default]
    Inherit,
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "pipe")]
    Pipe { name: String },
}

/// IO configuration for task execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IOConfig {
    pub stdin: IOTarget,
    pub stdout: IOTarget,
    pub stderr: IOTarget,
}

/// Directory binding options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryOptions {
    pub read_write: bool,
    pub allow_devices: bool,
    pub no_exec: bool,
    pub is_filesystem: bool,
    pub is_tmp: bool,
    pub no_recursive: bool,
}

/// Directory binding rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryRule {
    pub inside_path: PathBuf,
    pub outside_path: Option<PathBuf>,
    pub options: DirectoryOptions,
}

/// Environment variable rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvRule {
    Inherit(String),
    Set(String, String),
    FullEnv,
}

/// Resource limits for sandbox execution.
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

/// Run configuration for a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunOptions {
    pub resource_limits: ResourceLimits,
    pub wait: bool,
    pub as_uid: Option<u32>,
    pub as_gid: Option<u32>,
    pub stdin: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
    pub env_rules: Vec<EnvRule>,
    pub directory_rules: Vec<DirectoryRule>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            resource_limits: ResourceLimits {
                time_limit: None,
                wall_time_limit: None,
                extra_time: None,
                memory_limit: None,
                stack_limit: None,
                open_files_limit: None,
                file_size_limit: None,
                process_limit: Some(1),
            },
            wait: true,
            as_uid: None,
            as_gid: None,
            stdin: None,
            stdout: None,
            stderr: None,
            env_rules: vec![EnvRule::FullEnv],
            directory_rules: vec![],
        }
    }
}

/// Configuration for step-level caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCacheConfig {
    pub key_inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Task definition within an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub env_ref: String,
    pub argv: Vec<String>,
    pub conf: RunOptions,
    pub io: IOConfig,
    pub collect: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub cache: Option<StepCacheConfig>,
}

/// Channel definition for inter-task communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub buffer_size: Option<usize>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            name: String::new(),
            buffer_size: Some(8192),
        }
    }
}

/// A single operation task dispatched to the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTask {
    pub environments: Vec<Environment>,
    pub tasks: Vec<Step>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    /// Optional priority for dispatching the operation (1-5, 1 is highest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
}

/// Result of a worker operation batch.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub task_results: HashMap<String, TaskExecutionResult>,
    pub error: Option<String>,
}

/// Per-task result within an operation batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub success: bool,
    pub sandbox_result: ExecutionResult,
    #[serde(default)]
    pub collected_outputs: HashMap<String, String>,
}

/// Sandbox execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    /// CPU time used, in seconds.
    #[serde(default)]
    pub time_used: f64,
    /// Wall-clock time used, in seconds.
    #[serde(default)]
    pub wall_time_used: f64,
    /// Peak memory usage, in kilobytes.
    #[serde(default)]
    pub memory_used: Option<u32>,
    #[serde(default)]
    pub killed: bool,
    #[serde(default)]
    pub cg_oom_killed: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
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

pub type SandboxResult = ExecutionResult;
