use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use extism::Plugin;
use tracing::{info, instrument};

use crate::{
    config::PluginConfig, error::PluginError, loader::PluginBundle, runtime::PluginBuilder,
    traits::PluginManager,
};

/// Manages the lifecycle and execution of Extism plugins.
///
/// This manager handles thread-safe access to plugin instances. Since Extism plugins
/// are stateful and single-threaded, each instance is wrapped in a Mutex.
pub struct ExtismPluginManager {
    config: PluginConfig,
    registry: Arc<RwLock<HashMap<String, Mutex<Plugin>>>>,
}

impl ExtismPluginManager {
    pub fn new(config: PluginConfig) -> Self {
        // Ensure the base plugins directory exists upon initialization
        if !config.plugins_dir.exists() {
            let _ = fs::create_dir_all(&config.plugins_dir);
        }

        Self {
            config,
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl PluginManager for ExtismPluginManager {
    #[instrument(skip(self), fields(plugin_id = %plugin_id))]
    fn load_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        info!("Attempting to load plugin bundle...");

        let plugin_dir = self.config.plugins_dir.join(plugin_id);
        let bundle = PluginBundle::load_from_dir(&plugin_dir)?;

        // TODO: Make this environment-specific in the future
        let entry = bundle
            .manifest
            .server
            .as_ref()
            .map(|c| c.entry.clone())
            .ok_or_else(|| {
                PluginError::LoadFailed("No [server] config found in manifest".into())
            })?;

        let wasm_path = bundle.root_dir.join(entry);

        let plugin = PluginBuilder::new(wasm_path)
            .with_wasi(self.config.enable_wasi)
            .build()?;

        let mut registry = self
            .registry
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;

        registry.insert(plugin_id.to_string(), Mutex::new(plugin));

        info!(
            "Plugin '{}' (v{}) loaded successfully",
            bundle.manifest.name, bundle.manifest.version
        );
        Ok(())
    }

    fn has_plugin(&self, plugin_id: &str) -> bool {
        self.registry
            .read()
            .map(|r| r.contains_key(plugin_id))
            .unwrap_or(false)
    }

    #[instrument(skip(self, input), fields(id = %plugin_id, func = %func_name))]
    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError> {
        // Acquire read lock to find the plugin
        let registry = self
            .registry
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        let plugin_mutex = registry
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        // Acquire exclusive lock on the specific plugin instance for execution
        let mut plugin = plugin_mutex
            .lock()
            .map_err(|_| PluginError::Internal("Failed to acquire plugin mutex".into()))?;

        // Execute the function in the Wasm module
        let output: Vec<u8> = plugin
            .call(func_name, input)
            .map_err(|e| PluginError::ExecutionFailed(e.to_string()))?;

        Ok(output)
    }
}
