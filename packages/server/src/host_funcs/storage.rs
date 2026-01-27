use extism::host_fn;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::Value;
use tracing::error;

use crate::entity::plugin_storage;

// KV Set: Upsert (Insert or Update) data for a collection
host_fn!(pub store_set(user_data: (String, DatabaseConnection); collection: String, key: String, value: String) -> () {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    // Parse input string as JSON
    let json_val: Value = serde_json::from_str(&value)
        .map_err(|e| extism::Error::msg(format!("Invalid JSON: {}", e)))?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let model = plugin_storage::ActiveModel {
                plugin_id: Set(plugin_id.clone()),
                collection: Set(collection),
                key: Set(key),
                data: Set(json_val),
                created_at: Set(chrono::Utc::now()),
            };

            plugin_storage::Entity::insert(model)
                .on_conflict(
                    OnConflict::columns([
                        plugin_storage::Column::PluginId,
                        plugin_storage::Column::Collection,
                        plugin_storage::Column::Key,
                    ])
                    .update_columns([
                        plugin_storage::Column::Data,
                        plugin_storage::Column::CreatedAt,
                    ])
                    .to_owned(),
                )
                .exec(db)
                .await
        })
    })
    .map_err(|e| {
        error!("DB store_set error: {}", e);
        extism::Error::msg("Database error")
    })?;

    Ok(())
});

// KV Get: Retrieve data by collection
host_fn!(pub store_get(user_data: (String, DatabaseConnection); collection: String, key: String) -> String {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_storage::Entity::find()
                .filter(plugin_storage::Column::PluginId.eq(plugin_id))
                .filter(plugin_storage::Column::Collection.eq(collection))
                .filter(plugin_storage::Column::Key.eq(key))
                .one(db)
                .await
        })
    })
        .map_err(|e| {
            error!("DB store_get error: {}", e);
            extism::Error::msg("Database error")
        })?;

    match result {
        Some(record) => Ok(record.data.to_string()),
        None => Ok("null".to_string()), // Return JSON null if not found
    }
});
