use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};

use crate::entity::{contest, contest_problem, contest_user, problem};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::utils::soft_delete::SoftDeletable;

pub async fn is_problem_in_contest<C: sea_orm::ConnectionTrait>(
    db: &C,
    contest_id: i32,
    problem_id: i32,
) -> Result<bool, AppError> {
    let exists = contest_problem::Entity::find_by_id((contest_id, problem_id))
        .one(db)
        .await?
        .is_some();
    Ok(exists)
}

pub async fn check_contest_access<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
    contest: &contest::Model,
) -> Result<(), AppError> {
    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }
    let now = chrono::Utc::now();
    if contest.activate_time.is_none_or(|at| at > now)
        || contest.deactivate_time.is_some_and(|dt| dt <= now)
    {
        return Err(AppError::NotFound("Contest not found".into()));
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

pub async fn find_contest<C: sea_orm::ConnectionTrait>(
    db: &C,
    id: i32,
) -> Result<contest::Model, AppError> {
    contest::Entity::find_active_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))
}

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

pub fn require_contest_started(
    auth_user: &AuthUser,
    contest: &contest::Model,
) -> Result<(), AppError> {
    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }
    let now = chrono::Utc::now();
    if contest.activate_time.is_none_or(|at| at > now)
        || contest.deactivate_time.is_some_and(|dt| dt <= now)
    {
        return Err(AppError::NotFound("Contest not found".into()));
    }
    if now < contest.start_time {
        return Err(AppError::Validation("Contest has not started yet".into()));
    }
    Ok(())
}

pub fn require_contest_running(
    auth_user: &AuthUser,
    contest: &contest::Model,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }
    if contest.activate_time.is_none_or(|at| at > now)
        || contest.deactivate_time.is_some_and(|dt| dt <= now)
    {
        return Err(AppError::NotFound("Contest not found".into()));
    }
    if now < contest.start_time {
        return Err(AppError::Validation("Contest has not started yet".into()));
    }
    if now >= contest.end_time {
        return Err(AppError::Validation("Contest has already ended".into()));
    }
    Ok(())
}

pub async fn is_contest_participant<C: sea_orm::ConnectionTrait>(
    db: &C,
    contest_id: i32,
    user_id: i32,
) -> Result<bool, AppError> {
    let exists = contest_user::Entity::find()
        .filter(contest_user::Column::ContestId.eq(contest_id))
        .filter(contest_user::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .is_some();
    Ok(exists)
}

pub async fn require_contest_participant<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
    contest: &contest::Model,
) -> Result<(), AppError> {
    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }
    let is_participant = is_contest_participant(db, contest.id, auth_user.user_id).await?;
    if is_participant {
        return Ok(());
    }
    if contest.is_public {
        return Err(AppError::PermissionDenied);
    }
    Err(AppError::NotFound("Contest not found".into()))
}

pub async fn can_access_problem_via_contest<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
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

    if auth_user.has_permission("contest:manage") {
        return Ok(());
    }

    let now = chrono::Utc::now();

    let has_public = contest::Entity::find_active()
        .filter(contest::Column::Id.is_in(contest_ids.clone()))
        .filter(contest::Column::IsPublic.eq(true))
        .filter(contest::Column::StartTime.lte(now))
        .one(db)
        .await?
        .is_some();
    if has_public {
        return Ok(());
    }

    let started_contest_ids: Vec<i32> = contest::Entity::find_active()
        .filter(contest::Column::Id.is_in(contest_ids))
        .filter(contest::Column::StartTime.lte(now))
        .select_only()
        .column(contest::Column::Id)
        .into_tuple()
        .all(db)
        .await?;

    if !started_contest_ids.is_empty() {
        let is_participant = contest_user::Entity::find()
            .filter(contest_user::Column::ContestId.is_in(started_contest_ids))
            .filter(contest_user::Column::UserId.eq(auth_user.user_id))
            .one(db)
            .await?
            .is_some();
        if is_participant {
            return Ok(());
        }
    }

    Err(AppError::NotFound("Problem not found".into()))
}

pub async fn require_problem_read_access<C: sea_orm::ConnectionTrait>(
    db: &C,
    auth_user: &AuthUser,
    problem_id: i32,
) -> Result<(), AppError> {
    let problem = problem::Entity::find_active_by_id(problem_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;
    if auth_user.has_permission("problem:create") || auth_user.has_permission("problem:edit") {
        return Ok(());
    }
    if problem.is_public {
        // Published to the public problemset; readable by any authenticated
        // user even when the problem belongs to zero contests.
        return Ok(());
    }
    // Hidden draft: only reachable through a contest the user can access.
    // A problem in no accessible contest stays 404.
    can_access_problem_via_contest(db, auth_user, problem_id).await
}

#[cfg(test)]
mod problem_read_access_tests {
    use sea_orm::{DatabaseBackend, MockDatabase};

    use super::*;

    fn user(permissions: &[&str]) -> AuthUser {
        AuthUser {
            user_id: 42,
            username: "contestant".into(),
            roles: vec![],
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
        }
    }

    fn problem_row(is_public: bool) -> problem::Model {
        let now = chrono::Utc::now();
        problem::Model {
            id: 1,
            title: "Standalone".into(),
            content: "statement".into(),
            time_limit: 1000,
            memory_limit: 262_144,
            problem_type: "batch".into(),
            checker_format: "exact".into(),
            default_contest_type: "ioi".into(),
            show_test_details: false,
            is_public,
            submission_format: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    /// A non-public problem attached to zero contests must stay 404 for an
    /// unprivileged user (the pre-existing IDOR protection).
    #[tokio::test]
    async fn hidden_draft_in_zero_contests_is_not_found_for_unprivileged_user() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: fetch the problem (exists, but not public)
            .append_query_results([vec![problem_row(false)]])
            // 2: contest_problem lookup finds no contests
            .append_query_results([Vec::<contest_problem::Model>::new()])
            .into_connection();

        let err = require_problem_read_access(&db, &user(&[]), 1)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "hidden zero-contest problem must be NotFound, got {err:?}"
        );
    }

    /// An explicitly published (`is_public = true`) problem is readable by any
    /// authenticated user even when it belongs to zero contests. Only one
    /// query result is stubbed: reaching for the contest tables would error.
    #[tokio::test]
    async fn public_problem_in_zero_contests_is_readable_by_unprivileged_user() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![problem_row(true)]])
            .into_connection();

        require_problem_read_access(&db, &user(&[]), 1)
            .await
            .expect("published standalone problem must be readable");
    }

    /// Editors keep full access regardless of `is_public`.
    #[tokio::test]
    async fn editor_can_read_hidden_problem() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![problem_row(false)]])
            .into_connection();

        require_problem_read_access(&db, &user(&["problem:edit"]), 1)
            .await
            .expect("editor must read hidden problems");
    }

    /// A missing (or soft-deleted) problem is 404 even for editors — the gate
    /// fetches the row before any permission short-circuit.
    #[tokio::test]
    async fn missing_problem_is_not_found_even_for_editor() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([Vec::<problem::Model>::new()])
            .into_connection();

        let err = require_problem_read_access(&db, &user(&["problem:edit"]), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
