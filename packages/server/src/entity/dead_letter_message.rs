use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Dead letter queue message for failed job processing.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "dead_letter_message")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    #[sea_orm(unique)]
    pub message_id: String,

    #[sea_orm(column_name = "direction", indexed)]
    pub message_type: String,

    #[sea_orm(indexed)]
    pub submission_id: Option<i32>,

    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,

    #[sea_orm(column_type = "Text")]
    pub error_message: String,

    /// Machine-readable error code (MAX_RETRIES_EXCEEDED, PROCESSING_ERROR, etc.).
    #[sea_orm(indexed)]
    pub error_code: String,

    pub retry_count: i32,

    /// Full retry history as JSON array: [{attempt, error, timestamp}]
    #[sea_orm(column_type = "JsonBinary")]
    pub retry_history: serde_json::Value,

    pub first_failed_at: DateTimeUtc,

    pub created_at: DateTimeUtc,

    #[sea_orm(default_value = false, indexed)]
    pub resolved: bool,

    pub resolved_at: Option<DateTimeUtc>,

    pub resolved_by: Option<i32>,
}

impl ActiveModelBehavior for ActiveModel {}
