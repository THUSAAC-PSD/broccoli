use crate::error::SdkError;
use crate::traits::PluginHost;
use crate::types::{
    StartEvaluateBatchInput, SubmissionUpdate, TestCaseResultRow, TestCaseRow, TestCaseVerdict,
};

/// Zero-size host that delegates to FFI. Only available on WASM targets.
pub struct WasmHost;

impl PluginHost for WasmHost {
    fn query_test_cases(&self, problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError> {
        crate::db::query_test_cases(problem_id)
    }

    fn start_evaluate_batch(&self, input: &StartEvaluateBatchInput) -> Result<String, SdkError> {
        crate::host::evaluate::start_batch(input)
    }

    fn get_next_evaluate_result(
        &self,
        batch_id: &str,
        timeout_ms: u64,
    ) -> Result<Option<TestCaseVerdict>, SdkError> {
        crate::host::evaluate::get_next_result(batch_id, timeout_ms)
    }

    fn cancel_evaluate_batch(&self, batch_id: &str) -> Result<(), SdkError> {
        crate::host::evaluate::cancel_batch(batch_id)
    }

    fn update_submission(&self, update: &SubmissionUpdate) -> Result<(), SdkError> {
        crate::db::update_submission(update)
    }

    fn insert_test_case_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError> {
        crate::db::insert_test_case_results(results)
    }

    fn log_info(&self, msg: &str) -> Result<(), SdkError> {
        crate::host::logger::log_info(msg)
    }
}
