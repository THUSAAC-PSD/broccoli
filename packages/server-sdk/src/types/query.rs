use serde::{Deserialize, Serialize};

/// DB query result for test case listing (id, score, position, is_sample).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseRow {
    pub id: i32,
    pub score: f64,
    pub is_sample: bool,
    pub position: i32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// Inline input data for custom run test cases (not from DB).
    #[serde(default)]
    pub inline_input: Option<String>,
    /// Inline expected output for custom run test cases. None means no checking (use "none" checker).
    #[serde(default)]
    pub inline_expected_output: Option<String>,
    /// True if this is a synthetic (custom run) test case, not backed by a DB row.
    #[serde(default)]
    pub is_custom: bool,
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
    /// Opaque checker config from plugin_config table (namespace="checker").
    #[serde(default)]
    pub checker_config: Option<serde_json::Value>,
}
