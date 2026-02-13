use sea_orm::*;
use tracing::instrument;

use crate::entity::plugin as plugin_entity;
use crate::state::AppState;

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
        } else {
            tracing::info!("Plugin '{}' is disabled, skipping load", plugin.id);
        }
    }

    Ok(())
}
