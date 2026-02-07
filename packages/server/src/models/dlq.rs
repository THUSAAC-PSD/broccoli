use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dlq::DlqStats;
use crate::entity::dead_letter_message;

use super::shared::Pagination;

/// Query parameters for listing DLQ messages.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListDlqParams {
    /// Filter by message type.
    #[param(example = "judge_job")]
    pub message_type: Option<String>,
    /// Filter by resolved status.
    #[param(example = false)]
    pub resolved: Option<bool>,
    /// Page number (1-indexed).
    #[param(example = 1)]
    pub page: Option<u64>,
    /// Items per page (1-100, default 20).
    #[param(example = 20)]
    pub per_page: Option<u64>,
}

/// DLQ message summary for list views.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DlqMessageResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "job-abc123")]
    pub message_id: String,
    #[schema(example = "judge_job")]
    pub message_type: String,
    /// Submission ID (null if unknown, e.g., deserialization failure).
    #[schema(example = 42)]
    pub submission_id: Option<i32>,
    #[schema(example = "MAX_RETRIES_EXCEEDED")]
    pub error_code: String,
    #[schema(example = "Database connection timeout")]
    pub error_message: String,
    #[schema(example = 3)]
    pub retry_count: i32,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub first_failed_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:05:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = false)]
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    /// User ID who resolved this message (null for automatic resolution).
    pub resolved_by: Option<i32>,
}

impl From<dead_letter_message::Model> for DlqMessageResponse {
    fn from(m: dead_letter_message::Model) -> Self {
        Self {
            id: m.id,
            message_id: m.message_id,
            message_type: m.message_type,
            submission_id: m.submission_id,
            error_code: m.error_code,
            error_message: m.error_message,
            retry_count: m.retry_count,
            first_failed_at: m.first_failed_at,
            created_at: m.created_at,
            resolved: m.resolved,
            resolved_at: m.resolved_at,
            resolved_by: m.resolved_by,
        }
    }
}

/// Full DLQ message details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DlqMessageDetailResponse {
    #[schema(example = 1)]
    pub id: i32,
    #[schema(example = "job-abc123")]
    pub message_id: String,
    #[schema(example = "judge_job")]
    pub message_type: String,
    /// Submission ID (null if unknown, e.g., deserialization failure).
    #[schema(example = 42)]
    pub submission_id: Option<i32>,
    /// Full message payload for replay.
    pub payload: serde_json::Value,
    #[schema(example = "MAX_RETRIES_EXCEEDED")]
    pub error_code: String,
    #[schema(example = "Database connection timeout")]
    pub error_message: String,
    #[schema(example = 3)]
    pub retry_count: i32,
    /// Retry history: array of {attempt, error, timestamp}.
    pub retry_history: serde_json::Value,
    #[schema(example = "2025-09-01T08:00:00Z")]
    pub first_failed_at: DateTime<Utc>,
    #[schema(example = "2025-09-01T08:05:00Z")]
    pub created_at: DateTime<Utc>,
    #[schema(example = false)]
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    /// User ID who resolved this message (null for automatic resolution).
    pub resolved_by: Option<i32>,
}

impl From<dead_letter_message::Model> for DlqMessageDetailResponse {
    fn from(m: dead_letter_message::Model) -> Self {
        Self {
            id: m.id,
            message_id: m.message_id,
            message_type: m.message_type,
            submission_id: m.submission_id,
            payload: m.payload,
            error_code: m.error_code,
            error_message: m.error_message,
            retry_count: m.retry_count,
            retry_history: m.retry_history,
            first_failed_at: m.first_failed_at,
            created_at: m.created_at,
            resolved: m.resolved,
            resolved_at: m.resolved_at,
            resolved_by: m.resolved_by,
        }
    }
}

/// Paginated list of DLQ messages.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DlqListResponse {
    pub data: Vec<DlqMessageResponse>,
    pub pagination: Pagination,
}

/// Unresolved message counts by message type.
#[derive(Serialize, utoipa::ToSchema)]
pub struct MessageTypeCounts {
    /// Number of unresolved judge_job messages.
    #[schema(example = 3)]
    pub judge_job: u64,
    /// Number of unresolved judge_result messages.
    #[schema(example = 2)]
    pub judge_result: u64,
}

/// DLQ statistics.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DlqStatsResponse {
    /// Total unresolved (active) messages.
    #[schema(example = 5)]
    pub total_unresolved: u64,
    /// Total resolved messages.
    #[schema(example = 42)]
    pub total_resolved: u64,
    /// Unresolved count by message type.
    pub unresolved_by_message_type: MessageTypeCounts,
    /// Unresolved count by error code.
    pub unresolved_by_error_code: HashMap<String, u64>,
}

impl From<DlqStats> for DlqStatsResponse {
    fn from(s: DlqStats) -> Self {
        Self {
            total_unresolved: s.total_unresolved,
            total_resolved: s.total_resolved,
            unresolved_by_message_type: MessageTypeCounts {
                judge_job: s.judge_job_count,
                judge_result: s.judge_result_count,
            },
            unresolved_by_error_code: s.unresolved_by_error_code,
        }
    }
}

/// Response for retry action.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DlqRetryResponse {
    /// Status message.
    #[schema(example = "Message requeued for processing")]
    pub message: String,
}
