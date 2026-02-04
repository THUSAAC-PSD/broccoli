use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};

use super::shared::{Pagination, validate_optional_position, validate_reorder_ids, validate_title};
use crate::error::AppError;

/// Request body for creating a contest.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateContestRequest {
    /// Contest title (trimmed, 1-256 chars).
    #[schema(example = "Weekly Contest #42")]
    pub title: String,
    /// Contest description (non-empty, max 1 MB).
    #[schema(example = "Welcome to this week's programming contest.")]
    pub description: String,
    /// Contest start time (must be before end_time).
    #[schema(example = "2025-10-01T14:00:00Z")]
    pub start_time: DateTime<Utc>,
    /// Contest end time (must be after start_time).
    #[schema(example = "2025-10-01T17:00:00Z")]
    pub end_time: DateTime<Utc>,
    /// Whether the contest is visible to all users.
    #[schema(example = true)]
    pub is_public: bool,
}

/// PATCH body for updating a contest. Only provided fields are modified.
#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateContestRequest {
    /// Contest title (trimmed, 1-256 chars).
    #[schema(example = "Weekly Contest #42 (Extended)")]
    pub title: Option<String>,
    /// Contest description (non-empty, max 1 MB).
    #[schema(example = "Updated description...")]
    pub description: Option<String>,
    /// Contest start time (must be before end_time).
    #[schema(example = "2025-10-01T13:00:00Z")]
    pub start_time: Option<DateTime<Utc>>,
    /// Contest end time (must be after start_time).
    #[schema(example = "2025-10-01T18:00:00Z")]
    pub end_time: Option<DateTime<Utc>>,
    /// Whether the contest is visible to all users.
    #[schema(example = false)]
    pub is_public: Option<bool>,
}

/// Request body for adding a problem to a contest.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddContestProblemRequest {
    /// ID of the problem to associate.
    #[schema(example = 1)]
    pub problem_id: i32,
    /// Short label for the problem within the contest (1-10 chars, must be unique).
    #[schema(example = "A")]
    pub label: String,
    /// Display position (0-based). Auto-assigned if omitted.
    #[schema(example = 0)]
    pub position: Option<i32>,
}

/// PATCH body for updating a contest problem's label or position.
#[derive(Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct UpdateContestProblemRequest {
    /// Short label for the problem within the contest (1-10 chars, must be unique).
    #[schema(example = "B")]
    pub label: Option<String>,
    /// Display position (0-based).
    #[schema(example = 1)]
    pub position: Option<i32>,
}

/// Request body for adding a participant to a contest (admin action).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct AddParticipantRequest {
    /// ID of the user to add.
    #[schema(example = 7)]
    pub user_id: i32,
}

/// Request body for reordering problems in a contest.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReorderContestProblemsRequest {
    /// Ordered list of problem_ids. Positions assigned 0, 1, 2... by array index.
    /// Must contain exactly the problem_ids currently in the contest.
    #[schema(example = json!([3, 1, 2]))]
    pub problem_ids: Vec<i32>,
}

/// Query parameters for contest listing.
#[derive(Deserialize, utoipa::IntoParams)]
pub struct ContestListQuery {
    #[param(example = 1)]
    pub page: Option<u64>,
    #[param(example = 20)]
    pub per_page: Option<u64>,
    #[param(example = "weekly")]
    pub search: Option<String>,
    /// Sort field: `created_at` (default), `updated_at`, `start_time`, or `title`.
    #[param(example = "start_time")]
    pub sort_by: Option<String>,
    /// Sort direction: `asc` or `desc` (default).
    #[param(example = "asc")]
    pub sort_order: Option<String>,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full contest details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ContestResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Weekly Contest #42")]
    pub title: String,
    #[schema(example = "Welcome to this week's contest.")]
    pub description: String,
    #[schema(example = "2025-10-01T14:00:00Z")]
    pub start_time: DateTime<Utc>,
    #[schema(example = "2025-10-01T17:00:00Z")]
    pub end_time: DateTime<Utc>,
    #[schema(example = true)]
    pub is_public: bool,
    #[schema(example = "2025-09-25T10:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-25T10:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

/// Contest summary for list views (description omitted).
#[derive(Serialize, FromQueryResult, utoipa::ToSchema)]
pub struct ContestListItem {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "Weekly Contest #42")]
    pub title: String,
    #[schema(example = "2025-10-01T14:00:00Z")]
    pub start_time: DateTime<Utc>,
    #[schema(example = "2025-10-01T17:00:00Z")]
    pub end_time: DateTime<Utc>,
    #[schema(example = true)]
    pub is_public: bool,
    #[schema(example = "2025-09-25T10:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = "2025-09-25T10:30:00Z")]
    pub updated_at: DateTime<Utc>,
}

/// Paginated list of contests.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ContestListResponse {
    pub data: Vec<ContestListItem>,
    pub pagination: Pagination,
}

/// A problem associated with a contest.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ContestProblemResponse {
    #[schema(example = 1)]
    pub contest_id: i32,
    #[schema(example = 5)]
    pub problem_id: i32,
    #[schema(example = "A")]
    pub label: String,
    #[schema(example = 0)]
    pub position: i32,
    #[schema(example = "Two Sum")]
    pub problem_title: String,
}

/// A participant in a contest.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ContestParticipantResponse {
    #[schema(example = 1)]
    pub contest_id: i32,
    #[schema(example = 7)]
    pub user_id: i32,
    #[schema(example = "alice_wonder")]
    pub username: String,
    #[schema(example = "2025-09-30T12:00:00Z")]
    pub registered_at: DateTime<Utc>,
}

impl From<crate::entity::contest::Model> for ContestResponse {
    fn from(m: crate::entity::contest::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            description: m.description,
            start_time: m.start_time,
            end_time: m.end_time,
            is_public: m.is_public,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

pub fn validate_create_contest(req: &CreateContestRequest) -> Result<(), AppError> {
    validate_title(&req.title)?;
    if req.description.trim().is_empty() || req.description.len() > 1_000_000 {
        return Err(AppError::Validation(
            "Description must be non-empty and at most 1MB".into(),
        ));
    }
    if req.end_time <= req.start_time {
        return Err(AppError::Validation(
            "end_time must be after start_time".into(),
        ));
    }
    Ok(())
}

pub fn validate_update_contest(req: &UpdateContestRequest) -> Result<(), AppError> {
    if let Some(ref title) = req.title {
        validate_title(title)?;
    }
    if let Some(ref description) = req.description
        && (description.trim().is_empty() || description.len() > 1_000_000)
    {
        return Err(AppError::Validation(
            "Description must be non-empty and at most 1MB".into(),
        ));
    }
    if let (Some(start), Some(end)) = (req.start_time, req.end_time)
        && end <= start
    {
        return Err(AppError::Validation(
            "end_time must be after start_time".into(),
        ));
    }
    Ok(())
}

pub fn validate_add_contest_problem(req: &AddContestProblemRequest) -> Result<(), AppError> {
    let label = req.label.trim();
    if label.is_empty() || label.chars().count() > 10 {
        return Err(AppError::Validation("Label must be 1-10 characters".into()));
    }
    validate_optional_position(req.position)
}

pub fn validate_reorder_contest_problems(
    req: &ReorderContestProblemsRequest,
) -> Result<(), AppError> {
    validate_reorder_ids(&req.problem_ids, "problem_id")
}

pub fn validate_update_contest_problem(req: &UpdateContestProblemRequest) -> Result<(), AppError> {
    if let Some(ref label) = req.label {
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 10 {
            return Err(AppError::Validation("Label must be 1-10 characters".into()));
        }
    }
    validate_optional_position(req.position)
}
