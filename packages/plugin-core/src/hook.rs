use anyhow::Result;
use async_trait::async_trait;
use common::event::GenericEvent;
use common::hook::{GenericHook, GenericHookAction};
use std::sync::Arc;

use crate::traits::{PluginManager, PluginManagerExt};

/// Plugin-based hook that calls a plugin function
pub struct PluginHook<M: PluginManager> {
    plugin_manager: Arc<M>,
    plugin_id: String,
    function_name: String,
    topics: Vec<&'static str>,
}

impl<M: PluginManager> PluginHook<M> {
    pub fn new(
        plugin_manager: Arc<M>,
        plugin_id: String,
        function_name: String,
        topics: Vec<&'static str>,
    ) -> Self {
        Self {
            plugin_manager,
            plugin_id,
            function_name,
            topics,
        }
    }
}

#[async_trait]
impl<M: PluginManager + Send + Sync + 'static> GenericHook for PluginHook<M> {
    fn id(&self) -> &str {
        &self.plugin_id
    }

    fn topics(&self) -> &[&str] {
        &self.topics
    }

    async fn on_event(&self, event: &GenericEvent) -> Result<GenericHookAction> {
        // Call the plugin function with the event payload
        match self
            .plugin_manager
            .call::<_, serde_json::Value>(&self.plugin_id, &self.function_name, &event.payload)
            .await
        {
            // TODO: Handle modified event from plugin
            Ok(_) => Ok(GenericHookAction::Pass),
            Err(e) => {
                tracing::warn!(
                    "Plugin hook '{}' on plugin '{}' failed: {}",
                    self.function_name,
                    self.plugin_id,
                    e
                );
                // Continue with original event on error
                Ok(GenericHookAction::Pass)
            }
        }
    }
}
