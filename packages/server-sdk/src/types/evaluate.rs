use serde::{Deserialize, Serialize};

use super::submission::SourceFile;
use super::verdict::Verdict;

/// Input to evaluator plugin handler's build_ops handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEvalOpsInput {
    pub problem_id: i32,
    pub test_case_id: i32,
    pub solution_source: Vec<SourceFile>,
    pub solution_language: String,
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
}

/// Input for start_evaluate_batch host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartEvaluateBatchInput {
    pub problem_type: String,
    pub test_cases: Vec<BuildEvalOpsInput>,
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
        }
    }
}
