use serde::{Deserialize, Serialize};

use super::operation::ResourceLimits;
use super::submission::SourceFile;
use super::verdict::Verdict;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateCaseInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    #[serde(default)]
    pub contest_id: Option<i32>,
    #[serde(default)]
    pub inline_input: Option<String>,
    #[serde(default)]
    pub inline_expected_output: Option<String>,
    /// Pin the resulting operation to a specific worker. Set by contest
    /// plugins from `OnSubmissionInput.target_worker_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEvalOpsInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    #[serde(default)]
    pub contest_id: Option<i32>,

    #[serde(default)]
    pub test_input: String,
    #[serde(default)]
    pub expected_output: String,
    #[serde(default)]
    pub checker_format: Option<String>,
    #[serde(default)]
    pub checker_config: Option<serde_json::Value>,
    #[serde(default)]
    pub checker_source: Option<Vec<SourceFile>>,

    #[serde(default)]
    pub additional_file_refs: Vec<FileRef>,

    /// Forwarded from `StartEvaluateCaseInput.target_worker_id`. Evaluator
    /// plugins copy this onto each `OperationTask` they build so the host
    /// routes the operation to the pinned worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateBatchInput {
    pub problem_type: String,
    pub test_cases: Vec<StartEvaluateCaseInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseVerdict {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used_ms: Option<i64>,
    pub memory_used_kb: Option<i64>,
    pub message: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
}

impl TestCaseVerdict {
    pub fn accepted(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::Accepted,
            score: 1.0,
            time_used_ms: Some(100),
            memory_used_kb: Some(1024),
            message: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn wrong_answer(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            time_used_ms: Some(50),
            memory_used_kb: Some(512),
            message: Some("Wrong answer".into()),
            stdout: None,
            stderr: None,
        }
    }

    pub fn tle(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::TimeLimitExceeded,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: Some(512),
            message: Some("Time limit exceeded".into()),
            stdout: None,
            stderr: None,
        }
    }

    pub fn compile_error(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::CompileError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("Compilation failed".into()),
            stdout: None,
            stderr: None,
        }
    }

    pub fn system_error(tc_id: i32) -> Self {
        Self {
            test_case_id: tc_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("System error".into()),
            stdout: None,
            stderr: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRef {
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub blob_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageInput {
    pub language_id: String,
    pub submitted_files: Vec<String>,
    pub additional_files: Vec<FileRef>,
    #[serde(default)]
    pub problem_id: Option<i32>,
    #[serde(default)]
    pub contest_id: Option<i32>,
    #[serde(default)]
    pub overrides: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageOutput {
    pub compile: Option<CompileSpec>,
    pub run: RunSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileSpec {
    pub command: Vec<String>,
    pub cache_inputs: Vec<String>,
    pub outputs: Vec<OutputSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<ResourceLimits>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "pattern")]
pub enum OutputSpec {
    File(String),
    Glob(String),
}

impl OutputSpec {
    pub fn validate(&self) -> Result<(), String> {
        let (value, kind) = match self {
            OutputSpec::File(v) => (v.as_str(), "filename"),
            OutputSpec::Glob(v) => (v.as_str(), "glob"),
        };
        if value.is_empty() {
            return Err(format!("Output {kind} must not be empty"));
        }
        if value.contains("..") || value.starts_with('/') || value.contains('\\') {
            return Err(format!(
                "Output {kind} '{value}' contains unsafe path components"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSpec {
    pub command: Vec<String>,
    pub extra_files: Vec<String>,
}
