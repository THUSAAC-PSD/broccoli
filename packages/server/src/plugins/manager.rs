use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use extism::{Manifest, Plugin, Wasm};
use tracing::{info, instrument};

use crate::plugins::{
    config::PluginConfig, error::PluginError, manifest::PluginManifest, traits::PluginManager,
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

    /// Internal helper to locate and parse the plugin manifest file.
    fn read_manifest(&self, plugin_id: &str) -> Result<(PluginManifest, PathBuf), PluginError> {
        let plugin_dir = self.config.plugins_dir.join(plugin_id);

        if !plugin_dir.exists() || !plugin_dir.is_dir() {
            return Err(PluginError::NotFound(format!(
                "Directory for plugin '{}' not found at {:?}",
                plugin_id, plugin_dir
            )));
        }

        let toml_path = plugin_dir.join("plugin.toml");
        let toml_content = fs::read_to_string(&toml_path)
            .map_err(|e| PluginError::LoadFailed(format!("Failed to read plugin.toml: {}", e)))?;

        let manifest: PluginManifest = toml::from_str(&toml_content)
            .map_err(|e| PluginError::LoadFailed(format!("Invalid plugin.toml syntax: {}", e)))?;

        Ok((manifest, plugin_dir))
    }
}

#[async_trait]
impl PluginManager for ExtismPluginManager {
    #[instrument(skip(self), fields(plugin_id = %plugin_id))]
    fn load_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        info!("Attempting to load plugin bundle...");

        // Parse the configuration from plugin.toml
        let (manifest_data, plugin_dir) = self.read_manifest(plugin_id)?;

        // Resolve the absolute path to the Wasm binary
        let wasm_path = plugin_dir.join(&manifest_data.backend.wasm);
        if !wasm_path.exists() {
            return Err(PluginError::NotFound(format!(
                "Wasm binary not found at {:?}",
                wasm_path
            )));
        }

        // Initialize the Extism Manifest
        let wasm = Wasm::file(wasm_path);
        let extism_manifest = Manifest::new([wasm]);

        // Create the plugin instance
        // The third argument controls WASI support based on system config
        let plugin = Plugin::new(&extism_manifest, [], self.config.enable_wasi)?;

        // Acquire write lock to update the registry
        let mut registry = self
            .registry
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;

        registry.insert(plugin_id.to_string(), Mutex::new(plugin));

        info!(
            "Plugin '{}' (v{}) loaded successfully",
            manifest_data.name, manifest_data.version
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
