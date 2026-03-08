use serde::{Deserialize, Serialize};

use super::submission::SourceFile;
use super::verdict::Verdict;

/// Input to checker format plugin's parse_verdict handler.
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
    /// Opaque config blob from the plugin_config table (namespace="checker").
    /// Checkers that support configuration (e.g., float tolerances) deserialize this.
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

/// Output from checker format plugin handlers (returned by run_checker host fn).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerVerdict {
    pub verdict: Verdict,
    pub score: f64,
    pub message: Option<String>,
}

/// Input from evaluator plugins to the run_checker host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckerInput {
    pub format: String,
    #[serde(flatten)]
    pub input: CheckerParseInput,
}
