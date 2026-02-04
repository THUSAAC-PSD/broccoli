use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::AppError;

/// Serde helper for PATCH semantics on nullable fields.
///
/// * JSON field absent  => `None`          (don't update)
/// * JSON field = null  => `Some(None)`    (set to NULL)
/// * JSON field = value => `Some(Some(v))` (set to value)
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Deserialize)]
pub struct CreateProblemRequest {
    pub title: String,
    pub content: String,
    pub time_limit: i32,
    pub memory_limit: i32,
}

#[derive(Deserialize, Default, PartialEq)]
pub struct UpdateProblemRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub time_limit: Option<i32>,
    pub memory_limit: Option<i32>,
}

#[derive(Serialize)]
pub struct ProblemResponse {
    pub id: i32,
    pub title: String,
    pub content: String,
    pub time_limit: i32,
    pub memory_limit: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult)]
pub struct ProblemListItem {
    pub id: i32,
    pub title: String,
    pub time_limit: i32,
    pub memory_limit: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ProblemListResponse {
    pub data: Vec<ProblemListItem>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct Pagination {
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
    pub total_pages: u64,
}

#[derive(Deserialize)]
pub struct ProblemListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTestCaseRequest {
    pub input: String,
    pub expected_output: String,
    pub score: i32,
    pub is_sample: bool,
    pub position: Option<i32>,
    pub description: Option<String>,
}

#[derive(Deserialize, Default, PartialEq)]
pub struct UpdateTestCaseRequest {
    pub input: Option<String>,
    pub expected_output: Option<String>,
    pub score: Option<i32>,
    pub is_sample: Option<bool>,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
}

#[derive(Serialize)]
pub struct TestCaseResponse {
    pub id: i32,
    pub input: String,
    pub expected_output: String,
    pub score: i32,
    pub description: Option<String>,
    pub is_sample: bool,
    pub position: i32,
    pub problem_id: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult)]
pub struct TestCaseListItem {
    pub id: i32,
    pub score: i32,
    pub description: Option<String>,
    pub is_sample: bool,
    pub position: i32,
    pub input_preview: String,
    pub output_preview: String,
    pub problem_id: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct UploadTestCasesResponse {
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
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 256 {
        return Err(AppError::Validation(
            "Title must be 1-256 characters".into(),
        ));
    }
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
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 256 {
            return Err(AppError::Validation(
                "Title must be 1-256 characters".into(),
            ));
        }
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
    if let Some(pos) = req.position
        && pos < 0
    {
        return Err(AppError::Validation("Position must be >= 0".into()));
    }
    if let Some(ref desc) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_update_test_case(req: &UpdateTestCaseRequest) -> Result<(), AppError> {
    if let Some(score) = req.score
        && !(0..=10_000).contains(&score)
    {
        return Err(AppError::Validation("Score must be 0-10000".into()));
    }
    if let Some(pos) = req.position
        && pos < 0
    {
        return Err(AppError::Validation("Position must be >= 0".into()));
    }
    if let Some(Some(ref desc)) = req.description
        && desc.trim().chars().count() > 256
    {
        return Err(AppError::Validation(
            "Description must be at most 256 characters".into(),
        ));
    }
    Ok(())
}

/// Escape LIKE wildcard characters in a search string.
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
