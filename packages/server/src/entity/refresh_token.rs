use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "refresh_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub selector: String, // Unique identifier for the refresh token
    pub validator: String, // Hashed validator for secure comparison

    pub user_id: i32,

    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,

    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Entity {
    pub async fn revoke_all_for_user<C>(db: &C, user_id: i32) -> Result<(), sea_orm::DbErr>
    where
        C: ConnectionTrait,
    {
        Self::delete_many()
            .filter(Column::UserId.eq(user_id))
            .exec(db)
            .await?;
        Ok(())
    }
}
