use crate::entity::problem;
use crate::error::AppError;
use crate::utils::soft_delete::SoftDeletable;

/// Find a problem by ID or return 404.
pub async fn find_problem<C: sea_orm::ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<problem::Model, AppError> {
    problem::Entity::find_active_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))
}
