use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "clarification")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub contest_id: i32,
    /// User who created this clarification (question asker / announcement poster).
    pub author_id: i32,
    /// The clarification body.
    #[sea_orm(column_type = "Text")]
    pub content: String,

    /// One of: "announcement", "question", "direct_message".
    pub clarification_type: String,
    /// Target user for direct messages; NULL for announcements and questions.
    pub recipient_id: Option<i32>,
    /// Whether this clarification is visible to all participants.
    #[sea_orm(default_value = false)]
    pub is_public: bool,

    /// Admin reply content (for questions / direct messages).
    #[sea_orm(column_type = "Text", nullable)]
    pub reply_content: Option<String>,
    /// Admin who replied.
    pub reply_author_id: Option<i32>,
    /// Whether the reply is broadcast to all participants.
    #[sea_orm(default_value = false)]
    pub reply_is_public: bool,
    pub replied_at: Option<DateTimeUtc>,

    #[sea_orm(belongs_to, from = "contest_id", to = "id")]
    pub contest: HasOne<super::contest::Entity>,
    #[sea_orm(belongs_to, from = "author_id", to = "id")]
    pub author: HasOne<super::user::Entity>,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
