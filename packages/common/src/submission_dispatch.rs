pub use broccoli_server_sdk::types::{
    BuildEvalOpsInput, CheckerParseInput, CheckerVerdict, OnSubmissionInput, OnSubmissionOutput,
    RunCheckerInput, SourceFile, StartEvaluateBatchInput, TestCaseVerdict, Verdict as SdkVerdict,
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Language compilation/execution configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LanguageConfig {
    pub compile_cmd: Option<Vec<String>>,
    pub run_cmd: Vec<String>,
    pub source_ext: String,
    pub compile_time_limit_ms: u64,
}

/// Information about a test case (returned by get_test_case_info host fn)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TestCaseInfo {
    pub id: i32,
    pub input: String,
    pub expected_output: String,
    pub score: i32,
    pub is_sample: bool,
}

/// Information about a problem (returned by get_problem_info host fn)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProblemInfo {
    pub id: i32,
    pub problem_type: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
}
