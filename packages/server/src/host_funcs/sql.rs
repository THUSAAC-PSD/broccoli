use extism::host_fn;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use tracing::error;

// Executes a raw SQL statement (INSERT, UPDATE, DELETE, DDL).
// Returns the number of affected rows.
host_fn!(pub db_execute(user_data: (String, DatabaseConnection); sql: String) -> u64 {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (_, db) = &*user_data;

    // Direct execution without parameter binding
    let stmt = Statement::from_string(
        DbBackend::Postgres,
        &sql,
    );

    let exec_result = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            db.execute_raw(stmt).await
        })
    })
    .map_err(|e| {
        error!("Raw SQL execute error: {}", e);
        extism::Error::msg(format!("DB Error: {}", e))
    })?;

    Ok(exec_result.rows_affected())
});

// Executes a raw SQL query (SELECT).
// Returns the result set as a JSON string.
host_fn!(pub db_query(user_data: (String, DatabaseConnection); sql: String) -> String {
    let user_data_guard = user_data.get()?;
    let user_data = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
    let (_, db) = &*user_data;

    // Wrap SQL to let Postgres handle JSON serialization
    let wrapped_sql = format!(
        "SELECT json_agg(t) AS json_data FROM ({}) AS t",
        sql
    );

    let stmt = Statement::from_string(
        DbBackend::Postgres,
        &wrapped_sql,
    );

    let query_result = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            db.query_one_raw(stmt).await
        })
    })
    .map_err(|e| {
        error!("Raw SQL query error: {}", e);
        extism::Error::msg(format!("DB Error: {}", e))
    })?;

    match query_result {
        Some(res) => {
            // Retrieve the pre-formatted JSON string
            let json_val: Option<serde_json::Value> = res.try_get("", "json_data")
                .map_err(|e| extism::Error::msg(format!("Failed to retrieve json: {}", e)))?;

            match json_val {
                Some(v) => Ok(v.to_string()),
                None => Ok("[]".to_string()),
            }
        },
        None => Ok("[]".to_string()),
    }
});
