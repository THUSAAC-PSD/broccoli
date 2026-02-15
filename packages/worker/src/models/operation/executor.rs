use tokio::sync::Mutex;

use super::models::OperationTask;
use super::sandbox::isolate::IsolateSandboxManager;
use crate::models::operation::handler::OperationHandler;
use anyhow::Result;
use async_trait::async_trait;
use common::worker::*;

/// Executor for running operations with isolated sandboxes
pub struct OperationTaskExecutor {
    operation_executor: Mutex<OperationHandler<IsolateSandboxManager>>,
}

impl OperationTaskExecutor {
    pub fn new() -> Self {
        Self {
            operation_executor: Mutex::new(OperationHandler::new(IsolateSandboxManager)),
        }
    }
}

impl Default for OperationTaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor for OperationTaskExecutor {
    fn if_accept(&self, task_type: &str) -> bool {
        task_type == "operation"
    }
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        // Deserialize the payload into an Operation
        let operation: OperationTask = serde_json::from_value(task.payload.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialize operation config: {}", e))?;

        // Execute the operation
        let mut operation_executor = self.operation_executor.lock().await;
        match operation_executor.execute(&operation).await {
            Ok(result) => Ok(TaskResult {
                task_id: task.id,
                success: result.success,
                output: serde_json::to_value(result)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))?,
            }),
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }
}
