use serde::de::DeserializeOwned;

use crate::error::SdkError;

/// Execute a SELECT query and deserialize the result rows.
pub fn db_query<T: DeserializeOwned>(sql: &str) -> Result<Vec<T>, SdkError> {
    let result_json = unsafe { super::raw::db_query(sql.to_string())? };
    let rows: Vec<T> = serde_json::from_str(&result_json)?;
    Ok(rows)
}

/// Execute an INSERT/UPDATE/DELETE statement.
pub fn db_execute(sql: &str) -> Result<(), SdkError> {
    unsafe { super::raw::db_execute(sql.to_string())? };
    Ok(())
}
