use serde::{Deserialize, Serialize};

use super::query::TestCaseRow;
use super::submission::SourceFile;

/// Input to a plugin's on_code_run handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnCodeRunInput {
    pub id: i32,
    pub user_id: i32,
    pub problem_id: i32,
    pub contest_id: Option<i32>,
    pub files: Vec<SourceFile>,
    pub language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    pub problem_type: String,
    /// Pre-resolved custom test cases.
    #[serde(default)]
    pub test_cases: Vec<TestCaseRow>,
}

/// Output from a plugin's on_code_run handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnCodeRunOutput {
    pub success: bool,
    pub error_message: Option<String>,
}
