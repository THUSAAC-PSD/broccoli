use common::hook::{Hook, HookRegistry};
use common::worker::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::WorkerError;
use crate::models::executor::NativeExecutor;
use crate::models::judge::handle_judge;
use crate::models::operation::OperationTaskExecutor;

pub struct Worker {
    executors: Arc<Mutex<HashMap<String, Arc<dyn Executor>>>>,
    hook_registry: Arc<Mutex<HookRegistry>>,
}

impl Worker {
    pub fn new() -> Self {
        let worker = Self {
            executors: Arc::new(Mutex::new(HashMap::new())),
            hook_registry: Arc::new(Mutex::new(HookRegistry::new(()))),
        };

        let native = NativeExecutor::new();
        native.register_handler("judge".into(), handle_judge);
        worker.register_executor("native", Arc::new(native));
        worker.register_executor("operation", Arc::new(OperationTaskExecutor::new()));
        // TODO: WasmExecutor?
        worker
    }

    pub fn register_executor(&self, name: &str, executor: Arc<dyn Executor>) {
        self.executors
            .lock()
            .unwrap()
            .insert(name.to_string(), executor);
    }

    #[allow(dead_code)]
    pub fn add_hook<H: Hook<TaskEvent, Context = ()> + 'static>(
        &self,
        hook: H,
    ) -> Result<(), WorkerError> {
        self.hook_registry
            .lock()
            .unwrap()
            .add_hook(hook)
            .map_err(|e| WorkerError::Internal(e.to_string()))
    }

    pub async fn execute_task(&self, task: Task) -> Result<TaskResult, WorkerError> {
        let hook_manager = { self.hook_registry.lock().unwrap().clone() };

        // TODO: take use of return value of trigger, e.g., modified event

        // Trigger task started event
        let _ = hook_manager
            .trigger(&TaskEvent::Started { task: task.clone() })
            .await;

        let executor = self
            .executors
            .lock()
            .unwrap()
            .get(&task.executor_name)
            .cloned();

        let result = if let Some(executor) = executor {
            if !executor.if_accept(&task.task_type) {
                return Err(WorkerError::External(format!(
                    "Executor '{}' does not accept task type '{}'",
                    task.executor_name, task.task_type
                )));
            }
            match executor.execute(task.clone()).await {
                Ok(result) => {
                    // Trigger task completed event
                    let _ = hook_manager
                        .trigger(&TaskEvent::Completed {
                            result: result.clone(),
                        })
                        .await;
                    result
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Trigger task failed event
                    let _ = hook_manager
                        .trigger(&TaskEvent::Failed {
                            task: task.clone(),
                            error: error_msg.clone(),
                        })
                        .await;
                    TaskResult {
                        task_id: task.id,
                        success: false,
                        output: serde_json::json!({ "error": error_msg }),
                    }
                }
            }
        } else {
            return Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": format!("No executor found for '{}'", task.executor_name) }),
            });
        };

        Ok(result)
    }
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}
