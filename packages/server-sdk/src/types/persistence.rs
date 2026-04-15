use super::verdict::Verdict;

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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubmissionUpdate {
    pub submission_id: i32,
    pub judge_epoch: i32,
    pub status: Option<SubmissionStatus>,
    pub verdict: Option<Option<Verdict>>,
    pub score: Option<f64>,
    pub time_used: Option<Option<i32>>,
    pub memory_used: Option<Option<i32>>,
    pub compile_output: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
}

impl SubmissionUpdate {
    pub fn new(submission_id: i32, judge_epoch: i32) -> Self {
        Self {
            submission_id,
            judge_epoch,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCaseResultRow {
    pub submission_id: i32,
    pub test_case_id: Option<i32>,
    pub run_index: Option<i32>,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

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

impl CodeRunUpdate {
    pub fn new(code_run_id: i32) -> Self {
        Self {
            code_run_id,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeRunResultRow {
    pub code_run_id: i32,
    pub run_index: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}
