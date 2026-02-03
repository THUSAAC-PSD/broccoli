use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// The role assigned to newly registered users.
pub const DEFAULT_ROLE: &str = "contestant";

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "role")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub name: String,

    #[sea_orm(has_many)]
    pub users: HasMany<super::user::Entity>,

    #[sea_orm(has_many)]
    pub permissions: HasMany<super::role_permission::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
