use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub use super::shared::{Pagination, escape_like};
use super::shared::{
    double_option, validate_optional_position, validate_reorder_ids, validate_title,
};

/// Request body for creating a problem.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateProblemRequest {
    /// Problem title (trimmed, 1-256 chars).
    #[schema(example = "Two Sum")]
    pub title: String,
    /// Problem statement in Markdown (non-empty, max 1 MB).
    #[schema(example = "Given an array of integers `nums` and an integer `target`...")]
    pub content: String,
    /// Execution time limit in milliseconds (1-30000).
    #[schema(example = 1000)]
    pub time_limit: i32,
    /// Memory limit in kilobytes (1-1048576).
    #[schema(example = 262144)]
    pub memory_limit: i32,
    /// Whether contestants see full input/output for all test cases.
    /// If omitted, defaults to false.
    #[schema(example = false)]
    pub show_test_details: Option<bool>,
}

/// PATCH body for updating a problem. Only provided fields are modified.
#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateProblemRequest {
    /// Problem title (trimmed, 1-256 chars).
    #[schema(example = "Two Sum (Easy)")]
    pub title: Option<String>,
    /// Problem statement in Markdown (non-empty, max 1 MB).
    #[schema(example = "Updated problem statement...")]
    pub content: Option<String>,
    /// Execution time limit in milliseconds (1-30000).
    #[schema(example = 2000)]
    pub time_limit: Option<i32>,
    /// Memory limit in kilobytes (1-1048576).
    #[schema(example = 524288)]
    pub memory_limit: Option<i32>,
    /// Whether contestants see full input/output for all test cases.
    #[schema(example = true)]
    pub show_test_details: Option<bool>,
}

/// Full problem details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Two Sum")]
    pub title: String,
    #[schema(example = "Given an array of integers...")]
    pub content: String,
    #[schema(example = 1000)]
    pub time_limit: i32,
    #[schema(example = 262144)]
    pub memory_limit: i32,
    /// Whether contestants see full input/output for all test cases.
    #[schema(example = false)]
    pub show_test_details: bool,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

/// Problem summary for list views (content omitted).
#[derive(Serialize, FromQueryResult, utoipa::ToSchema)]
pub struct ProblemListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Two Sum")]
    pub title: String,
    #[schema(example = 1000)]
    pub time_limit: i32,
    #[schema(example = 262144)]
    pub memory_limit: i32,
    /// Whether contestants see full input/output for all test cases.
    #[schema(example = false)]
    pub show_test_details: bool,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

/// Paginated list of problems.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemListResponse {
    pub data: Vec<ProblemListItem>,
    pub pagination: Pagination,
}

/// Query parameters for problem listing.
#[derive(Deserialize, utoipa::IntoParams)]
pub struct ProblemListQuery {
    #[param(example = 1)]
    pub page: Option<u64>,
    #[param(example = 20)]
    pub per_page: Option<u64>,
    #[param(example = "sum")]
    pub search: Option<String>,
    /// Sort field: `created_at` (default), `updated_at`, or `title`.
    #[param(example = "created_at")]
    pub sort_by: Option<String>,
    /// Sort direction: `asc` or `desc` (default).
    #[param(example = "desc")]
    pub sort_order: Option<String>,
}

/// Request body for creating a test case.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTestCaseRequest {
    /// Input data. May be empty for output-only or custom-checker problems.
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input: String,
    /// Expected output. May be empty for custom-checker problems.
    #[schema(example = "0 1")]
    pub expected_output: String,
    /// Point value for this test case (0-10000).
    #[schema(example = 10)]
    pub score: i32,
    /// Whether this test case is visible to contestants.
    #[schema(example = true)]
    pub is_sample: bool,
    /// Display position (0-based). Auto-assigned if omitted.
    #[schema(example = 0)]
    pub position: Option<i32>,
    /// Optional human-readable description (max 256 chars).
    #[schema(example = "Basic case")]
    pub description: Option<String>,
}

/// PATCH body for updating a test case. Only provided fields are modified.
#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateTestCaseRequest {
    /// Input data. May be empty for output-only or custom-checker problems.
    #[schema(example = "5\n1 2 3 4 5\n3")]
    pub input: Option<String>,
    /// Expected output. May be empty for custom-checker problems.
    #[schema(example = "1 2")]
    pub expected_output: Option<String>,
    /// Point value for this test case (0-10000).
    #[schema(example = 20)]
    pub score: Option<i32>,
    /// Whether this test case is visible to contestants.
    #[schema(example = false)]
    pub is_sample: Option<bool>,
    /// Display position (0-based).
    #[schema(example = 1)]
    pub position: Option<i32>,
    /// Set to a string value to update, set to `null` to clear, or omit to leave unchanged.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, example = "Updated edge case")]
    pub description: Option<Option<String>>,
}

/// Request body for reordering test cases.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReorderTestCasesRequest {
    /// Ordered list of test_case_ids. Positions assigned 0, 1, 2, ... by array index.
    #[schema(example = json!([3, 1, 2]))]
    pub test_case_ids: Vec<i32>,
}

/// Full test case details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TestCaseResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input: String,
    #[schema(example = "0 1")]
    pub expected_output: String,
    #[schema(example = 10)]
    pub score: i32,
    #[schema(example = "Basic case")]
    pub description: Option<String>,
    #[schema(example = true)]
    pub is_sample: bool,
    #[schema(example = 0)]
    pub position: i32,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "2025-09-01T09:00:00Z")]
    pub created_at: DateTime<Utc>,
}

/// Test case summary (input/output truncated to 100-char previews).
#[derive(Serialize, FromQueryResult, utoipa::ToSchema)]
pub struct TestCaseListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = 10)]
    pub score: i32,
    #[schema(example = "Basic case")]
    pub description: Option<String>,
    #[schema(example = true)]
    pub is_sample: bool,
    #[schema(example = 0)]
    pub position: i32,
    #[schema(example = "4\n2 7 11 15\n9")]
    pub input_preview: String,
    #[schema(example = "0 1")]
    pub output_preview: String,
    #[schema(example = 1)]
    pub problem_id: i32,
    #[schema(example = "2025-09-01T09:00:00Z")]
    pub created_at: DateTime<Utc>,
}

/// Response from ZIP upload.
#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadTestCasesResponse {
    /// Number of test cases created.
    #[schema(example = 5)]
    pub created: usize,
    pub test_cases: Vec<TestCaseListItem>,
}

impl From<crate::entity::problem::Model> for ProblemResponse {
    fn from(m: crate::entity::problem::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            content: m.content,
            time_limit: m.time_limit,
            memory_limit: m.memory_limit,
            show_test_details: m.show_test_details,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl From<crate::entity::test_case::Model> for TestCaseResponse {
    fn from(m: crate::entity::test_case::Model) -> Self {
        Self {
            id: m.id,
            input: m.input,
            expected_output: m.expected_output,
            score: m.score,
            description: m.description,
            is_sample: m.is_sample,
            position: m.position,
            problem_id: m.problem_id,
            created_at: m.created_at,
        }
    }
}

pub const PREVIEW_LENGTH: usize = 100;

pub fn truncate_preview(s: &str) -> String {
    match s.char_indices().nth(PREVIEW_LENGTH) {
        Some((byte_idx, _)) => format!("{}...", &s[..byte_idx]),
        None => s.to_string(),
    }
}

pub fn validate_create_problem(req: &CreateProblemRequest) -> Result<(), AppError> {
    validate_title(&req.title)?;
    if req.content.trim().is_empty() || req.content.len() > 1_000_000 {
        return Err(AppError::Validation(
            "Content must be non-empty and at most 1MB".into(),
        ));
    }
    if !(1..=30000).contains(&req.time_limit) {
        return Err(AppError::Validation("Time limit must be 1-30000 ms".into()));
    }
    if !(1..=1_048_576).contains(&req.memory_limit) {
        return Err(AppError::Validation(
            "Memory limit must be 1-1048576 KB".into(),
        ));
    }
    Ok(())
}

pub fn validate_update_problem(req: &UpdateProblemRequest) -> Result<(), AppError> {
    if let Some(ref title) = req.title {
        validate_title(title)?;
    }
    if let Some(ref content) = req.content
        && (content.trim().is_empty() || content.len() > 1_000_000)
    {
        return Err(AppError::Validation(
            "Content must be non-empty and at most 1MB".into(),
        ));
    }
    if let Some(tl) = req.time_limit
        && !(1..=30000).contains(&tl)
    {
        return Err(AppError::Validation("Time limit must be 1-30000 ms".into()));
    }
    if let Some(ml) = req.memory_limit
        && !(1..=1_048_576).contains(&ml)
    {
        return Err(AppError::Validation(
            "Memory limit must be 1-1048576 KB".into(),
        ));
    }
    Ok(())
}

pub fn validate_create_test_case(req: &CreateTestCaseRequest) -> Result<(), AppError> {
    if !(0..=10_000).contains(&req.score) {
        return Err(AppError::Validation("Score must be 0-10000".into()));
    }
    validate_optional_position(req.position)?;
    if let Some(ref desc) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_reorder_test_cases(req: &ReorderTestCasesRequest) -> Result<(), AppError> {
    validate_reorder_ids(&req.test_case_ids, "test_case_id")
}

pub fn validate_update_test_case(req: &UpdateTestCaseRequest) -> Result<(), AppError> {
    if let Some(score) = req.score
        && !(0..=10_000).contains(&score)
    {
        return Err(AppError::Validation("Score must be 0-10000".into()));
    }
    validate_optional_position(req.position)?;
    if let Some(Some(ref desc)) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    Ok(())
}
