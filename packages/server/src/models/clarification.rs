use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for creating a clarification.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateClarificationRequest {
    /// The clarification body (1 – 10 000 chars).
    #[schema(example = "Is the input guaranteed to be sorted?")]
    pub content: String,
    /// Type of clarification: `announcement`, `question`, or `direct_message`.
    #[schema(example = "question")]
    pub clarification_type: String,
    /// Target user ID (required when `clarification_type` is `direct_message`).
    #[schema(example = 7)]
    pub recipient_id: Option<i32>,
    /// Override default visibility. Announcements are always public.
    #[schema(example = false)]
    pub is_public: Option<bool>,
}

/// Request body for replying to a clarification.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReplyClarificationRequest {
    /// Reply content (1 – 10 000 chars).
    #[schema(example = "Yes, the input is always sorted in ascending order.")]
    pub content: String,
    /// If true the reply is visible to all contest participants (admin-only flag).
    #[schema(example = true)]
    pub is_public: bool,
}

/// Request body for resolving / reopening a clarification thread.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ResolveClarificationRequest {
    /// true = resolve, false = reopen.
    #[schema(example = true)]
    pub resolved: bool,
}

/// Query params for toggling reply visibility.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ToggleReplyPublicQuery {
    /// If true, also make the parent question visible to all when making a reply public.
    pub include_question: Option<bool>,
}

/// Optional query params for listing clarifications.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClarificationListQuery {
    /// Filter by type: `announcement`, `question`, or `direct_message`.
    #[param(example = "question")]
    pub r#type: Option<String>,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// A single reply within a clarification thread.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ClarificationReplyResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = 42)]
    pub author_id: i32,
    #[schema(example = "admin")]
    pub author_name: String,
    pub content: String,
    #[schema(example = true)]
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
}

/// A single clarification with author names resolved.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ClarificationResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = 1)]
    pub contest_id: i32,
    #[schema(example = 101)]
    pub author_id: i32,
    #[schema(example = "alice")]
    pub author_name: String,
    pub content: String,
    #[schema(example = "question")]
    pub clarification_type: String,
    pub recipient_id: Option<i32>,
    pub recipient_name: Option<String>,
    #[schema(example = false)]
    pub is_public: bool,

    // Legacy single-reply fields (kept for backwards compat, populated from first reply)
    pub reply_content: Option<String>,
    pub reply_author_id: Option<i32>,
    pub reply_author_name: Option<String>,
    #[schema(example = false)]
    pub reply_is_public: bool,
    pub replied_at: Option<DateTime<Utc>>,

    /// All replies in chronological order.
    pub replies: Vec<ClarificationReplyResponse>,

    /// Whether this thread is resolved.
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<i32>,
    pub resolved_by_name: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for listing clarifications.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ClarificationListResponse {
    pub data: Vec<ClarificationResponse>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const VALID_TYPES: &[&str] = &["announcement", "question", "direct_message"];

pub fn validate_create_clarification(req: &CreateClarificationRequest) -> Result<(), AppError> {
    let content = req.content.trim();
    if content.is_empty() || content.chars().count() > 10_000 {
        return Err(AppError::Validation(
            "Content must be 1 – 10 000 characters".into(),
        ));
    }
    if !VALID_TYPES.contains(&req.clarification_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Invalid clarification_type '{}'. Must be one of: {}",
            req.clarification_type,
            VALID_TYPES.join(", ")
        )));
    }
    if req.clarification_type == "direct_message" && req.recipient_id.is_none() {
        return Err(AppError::Validation(
            "recipient_id is required for direct_message".into(),
        ));
    }
    Ok(())
}

pub fn validate_reply_clarification(req: &ReplyClarificationRequest) -> Result<(), AppError> {
    let content = req.content.trim();
    if content.is_empty() || content.chars().count() > 10_000 {
        return Err(AppError::Validation(
            "Reply content must be 1 – 10 000 characters".into(),
        ));
    }
    Ok(())
}
