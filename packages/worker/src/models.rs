use anyhow::Result;
use async_trait::async_trait;
use common::worker::*;
use plugin_core::hook::{Hook, HookManager};
use plugin_core::traits::{PluginManager, PluginManagerExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Native executor to run Rust functions
pub struct NativeExecutor {
    handlers: Arc<
        Mutex<
            HashMap<
                String,
                Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>,
            >,
        >,
    >,
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_handler<F>(&self, task_type: String, handler: F)
    where
        F: Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync + 'static,
    {
        self.handlers
            .lock()
            .unwrap()
            .insert(task_type, Box::new(handler));
    }
}

#[async_trait]
impl Executor for NativeExecutor {
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let handlers = self.handlers.lock().unwrap();

        if let Some(handler) = handlers.get(&task.task_type) {
            match handler(task.payload) {
                Ok(output) => Ok(TaskResult {
                    task_id: task.id,
                    success: true,
                    output,
                }),
                Err(e) => Ok(TaskResult {
                    task_id: task.id,
                    success: false,
                    output: serde_json::json!({ "error": e.to_string() }),
                }),
            }
        } else {
            Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": "Unknown task type" }),
            })
        }
    }
}

pub struct WasmExecutor<M: PluginManager> {
    plugin_manager: Arc<M>,
    plugin_id: String,
    function_name: String,
}

impl<M: PluginManager> WasmExecutor<M> {
    pub fn new(plugin_manager: Arc<M>, plugin_id: String) -> Self {
        Self {
            plugin_manager,
            plugin_id,
            function_name: "execute_task".to_string(),
        }
    }

    pub fn with_function(plugin_manager: Arc<M>, plugin_id: String, function_name: String) -> Self {
        Self {
            plugin_manager,
            plugin_id,
            function_name,
        }
    }
}

#[async_trait]
impl<M: PluginManager + Send + Sync> Executor for WasmExecutor<M> {
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let task_id = task.id.clone();
        match self
            .plugin_manager
            .call(&self.plugin_id, &self.function_name, task)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => Ok(TaskResult {
                task_id,
                success: false,
                output: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }
}

pub struct Worker {
    executors: Arc<Mutex<HashMap<String, Arc<dyn Executor>>>>,
    hook_manager: Arc<Mutex<HookManager<TaskEvent>>>,
}

impl Worker {
    pub fn new() -> Self {
        Self {
            executors: Arc::new(Mutex::new(HashMap::new())),
            hook_manager: Arc::new(Mutex::new(HookManager::new())),
        }
    }

    pub fn register_executor(&self, name: String, executor: Arc<dyn Executor>) {
        self.executors.lock().unwrap().insert(name, executor);
    }

    pub fn add_hook(&self, hook: Arc<dyn Hook<TaskEvent>>) {
        self.hook_manager.lock().unwrap().add_hook(hook);
    }

    pub async fn execute_task(&self, task: Task, executor_name: &str) -> Result<TaskResult> {
        let hook_manager = { self.hook_manager.lock().unwrap().clone() };

        // Trigger task started event
        hook_manager
            .trigger(&TaskEvent::Started { task: task.clone() })
            .await;

        let executor = self.executors.lock().unwrap().get(executor_name).cloned();

        let result = if let Some(executor) = executor {
            match executor.execute(task.clone()).await {
                Ok(result) => {
                    // Trigger task completed event
                    hook_manager
                        .trigger(&TaskEvent::Completed {
                            result: result.clone(),
                        })
                        .await;
                    result
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Trigger task failed event
                    hook_manager
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
            TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": "Executor not found" }),
            }
        };

        Ok(result)
    }
}

// Example hook: Simple logger
pub struct LoggerHook;

#[async_trait]
impl Hook<TaskEvent> for LoggerHook {
    async fn on_event(&self, event: &TaskEvent) -> Result<()> {
        match event {
            TaskEvent::Started { task } => {
                tracing::info!("Task started: {} (type: {})", task.id, task.task_type);
            }
            TaskEvent::Completed { result } => {
                tracing::info!(
                    "Task completed: {} (success: {})",
                    result.task_id,
                    result.success
                );
            }
            TaskEvent::Failed { task, error } => {
                tracing::error!("Task error: {} - {}", task.id, error);
            }
        }
        Ok(())
    }
}
