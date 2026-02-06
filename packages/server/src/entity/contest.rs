use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "contest")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub description: String, // in Markdown
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

    #[sea_orm(has_many, via = "contest_user")]
    pub users: HasMany<super::user::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub problems: HasMany<super::problem::Entity>,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
