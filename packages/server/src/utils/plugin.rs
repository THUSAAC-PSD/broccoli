use plugin_core::traits::PluginManager;
use sea_orm::*;
use std::sync::Arc;
use tracing::instrument;

use plugin_core::error::PluginError;
use plugin_core::hook::PluginHook;
use plugin_core::manifest::PluginManifest;

use crate::entity::plugin as plugin_entity;
use crate::state::{AppState, RegistryState};

/// Purges all runtime registry and hook entries that were registered by the given plugin.
/// This must be called before reloading or unloading a plugin so stale references do not survive.
pub async fn purge_plugin_registrations(registries: &RegistryState, plugin_id: &str) {
    registries
        .contest_type_registry
        .write()
        .await
        .retain(|_, h| h.plugin_id != plugin_id);
    registries
        .evaluator_registry
        .write()
        .await
        .retain(|_, h| h.plugin_id != plugin_id);
    registries
        .checker_format_registry
        .write()
        .await
        .retain(|_, h| h.plugin_id != plugin_id);
    registries
        .language_resolver_registry
        .write()
        .await
        .retain(|_, h| h.plugin_id != plugin_id);
    registries
        .hook_registry
        .write()
        .await
        .unregister_plugin(plugin_id);
}

/// Calls a plugin's `init()` function, handling expected non-error cases gracefully.
pub async fn call_plugin_init(
    plugins: &dyn PluginManager,
    plugin_id: &str,
) -> Result<(), PluginError> {
    match plugins.call_raw(plugin_id, "init", vec![]).await {
        Ok(_) => {
            tracing::info!("Plugin '{}' init() complete", plugin_id);
            Ok(())
        }
        Err(plugin_core::error::PluginError::NoRuntime(_)) => {
            tracing::debug!("Plugin '{}' is frontend-only, skipping init()", plugin_id);
            Ok(())
        }
        Err(plugin_core::error::PluginError::FunctionNotFound { .. }) => {
            tracing::debug!("Plugin '{}' has no init() function (optional)", plugin_id);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn plugin_manifest(
    plugins: &dyn PluginManager,
    plugin_id: &str,
) -> Result<PluginManifest, PluginError> {
    let registry = plugins
        .get_registry()
        .read()
        .map_err(|_| PluginError::Internal("Failed to acquire plugin registry read lock".into()))?;
    let entry = registry
        .get(plugin_id)
        .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
    Ok(entry.manifest.clone())
}

fn mark_plugin_failed(
    plugins: &dyn PluginManager,
    plugin_id: &str,
    error: &PluginError,
) -> Result<(), PluginError> {
    let mut registry = plugins.get_registry().write().map_err(|_| {
        PluginError::Internal("Failed to acquire plugin registry write lock".into())
    })?;
    let entry = registry
        .get_mut(plugin_id)
        .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
    entry.runtime = None;
    entry.status = plugin_core::registry::PluginStatus::Failed(error.to_string());
    Ok(())
}

pub async fn activate_plugin(state: &AppState, plugin_id: &str) -> Result<(), PluginError> {
    state.plugins.load_plugin(plugin_id)?;

    let manifest = plugin_manifest(state.plugins.as_ref(), plugin_id)?;
    register_plugin_hooks(state, plugin_id, &manifest).await;

    match call_plugin_init(state.plugins.as_ref(), plugin_id).await {
        Ok(()) => {
            state.plugins.update_translations()?;
            Ok(())
        }
        Err(error) => {
            purge_plugin_registrations(&state.registries, plugin_id).await;
            let _ = state.plugins.unload_plugin(plugin_id);
            let _ = mark_plugin_failed(state.plugins.as_ref(), plugin_id, &error);
            tracing::error!("Plugin '{}' init() failed: {}", plugin_id, error);
            Err(error)
        }
    }
}

#[instrument(skip(state))]
pub async fn sync_plugins(state: &AppState) -> anyhow::Result<()> {
    state.plugins.discover_plugins()?;

    for plugin in state.plugins.list_plugins()? {
        let plugin_id = plugin.id.clone();

        let plugin_model = plugin_entity::Entity::find_by_id(plugin_id.clone())
            .one(&state.db)
            .await?;
        let plugin_model = match plugin_model {
            None => {
                // Insert new plugin record if it doesn't exist
                let new_plugin = plugin_entity::ActiveModel {
                    id: Set(plugin_id),
                    is_enabled: Set(true),
                    updated_at: Set(chrono::Utc::now()),
                };
                new_plugin.insert(&state.db).await?
            }
            Some(existing_plugin) => existing_plugin,
        };

        if plugin_model.is_enabled {
            match activate_plugin(state, &plugin.id).await {
                Ok(()) => {
                    tracing::info!("Plugin '{}' loaded successfully", plugin.id);
                }
                Err(error) => {
                    tracing::error!("Plugin '{}' activation failed: {}", plugin.id, error);
                }
            }
        } else {
            tracing::info!("Plugin '{}' is disabled, skipping load", plugin.id);
        }
    }

    Ok(())
}

/// Register hooks from a plugin's manifest into the server hook registry.
async fn register_plugin_hooks(
    state: &AppState,
    plugin_id: &str,
    manifest: &plugin_core::manifest::PluginManifest,
) {
    let server_config = match &manifest.server {
        Some(sc) => sc,
        None => return,
    };

    if server_config.hooks.is_empty() {
        return;
    }

    let mut registry = state.registries.hook_registry.write().await;

    for decl in &server_config.hooks {
        // Warn (but allow) notify mode on before_* topics.
        if decl.mode == plugin_core::hook::HookMode::Notify && decl.topic.starts_with("before_") {
            tracing::warn!(
                plugin_id,
                topic = %decl.topic,
                function = %decl.function,
                "Notify hook registered on blocking topic (before_*). \
                 Its response will be ignored at runtime.",
            );
        }

        // Warn (but allow) blocking mode on after_* topics.
        if decl.mode == plugin_core::hook::HookMode::Blocking && decl.topic.starts_with("after_") {
            tracing::warn!(
                plugin_id,
                topic = %decl.topic,
                function = %decl.function,
                "Blocking hook registered on background topic (after_*). \
                 Reject/Stop responses will be silently discarded at runtime.",
            );
        }

        let hook = Arc::new(PluginHook::new(
            state.plugins.clone(),
            plugin_id.to_string(),
            decl.function.clone(),
            vec![decl.topic.clone()],
            decl.scope,
            decl.mode,
        ));

        registry.register(hook);

        tracing::info!(
            plugin_id,
            topic = %decl.topic,
            function = %decl.function,
            scope = ?decl.scope,
            mode = ?decl.mode,
            "Registered hook",
        );
    }
}
