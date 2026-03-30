use extism::host_fn;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use serde::Deserialize;
use serde_json::Value;
use tracing::error;

use crate::entity::plugin_storage;

/// Default collection name used by the SDK's JSON-based storage API.
const DEFAULT_COLLECTION: &str = "default";

#[derive(Deserialize)]
struct StoreSetInput {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct StoreGetInput {
    key: String,
}

// KV Set: Upsert data. Accepts JSON: {"key": "...", "value": "..."}
host_fn!(pub store_set(user_data: (String, DatabaseConnection); input: String) -> () {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreSetInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_set input: {}", e)))?;

    // Store the value as a JSON string
    let json_val = Value::String(parsed.value);

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let model = plugin_storage::ActiveModel {
                plugin_id: Set(plugin_id.clone()),
                collection: Set(DEFAULT_COLLECTION.to_string()),
                key: Set(parsed.key),
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

// KV Get: Retrieve data. Accepts JSON: {"key": "..."}
// Returns JSON: {"value": "..."} or {"value": null}
host_fn!(pub store_get(user_data: (String, DatabaseConnection); input: String) -> String {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreGetInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_get input: {}", e)))?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_storage::Entity::find()
                .filter(plugin_storage::Column::PluginId.eq(plugin_id))
                .filter(plugin_storage::Column::Collection.eq(DEFAULT_COLLECTION))
                .filter(plugin_storage::Column::Key.eq(&parsed.key))
                .one(db)
                .await
        })
    })
        .map_err(|e| {
            error!("DB store_get error: {}", e);
            extism::Error::msg("Database error")
        })?;

    match result {
        Some(record) => {
            // Unwrap stored JSON string value back to plain string for the SDK
            let value_str = match &record.data {
                Value::String(s) => Value::String(s.clone()),
                other => other.clone(),
            };
            Ok(serde_json::json!({ "value": value_str }).to_string())
        }
        None => Ok(serde_json::json!({ "value": null }).to_string()),
    }
});

#[derive(Deserialize)]
struct StoreCasInput {
    key: String,
    expected: Option<String>,
    new: String,
}

// Set value only if current value matches expected.
//
// Returns JSON: {"swapped": true/false}
host_fn!(pub store_compare_and_set(user_data: (String, DatabaseConnection); input: String) -> String {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreCasInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_compare_and_set input: {e}")))?;

    let swapped = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let txn = db.begin().await?;

            // Read with FOR UPDATE to serialize concurrent CAS on the same key
            let current = plugin_storage::Entity::find()
                .filter(plugin_storage::Column::PluginId.eq(plugin_id))
                .filter(plugin_storage::Column::Collection.eq(DEFAULT_COLLECTION))
                .filter(plugin_storage::Column::Key.eq(&parsed.key))
                .lock_exclusive()
                .one(&txn)
                .await?;

            let current_str = current.as_ref().map(|r| match &r.data {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });

            if current_str.as_deref() != parsed.expected.as_deref() {
                txn.commit().await?;
                return Ok::<bool, sea_orm::DbErr>(false);
            }

            let json_val = Value::String(parsed.new);
            let model = plugin_storage::ActiveModel {
                plugin_id: Set(plugin_id.clone()),
                collection: Set(DEFAULT_COLLECTION.to_string()),
                key: Set(parsed.key),
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
                .exec(&txn)
                .await?;

            txn.commit().await?;
            Ok(true)
        })
    })
    .map_err(|e| {
        error!("DB store_compare_and_set error: {e}");
        extism::Error::msg("Database error")
    })?;

    Ok(serde_json::json!({ "swapped": swapped }).to_string())
});
