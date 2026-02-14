use chrono::{DateTime, Utc};
use common::{SubmissionStatus, Verdict};
use serde::{Deserialize, Serialize};

use crate::entity::submission::SubmissionFile;
use crate::error::AppError;
use crate::utils::filename::validate_flat_filename;

use super::shared::Pagination;

/// A single file in a submission.
#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
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
    /// Programming language (e.g., "cpp", "java", "python").
    #[schema(example = "cpp")]
    pub language: String,
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
    #[schema(example = "2025-10-01T14:30:00Z")]
    pub created_at: DateTime<Utc>,
    /// Total score if judged, null otherwise.
    #[schema(example = 100)]
    pub score: Option<i32>,
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
    pub verdict: Option<Verdict>,
    /// Total score across all test cases.
    #[schema(example = 100)]
    pub score: Option<i32>,
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
    pub verdict: Verdict,
    #[schema(example = 10)]
    pub score: i32,
    /// Time used in milliseconds.
    #[schema(example = 5)]
    pub time_used: Option<i32>,
    /// Memory used in kilobytes.
    #[schema(example = 256)]
    pub memory_used: Option<i32>,
    #[schema(example = 1)]
    pub test_case_id: i32,
    /// Program stdout.
    pub stdout: Option<String>,
    /// Program stderr.
    pub stderr: Option<String>,
    /// Custom checker feedback.
    pub checker_output: Option<String>,
}

/// Maximum total size of all files in bytes.
pub const DEFAULT_MAX_SUBMISSION_SIZE: usize = 1_048_576; // 1 MB

/// Validate a submission creation request.
pub fn validate_create_submission(
    req: &CreateSubmissionRequest,
    max_size: usize,
) -> Result<(), AppError> {
    use std::collections::HashSet;

    if req.files.is_empty() {
        return Err(AppError::Validation("At least one file is required".into()));
    }

    let mut total_size = 0usize;
    let mut seen_filenames = HashSet::with_capacity(req.files.len());

    for file in &req.files {
        // Validate filename using shared validation
        let filename = validate_flat_filename(&file.filename)
            .map_err(|e| AppError::Validation(e.message().into()))?;

        // Check for duplicates
        if !seen_filenames.insert(filename) {
            return Err(AppError::Validation(format!(
                "Duplicate filename: '{}'",
                filename
            )));
        }

        // Content must not be empty
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

    Ok(())
}

/// Validate submission list query parameters.
pub fn validate_submission_list_query(query: &SubmissionListQuery) -> Result<(), AppError> {
    if let Some(ref sort_by) = query.sort_by {
        const ALLOWED_SORT_FIELDS: &[&str] = &["created_at", "status"];
        if !ALLOWED_SORT_FIELDS.contains(&sort_by.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid sort_by field '{}'. Allowed: created_at, status",
                sort_by
            )));
        }
    }

    if let Some(ref sort_order) = query.sort_order
        && !["asc", "desc"].contains(&sort_order.to_lowercase().as_str())
    {
        return Err(AppError::Validation(
            "sort_order must be 'asc' or 'desc'".into(),
        ));
    }

    Ok(())
}

/// Request body for bulk-rejudging submissions by filter.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkRejudgeRequest {
    /// Filter by problem ID.
    #[schema(example = 5)]
    pub problem_id: Option<i32>,
    /// Filter by contest ID.
    #[schema(example = 2)]
    pub contest_id: Option<i32>,
    /// Filter by language.
    #[schema(example = "cpp")]
    pub language: Option<String>,
    /// Filter by verdict (PascalCase: Accepted, WrongAnswer, TimeLimitExceeded, MemoryLimitExceeded, RuntimeError, SystemError).
    #[schema(example = "WrongAnswer")]
    pub verdict: Option<String>,
    /// Filter by user ID.
    #[schema(example = 7)]
    pub user_id: Option<i32>,
}

/// Response from bulk rejudge.
#[derive(Serialize, utoipa::ToSchema)]
pub struct BulkRejudgeResponse {
    /// Number of submissions queued for rejudging.
    #[schema(example = 1234)]
    pub queued: usize,
}

pub fn validate_bulk_rejudge(req: &BulkRejudgeRequest) -> Result<(), AppError> {
    if req.problem_id.is_none()
        && req.contest_id.is_none()
        && req.language.is_none()
        && req.verdict.is_none()
        && req.user_id.is_none()
    {
        return Err(AppError::Validation(
            "At least one filter field must be provided".into(),
        ));
    }

    if let Some(ref verdict) = req.verdict {
        verdict
            .parse::<Verdict>()
            .map_err(|e| AppError::Validation(e.to_string()))?;
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
            stdout: m.stdout,
            stderr: m.stderr,
            checker_output: m.checker_output,
        }
    }
}
