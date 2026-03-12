use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::utils::soft_delete::SoftDeletable;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub username: String,
    pub password: String,
    #[sea_orm(default_value = "contestant")]
    pub role: String,
    #[sea_orm(belongs_to, from = "role", to = "name")]
    pub role_ref: Option<super::role::Entity>,

    #[sea_orm(has_many)]
    pub submissions: HasMany<super::submission::Entity>,

    #[sea_orm(has_many, via = "contest_user")]
    pub contests: HasMany<super::contest::Entity>,

    pub created_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}

impl SoftDeletable for Entity {
    type DeletedAtColumn = Column;
    fn deleted_at() -> Self::DeletedAtColumn {
        Column::DeletedAt
    }
}
