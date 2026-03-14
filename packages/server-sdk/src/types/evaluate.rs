use serde::{Deserialize, Serialize};

use super::submission::SourceFile;
use super::verdict::Verdict;

/// Contest/plugin-facing input for starting evaluation of one test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateCaseInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
}

/// Server-enriched input forwarded to the evaluator plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEvalOpsInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,

    /// Test case input (stdin content). Server-enriched.
    #[serde(default)]
    pub test_input: String,
    /// Expected output for checker. Server-enriched.
    #[serde(default)]
    pub expected_output: String,
    /// Checker format name (e.g. "exact", "tokens"). Server-enriched.
    #[serde(default)]
    pub checker_format: Option<String>,
    /// Opaque checker config blob. Server-enriched.
    #[serde(default)]
    pub checker_config: Option<serde_json::Value>,
    /// Checker source files (for custom/testlib checkers). Server-enriched.
    #[serde(default)]
    pub checker_source: Option<Vec<SourceFile>>,
}

/// Input for start_evaluate_batch host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateBatchInput {
    pub problem_type: String,
    pub test_cases: Vec<StartEvaluateCaseInput>,
}

/// Verdict for a single test case, returned by evaluator's evaluate function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseVerdict {
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub score: f64,
    /// Time used, in milliseconds.
    pub time_used_ms: Option<i64>,
    /// Memory used, in kilobytes.
    pub memory_used_kb: Option<i64>,
    pub message: Option<String>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
}

impl TestCaseVerdict {
    /// Convenience constructor: Accepted with default time/memory.
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

    /// Convenience constructor: WrongAnswer with default time/memory.
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

    /// Convenience constructor: TimeLimitExceeded.
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

    /// Convenience constructor: CompileError.
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

    /// Convenience constructor: SystemError.
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
