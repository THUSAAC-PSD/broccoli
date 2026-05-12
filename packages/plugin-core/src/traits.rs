use std::time::Duration;

use async_trait::async_trait;
use extism::{Manifest, PluginBuilder, PoolBuilder, Wasm};
use opentelemetry::KeyValue;
use serde::{Serialize, de::DeserializeOwned};
use tracing::{debug, error, info, instrument, warn};

use crate::config::PluginConfig;
use crate::error::{AssetError, PluginError};
use crate::host::HostFunctionRegistry;
use crate::i18n::{I18nRegistry, TranslationMap};
use crate::manifest::PluginManifest;
use crate::registry::{PluginEntry, PluginInfo, PluginRegistry, PluginStatus};

#[async_trait]
pub trait PluginManager: Send + Sync {
    fn get_registry(&self) -> &PluginRegistry;

    fn get_config(&self) -> &PluginConfig;

    fn get_host_functions(&self) -> &HostFunctionRegistry;

    fn get_i18n_registry(&self) -> &I18nRegistry;

    fn get_metrics(&self) -> Option<&common::metrics::Metrics> {
        None
    }

    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)>;

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
            return Ok(());
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
            let max_instances = self.get_config().pool_max_instances;
            let pool = PoolBuilder::new()
                .with_max_instances(max_instances)
                .build(move || {
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
        plugin_entry.status = PluginStatus::Unloaded;

        info!("Plugin {} unloaded successfully", plugin_entry.manifest);

        Ok(())
    }

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

    fn list_plugins(&self) -> Result<Vec<PluginInfo>, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        Ok(registry.values().map(PluginInfo::from).collect())
    }

    fn has_plugin(&self, plugin_id: &str) -> Result<bool, PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        Ok(registry.contains_key(plugin_id))
    }

    fn resolve_plugin_asset(
        &self,
        plugin_id: &str,
        asset_path: &str,
    ) -> Result<std::path::PathBuf, AssetError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| AssetError::Internal("Failed to acquire registry read lock".into()))?;

        let plugin_entry = registry.get(plugin_id).ok_or(AssetError::NotFound)?;
        plugin_entry.resolve_web_asset(asset_path)
    }

    fn update_translations(&self) -> Result<(), PluginError> {
        let registry = self
            .get_registry()
            .read()
            .map_err(|_| PluginError::Internal("Failed to acquire registry read lock".into()))?;

        let i18n = self.get_i18n_registry();
        i18n.clear();

        let loaded_plugins = registry
            .values()
            .filter(|e| e.status == PluginStatus::Loaded);
        for plugin_entry in loaded_plugins {
            for (lang, path) in &plugin_entry.manifest.translations {
                let translation_path = plugin_entry.root_dir.join(path);
                let content = std::fs::read_to_string(&translation_path).map_err(|e| {
                    PluginError::LoadFailed(format!(
                        "Failed to read translation file '{}': {}",
                        translation_path.display(),
                        e
                    ))
                })?;
                let translation: TranslationMap = toml::from_str(&content).map_err(|e| {
                    PluginError::LoadFailed(format!(
                        "Invalid translation file syntax in '{}': {}",
                        translation_path.display(),
                        e
                    ))
                })?;
                i18n.merge(lang.clone(), translation);
            }
        }

        Ok(())
    }

    fn reload_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let root_dir = {
            let registry = self.get_registry().read().map_err(|_| {
                PluginError::Internal("Failed to acquire registry read lock".into())
            })?;
            let entry = registry
                .get(plugin_id)
                .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
            entry.root_dir.clone()
        };

        let new_entry = PluginEntry::from_dir(&root_dir)?;

        {
            let mut registry = self.get_registry().write().map_err(|_| {
                PluginError::Internal("Failed to acquire registry write lock".into())
            })?;
            registry.insert(plugin_id.to_string(), new_entry);
        }

        self.load_plugin(plugin_id)?;
        Ok(())
    }

    fn rediscover_plugins(&self) -> Result<Vec<String>, PluginError> {
        let plugins_dir = &self.get_config().plugins_dir;

        if !plugins_dir.exists() || !plugins_dir.is_dir() {
            return Err(PluginError::DiscoveryFailed(format!(
                "Plugins directory '{}' does not exist or is not a directory",
                plugins_dir.display()
            )));
        }

        let entries = std::fs::read_dir(plugins_dir).map_err(PluginError::Io)?;
        let mut registry = self
            .get_registry()
            .write()
            .map_err(|_| PluginError::Internal("Failed to acquire registry write lock".into()))?;
        let mut new_ids = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                PluginError::LoadFailed(format!("Failed to read plugin entry: {}", e))
            })?;
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                let id = entry.file_name().to_string_lossy().to_string();
                if registry.contains_key(&id) {
                    continue;
                }
                match PluginEntry::from_dir(&entry.path()) {
                    Ok(plugin_entry) => {
                        info!("Discovered new plugin: {}", plugin_entry.manifest);
                        registry.insert(id.clone(), plugin_entry);
                        new_ids.push(id);
                    }
                    Err(e) => {
                        error!("Failed to parse plugin '{}': {}", id, e);
                    }
                }
            }
        }

        Ok(new_ids)
    }

    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError> {
        self.call_raw_with_host_context(plugin_id, func_name, input, None)
            .await
    }

    #[instrument(skip(self, input, host_context), fields(plugin_id = %plugin_id, function = %func_name))]
    async fn call_raw_with_host_context(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
        host_context: Option<serde_json::Value>,
    ) -> Result<Vec<u8>, PluginError> {
        let start = std::time::Instant::now();

        let timeout = Duration::from_secs(self.get_config().call_timeout_secs);
        let pool = {
            let registry = self.get_registry().read().map_err(|_| {
                PluginError::Internal("Failed to acquire registry read lock".into())
            })?;

            let plugin_entry = registry
                .get(plugin_id)
                .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

            if plugin_entry.status != PluginStatus::Loaded {
                return Err(PluginError::NotLoaded(plugin_id.to_string()));
            }

            plugin_entry
                .runtime
                .as_ref()
                .ok_or_else(|| PluginError::NoRuntime(plugin_id.to_string()))?
                .clone()
        };

        let metrics = self.get_metrics().cloned();
        let plugin_id_owned = plugin_id.to_string();
        let func_name_owned = func_name.to_string();

        let result = tokio::task::spawn_blocking({
            let plugin_id = plugin_id_owned.clone();
            let func_name = func_name_owned.clone();
            let metrics = metrics.clone();
            move || {
                crate::host_context::with_context(host_context, || {
                    let acquire_start = std::time::Instant::now();
                    let plugin = match pool.get(timeout) {
                        Ok(Some(plugin)) => {
                            if let Some(metrics) = metrics.as_ref() {
                                metrics.plugin_instance_acquire_duration.record(
                                    acquire_start.elapsed().as_secs_f64(),
                                    &[
                                        KeyValue::new("plugin.id", plugin_id.clone()),
                                        KeyValue::new("plugin.function", func_name.clone()),
                                        KeyValue::new("outcome", "success"),
                                    ],
                                );
                            }
                            plugin
                        }
                        Ok(None) => {
                            if let Some(metrics) = metrics.as_ref() {
                                metrics.plugin_instance_acquire_duration.record(
                                    acquire_start.elapsed().as_secs_f64(),
                                    &[
                                        KeyValue::new("plugin.id", plugin_id.clone()),
                                        KeyValue::new("plugin.function", func_name.clone()),
                                        KeyValue::new("outcome", "timeout"),
                                    ],
                                );
                                metrics.plugin_instance_acquire_failures.add(
                                    1,
                                    &[
                                        KeyValue::new("plugin.id", plugin_id.clone()),
                                        KeyValue::new("plugin.function", func_name.clone()),
                                        KeyValue::new("failure.kind", "timeout"),
                                    ],
                                );
                            }
                            return Err(PluginError::PoolTimeout(plugin_id.clone()));
                        }
                        Err(e) => {
                            if let Some(metrics) = metrics.as_ref() {
                                metrics.plugin_instance_acquire_duration.record(
                                    acquire_start.elapsed().as_secs_f64(),
                                    &[
                                        KeyValue::new("plugin.id", plugin_id.clone()),
                                        KeyValue::new("plugin.function", func_name.clone()),
                                        KeyValue::new("outcome", "error"),
                                    ],
                                );
                                metrics.plugin_instance_acquire_failures.add(
                                    1,
                                    &[
                                        KeyValue::new("plugin.id", plugin_id.clone()),
                                        KeyValue::new("plugin.function", func_name.clone()),
                                        KeyValue::new("failure.kind", "error"),
                                    ],
                                );
                            }
                            return Err(PluginError::Internal(format!(
                                "Failed to acquire runtime instance for plugin '{}': {}",
                                plugin_id, e
                            )));
                        }
                    };

                    if !plugin.plugin().function_exists(&func_name) {
                        return Err(PluginError::FunctionNotFound {
                            plugin_id: plugin_id.clone(),
                            func_name: func_name.clone(),
                        });
                    }

                    plugin.call(func_name.as_str(), input).map_err(|e| {
                        PluginError::ExecutionFailed {
                            plugin_id: plugin_id.clone(),
                            func_name: func_name.clone(),
                            message: e.to_string(),
                        }
                    })
                })
            }
        })
        .await
        .map_err(|e| PluginError::Internal(format!("plugin task join failed: {e}")))?;

        let duration = start.elapsed();
        if let Some(metrics) = self.get_metrics() {
            let outcome = if result.is_ok() { "success" } else { "error" };
            let attrs = [
                KeyValue::new("plugin.id", plugin_id.to_string()),
                KeyValue::new("plugin.function", func_name.to_string()),
                KeyValue::new("outcome", outcome),
            ];
            metrics
                .plugin_call_duration
                .record(duration.as_secs_f64(), &attrs);
            if result.is_err() {
                metrics.plugin_call_failures.add(1, &attrs);
            }
        }
        debug!(
            duration_ms = duration.as_millis() as u64,
            success = result.is_ok(),
            "plugin call completed"
        );

        Ok(result?)
    }
}

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

impl<T: ?Sized + PluginManager> PluginManagerExt for T {}
