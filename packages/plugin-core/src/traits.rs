use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use extism::Plugin;
use serde::{Serialize, de::DeserializeOwned};
use tracing::{info, instrument};

use crate::config::PluginConfig;
use crate::error::PluginError;
use crate::host::HostFunctionRegistry;
use crate::loader::PluginBundle;
use crate::manifest::PluginManifest;
use crate::runtime::PluginBuilder;

pub type PluginMap = Arc<RwLock<HashMap<String, Mutex<Plugin>>>>;

#[async_trait]
pub trait PluginManager: Send + Sync {
    /// Returns a reference to the plugin registry.
    fn get_registry(&self) -> &PluginMap;

    /// Returns a reference to the plugin config.
    fn get_config(&self) -> &PluginConfig;

    /// Returns a reference to the host function registry.
    fn get_host_functions(&self) -> &HostFunctionRegistry;

    /// Resolves the appropriate entry point and permissions from the manifest.
    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)>;

    #[instrument(skip(self), fields(plugin_id = %plugin_id))]
    fn load_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        info!("Attempting to load plugin bundle...");

        let plugin_dir = self.get_config().plugins_dir.join(plugin_id);
        let bundle = PluginBundle::load_from_dir(&plugin_dir)?;

        let (entry, permissions) = self
            .resolve(&bundle.manifest)
            .ok_or_else(|| PluginError::LoadFailed("No matching entry found in manifest".into()))?;

        let wasm_path = bundle.root_dir.join(entry);

        let plugin = PluginBuilder::new(wasm_path)
            .with_wasi(true)
            .with_permissions(plugin_id, &permissions, self.get_host_functions())
            .build()?;

        let mut registry = self
            .get_registry()
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
        self.get_registry()
            .read()
            .map(|r| r.contains_key(plugin_id))
            .unwrap_or(false)
    }

    /// Low-level execution using raw bytes.
    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError> {
        // Acquire read lock to find the plugin
        let registry = self
            .get_registry()
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

/// Extension trait for typed calls.
/// Automatically implemented for any T that implements PluginManager.
#[async_trait]
pub trait PluginManagerExt: PluginManager {
    async fn call<T, R>(&self, plugin_id: &str, func_name: &str, input: T) -> Result<R, PluginError>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned + Send + Sync,
    {
        let input_bytes = serde_json::to_vec(&input)?;

        let output_bytes = self.call_raw(plugin_id, func_name, input_bytes).await?;

        let result = serde_json::from_slice(&output_bytes)?;
        Ok(result)
    }
}

// Blanket implementation
impl<T: ?Sized + PluginManager> PluginManagerExt for T {}
