use anyhow::Result;
use async_trait::async_trait;
use common::worker::*;
use plugin_core::traits::{PluginManager, PluginManagerExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type TaskHandlerFn = Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>;

pub struct NativeExecutor {
    handlers: Arc<Mutex<HashMap<String, TaskHandlerFn>>>,
}

impl NativeExecutor {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[allow(dead_code)]
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
    fn if_accept(&self, task_type: &str) -> bool {
        self.handlers.lock().unwrap().contains_key(task_type)
    }
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

#[allow(dead_code)]
pub struct WasmExecutor<M: PluginManager> {
    plugin_manager: Arc<M>,
    plugin_id: String,
    function_name: String,
}

#[allow(dead_code)]
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
    fn if_accept(&self, _task_type: &str) -> bool {
        // For simplicity, we assume this executor can handle all task types.
        true
    }
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
