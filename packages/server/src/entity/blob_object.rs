use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "blob_object")]
pub struct Model {
    /// SHA-256 content hash.
    #[sea_orm(primary_key, auto_increment = false)]
    pub content_hash: String,

    /// Size of the blob in bytes.
    pub size: i64,

    pub created_at: DateTimeUtc,

    #[sea_orm(has_many)]
    pub blob_refs: HasMany<super::blob_ref::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
