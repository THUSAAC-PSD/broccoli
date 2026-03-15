use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;

use crate::error::SdkError;

/// Response envelope from DB host functions.
#[derive(serde::Deserialize)]
struct HostDbResponse {
    data: Option<JsonValue>,
    error: Option<String>,
}

impl HostDbResponse {
    fn into_result(self) -> Result<Option<JsonValue>, SdkError> {
        if let Some(err) = self.error {
            return Err(SdkError::Database(err));
        }
        Ok(self.data)
    }
}

/// Execute a SELECT query without parameters and deserialize the result rows.
pub fn db_query<T: DeserializeOwned>(sql: &str) -> Result<Vec<T>, SdkError> {
    db_query_with_args(sql, &[] as &[JsonValue])
}

/// Execute a parameterized SELECT query and deserialize the result rows.
pub fn db_query_with_args<T: DeserializeOwned>(
    sql: &str,
    args: &[impl serde::Serialize],
) -> Result<Vec<T>, SdkError> {
    let args_json = serde_json::to_string(args)?;
    let result_json = unsafe { super::raw::db_query(sql.to_string(), args_json)? };
    let resp: HostDbResponse = serde_json::from_str(&result_json)?;
    let data = resp.into_result()?;
    match data {
        Some(v) => Ok(serde_json::from_value(v)?),
        None => Ok(Vec::new()),
    }
}

/// Execute an INSERT/UPDATE/DELETE statement without parameters.
pub fn db_execute(sql: &str) -> Result<(), SdkError> {
    db_execute_with_args(sql, &[] as &[JsonValue])?;
    Ok(())
}

/// Execute a parameterized INSERT/UPDATE/DELETE statement.
/// Returns the number of affected rows.
pub fn db_execute_with_args(sql: &str, args: &[impl serde::Serialize]) -> Result<u64, SdkError> {
    let args_json = serde_json::to_string(args)?;
    let result_json = unsafe { super::raw::db_execute(sql.to_string(), args_json)? };
    let resp: HostDbResponse = serde_json::from_str(&result_json)?;
    let data = resp.into_result()?;
    match data {
        Some(v) => Ok(v.as_u64().unwrap_or(0)),
        None => Ok(0),
    }
}
