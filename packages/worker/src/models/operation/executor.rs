use tokio::sync::Mutex;

use super::models::OperationTask;
use super::sandbox::SandboxManager;
use super::sandbox::isolate::IsolateSandboxManager;
use super::sandbox::mock::MockSandboxManager;
use crate::config::WorkerAppConfig;
use crate::models::operation::handler::OperationHandler;
use anyhow::Result;
use async_trait::async_trait;
use common::worker::*;
use tracing::{info, warn};

/// Executor for running operations with isolated sandboxes
pub struct OperationTaskExecutor {
    operation_executor: Mutex<OperationHandler>,
}

impl OperationTaskExecutor {
    pub fn new() -> Self {
        Self::new_with_sandbox_manager(Self::sandbox_manager_from_config())
    }

    pub fn new_with_sandbox_manager(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
    ) -> Self {
        Self {
            operation_executor: Mutex::new(OperationHandler::new(sandbox_manager)),
        }
    }

    fn sandbox_manager_from_config() -> Box<dyn SandboxManager + Send + Sync> {
        let backend = match WorkerAppConfig::load() {
            Ok(cfg) => cfg.worker.sandbox_backend,
            Err(error) => {
                warn!(error = %error, "Failed to load worker config, fallback to isolate sandbox");
                "isolate".to_string()
            }
        };

        if backend.eq_ignore_ascii_case("mock") {
            info!(sandbox_backend = "mock", "Using operation sandbox backend");
            Box::new(MockSandboxManager::default())
        } else {
            if !backend.eq_ignore_ascii_case("isolate") {
                warn!(sandbox_backend = %backend, "Unknown sandbox backend, fallback to isolate");
            }
            info!(
                sandbox_backend = "isolate",
                "Using operation sandbox backend"
            );
            Box::new(IsolateSandboxManager)
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
