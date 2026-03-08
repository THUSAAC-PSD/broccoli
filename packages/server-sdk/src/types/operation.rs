use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a worker operation batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub sandbox_result: SandboxResult,
}

/// Sandbox execution outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
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
