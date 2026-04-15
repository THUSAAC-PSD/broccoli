use serde::{Deserialize, Serialize};

use super::query::TestCaseRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub filename: String,
    pub content: String,
}

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
    #[serde(default)]
    pub test_cases: Vec<TestCaseRow>,
    #[serde(default)]
    pub judge_epoch: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnSubmissionOutput {
    pub success: bool,
    pub error_message: Option<String>,
}
