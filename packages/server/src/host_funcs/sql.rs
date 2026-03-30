use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use extism::host_fn;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, Statement,
    TransactionTrait,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use tracing::error;
use uuid::Uuid;

/// Shared map of active transactions, keyed by UUID.
pub type TransactionMap = Arc<StdMutex<HashMap<String, DatabaseTransaction>>>;

/// Response wrapper to pass results or errors back to the plugin.
#[derive(Serialize)]
struct HostDbResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HostDbResponse {
    fn ok(data: JsonValue) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            data: None,
            error: Some(msg.into()),
        }
    }
    fn to_json_string(&self) -> Result<String, extism::Error> {
        serde_json::to_string(self)
            .map_err(|e| extism::Error::msg(format!("Serialization error: {}", e)))
    }
}

/// Helper function to convert JSON values from the plugin into SeaORM parameter values.
fn json_to_sea_value(v: JsonValue) -> sea_orm::Value {
    match v {
        JsonValue::Null => sea_orm::Value::String(None),
        JsonValue::Bool(b) => sea_orm::Value::Bool(Some(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                sea_orm::Value::BigInt(Some(i))
            } else if let Some(f) = n.as_f64() {
                sea_orm::Value::Double(Some(f))
            } else {
                sea_orm::Value::String(Some(n.to_string()))
            }
        }
        JsonValue::String(s) => sea_orm::Value::String(Some(s)),
        // Complex objects and arrays are passed as JSONB
        v => sea_orm::Value::Json(Some(Box::new(v))),
    }
}

/// Helper function to parse args array from JSON string.
fn parse_args(args_json: &str) -> Result<Vec<sea_orm::Value>, extism::Error> {
    if args_json.trim().is_empty() {
        return Ok(vec![]);
    }
    let json_arr: Vec<JsonValue> = serde_json::from_str(args_json)
        .map_err(|e| extism::Error::msg(format!("Invalid args JSON: {}", e)))?;

    Ok(json_arr.into_iter().map(json_to_sea_value).collect())
}

// Executes a raw SQL statement with parameters.
// Returns the number of affected rows.
host_fn!(pub db_execute(user_data: (String, DatabaseConnection); sql: String, args: String) -> String {
    let user_data_guard = user_data.get()?;
    let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (_, db) = &*ctx;

    let values = parse_args(&args)?;
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);

    let exec_result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            db.execute_raw(stmt).await
        })
    });

    match exec_result {
        Ok(res) => HostDbResponse::ok(JsonValue::from(res.rows_affected())).to_json_string(),
        Err(e) => {
            error!("DB execution error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

// Executes a parameterized raw SQL query.
// Returns the result set as a JSON string array.
host_fn!(pub db_query(user_data: (String, DatabaseConnection); sql: String, args: String) -> String {
    let user_data_guard = user_data.get()?;
    let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (_, db) = &*ctx;

    let values = parse_args(&args)?;
    let wrapped_sql = format!(
        "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM ({}) AS t",
        sql
    );
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, wrapped_sql, values);

    let query_result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            db.query_one_raw(stmt).await
        })
    });

    match query_result {
        Ok(Some(res)) => {
            let json_val: serde_json::Value = res.try_get("", "json_data")
                .unwrap_or(serde_json::json!([]));
            HostDbResponse::ok(json_val).to_json_string()
        },
        Ok(None) => HostDbResponse::ok(serde_json::json!([])).to_json_string(),
        Err(e) => {
            error!("DB query error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

// Begins a new database transaction. Returns {"txn_id": "<uuid>"}.
host_fn!(pub db_begin(user_data: (String, DatabaseConnection, TransactionMap); _input: String) -> String {
    let (db, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.1.clone(), ctx.2.clone())
    };

    let txn = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(db.begin())
    }).map_err(|e| {
        error!("DB begin error: {}", e);
        extism::Error::msg(e.to_string())
    })?;

    let txn_id = Uuid::new_v4().to_string();
    txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?
        .insert(txn_id.clone(), txn);

    HostDbResponse::ok(serde_json::json!({"txn_id": txn_id})).to_json_string()
});

// Executes a SELECT query within an existing transaction.
host_fn!(pub db_query_in(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String, sql: String, args: String) -> String {
    let txn_map = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        ctx.2.clone()
    };

    let values = parse_args(&args)?;
    let wrapped_sql = format!(
        "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM ({}) AS t",
        sql
    );
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, wrapped_sql, values);

    let mut map_guard = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
    let txn = map_guard.get_mut(&txn_id)
        .ok_or_else(|| extism::Error::msg(format!("Transaction not found: {txn_id}")))?;

    let query_result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(txn.query_one_raw(stmt))
    });

    match query_result {
        Ok(Some(res)) => {
            let json_val: serde_json::Value = res.try_get("", "json_data")
                .unwrap_or(serde_json::json!([]));
            HostDbResponse::ok(json_val).to_json_string()
        },
        Ok(None) => HostDbResponse::ok(serde_json::json!([])).to_json_string(),
        Err(e) => {
            error!("DB query_in error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

// Executes a statement within an existing transaction.
host_fn!(pub db_execute_in(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String, sql: String, args: String) -> String {
    let txn_map = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        ctx.2.clone()
    };

    let values = parse_args(&args)?;
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);

    let mut map_guard = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
    let txn = map_guard.get_mut(&txn_id)
        .ok_or_else(|| extism::Error::msg(format!("Transaction not found: {txn_id}")))?;

    let exec_result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(txn.execute_raw(stmt))
    });

    match exec_result {
        Ok(res) => HostDbResponse::ok(JsonValue::from(res.rows_affected())).to_json_string(),
        Err(e) => {
            error!("DB execute_in error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

// Commits an active transaction. Removes it from the map.
host_fn!(pub db_commit(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String) -> String {
    let txn_map = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        ctx.2.clone()
    };

    let txn = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?
        .remove(&txn_id)
        .ok_or_else(|| extism::Error::msg(format!("Transaction not found: {txn_id}")))?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(txn.commit())
    });

    match result {
        Ok(()) => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
        Err(e) => {
            error!("DB commit error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

// Rolls back an active transaction. Removes it from the map.
host_fn!(pub db_rollback(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String) -> String {
    let txn_map = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        ctx.2.clone()
    };

    let txn = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?
        .remove(&txn_id);

    match txn {
        Some(txn) => {
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(txn.rollback())
            });
            match result {
                Ok(()) => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
                Err(e) => {
                    error!("DB rollback error: {}", e);
                    HostDbResponse::err(e.to_string()).to_json_string()
                }
            }
        }
        // Already committed/rolled back — not an error (idempotent for Drop safety)
        None => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
    }
});
