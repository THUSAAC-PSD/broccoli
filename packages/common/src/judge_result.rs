use crate::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgeSystemErrorInfo {
    /// Machine-readable error code (e.g., "MQ_ERROR", "SANDBOX_ERROR").
    pub code: String,
    /// Human-readable error description.
    pub message: String,
}

impl JudgeSystemErrorInfo {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Result from worker after judging a submission.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JudgeResult {
    /// Original job ID.
    pub job_id: String,
    /// Submission that was judged.
    pub submission_id: i32,
    /// Final status after judging.
    pub status: SubmissionStatus,
    /// Execution verdict (None if compilation failed or system error).
    pub verdict: Option<Verdict>,
    /// Total score across all test cases.
    pub score: Option<i32>,
    /// Maximum time used across all test cases (milliseconds).
    pub time_used: Option<i32>,
    /// Maximum memory used across all test cases (kilobytes).
    pub memory_used: Option<i32>,
    /// Compiler output (stdout/stderr).
    pub compile_output: Option<String>,
    /// Structured error info (only for SystemError status).
    pub error_info: Option<JudgeSystemErrorInfo>,
    /// Individual test case results.
    pub test_case_results: Vec<TestCaseJudgeResult>,
}

impl JudgeResult {
    /// Create a result indicating system error.
    pub fn system_error(
        job_id: String,
        submission_id: i32,
        error_info: JudgeSystemErrorInfo,
    ) -> Self {
        Self {
            job_id,
            submission_id,
            status: SubmissionStatus::SystemError,
            verdict: None,
            score: None,
            time_used: None,
            memory_used: None,
            compile_output: None,
            error_info: Some(error_info),
            test_case_results: vec![],
        }
    }
}

/// Result for a single test case execution.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TestCaseJudgeResult {
    /// Test case ID that was executed.
    pub test_case_id: i32,
    /// Verdict for this test case.
    pub verdict: Verdict,
    /// Points earned for this test case.
    pub score: i32,
    /// Time used in milliseconds.
    pub time_used: Option<i32>,
    /// Memory used in kilobytes.
    pub memory_used: Option<i32>,
    /// Program stdout.
    pub stdout: Option<String>,
    /// Program stderr.
    pub stderr: Option<String>,
    /// Custom checker feedback.
    pub checker_output: Option<String>,
}
