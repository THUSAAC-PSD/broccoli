use std::cell::Cell;

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

#[derive(serde::Deserialize)]
struct BeginResponse {
    txn_id: String,
}

/// Begin a new database transaction (READ COMMITTED isolation).
///
/// Returns a [`Transaction`] handle that auto-rollbacks on drop if neither
/// `commit()` nor `rollback()` is called.
pub fn db_begin() -> Result<Transaction, SdkError> {
    let result_json = unsafe { super::raw::db_begin("{}".to_string())? };
    let resp: HostDbResponse = serde_json::from_str(&result_json)?;
    let data = resp.into_result()?;
    let begin: BeginResponse = serde_json::from_value(
        data.ok_or_else(|| SdkError::Database("Empty begin response".into()))?,
    )?;
    Ok(Transaction {
        id: begin.txn_id,
        finished: Cell::new(false),
    })
}

/// An active database transaction handle.
///
/// Methods mirror [`db_query`]/[`db_execute`] but execute within the transaction.
/// [`commit()`](Transaction::commit) and [`rollback()`](Transaction::rollback)
/// consume `self` to prevent use-after-end at compile time.
/// If dropped without either, automatically rolls back.
pub struct Transaction {
    id: String,
    finished: Cell<bool>,
}

impl Transaction {
    /// Execute a SELECT query within this transaction.
    pub fn query<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>, SdkError> {
        self.query_with_args(sql, &[] as &[JsonValue])
    }

    /// Execute a parameterized SELECT query within this transaction.
    pub fn query_with_args<T: DeserializeOwned>(
        &self,
        sql: &str,
        args: &[impl serde::Serialize],
    ) -> Result<Vec<T>, SdkError> {
        let args_json = serde_json::to_string(args)?;
        let result_json =
            unsafe { super::raw::db_query_in(self.id.clone(), sql.to_string(), args_json)? };
        let resp: HostDbResponse = serde_json::from_str(&result_json)?;
        let data = resp.into_result()?;
        match data {
            Some(v) => Ok(serde_json::from_value(v)?),
            None => Ok(Vec::new()),
        }
    }

    /// Execute a statement within this transaction. Returns affected row count.
    pub fn execute(&self, sql: &str) -> Result<u64, SdkError> {
        self.execute_with_args(sql, &[] as &[JsonValue])
    }

    /// Execute a parameterized statement within this transaction.
    pub fn execute_with_args(
        &self,
        sql: &str,
        args: &[impl serde::Serialize],
    ) -> Result<u64, SdkError> {
        let args_json = serde_json::to_string(args)?;
        let result_json =
            unsafe { super::raw::db_execute_in(self.id.clone(), sql.to_string(), args_json)? };
        let resp: HostDbResponse = serde_json::from_str(&result_json)?;
        let data = resp.into_result()?;
        match data {
            Some(v) => Ok(v.as_u64().unwrap_or(0)),
            None => Ok(0),
        }
    }

    /// Commit the transaction.
    pub fn commit(self) -> Result<(), SdkError> {
        self.finished.set(true);
        let result_json = unsafe { super::raw::db_commit(self.id.clone())? };
        let resp: HostDbResponse = serde_json::from_str(&result_json)?;
        resp.into_result()?;
        Ok(())
    }

    /// Rollback the transaction.
    ///
    /// Dropping without commit has the same effect, but explicit rollback
    /// communicates intent.
    pub fn rollback(self) -> Result<(), SdkError> {
        self.finished.set(true);
        let result_json = unsafe { super::raw::db_rollback(self.id.clone())? };
        let resp: HostDbResponse = serde_json::from_str(&result_json)?;
        resp.into_result()?;
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.finished.get() {
            let _ = unsafe { super::raw::db_rollback(self.id.clone()) };
        }
    }
}
