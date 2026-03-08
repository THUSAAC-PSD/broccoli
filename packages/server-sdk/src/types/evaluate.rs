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
    pub time_used_ms: Option<i64>,
    pub memory_used_kb: Option<i64>,
    pub message: Option<String>,
}
