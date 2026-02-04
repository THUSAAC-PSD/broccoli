use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};

use super::shared::{Pagination, validate_optional_position, validate_reorder_ids, validate_title};
use crate::error::AppError;

#[derive(Deserialize)]
pub struct CreateContestRequest {
    pub title: String,
    pub description: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_public: bool,
}

#[derive(Deserialize, Default, PartialEq)]
pub struct UpdateContestRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub is_public: Option<bool>,
}

#[derive(Deserialize)]
pub struct AddContestProblemRequest {
    pub problem_id: i32,
    pub label: String,
    pub position: Option<i32>,
}

#[derive(Deserialize, Default, PartialEq)]
pub struct UpdateContestProblemRequest {
    pub label: Option<String>,
    pub position: Option<i32>,
}

#[derive(Deserialize)]
pub struct AddParticipantRequest {
    pub user_id: i32,
}

#[derive(Deserialize)]
pub struct ReorderContestProblemsRequest {
    /// Ordered list of problem_ids. Positions assigned 0, 1, 2… by array index.
    /// Must contain exactly the problem_ids currently in the contest.
    pub problem_ids: Vec<i32>,
}

#[derive(Deserialize)]
pub struct ContestListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ContestResponse {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult)]
pub struct ContestListItem {
    pub id: i32,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ContestListResponse {
    pub data: Vec<ContestListItem>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct ContestProblemResponse {
    pub contest_id: i32,
    pub problem_id: i32,
    pub label: String,
    pub position: i32,
    pub problem_title: String,
}

#[derive(Serialize)]
pub struct ContestParticipantResponse {
    pub contest_id: i32,
    pub user_id: i32,
    pub username: String,
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
    if let (Some(start), Some(end)) = (req.start_time, req.end_time) {
        if end <= start {
            return Err(AppError::Validation(
                "end_time must be after start_time".into(),
            ));
        }
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
