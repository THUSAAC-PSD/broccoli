use chrono::{DateTime, Utc};
use common::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::utils::filename::validate_flat_filename;

use super::shared::Pagination;

/// A single file in a multi-file submission.
/// Stored as JSON array in the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmissionFile {
    /// Filename (e.g., "Main.java", "solution.cpp")
    pub filename: String,
    /// Source code content
    pub content: String,
}

/// A single file in a submission.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SubmissionFileDto {
    /// Filename (e.g., "Main.java", "solution.cpp"). No path separators allowed.
    #[schema(example = "solution.cpp")]
    pub filename: String,
    /// Source code content.
    #[schema(example = "#include <iostream>\nint main() { return 0; }")]
    pub content: String,
}

impl From<SubmissionFileDto> for SubmissionFile {
    fn from(dto: SubmissionFileDto) -> Self {
        Self {
            filename: dto.filename,
            content: dto.content,
        }
    }
}

impl From<SubmissionFile> for SubmissionFileDto {
    fn from(file: SubmissionFile) -> Self {
        Self {
            filename: file.filename,
            content: file.content,
        }
    }
}

/// Request body for creating a submission.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSubmissionRequest {
    /// Source files. At least one file required.
    pub files: Vec<SubmissionFileDto>,
    /// Programming language (e.g., "cpp", "java", "python3").
    #[schema(example = "cpp")]
    pub language: String,
    /// Optional contest type override for standalone submissions (e.g., "ioi", "icpc").
    /// If omitted, uses the problem's default_contest_type.
    #[schema(example = "ioi")]
    pub contest_type: Option<String>,
}

/// Request body for running code against custom test cases.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct RunCodeRequest {
    /// Source files. At least one file required.
    pub files: Vec<SubmissionFileDto>,
    /// Programming language (e.g., "cpp", "java", "python3").
    #[schema(example = "cpp")]
    pub language: String,
    /// Custom test cases to run against. At least one required.
    pub custom_test_cases: Vec<CustomTestCaseInput>,
}

/// A custom test case for run code requests.
#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CustomTestCaseInput {
    /// Input data to feed to stdin.
    pub input: String,
    /// Expected output. If omitted, output is shown but not checked.
    pub expected_output: Option<String>,
}

/// Query parameters for submission listing.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubmissionListQuery {
    #[param(example = 1)]
    pub page: Option<u64>,
    #[param(example = 20)]
    pub per_page: Option<u64>,
    /// Filter by problem ID.
    #[param(example = 1)]
    pub problem_id: Option<i32>,
    /// Filter by user ID.
    #[param(example = 1)]
    pub user_id: Option<i32>,
    /// Filter by language.
    #[param(example = "cpp")]
    pub language: Option<String>,
    /// Filter by status.
    pub status: Option<SubmissionStatus>,
    /// Sort field: `created_at` (default), `status`.
    #[param(example = "created_at")]
    pub sort_by: Option<String>,
    /// Sort direction: `asc` or `desc` (default).
    #[param(example = "desc")]
    pub sort_order: Option<String>,
    /// Include run submissions in results. Default: false.
    #[serde(default)]
    pub include_runs: Option<bool>,
}

/// Full submission details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SubmissionResponse {
    #[schema(example = 1)]
    pub id: i32,
    pub files: Vec<SubmissionFileDto>,
    #[schema(example = "cpp")]
    pub language: String,
    pub status: SubmissionStatus,
    #[schema(example = 1)]
    pub user_id: i32,
    #[schema(example = "alice")]
    pub username: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "Two Sum")]
    pub problem_title: String,
    /// Contest ID if this is a contest submission, null otherwise.
    #[schema(example = 1)]
    pub contest_id: Option<i32>,
    /// Contest type used for judging this submission.
    #[schema(example = "ioi")]
    pub contest_type: String,
    /// Submission mode: "Submit" for formal submissions, "Run" for run-code executions.
    pub mode: String,
    /// Custom test cases (only populated for runs with custom test cases).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_test_cases: Option<Vec<CustomTestCaseInput>>,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    /// Judge result if judging is complete, null otherwise.
    pub result: Option<JudgeResultResponse>,
}

/// Submission summary for list views (files omitted).
#[derive(Serialize, utoipa::ToSchema)]
pub struct SubmissionListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "cpp")]
    pub language: String,
    pub status: SubmissionStatus,
    /// Execution verdict if judged, null otherwise.
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    #[schema(example = 1)]
    pub user_id: i32,
    #[schema(example = "alice")]
    pub username: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "Two Sum")]
    pub problem_title: String,
    /// Contest ID if this is a contest submission, null otherwise.
    pub contest_id: Option<i32>,
    /// Contest type used for judging.
    #[schema(example = "ioi")]
    pub contest_type: String,
    /// Submission mode: "Submit" for formal submissions, "Run" for run-code executions.
    pub mode: String,
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    /// Total score if judged, null otherwise.
    #[schema(example = 100.0)]
    pub score: Option<f64>,
    /// Total time used in ms if judged, null otherwise.
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    /// Total memory used in KB if judged, null otherwise.
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
}

/// Paginated list of submissions.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SubmissionListResponse {
    pub data: Vec<SubmissionListItem>,
    pub pagination: Pagination,
}

/// Judge result for a submission.
#[derive(Serialize, utoipa::ToSchema)]
pub struct JudgeResultResponse {
    /// Execution verdict (null if compilation failed or system error).
    #[schema(value_type = Option<String>, example = "Accepted")]
    pub verdict: Option<Verdict>,
    /// Total score across all test cases.
    #[schema(example = 100.0)]
    pub score: Option<f64>,
    /// Maximum time used in milliseconds.
    #[schema(example = 50)]
    pub time_used: Option<i32>,
    /// Maximum memory used in kilobytes.
    #[schema(example = 1024)]
    pub memory_used: Option<i32>,
    /// Compiler output (stdout/stderr).
    pub compile_output: Option<String>,
    /// System error message (only for SystemError status).
    pub error_message: Option<String>,
    /// When judging completed.
    pub judged_at: Option<DateTime<Utc>>,
    /// Individual test case results.
    pub test_case_results: Vec<TestCaseResultResponse>,
}

/// Result for a single test case.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TestCaseResultResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(value_type = String, example = "Accepted")]
    pub verdict: Verdict,
    #[schema(example = 10.0)]
    pub score: f64,
    /// Time used in milliseconds.
    #[schema(example = 5)]
    pub time_used: Option<i32>,
    /// Memory used in kilobytes.
    #[schema(example = 256)]
    pub memory_used: Option<i32>,
    /// DB test case ID. Null for custom run test cases.
    pub test_case_id: Option<i32>,
    /// 0-based index for custom run test cases. Null for DB-backed test cases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_index: Option<i32>,
    /// Test case input (only visible for sample test cases or when user has view_all permission).
    pub input: Option<String>,
    /// Expected output (only visible for sample test cases or when user has view_all permission).
    pub expected_output: Option<String>,
    /// Program stdout.
    pub stdout: Option<String>,
    /// Program stderr.
    pub stderr: Option<String>,
    /// Custom checker feedback.
    pub checker_output: Option<String>,
}

/// Request body for bulk-rejudging submissions by explicit IDs.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkRejudgeRequest {
    /// Submission IDs to rejudge. Duplicate IDs are ignored.
    pub submission_ids: Vec<i32>,
}

/// Response from bulk rejudge.
#[derive(Serialize, utoipa::ToSchema)]
pub struct BulkRejudgeResponse {
    /// Number of submissions queued for rejudging.
    #[schema(example = 1234)]
    pub queued: usize,
}

pub fn validate_bulk_rejudge(req: &BulkRejudgeRequest) -> Result<(), AppError> {
    if req.submission_ids.is_empty() {
        return Err(AppError::Validation(
            "submission_ids cannot be empty".into(),
        ));
    }

    if let Some(invalid_id) = req.submission_ids.iter().copied().find(|id| *id <= 0) {
        return Err(AppError::Validation(format!(
            "submission_ids contains invalid id {invalid_id}. IDs must be positive integers."
        )));
    }

    Ok(())
}

impl From<crate::entity::test_case_result::Model> for TestCaseResultResponse {
    fn from(m: crate::entity::test_case_result::Model) -> Self {
        Self {
            id: m.id,
            verdict: m.verdict,
            score: m.score,
            time_used: m.time_used,
            memory_used: m.memory_used,
            test_case_id: m.test_case_id,
            run_index: m.run_index,
            input: None,
            expected_output: None,
            stdout: m.stdout,
            stderr: m.stderr,
            checker_output: m.checker_output,
        }
    }
}

/// Validate a run code request.
pub fn validate_run_code(req: &RunCodeRequest, max_size: usize) -> Result<(), AppError> {
    use std::collections::HashSet;

    if req.files.is_empty() {
        return Err(AppError::Validation("At least one file is required".into()));
    }

    let mut total_size = 0usize;
    let mut seen_filenames = HashSet::with_capacity(req.files.len());

    for file in &req.files {
        let filename = validate_flat_filename(&file.filename)
            .map_err(|e| AppError::Validation(e.message().into()))?;

        if !seen_filenames.insert(filename) {
            return Err(AppError::Validation(format!(
                "Duplicate filename: '{}'",
                filename
            )));
        }

        if file.content.is_empty() {
            return Err(AppError::Validation(format!(
                "File '{}' cannot be empty",
                filename
            )));
        }

        total_size = total_size.saturating_add(file.content.len());
    }

    if total_size > max_size {
        return Err(AppError::Validation(format!(
            "Total submission size ({} bytes) exceeds maximum ({} bytes)",
            total_size, max_size
        )));
    }

    let language = req.language.trim();
    if language.is_empty() {
        return Err(AppError::Validation("Language is required".into()));
    }

    if req.custom_test_cases.is_empty() {
        return Err(AppError::Validation(
            "At least one custom test case is required".into(),
        ));
    }
    if req.custom_test_cases.len() > 10 {
        return Err(AppError::Validation(
            "Maximum 10 custom test cases allowed".into(),
        ));
    }
    for (i, tc) in req.custom_test_cases.iter().enumerate() {
        if tc.input.len() > 1_048_576 {
            return Err(AppError::Validation(format!(
                "Custom test case {} input exceeds 1MB limit",
                i
            )));
        }
        if let Some(ref expected) = tc.expected_output
            && expected.len() > 1_048_576
        {
            return Err(AppError::Validation(format!(
                "Custom test case {} expected_output exceeds 1MB limit",
                i
            )));
        }
    }

    Ok(())
}
