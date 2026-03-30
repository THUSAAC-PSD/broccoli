use serde::{Deserialize, Serialize};

use super::query::TestCaseRow;

/// A named file with content, used for submission files and evaluator source files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub filename: String,
    pub content: String,
}

/// Input to contest type plugin handler's on_submission handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnSubmissionInput {
    pub submission_id: i32,
    pub user_id: i32,
    pub problem_id: i32,
    pub contest_id: Option<i32>,
    pub files: Vec<SourceFile>,
    pub language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    pub problem_type: String,
    /// Pre-resolved test cases.
    #[serde(default)]
    pub test_cases: Vec<TestCaseRow>,
    /// Rejudge epoch. Stale results from a previous epoch are discarded.
    #[serde(default)]
    pub judge_epoch: i32,
}

/// Output from contest type plugin handler.
///
/// `success`: whether the submission was processed without errors (not whether it passed).
/// `error_message`: describes the failure reason when `success` is false.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnSubmissionOutput {
    pub success: bool,
    pub error_message: Option<String>,
}
