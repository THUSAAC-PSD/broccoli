use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::operation::sandbox::{ExecutionResult, RunOptions, SandboxOptions};

/// File source for initial environment setup
/// TODO: Fetch from db
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionFile {
    /// File path in the system
    #[serde(rename = "path")]
    Path(String),
    /// Inline content
    #[serde(rename = "content")]
    Content(String),
}

/// Environment configuration - represents a sandbox instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    /// Initial files: (target_path, source)
    pub files_in: Vec<(String, SessionFile)>,
    pub conf: SandboxOptions,
}

/// IO target for task stdin/stdout/stderr
/// Null: discard
/// Inherit: inherit from worker
/// File: related file path in sandbox
/// Pipe: read/write from named pipe. This will create a pipe file in folder /pipes in sandbox, and the worker will read/write from it to transfer data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum IOTarget {
    /// Discard IO
    #[serde(rename = "null")]
    Null,
    /// Inherit
    #[serde(rename = "inherit")]
    #[default]
    Inherit,
    /// File path in sandbox
    #[serde(rename = "file")]
    File(String),
    /// Read/Write from pipe
    #[serde(rename = "pipe")]
    Pipe(String),
}

/// IO configuration for task execution
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IOConfig {
    pub stdin: IOTarget,
    pub stdout: IOTarget,
    pub stderr: IOTarget,
}

pub type RunConfig = RunOptions;

/// Task definition within an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub env_ref: String, // Reference to environment id
    pub argv: Vec<String>,
    pub conf: RunConfig,
    pub io: IOConfig,
    pub collect: Vec<String>, // Files to collect after execution
    #[serde(default)]
    pub depends_on: Vec<String>, // Task IDs to wait for
}

/// Channel/Pipe definition for inter-task communication
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTask {
    pub environments: Vec<Environment>,
    pub tasks: Vec<Step>,
    pub channels: Vec<Channel>,
}

/// Operation result after execution
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub task_results: HashMap<String, TaskExecutionResult>,
    pub error: Option<String>,
}

/// Individual task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub success: bool,
    pub sandbox_result: ExecutionResult,
}
