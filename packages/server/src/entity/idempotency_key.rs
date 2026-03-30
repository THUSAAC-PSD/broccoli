use sea_orm::entity::prelude::*;

/// Stores idempotency keys for HTTP POST request deduplication.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "idempotency_key")]
pub struct Model {
    /// Client-provided idempotency key (1-255 chars, alphanumeric + hyphens).
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,

    /// User who owns this key (from JWT).
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i32,

    /// Request path this key was first used with (for mismatch validation).
    pub request_path: String,

    /// HTTP method (always POST for idempotent requests).
    pub request_method: String,

    /// "pending" while the handler is running, "completed" after success.
    pub status: String,

    /// HTTP status code of the cached response (only set when completed).
    pub response_status: Option<i16>,

    /// Raw JSON body of the cached response (only set for 2xx).
    #[sea_orm(column_type = "Text", nullable)]
    pub response_body: Option<String>,

    /// When this key was first claimed.
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,

    /// When the handler completed (None while pending).
    pub completed_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
