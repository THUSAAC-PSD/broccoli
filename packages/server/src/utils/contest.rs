use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use crate::entity::{contest, contest_problem, contest_user};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;

/// Verify the caller can access the given contest.
pub async fn check_contest_access<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
    contest: &contest::Model,
) -> Result<(), AppError> {
    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }
    if contest.is_public {
        return Ok(());
    }
    let is_participant = contest_user::Entity::find_by_id((contest.id, auth_user.user_id))
        .one(db)
        .await?
        .is_some();
    if is_participant {
        return Ok(());
    }
    Err(AppError::NotFound("Contest not found".into()))
}

/// Look up a contest by ID, returning 404 if not found.
pub async fn find_contest<C: sea_orm::ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<contest::Model, AppError> {
    contest::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))
}

/// Look up a contest-problem link, returning 404 if the problem is not in the contest.
pub async fn find_contest_problem<C: sea_orm::ConnectionTrait>(
    db: &C,
    contest_id: i32,
    problem_id: i32,
) -> Result<contest_problem::Model, AppError> {
    contest_problem::Entity::find_by_id((contest_id, problem_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest problem not found".into()))
}

/// Check if a user can access a problem through any contest, returning 404 otherwise.
pub async fn can_access_problem_via_contest<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: i32,
    problem_id: i32,
) -> Result<(), AppError> {
    let contest_ids: Vec<i32> = contest_problem::Entity::find()
        .filter(contest_problem::Column::ProblemId.eq(problem_id))
        .select_only()
        .column(contest_problem::Column::ContestId)
        .into_tuple()
        .all(db)
        .await?;

    if contest_ids.is_empty() {
        return Err(AppError::NotFound("Problem not found".into()));
    }

    let has_public = contest::Entity::find()
        .filter(contest::Column::Id.is_in(contest_ids.clone()))
        .filter(contest::Column::IsPublic.eq(true))
        .one(db)
        .await?
        .is_some();
    if has_public {
        return Ok(());
    }

    let is_participant = contest_user::Entity::find()
        .filter(contest_user::Column::ContestId.is_in(contest_ids))
        .filter(contest_user::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .is_some();
    if is_participant {
        return Ok(());
    }

    Err(AppError::NotFound("Problem not found".into()))
}
