use super::verdict::Verdict;

/// Submission status for judge pipeline updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Running,
    Judged,
    CompilationError,
}

impl SubmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Judged => "Judged",
            Self::CompilationError => "CompilationError",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Judged | Self::CompilationError)
    }
}

/// Data for updating a submission.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubmissionUpdate {
    pub submission_id: i32,
    pub status: Option<SubmissionStatus>,
    pub verdict: Option<Option<Verdict>>,
    pub score: Option<f64>,
    pub time_used: Option<Option<i32>>,
    pub memory_used: Option<Option<i32>>,
    pub compile_output: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
}

/// Data for inserting a single test case result row.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCaseResultRow {
    pub submission_id: i32,
    /// NULL for custom run test cases (which have no DB-backed test_case row).
    pub test_case_id: Option<i32>,
    /// 0-based index for custom run test cases. None for DB-backed test cases.
    pub run_index: Option<i32>,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Data for updating a code_run row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodeRunUpdate {
    pub code_run_id: i32,
    pub status: Option<SubmissionStatus>,
    pub verdict: Option<Option<Verdict>>,
    pub score: Option<f64>,
    pub time_used: Option<Option<i32>>,
    pub memory_used: Option<Option<i32>>,
    pub compile_output: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
}

/// Data for inserting a single code run result row.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRunResultRow {
    pub code_run_id: i32,
    /// 0-based index into the code_run's custom_test_cases array.
    pub run_index: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}
