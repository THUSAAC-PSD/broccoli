use crate::error::SdkError;
use crate::types::{
    StartEvaluateBatchInput, SubmissionUpdate, TestCaseResultRow, TestCaseRow, TestCaseVerdict,
};

/// Host function interface for contest-type plugins.
///
/// * In production (WASM): use `WasmHost` which delegates to FFI.
/// * In tests (native): use `MockHost` or implement your own mock.
pub trait PluginHost {
    fn query_test_cases(&self, problem_id: i32) -> Result<Vec<TestCaseRow>, SdkError>;
    fn start_evaluate_batch(&self, input: &StartEvaluateBatchInput) -> Result<String, SdkError>;
    fn get_next_evaluate_result(
        &self,
        batch_id: &str,
        timeout_ms: u64,
    ) -> Result<Option<TestCaseVerdict>, SdkError>;
    fn cancel_evaluate_batch(&self, batch_id: &str) -> Result<(), SdkError>;
    fn update_submission(&self, update: &SubmissionUpdate) -> Result<(), SdkError>;
    fn insert_test_case_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError>;
    fn log_info(&self, msg: &str) -> Result<(), SdkError>;
}
