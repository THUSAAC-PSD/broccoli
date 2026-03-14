use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::utils::soft_delete::SoftDeletable;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "contest")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub description: String, // in Markdown

    pub activate_time: Option<DateTimeUtc>,
    pub deactivate_time: Option<DateTimeUtc>,
    pub start_time: DateTimeUtc,
    pub end_time: DateTimeUtc,

    #[sea_orm(default_value = false)]
    pub is_public: bool,

    /// When true, all participants can see each other's submissions.
    /// When false, participants can only see their own submissions.
    #[sea_orm(default_value = false)]
    pub submissions_visible: bool,

    /// When true, participants can see compile output/errors for their submissions during contest.
    /// When false, compile output is hidden until contest ends.
    #[sea_orm(default_value = true)]
    pub show_compile_output: bool,

    /// When true, the participants list is visible to all who can view the contest.
    /// When false, only admins can see the participants list.
    #[sea_orm(default_value = true)]
    pub show_participants_list: bool,

    /// Contest type (e.g., "ioi", "icpc", "pvp"). Plugin dispatcher uses this to route submissions.
    pub contest_type: Option<String>,

    #[sea_orm(has_many, via = "contest_user")]
    pub users: HasMany<super::user::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub problems: HasMany<super::problem::Entity>,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}

impl SoftDeletable for Entity {
    type DeletedAtColumn = Column;
    fn deleted_at() -> Self::DeletedAtColumn {
        Column::DeletedAt
    }
}
