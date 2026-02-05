// Example: Simple Logger Hook

use anyhow::Result;
use async_trait::async_trait;
use common::hook::{Hook, HookAction};
use common::worker::TaskEvent;

/// A simple logger hook that logs all task events
pub struct LoggerHook;

#[async_trait]
impl Hook<TaskEvent> for LoggerHook {
    type Output = TaskEvent;
    type Context = ();

    fn id(&self) -> &str {
        "logger_hook"
    }

    fn topics(&self) -> &[&str] {
        &["task_started", "task_completed", "task_failed"]
    }

    async fn on_event(&self, _ctx: (), event: &TaskEvent) -> Result<HookAction<TaskEvent>> {
        match event {
            TaskEvent::Started { task } => {
                println!("✓ Task started: {} (type: {})", task.id, task.task_type);
            }
            TaskEvent::Completed { result } => {
                println!(
                    "✓ Task completed: {} (success: {})",
                    result.task_id, result.success
                );
            }
            TaskEvent::Failed { task, error } => {
                println!("✗ Task error: {} - {}", task.id, error);
            }
        }
        Ok(HookAction::Pass)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Example usage of LoggerHook
    let mut registry = common::hook::HookRegistry::new(());

    // Register the logger hook
    registry.add_hook(LoggerHook)?;

    println!("Logger hook example registered successfully!");
    println!("The LoggerHook will log all task events it receives.");

    Ok(())
}
