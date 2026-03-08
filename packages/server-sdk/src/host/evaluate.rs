use crate::error::SdkError;
use crate::types::{StartEvaluateBatchInput, TestCaseVerdict};

/// Start an evaluate batch and return the batch ID.
pub fn start_batch(input: &StartEvaluateBatchInput) -> Result<String, SdkError> {
    let input_json = serde_json::to_string(input)?;
    let response_json = unsafe { super::raw::start_evaluate_batch(input_json)? };
    let response: serde_json::Value = serde_json::from_str(&response_json)?;
    let batch_id = response["batch_id"]
        .as_str()
        .ok_or_else(|| {
            SdkError::HostCall("Missing batch_id in start_evaluate_batch response".into())
        })?
        .to_string();
    Ok(batch_id)
}

/// Poll for the next evaluate result. Returns `None` on timeout.
pub fn get_next_result(
    batch_id: &str,
    timeout_ms: u64,
) -> Result<Option<TestCaseVerdict>, SdkError> {
    let input = serde_json::json!({
        "batch_id": batch_id,
        "timeout_ms": timeout_ms,
    });
    let result_json =
        unsafe { super::raw::get_next_evaluate_result(serde_json::to_string(&input)?)? };
    let envelope: serde_json::Value = serde_json::from_str(&result_json)?;

    let result = envelope
        .get("result")
        .and_then(|v| if v.is_null() { None } else { Some(v) });

    match result {
        Some(v) => {
            let verdict: TestCaseVerdict = serde_json::from_value(v.clone())?;
            Ok(Some(verdict))
        }
        None => Ok(None),
    }
}

/// Cancel an evaluate batch (best-effort, ignores errors).
pub fn cancel_batch(batch_id: &str) -> Result<(), SdkError> {
    let input = serde_json::json!({ "batch_id": batch_id });
    unsafe { super::raw::cancel_evaluate_batch(serde_json::to_string(&input)?)? };
    Ok(())
}
