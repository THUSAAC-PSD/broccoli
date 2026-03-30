use std::cell::RefCell;
use std::collections::VecDeque;

use crate::error::SdkError;
use crate::types::{StartEvaluateBatchInput, TestCaseVerdict};

pub struct Eval {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: EvalMock,
}

#[cfg(target_arch = "wasm32")]
impl Eval {
    pub fn start_batch(&self, input: &StartEvaluateBatchInput) -> Result<String, SdkError> {
        let input_json = serde_json::to_string(input)?;
        let response_json = unsafe { crate::host::raw::start_evaluate_batch(input_json)? };
        let response: serde_json::Value = serde_json::from_str(&response_json)?;
        response["batch_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| {
                SdkError::HostCall("Missing batch_id in start_evaluate_batch response".into())
            })
    }

    pub fn next_result(
        &self,
        batch_id: &str,
        timeout_ms: u64,
    ) -> Result<Option<TestCaseVerdict>, SdkError> {
        let input = serde_json::json!({
            "batch_id": batch_id,
            "timeout_ms": timeout_ms,
        });
        let result_json =
            unsafe { crate::host::raw::get_next_evaluate_result(serde_json::to_string(&input)?)? };
        let envelope: serde_json::Value = serde_json::from_str(&result_json)?;

        match envelope.get("result").filter(|v| !v.is_null()) {
            Some(v) => Ok(Some(serde_json::from_value(v.clone())?)),
            None => Ok(None),
        }
    }

    pub fn cancel_batch(&self, batch_id: &str) -> Result<(), SdkError> {
        let input = serde_json::json!({ "batch_id": batch_id });
        unsafe { crate::host::raw::cancel_evaluate_batch(serde_json::to_string(&input)?)? };
        Ok(())
    }
}

// --- Mock ---

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct EvalMock {
    results: RefCell<VecDeque<TestCaseVerdict>>,
    start_errors: RefCell<VecDeque<SdkError>>,
    result_errors: RefCell<VecDeque<SdkError>>,
    batch_inputs: RefCell<Vec<StartEvaluateBatchInput>>,
    cancels: RefCell<Vec<String>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl EvalMock {
    pub fn new() -> Self {
        Self {
            results: RefCell::new(VecDeque::new()),
            start_errors: RefCell::new(VecDeque::new()),
            result_errors: RefCell::new(VecDeque::new()),
            batch_inputs: RefCell::new(Vec::new()),
            cancels: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Eval {
    pub fn start_batch(&self, input: &StartEvaluateBatchInput) -> Result<String, SdkError> {
        self.inner.batch_inputs.borrow_mut().push(input.clone());
        if let Some(err) = self.inner.start_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(format!(
            "mock-batch-{}",
            self.inner.batch_inputs.borrow().len()
        ))
    }

    pub fn next_result(
        &self,
        _batch_id: &str,
        _timeout_ms: u64,
    ) -> Result<Option<TestCaseVerdict>, SdkError> {
        if let Some(err) = self.inner.result_errors.borrow_mut().pop_front() {
            return Err(err);
        }
        Ok(self.inner.results.borrow_mut().pop_front())
    }

    pub fn cancel_batch(&self, batch_id: &str) -> Result<(), SdkError> {
        self.inner.cancels.borrow_mut().push(batch_id.to_string());
        Ok(())
    }

    pub fn queue_result(&self, v: TestCaseVerdict) {
        self.inner.results.borrow_mut().push_back(v);
    }

    pub fn queue_start_error(&self, err: SdkError) {
        self.inner.start_errors.borrow_mut().push_back(err);
    }

    pub fn queue_result_error(&self, err: SdkError) {
        self.inner.result_errors.borrow_mut().push_back(err);
    }

    pub fn batch_inputs(&self) -> Vec<StartEvaluateBatchInput> {
        self.inner.batch_inputs.borrow().clone()
    }

    pub fn was_cancelled(&self) -> bool {
        !self.inner.cancels.borrow().is_empty()
    }
}
