use anyhow::Result;
use async_trait::async_trait;
use extism::*;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use common::worker::*;

/// Native executor to run Rust functions
pub struct NativeExecutor {
    handlers: Arc<Mutex<HashMap<String, Box<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>>>>,
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
        self.handlers.lock().unwrap().insert(task_type, Box::new(handler));
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

// WASM plugin executor
pub struct WasmExecutor {
    plugin: Arc<Mutex<Plugin>>,
}

impl WasmExecutor {
    // TODO: wasi should not be enabled by default
    pub fn new(wasm_path: &str) -> Result<Self> {
        let manifest = Manifest::new([Wasm::file(wasm_path)]);
        let plugin = Plugin::new(&manifest, [], true)?;
        Ok(Self { 
            plugin: Arc::new(Mutex::new(plugin))
        })
    }
    pub fn new_from_bytes(wasm_bytes: &[u8]) -> Result<Self> {
        let manifest = Manifest::new([Wasm::data(wasm_bytes)]);
        let plugin = Plugin::new(&manifest, [], true)?;
        Ok(Self { 
            plugin: Arc::new(Mutex::new(plugin))
        })
    }
}

#[async_trait]
impl Executor for WasmExecutor {
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let input = serde_json::to_vec(&task)?;
        let mut plugin = self.plugin.lock().unwrap();
        
        match plugin.call::<&[u8], Vec<u8>>("execute_task", &input) {
            Ok(output_bytes) => {
                let result: TaskResult = serde_json::from_slice(&output_bytes)?;
                Ok(result)
            }
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }
}

pub struct Worker {
    executors: Arc<Mutex<HashMap<String, Arc<dyn Executor>>>>,
    hooks: Arc<Mutex<Vec<Arc<dyn Hook>>>>,
}

impl Worker {
    pub fn new() -> Self {
        Self {
            executors: Arc::new(Mutex::new(HashMap::new())),
            hooks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn register_executor(&self, name: String, executor: Arc<dyn Executor>) {
        self.executors.lock().unwrap().insert(name, executor);
    }

    pub fn add_hook(&self, hook: Arc<dyn Hook>) {
        self.hooks.lock().unwrap().push(hook);
    }

    pub async fn execute_task(&self, task: Task, executor_name: &str) -> Result<TaskResult> {
        let hooks = self.hooks.lock().unwrap().clone();
        for hook in hooks.iter() {
            let _ = hook.on_task_start(&task).await;
        }

        let executor = self.executors.lock().unwrap()
            .get(executor_name)
            .cloned();
            
        let result = if let Some(executor) = executor {
            match executor.execute(task.clone()).await {
                Ok(result) => {
                    // Trigger on_task_complete hooks
                    for hook in hooks.iter() {
                        let _ = hook.on_task_complete(&result).await;
                    }
                    result
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Trigger on_task_error hooks
                    for hook in hooks.iter() {
                        let _ = hook.on_task_error(&task, &error_msg).await;
                    }
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

pub struct WasmHook {
    plugin: Arc<Mutex<Plugin>>,
}

impl WasmHook {
    pub fn new(wasm_path: &str) -> Result<Self> {
        let manifest = Manifest::new([Wasm::file(wasm_path)]);
        let plugin = Plugin::new(&manifest, [], true)?;
        Ok(Self {
            plugin: Arc::new(Mutex::new(plugin)),
        })
    }

    pub fn new_from_bytes(wasm_bytes: &[u8]) -> Result<Self> {
        let manifest = Manifest::new([Wasm::data(wasm_bytes)]);
        let plugin = Plugin::new(&manifest, [], true)?;
        Ok(Self {
            plugin: Arc::new(Mutex::new(plugin)),
        })
    }

    // Helper to call plugin hook functions
    fn call_hook(&self, fn_name: &str, input: &[u8]) -> Result<()> {
        let mut plugin = self.plugin.lock().unwrap();
        
        match plugin.call::<&[u8], Vec<u8>>(fn_name, input) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into())
        }
    }
}

#[async_trait]
impl Hook for WasmHook {
    async fn on_task_start(&self, task: &Task) -> Result<()> {
        let input = serde_json::to_vec(task)?;
        self.call_hook("on_task_start", &input)
    }

    async fn on_task_complete(&self, result: &TaskResult) -> Result<()> {
        let input = serde_json::to_vec(result)?;
        self.call_hook("on_task_complete", &input)
    }

    async fn on_task_error(&self, task: &Task, error: &str) -> Result<()> {
        #[derive(Serialize)]
        struct ErrorPayload<'a> {
            task: &'a Task,
            error: &'a str,
        }
        let payload = ErrorPayload { task, error };
        let input = serde_json::to_vec(&payload)?;
        self.call_hook("on_task_error", &input)
    }
}

// Example hook: Simple logger
pub struct LoggerHook;

#[async_trait]
impl Hook for LoggerHook {
    async fn on_task_start(&self, task: &Task) -> Result<()> {
        tracing::info!("Task started: {} (type: {})", task.id, task.task_type);
        Ok(())
    }

    async fn on_task_complete(&self, result: &TaskResult) -> Result<()> {
        tracing::info!("Task completed: {} (success: {})", result.task_id, result.success);
        Ok(())
    }

    async fn on_task_error(&self, task: &Task, error: &str) -> Result<()> {
        tracing::error!("Task error: {} - {}", task.id, error);
        Ok(())
    }
}
