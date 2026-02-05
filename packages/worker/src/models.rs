use anyhow::Result;
use async_trait::async_trait;
use common::hook::{Hook, HookRegistry};
use common::worker::*;
use plugin_core::traits::{PluginManager, PluginManagerExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Function = Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>;

pub struct NativeExecutor {
    handlers: Arc<Mutex<HashMap<String, Function>>>,
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

impl Default for NativeExecutor {
    fn default() -> Self {
        Self::new()
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

pub struct TaskHandler {
    executors: Arc<Mutex<HashMap<String, Arc<dyn Executor>>>>,
    hook_registry: Arc<Mutex<HookRegistry>>,
}

impl TaskHandler {
    pub fn new() -> Self {
        Self {
            executors: Arc::new(Mutex::new(HashMap::new())),
            hook_registry: Arc::new(Mutex::new(HookRegistry::new(()))),
        }
    }

    pub fn register_executor(&self, name: String, executor: Arc<dyn Executor>) {
        self.executors.lock().unwrap().insert(name, executor);
    }

    pub fn add_hook<H: Hook<TaskEvent, Context = ()> + 'static>(&self, hook: H) -> Result<()> {
        self.hook_registry.lock().unwrap().add_hook(hook)
    }

    pub async fn execute_task(&self, task: Task, executor_name: &str) -> Result<TaskResult> {
        let hook_manager = { self.hook_registry.lock().unwrap().clone() };

        // TODO: take use of return value of trigger, e.g., modified event

        // Trigger task started event
        let _ = hook_manager
            .trigger(&TaskEvent::Started { task: task.clone() })
            .await;

        let executor = self.executors.lock().unwrap().get(executor_name).cloned();

        let result = if let Some(executor) = executor {
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
            TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": "Executor not found" }),
            }
        };

        Ok(result)
    }
}

impl Default for TaskHandler {
    fn default() -> Self {
        Self::new()
    }
}
