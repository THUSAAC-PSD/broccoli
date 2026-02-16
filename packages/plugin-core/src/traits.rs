use async_trait::async_trait;
use extism::{Manifest, PluginBuilder, Pool, Wasm};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;
use tracing::{error, info, instrument, warn};

use crate::config::PluginConfig;
use crate::error::PluginError;
use crate::host::HostFunctionRegistry;
use crate::manifest::PluginManifest;
use crate::registry::{PluginEntry, PluginInfo, PluginRegistry, PluginStatus};

#[async_trait]
pub trait PluginManager: Send + Sync {
    /// Returns a reference to the plugin registry.
    fn get_registry(&self) -> &PluginRegistry;

    /// Returns a reference to the plugin config.
    fn get_config(&self) -> &PluginConfig;

    /// Returns a reference to the host function registry.
    fn get_host_functions(&self) -> &HostFunctionRegistry;

    /// Resolves the appropriate Wasm entry point and permissions from the manifest.
    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)>;

    /// Scans the plugins directory and updates the registry with discovered plugins.
    fn discover_plugins(&self) -> Result<(), PluginError> {
        let plugins_dir = &self.get_config().plugins_dir;

        if !plugins_dir.exists() || !plugins_dir.is_dir() {
            return Err(PluginError::DiscoveryFailed(format!(
                "Plugins directory '{}' does not exist or is not a directory",
                plugins_dir.display()
            )));
        }

        info!(
            "Discovering plugins in directory: {}",
            plugins_dir.display()
        );

        let entries = std::fs::read_dir(plugins_dir).map_err(PluginError::Io)?;
        let mut registry = self
            .get_registry()
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                PluginError::LoadFailed(format!("Failed to read plugin entry: {}", e))
            })?;
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                let id = entry.file_name().to_string_lossy().to_string();
                match PluginEntry::from_dir(&entry.path()) {
                    Ok(plugin_entry) => {
                        info!("Discovered plugin: {}", plugin_entry.manifest);
                        if plugin_entry.manifest.is_hollow() {
                            warn!(
                                "Plugin '{}' is hollow. It might be misconfigured.",
                                plugin_entry.id
                            );
                        }
                        registry.insert(id, plugin_entry);
                    }
                    Err(e) => {
                        error!("Failed to parse plugin '{}': {}", id, e);
                    }
                }
            }
        }

        Ok(())
    }

    // Loads a plugin by its ID, initializing the Wasm runtime as needed.
    #[instrument(skip(self), fields(plugin_id = %plugin_id))]
    fn load_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut registry = self
            .get_registry()
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;

        let plugin_entry = registry
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        if plugin_entry.status == PluginStatus::Loaded {
            return Ok(()); // Already loaded
        }

        let mut runtime = None;
        if let Some((entry_point, permissions)) = self.resolve(&plugin_entry.manifest) {
            let wasm_path = plugin_entry.root_dir.join(&entry_point);
            if !wasm_path.exists() {
                return Err(PluginError::LoadFailed(format!(
                    "Wasm file not found at path: {}",
                    wasm_path.display()
                )));
            }

            let manifest = Manifest::new([Wasm::file(&wasm_path)]);
            let host_functions = self.get_host_functions().resolve(plugin_id, &permissions);
            let wasi = self.get_config().enable_wasi;
            let pool = Pool::new(move || {
                PluginBuilder::new(&manifest)
                    .with_wasi(wasi)
                    .with_functions(host_functions.clone())
                    .build()
            });

            runtime = Some(pool);
        }

        plugin_entry.runtime = runtime;
        plugin_entry.status = PluginStatus::Loaded;

        info!("Plugin {} loaded successfully", plugin_entry.manifest);
        Ok(())
    }

    /// Unloads a plugin by its ID, cleaning up the Wasm runtime and resources.
    #[instrument(skip(self), fields(plugin_id = %plugin_id))]
    fn unload_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut registry = self
            .get_registry()
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;

        let plugin_entry = registry
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        plugin_entry.runtime = None;
        plugin_entry.status = PluginStatus::Discovered;

        info!("Plugin {} unloaded successfully", plugin_entry.manifest);

        Ok(())
    }

    /// Checks if a plugin is currently loaded.
    fn is_plugin_loaded(&self, plugin_id: &str) -> Result<bool, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        let plugin_entry = registry
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        Ok(plugin_entry.status == PluginStatus::Loaded)
    }

    /// Lists all plugins.
    fn list_plugins(&self) -> Result<Vec<PluginInfo>, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        Ok(registry.values().map(PluginInfo::from).collect())
    }

    /// Checks if a plugin exists in the registry.
    fn has_plugin(&self, plugin_id: &str) -> Result<bool, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        Ok(registry.contains_key(plugin_id))
    }

    /// Low-level execution using raw bytes.
    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        let plugin_entry = registry
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        if plugin_entry.status != PluginStatus::Loaded {
            return Err(PluginError::NotLoaded(plugin_id.to_string()));
        }

        let pool = plugin_entry
            .runtime
            .as_ref()
            .ok_or_else(|| PluginError::NoRuntime(plugin_id.to_string()))?;

        let plugin = pool
            .get(Duration::new(1, 0))
            .map_err(|_| {
                PluginError::Internal(format!(
                    "Failed to acquire runtime instance for plugin '{}'",
                    plugin_id
                ))
            })?
            .ok_or_else(|| {
                PluginError::Internal(format!(
                    "Timeout while acquiring runtime instance for plugin '{}'",
                    plugin_id
                ))
            })?;

        let result = plugin
            .call(func_name, input)
            .map_err(|e| PluginError::ExecutionFailed {
                plugin_id: plugin_id.to_string(),
                func_name: func_name.to_string(),
                message: e.to_string(),
            })?;

        Ok(result)
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
