use extism::host_fn;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::error;

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
        JsonValue::Null => sea_orm::Value::Json(None),
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

#[derive(Deserialize)]
struct TxQuery {
    sql: String,
    #[serde(default)]
    args: Vec<JsonValue>,
}

// Executes multiple statements within a single database transaction.
// Input is a JSON array of { "sql": "...", "args": [...] }.
// Returns an array of affected rows, or aborts if any query fails.
host_fn!(pub db_transaction(user_data: (String, DatabaseConnection); queries_json: String) -> String {
    let user_data_guard = user_data.get()?;
    let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (_, db) = &*ctx;

    let queries: Vec<TxQuery> = serde_json::from_str(&queries_json)
        .map_err(|e| extism::Error::msg(format!("Invalid transaction JSON: {}", e)))?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let txn = db.begin().await?;
            let mut results = Vec::with_capacity(queries.len());

            for q in queries {
                let values: Vec<sea_orm::Value> = q.args.into_iter().map(json_to_sea_value).collect();
                let stmt = Statement::from_sql_and_values(DbBackend::Postgres, q.sql, values);
                let res = txn.execute_raw(stmt).await?;
                results.push(res.rows_affected());
            }

            txn.commit().await?;
            Ok::<Vec<u64>, sea_orm::DbErr>(results)
        })
    });

    match result {
        Ok(rows) => HostDbResponse::ok(JsonValue::from(rows)).to_json_string(),
        Err(e) => {
            error!("DB transaction error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});
