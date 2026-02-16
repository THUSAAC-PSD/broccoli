use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blob_ref")]
pub struct Model {
    /// UUIDv7 primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    /// Owner entity type (e.g. "problem", "plugin").
    pub owner_type: String,

    /// Owner entity ID (canonical string form).
    pub owner_id: String,

    /// Normalized virtual path within the owner's namespace.
    pub path: String,

    pub content_hash: String,

    #[sea_orm(belongs_to, from = "content_hash", to = "content_hash")]
    pub blob_object: Option<super::blob_object::Entity>,

    /// Original upload filename.
    pub filename: String,

    /// MIME content type.
    pub content_type: Option<String>,

    /// Purposefully denormalized to avoid JOINs for list queries.
    pub size: i64,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
