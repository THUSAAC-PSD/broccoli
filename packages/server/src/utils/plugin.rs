use plugin_core::traits::PluginManager;
use sea_orm::*;
use tracing::instrument;

use crate::entity::plugin as plugin_entity;
use crate::state::{AppState, RegistryState};

/// Purges all runtime registry entries (contest types, evaluators, checker formats)
/// that were registered by the given plugin. This must be called before reloading
/// a plugin so stale `PluginHandler` references don't survive.
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
}

/// Calls a plugin's `init()` function, handling expected non-error cases gracefully.
pub async fn call_plugin_init(plugins: &dyn PluginManager, plugin_id: &str) {
    match plugins.call_raw(plugin_id, "init", vec![]).await {
        Ok(_) => {
            tracing::info!("Plugin '{}' init() complete", plugin_id);
        }
        Err(plugin_core::error::PluginError::NoRuntime(_)) => {
            tracing::debug!("Plugin '{}' is frontend-only, skipping init()", plugin_id);
        }
        Err(plugin_core::error::PluginError::FunctionNotFound { .. }) => {
            tracing::debug!("Plugin '{}' has no init() function (optional)", plugin_id);
        }
        Err(e) => {
            tracing::error!("Plugin '{}' init() failed: {}", plugin_id, e);
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
            state.plugins.load_plugin(&plugin.id)?;
            tracing::info!("Plugin '{}' loaded successfully", plugin.id);
            call_plugin_init(state.plugins.as_ref(), &plugin.id).await;
        } else {
            tracing::info!("Plugin '{}' is disabled, skipping load", plugin.id);
        }
    }

    state.plugins.update_translations()?;

    Ok(())
}
