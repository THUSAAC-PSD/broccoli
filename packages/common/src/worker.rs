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

/// Task lifecycle events for hooks
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    Started { task: Task },
    Completed { result: TaskResult },
    Failed { task: Task, error: String },
}

/// Worker use executor to run tasks
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, task: Task) -> Result<TaskResult>;
}
