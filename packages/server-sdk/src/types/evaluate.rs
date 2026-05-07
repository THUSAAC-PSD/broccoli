use serde::{Deserialize, Serialize};

use super::operation::ResourceLimits;
use super::submission::SourceFile;
use super::verdict::Verdict;

pub const DEFAULT_EVALUATION_RESULT_TIMEOUT_MIN_MS: u64 = 15 * 60 * 1000;
pub const DEFAULT_EVALUATION_RESULT_TIMEOUT_MAX_MS: u64 = 60 * 60 * 1000;
pub const DEFAULT_EVALUATION_CHECKER_SLACK_S: f64 = 180.0;
pub const DEFAULT_EVALUATION_QUEUE_SLACK_S: f64 = 300.0;

#[derive(Debug, Clone, Copy)]
pub struct EvaluationTimeoutBudget {
    pub compile_units: u32,
    pub compile_time_limit_s: f64,
    pub compile_wall_time_multiplier: f64,
    pub compile_extra_time_s: f64,
    pub exec_time_limit_s: f64,
    pub exec_wall_time_multiplier: f64,
    pub exec_extra_time_s: f64,
    pub manager_time_limit_s: f64,
    pub manager_wall_time_multiplier: f64,
    pub manager_extra_time_s: f64,
    pub checker_slack_s: f64,
    pub queue_slack_s: f64,
    pub minimum_timeout_ms: u64,
    pub maximum_timeout_ms: u64,
}

impl EvaluationTimeoutBudget {
    pub fn default_for_time_limit_ms(time_limit_ms: i32) -> Self {
        Self {
            compile_units: 1,
            compile_time_limit_s: 30.0,
            compile_wall_time_multiplier: 2.0,
            compile_extra_time_s: 0.0,
            exec_time_limit_s: seconds_from_ms(time_limit_ms),
            exec_wall_time_multiplier: 5.0,
            exec_extra_time_s: 0.0,
            manager_time_limit_s: 0.0,
            manager_wall_time_multiplier: 1.0,
            manager_extra_time_s: 0.0,
            checker_slack_s: DEFAULT_EVALUATION_CHECKER_SLACK_S,
            queue_slack_s: DEFAULT_EVALUATION_QUEUE_SLACK_S,
            minimum_timeout_ms: DEFAULT_EVALUATION_RESULT_TIMEOUT_MIN_MS,
            maximum_timeout_ms: DEFAULT_EVALUATION_RESULT_TIMEOUT_MAX_MS,
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        let compile_s = self.compile_units as f64
            * positive_or_zero(
                self.compile_time_limit_s * self.compile_wall_time_multiplier
                    + self.compile_extra_time_s,
            );
        let exec_s = positive_or_zero(
            self.exec_time_limit_s * self.exec_wall_time_multiplier + self.exec_extra_time_s,
        );
        let manager_s = positive_or_zero(
            self.manager_time_limit_s * self.manager_wall_time_multiplier
                + self.manager_extra_time_s,
        );
        let slack_s = positive_or_zero(self.checker_slack_s) + positive_or_zero(self.queue_slack_s);
        let total_s = compile_s + exec_s + manager_s + slack_s;
        let computed = millis_from_seconds(total_s);
        let max = self.maximum_timeout_ms.max(self.minimum_timeout_ms);
        computed.clamp(self.minimum_timeout_ms, max)
    }
}

pub fn default_evaluation_result_timeout_ms(time_limit_ms: i32) -> u64 {
    EvaluationTimeoutBudget::default_for_time_limit_ms(time_limit_ms).timeout_ms()
}

pub fn seconds_from_ms(time_limit_ms: i32) -> f64 {
    positive_or_zero(time_limit_ms as f64 / 1000.0)
}

fn positive_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn millis_from_seconds(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let ms = (seconds * 1000.0).ceil();
    if ms >= u64::MAX as f64 {
        u64::MAX
    } else {
        ms as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TestCaseBodyRef {
    Inline {
        text: String,
    },
    Blob {
        hash: String,
    },
    #[default]
    Missing,
}

impl TestCaseBodyRef {
    pub fn inline(text: impl Into<String>) -> Self {
        Self::Inline { text: text.into() }
    }

    pub fn blob(hash: impl Into<String>) -> Self {
        Self::Blob { hash: hash.into() }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JudgeFile {
    Inline {
        text: String,
    },
    Blob {
        file: FileRef,
    },
    #[default]
    Missing,
}

impl JudgeFile {
    pub fn inline(text: impl Into<String>) -> Self {
        Self::Inline { text: text.into() }
    }

    pub fn blob(file: FileRef) -> Self {
        Self::Blob { file }
    }

    pub fn is_blob(&self) -> bool {
        matches!(self, Self::Blob { .. })
    }

    pub fn inline_text(&self) -> &str {
        match self {
            Self::Inline { text } => text,
            Self::Blob { .. } | Self::Missing => "",
        }
    }
}

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
    pub input: TestCaseBodyRef,
    #[serde(default)]
    pub expected_output: TestCaseBodyRef,
    #[serde(default)]
    pub is_custom: bool,
    /// Pin the resulting operation to a specific worker. Set by contest
    /// plugins from `OnSubmissionInput.target_worker_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<String>,
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[test]
    fn default_timeout_is_generous_floor_for_normal_limits() {
        assert_eq!(
            default_evaluation_result_timeout_ms(2000),
            DEFAULT_EVALUATION_RESULT_TIMEOUT_MIN_MS
        );
    }

    #[test]
    fn timeout_scales_with_large_problem_time_limit() {
        let budget = EvaluationTimeoutBudget {
            exec_time_limit_s: 600.0,
            ..EvaluationTimeoutBudget::default_for_time_limit_ms(1000)
        };

        assert!(budget.timeout_ms() > DEFAULT_EVALUATION_RESULT_TIMEOUT_MIN_MS);
    }

    #[test]
    fn configured_max_is_never_below_minimum() {
        let budget = EvaluationTimeoutBudget {
            minimum_timeout_ms: 120_000,
            maximum_timeout_ms: 60_000,
            ..EvaluationTimeoutBudget::default_for_time_limit_ms(1000)
        };

        assert_eq!(budget.timeout_ms(), 120_000);
    }

    #[test]
    fn start_evaluate_case_uses_typed_body_refs() {
        let input = StartEvaluateCaseInput {
            problem_id: 1,
            test_case_id: 2,
            solution_source: vec![],
            solution_language: "cpp".to_string(),
            time_limit_ms: 1000,
            memory_limit_kb: 262_144,
            contest_id: None,
            input: TestCaseBodyRef::inline("1 2\n"),
            expected_output: TestCaseBodyRef::blob("abc123"),
            is_custom: false,
            target_worker_id: None,
        };

        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json["input"],
            serde_json::json!({ "kind": "inline", "text": "1 2\n" })
        );
        assert_eq!(
            json["expected_output"],
            serde_json::json!({ "kind": "blob", "hash": "abc123" })
        );
        assert!(json.get("inline_input").is_none());
        assert!(json.get("input_blob_hash").is_none());
    }

    #[test]
    fn build_eval_ops_uses_typed_judge_files() {
        let input = BuildEvalOpsInput {
            problem_id: 1,
            test_case_id: 2,
            solution_source: vec![],
            solution_language: "cpp".to_string(),
            time_limit_ms: 1000,
            memory_limit_kb: 262_144,
            contest_id: None,
            test_input: JudgeFile::inline("1 2\n"),
            expected_output: JudgeFile::blob(FileRef {
                filename: "answer.txt".to_string(),
                content_type: Some("text/plain".to_string()),
                blob_hash: "abc123".to_string(),
                read_token: None,
            }),
            checker_format: Some("exact".to_string()),
            checker_config: None,
            checker_source: None,
            additional_file_refs: vec![],
            target_worker_id: None,
        };

        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(
            json["test_input"],
            serde_json::json!({ "kind": "inline", "text": "1 2\n" })
        );
        assert_eq!(
            json["expected_output"],
            serde_json::json!({
                "kind": "blob",
                "file": {
                    "filename": "answer.txt",
                    "content_type": "text/plain",
                    "blob_hash": "abc123"
                }
            })
        );
        assert!(json.get("test_input_ref").is_none());
        assert!(json.get("expected_output_ref").is_none());
    }
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
    pub test_input: JudgeFile,
    #[serde(default)]
    pub expected_output: JudgeFile,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub blob_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_token: Option<String>,
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
