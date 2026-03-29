use serde::{Deserialize, Serialize};

use super::query::TestCaseRow;

/// A named file with content, used for submission files and evaluator source files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub filename: String,
    pub content: String,
}

/// Submission mode: formal judged submission vs ephemeral run-code execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SubmissionMode {
    /// Normal submission: affects scoring, uses DB test cases.
    #[default]
    Submit,
    /// Run code: ephemeral, uses custom test cases, no scoring side effects.
    Run,
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
    /// Submission mode. Default: Submit.
    #[serde(default)]
    pub mode: SubmissionMode,
    /// Pre-resolved test cases.
    #[serde(default)]
    pub test_cases: Vec<TestCaseRow>,
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
