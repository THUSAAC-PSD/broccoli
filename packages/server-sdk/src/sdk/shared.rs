use crate::db::Params;
use crate::types::{SubmissionStatus, Verdict};

#[cfg(target_arch = "wasm32")]
use crate::error::SdkError;
#[cfg(target_arch = "wasm32")]
use serde::de::DeserializeOwned;
#[cfg(target_arch = "wasm32")]
use serde_json::Value as JsonValue;

/// Response envelope from DB host functions.
#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
pub(super) struct HostDbResponse {
    pub data: Option<JsonValue>,
    pub error: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl HostDbResponse {
    pub fn into_result(self) -> Result<Option<JsonValue>, SdkError> {
        if let Some(err) = self.error {
            return Err(SdkError::Database(err));
        }
        Ok(self.data)
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn parse_rows<T: DeserializeOwned>(data: Option<JsonValue>) -> Result<Vec<T>, SdkError> {
    match data {
        Some(v) => Ok(serde_json::from_value(v)?),
        None => Ok(Vec::new()),
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn parse_affected(data: Option<JsonValue>) -> u64 {
    data.and_then(|v| v.as_u64()).unwrap_or(0)
}

/// Execute a parameterized SELECT via raw FFI. Returns deserialized rows.
#[cfg(target_arch = "wasm32")]
pub(super) fn raw_query<T: DeserializeOwned>(
    sql: &str,
    args: &[impl serde::Serialize],
) -> Result<Vec<T>, SdkError> {
    let args_json = serde_json::to_string(args)?;
    let result_json = unsafe { crate::host::raw::db_query(sql.to_string(), args_json)? };
    let resp: HostDbResponse = serde_json::from_str(&result_json)?;
    parse_rows(resp.into_result()?)
}

/// Execute a parameterized statement via raw FFI. Returns affected row count.
#[cfg(target_arch = "wasm32")]
pub(super) fn raw_execute(sql: &str, args: &[impl serde::Serialize]) -> Result<u64, SdkError> {
    let args_json = serde_json::to_string(args)?;
    let result_json = unsafe { crate::host::raw::db_execute(sql.to_string(), args_json)? };
    let resp: HostDbResponse = serde_json::from_str(&result_json)?;
    Ok(parse_affected(resp.into_result()?))
}

/// Push SET clauses shared by submission and code_run updates.
pub(super) fn push_judge_sets(
    p: &mut Params,
    sets: &mut Vec<String>,
    status: &Option<SubmissionStatus>,
    verdict: &Option<Option<Verdict>>,
    score: &Option<f64>,
    time_used: &Option<Option<i32>>,
    memory_used: &Option<Option<i32>>,
    compile_output: &Option<Option<String>>,
    error_code: &Option<Option<String>>,
    error_message: &Option<Option<String>>,
) {
    if let Some(status) = status {
        sets.push(format!("status = {}", p.bind(status.as_str())));
        if status.is_terminal() {
            sets.push("judged_at = NOW()".into());
        }
    }

    match verdict {
        Some(Some(v)) => sets.push(format!("verdict = {}", p.bind(v.to_db_str()))),
        Some(None) => sets.push("verdict = NULL".into()),
        None => {}
    }

    if let Some(score) = score {
        let val = if score.is_finite() { *score } else { 0.0 };
        sets.push(format!("score = {}", p.bind(val)));
    }

    push_double_opt(p, sets, "time_used", time_used);
    push_double_opt(p, sets, "memory_used", memory_used);
    push_double_opt_str(p, sets, "compile_output", compile_output);
    push_double_opt_str(p, sets, "error_code", error_code);
    push_double_opt_str(p, sets, "error_message", error_message);
}

fn push_double_opt(p: &mut Params, sets: &mut Vec<String>, col: &str, val: &Option<Option<i32>>) {
    match val {
        Some(Some(v)) => sets.push(format!("{col} = {}", p.bind(*v))),
        Some(None) => sets.push(format!("{col} = NULL")),
        None => {}
    }
}

fn push_double_opt_str(
    p: &mut Params,
    sets: &mut Vec<String>,
    col: &str,
    val: &Option<Option<String>>,
) {
    match val {
        Some(Some(v)) => sets.push(format!("{col} = {}", p.bind(v.as_str()))),
        Some(None) => sets.push(format!("{col} = NULL")),
        None => {}
    }
}
