use crate::error::SdkError;
use crate::types::{
    CodeRunResultRow, CodeRunUpdate, StartEvaluateBatchInput, SubmissionUpdate, TestCaseResultRow,
    TestCaseRow, TestCaseVerdict,
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
    fn update_submission(&self, update: &SubmissionUpdate) -> Result<u64, SdkError>;
    fn insert_test_case_results(&self, results: &[TestCaseResultRow]) -> Result<(), SdkError>;
    fn update_code_run(&self, update: &CodeRunUpdate) -> Result<(), SdkError>;
    fn insert_code_run_results(&self, results: &[CodeRunResultRow]) -> Result<(), SdkError>;
    /// Delete all test case results for a submission. Used before re-evaluation
    /// to prevent duplicate rows on MQ redelivery.
    fn delete_test_case_results(&self, submission_id: i32) -> Result<(), SdkError>;
    fn log_info(&self, msg: &str) -> Result<(), SdkError>;
}
