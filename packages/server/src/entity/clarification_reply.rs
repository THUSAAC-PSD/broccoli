use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "clarification_reply")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub clarification_id: i32,
    /// The user who wrote this reply.
    pub author_id: i32,
    /// Reply body.
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// Whether this reply is visible to all participants.
    #[sea_orm(default_value = false)]
    pub is_public: bool,

    #[sea_orm(belongs_to, from = "clarification_id", to = "id")]
    pub clarification: HasOne<super::clarification::Entity>,
    #[sea_orm(belongs_to, from = "author_id", to = "id")]
    pub author: HasOne<super::user::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
