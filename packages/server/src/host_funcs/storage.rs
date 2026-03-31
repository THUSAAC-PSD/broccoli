use std::collections::HashMap;

use extism::host_fn;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter, Set,
    Statement, TransactionTrait,
};
use serde::Deserialize;
use serde_json::Value;
use tracing::error;

use crate::entity::plugin_storage;

/// Default collection name used by the SDK's JSON-based storage API.
const DEFAULT_COLLECTION: &str = "default";

/// Extract the plain string from a stored JSON value.
fn extract_str(data: &Value) -> String {
    match data {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[derive(Deserialize)]
struct StoreGetInput {
    keys: Vec<String>,
}

// Batch get.
//
// Accepts JSON: {"keys": ["k1", "k2"]}
// Returns JSON: {"values": {"k1": "v1", "k2": "v2"}}
host_fn!(pub store_get(user_data: (String, DatabaseConnection); input: String) -> String {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreGetInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_get input: {e}")))?;

    if parsed.keys.is_empty() {
        return Ok(serde_json::json!({ "values": {} }).to_string());
    }

    let results = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_storage::Entity::find()
                .filter(plugin_storage::Column::PluginId.eq(plugin_id))
                .filter(plugin_storage::Column::Collection.eq(DEFAULT_COLLECTION))
                .filter(plugin_storage::Column::Key.is_in(&parsed.keys))
                .all(db)
                .await
        })
    })
    .map_err(|e| {
        error!("DB store_get error: {e}");
        extism::Error::msg("Database error")
    })?;

    let values: HashMap<&str, String> = results
        .iter()
        .map(|r| (r.key.as_str(), extract_str(&r.data)))
        .collect();

    Ok(serde_json::json!({ "values": values }).to_string())
});

#[derive(Deserialize)]
struct StoreSetEntry {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct StoreSetInput {
    entries: Vec<StoreSetEntry>,
}

// Batch set.
//
// Accepts JSON: {"entries": [{"key": "k", "value": "v"}, ...]}
host_fn!(pub store_set(user_data: (String, DatabaseConnection); input: String) -> () {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreSetInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_set input: {e}")))?;

    if parsed.entries.is_empty() {
        return Ok(());
    }

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let txn = db.begin().await?;

            for entry in parsed.entries {
                let model = plugin_storage::ActiveModel {
                    plugin_id: Set(plugin_id.clone()),
                    collection: Set(DEFAULT_COLLECTION.to_string()),
                    key: Set(entry.key),
                    data: Set(Value::String(entry.value)),
                    created_at: Set(chrono::Utc::now()),
                };

                plugin_storage::Entity::insert(model)
                    .on_conflict(
                        OnConflict::columns([
                            plugin_storage::Column::PluginId,
                            plugin_storage::Column::Collection,
                            plugin_storage::Column::Key,
                        ])
                        .update_columns([plugin_storage::Column::Data])
                        .to_owned(),
                    )
                    .exec(&txn)
                    .await?;
            }

            txn.commit().await?;
            Ok::<(), sea_orm::DbErr>(())
        })
    })
    .map_err(|e| {
        error!("DB store_set error: {e}");
        extism::Error::msg("Database error")
    })?;

    Ok(())
});

#[derive(Deserialize)]
struct StoreDeleteInput {
    keys: Vec<String>,
}

// Batch delete.
//
// Accepts JSON: {"keys": ["k1", "k2"]}
host_fn!(pub store_delete(user_data: (String, DatabaseConnection); input: String) -> () {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (plugin_id, db) = &*user_data;

    let parsed: StoreDeleteInput = serde_json::from_str(&input)
        .map_err(|e| extism::Error::msg(format!("Invalid store_delete input: {e}")))?;

    if parsed.keys.is_empty() {
        return Ok(());
    }

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            plugin_storage::Entity::delete_many()
                .filter(plugin_storage::Column::PluginId.eq(plugin_id))
                .filter(plugin_storage::Column::Collection.eq(DEFAULT_COLLECTION))
                .filter(plugin_storage::Column::Key.is_in(&parsed.keys))
                .exec(db)
                .await
        })
    })
    .map_err(|e| {
        error!("DB store_delete error: {e}");
        extism::Error::msg("Database error")
    })?;

    Ok(())
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
            match parsed.expected {
                None => {
                    // Create-if-absent
                    // If rows_affected == 1, we won the race. If 0, key already exists.
                    let model = plugin_storage::ActiveModel {
                        plugin_id: Set(plugin_id.clone()),
                        collection: Set(DEFAULT_COLLECTION.to_string()),
                        key: Set(parsed.key),
                        data: Set(Value::String(parsed.new)),
                        created_at: Set(chrono::Utc::now()),
                    };
                    let result = plugin_storage::Entity::insert(model)
                        .on_conflict(
                            OnConflict::columns([
                                plugin_storage::Column::PluginId,
                                plugin_storage::Column::Collection,
                                plugin_storage::Column::Key,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec_without_returning(db)
                        .await?;
                    Ok::<bool, sea_orm::DbErr>(result > 0)
                }
                Some(expected_val) => {
                    // Update-if-match
                    let result = db.execute_raw(Statement::from_sql_and_values(
                        DbBackend::Postgres,
                        "UPDATE plugin_storage SET data = $1 \
                         WHERE plugin_id = $2 AND collection = $3 AND key = $4 AND data = $5",
                        [
                            Value::String(parsed.new).into(),
                            plugin_id.clone().into(),
                            DEFAULT_COLLECTION.into(),
                            parsed.key.into(),
                            Value::String(expected_val).into(),
                        ],
                    )).await?;
                    Ok(result.rows_affected() == 1)
                }
            }
        })
    })
    .map_err(|e| {
        error!("DB store_compare_and_set error: {e}");
        extism::Error::msg("Database error")
    })?;

    Ok(serde_json::json!({ "swapped": swapped }).to_string())
});
