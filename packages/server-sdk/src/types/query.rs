use serde::{Deserialize, Serialize};

/// DB query result for test case listing (id, score, position, is_sample).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseRow {
    pub id: i32,
    pub score: i32,
    pub is_sample: bool,
    pub position: i32,
}

/// DB query result for test case content (input and expected output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseData {
    pub input: String,
    pub expected_output: String,
}

/// DB query result for problem checker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemCheckerInfo {
    pub id: i32,
    pub checker_source: Option<serde_json::Value>,
    pub checker_format: Option<String>,
}
