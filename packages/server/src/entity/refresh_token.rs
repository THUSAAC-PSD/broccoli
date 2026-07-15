use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "refresh_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub selector: String,
    pub validator: String,

    pub user_id: i32,

    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,

    /// When this token was rotated out and superseded. `None` for a live token.
    /// A rotated token is retained (with this set) rather than deleted so that a
    /// replay of an already-rotated token can be detected as reuse (a stolen
    /// refresh chain) instead of being indistinguishable from a random invalid
    /// selector. Rows are cleaned up once expired.
    #[sea_orm(nullable)]
    pub revoked_at: Option<DateTimeUtc>,

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
