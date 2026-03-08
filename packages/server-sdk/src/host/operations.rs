use crate::error::SdkError;
use crate::types::OperationResult;

/// Start an operation batch and return the batch ID.
pub fn start_batch(ops_json: &str) -> Result<String, SdkError> {
    let response_json = unsafe { super::raw::start_operation_batch(ops_json.to_string())? };
    let response: serde_json::Value = serde_json::from_str(&response_json)?;
    let batch_id = response["batch_id"]
        .as_str()
        .ok_or_else(|| {
            SdkError::HostCall("Missing batch_id in start_operation_batch response".into())
        })?
        .to_string();
    Ok(batch_id)
}

/// Poll for the next operation result. Returns the OperationResult or error on timeout.
pub fn wait_for_result(batch_id: &str, timeout_ms: u64) -> Result<OperationResult, SdkError> {
    let input = serde_json::json!({
        "batch_id": batch_id,
        "timeout_ms": timeout_ms,
    });
    let result_json =
        unsafe { super::raw::get_next_operation_result(serde_json::to_string(&input)?)? };
    let envelope: serde_json::Value = serde_json::from_str(&result_json)?;

    let task_result = envelope
        .get("result")
        .and_then(|v| if v.is_null() { None } else { Some(v) })
        .ok_or_else(|| SdkError::HostCall("Operation timed out — no result received".into()))?;

    let output = task_result
        .get("output")
        .ok_or_else(|| SdkError::HostCall("Missing 'output' in TaskResult".into()))?;

    match serde_json::from_value::<OperationResult>(output.clone()) {
        Ok(result) => Ok(result),
        Err(_) => {
            let error_msg = output
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown operation error");
            Err(SdkError::HostCall(format!(
                "Operation failed at worker: {}",
                error_msg
            )))
        }
    }
}

/// Cancel an operation batch (best-effort).
pub fn cancel_batch(batch_id: &str) -> Result<(), SdkError> {
    let input = serde_json::json!({ "batch_id": batch_id });
    unsafe { super::raw::cancel_operation_batch(serde_json::to_string(&input)?)? };
    Ok(())
}
