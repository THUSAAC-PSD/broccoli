use crate::error::SdkError;
use crate::types::{OperationResult, OperationTask};

pub struct Operations {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: OperationsMock,
}

#[cfg(target_arch = "wasm32")]
impl Operations {
    pub fn start_batch(&self, tasks: &[OperationTask]) -> Result<String, SdkError> {
        let ops_json = serde_json::to_string(tasks)?;
        let response_json = unsafe { crate::host::raw::start_operation_batch(ops_json)? };
        let response: serde_json::Value = serde_json::from_str(&response_json)?;
        response["batch_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                SdkError::HostCall("Missing batch_id in start_operation_batch response".into())
            })
    }

    pub fn next_result(
        &self,
        batch_id: &str,
        timeout_ms: u64,
    ) -> Result<OperationResult, SdkError> {
        let input = serde_json::json!({
            "batch_id": batch_id,
            "timeout_ms": timeout_ms,
        });
        let result_json =
            unsafe { crate::host::raw::get_next_operation_result(serde_json::to_string(&input)?)? };
        let envelope: serde_json::Value = serde_json::from_str(&result_json)?;

        let task_result = envelope
            .get("result")
            .filter(|v| !v.is_null())
            .ok_or_else(|| SdkError::HostCall("Operation timed out — no result received".into()))?;

        let output = task_result
            .get("output")
            .ok_or_else(|| SdkError::HostCall("Missing 'output' in TaskResult".into()))?;

        serde_json::from_value::<OperationResult>(output.clone()).map_err(|_| {
            let error_msg = output
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown operation error");
            SdkError::HostCall(format!("Operation failed at worker: {error_msg}"))
        })
    }

    pub fn cancel_batch(&self, batch_id: &str) -> Result<(), SdkError> {
        let input = serde_json::json!({ "batch_id": batch_id });
        unsafe { crate::host::raw::cancel_operation_batch(serde_json::to_string(&input)?)? };
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct OperationsMock;

#[cfg(not(target_arch = "wasm32"))]
impl OperationsMock {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Operations {
    pub fn start_batch(&self, _tasks: &[OperationTask]) -> Result<String, SdkError> {
        Ok("mock-op-batch".to_string())
    }

    pub fn next_result(
        &self,
        _batch_id: &str,
        _timeout_ms: u64,
    ) -> Result<OperationResult, SdkError> {
        Err(SdkError::Other("Mock operations not implemented".into()))
    }

    pub fn cancel_batch(&self, _batch_id: &str) -> Result<(), SdkError> {
        Ok(())
    }
}
