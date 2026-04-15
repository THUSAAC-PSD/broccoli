use serde::{Deserialize, Serialize};

use super::submission::SourceFile;
use super::verdict::Verdict;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerParseInput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub expected_output: String,
    #[serde(default)]
    pub test_input: String,
    #[serde(default)]
    pub checker_source: Option<Vec<SourceFile>>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerVerdict {
    pub verdict: Verdict,
    pub score: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckerInput {
    pub format: String,
    #[serde(flatten)]
    pub input: CheckerParseInput,
}
