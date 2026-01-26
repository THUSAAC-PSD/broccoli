use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "problem")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    pub content: String,   // in Markdown
    pub time_limit: i32,   // in milliseconds
    pub memory_limit: i32, // in kilobytes

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    #[sea_orm(has_many, via = "contest_problem")]
    pub contests: HasMany<super::contest::Entity>,

    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
