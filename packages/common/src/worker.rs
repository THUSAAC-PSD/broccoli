use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Task used for worker to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: serde_json::Value,
}

// TODO: extend hook functionality
#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_task_start(&self, task: &Task) -> Result<()>;
    async fn on_task_complete(&self, result: &TaskResult) -> Result<()>;
    async fn on_task_error(&self, task: &Task, error: &str) -> Result<()>;
}

/// Worker use executor to run tasks
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, task: Task) -> Result<TaskResult>;
}
