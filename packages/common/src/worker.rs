use crate::event::Event;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Task used for worker to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: String,
    pub executor_name: String,
    pub payload: serde_json::Value,
    pub result_queue: String,
    /// Optional priority level (1-5, where 1 is highest priority)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Task lifecycle events for hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    Started { task: Task },
    Completed { result: TaskResult },
    Failed { task: Task, error: String },
}

impl Event for TaskEvent {
    fn topic(&self) -> &str {
        match self {
            TaskEvent::Started { .. } => "task_started",
            TaskEvent::Completed { .. } => "task_completed",
            TaskEvent::Failed { .. } => "task_failed",
        }
    }
}

/// Worker use executor to run tasks
#[async_trait]
pub trait Executor: Send + Sync {
    fn if_accept(&self, _task_type: &str) -> bool;
    async fn execute(&self, task: Task) -> Result<TaskResult>;
}
