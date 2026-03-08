use super::verdict::Verdict;

/// Submission status after judging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Judged,
    CompilationError,
}

impl SubmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Judged => "Judged",
            Self::CompilationError => "CompilationError",
        }
    }
}

/// Data for updating a submission after judging.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmissionUpdate {
    pub submission_id: i32,
    pub status: SubmissionStatus,
    pub verdict: Option<Verdict>,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
}

/// Data for inserting a single test case result row.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCaseResultRow {
    pub submission_id: i32,
    pub test_case_id: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
}
