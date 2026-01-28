use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use std::sync::Arc;

use crate::traits::{PluginManager, PluginManagerExt};

/// Generic hook system
/// E: Event type
#[async_trait]
pub trait Hook<E>: Send + Sync
where
    E: Send + Sync,
{
    async fn on_event(&self, event: &E) -> Result<()>;
}

/// Simple hook manager to manage multiple hooks
#[derive(Clone)]
pub struct HookManager<E>
where
    E: Send + Sync,
{
    hooks: Vec<Arc<dyn Hook<E>>>,
}

impl<E> HookManager<E>
where
    E: Send + Sync,
{
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn add_hook(&mut self, hook: Arc<dyn Hook<E>>) {
        self.hooks.push(hook);
    }

    /// Trigger all hooks for an event
    pub async fn trigger(&self, event: &E) {
        for hook in &self.hooks {
            let _ = hook.on_event(event).await;
        }
    }
}

impl<E> Default for HookManager<E>
where
    E: Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin provided hook
pub struct PluginHook<M: PluginManager> {
    plugin_manager: Arc<M>,
    plugin_id: String,
    function_name: String,
}

impl<M: PluginManager> PluginHook<M> {
    pub fn new(plugin_manager: Arc<M>, plugin_id: String, function_name: String) -> Self {
        Self {
            plugin_manager,
            plugin_id,
            function_name,
        }
    }
}

#[async_trait]
impl<M, E> Hook<E> for PluginHook<M>
where
    M: PluginManager + Send + Sync,
    E: Serialize + Send + Sync,
{
    async fn on_event(&self, event: &E) -> Result<()> {
        if let Err(e) = self
            .plugin_manager
            .call::<_, ()>(&self.plugin_id, &self.function_name, event)
            .await
        {
            tracing::warn!(
                "Hook '{}' on plugin '{}' failed: {}",
                self.function_name,
                self.plugin_id,
                e
            );
        }
        Ok(())
    }
}
