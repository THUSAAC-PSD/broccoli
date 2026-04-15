use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub inline_input: Option<String>,
    #[serde(default)]
    pub inline_expected_output: Option<String>,
    #[serde(default)]
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseData {
    pub input: String,
    pub expected_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemCheckerInfo {
    pub id: i32,
    pub checker_source: Option<serde_json::Value>,
    pub checker_format: Option<String>,
    #[serde(default)]
    pub checker_config: Option<serde_json::Value>,
}
