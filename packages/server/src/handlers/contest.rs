use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::prelude::Expr;
use sea_orm::sea_query::{Func, LikeExpr, Query as SeaQuery};
use sea_orm::*;
use tracing::instrument;

use crate::entity::{contest, contest_problem, contest_user, problem, user};
use crate::error::AppError;
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::contest::*;
use crate::models::shared::{Pagination, escape_like};
use crate::state::AppState;

#[instrument(skip(state, auth_user, payload), fields(title = %payload.title))]
pub async fn create_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<CreateContestRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:create")?;
    validate_create_contest(&payload)?;

    let now = chrono::Utc::now();
    let new_contest = contest::ActiveModel {
        title: Set(payload.title.trim().to_string()),
        description: Set(payload.description),
        start_time: Set(payload.start_time),
        end_time: Set(payload.end_time),
        is_public: Set(payload.is_public),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = new_contest.insert(&state.db).await?;

    Ok((StatusCode::CREATED, Json(ContestResponse::from(model))))
}

#[instrument(skip(state, auth_user, query))]
pub async fn list_contests(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ContestListQuery>,
) -> Result<Json<ContestListResponse>, AppError> {
    let page = Ord::max(query.page.unwrap_or(1), 1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let mut select = contest::Entity::find();

    if !auth_user.has_permission("contest:manage") {
        select = select.filter(
            Condition::any()
                .add(contest::Column::IsPublic.eq(true))
                .add(
                    contest::Column::Id.in_subquery(
                        SeaQuery::select()
                            .column(contest_user::Column::ContestId)
                            .from(contest_user::Entity)
                            .and_where(contest_user::Column::UserId.eq(auth_user.user_id))
                            .to_owned(),
                    ),
                ),
        );
    }

    if let Some(ref search) = query.search {
        let term = escape_like(search.trim());
        if !term.is_empty() {
            select = select.filter(
                Expr::expr(Func::lower(Expr::col(contest::Column::Title)))
                    .like(LikeExpr::new(format!("%{}%", term.to_lowercase())).escape('\\')),
            );
        }
    }

    let sort_by = query.sort_by.as_deref().unwrap_or("created_at");
    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        Order::Asc
    } else {
        Order::Desc
    };
    let sort_column = match sort_by {
        "created_at" => contest::Column::CreatedAt,
        "updated_at" => contest::Column::UpdatedAt,
        "start_time" => contest::Column::StartTime,
        "title" => contest::Column::Title,
        _ => {
            return Err(AppError::Validation(
                "sort_by must be one of: created_at, updated_at, start_time, title".into(),
            ));
        }
    };

    let total = select
        .clone()
        .paginate(&state.db, per_page)
        .num_items()
        .await?;

    select = select.order_by(sort_column, sort_order);
    let total_pages = total.div_ceil(per_page);

    let data = select
        .select_only()
        .column(contest::Column::Id)
        .column(contest::Column::Title)
        .column(contest::Column::StartTime)
        .column(contest::Column::EndTime)
        .column(contest::Column::IsPublic)
        .column(contest::Column::CreatedAt)
        .column(contest::Column::UpdatedAt)
        .offset(Some((page - 1) * per_page))
        .limit(Some(per_page))
        .into_model::<ContestListItem>()
        .all(&state.db)
        .await?;

    Ok(Json(ContestListResponse {
        data,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages,
        },
    }))
}

#[instrument(skip(state, auth_user), fields(id))]
pub async fn get_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ContestResponse>, AppError> {
    let model = find_contest(&state.db, id).await?;
    check_contest_access(&state.db, &auth_user, &model).await?;
    Ok(Json(model.into()))
}

#[instrument(skip(state, auth_user, payload), fields(id))]
pub async fn update_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    AppJson(payload): AppJson<UpdateContestRequest>,
) -> Result<Json<ContestResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_update_contest(&payload)?;

    if payload == UpdateContestRequest::default() {
        let existing = find_contest(&state.db, id).await?;
        return Ok(Json(existing.into()));
    }

    let txn = state.db.begin().await?;
    let existing = find_contest_for_update(&txn, id).await?;

    // Cross-field time validation against existing values
    let effective_start = payload.start_time.unwrap_or(existing.start_time);
    let effective_end = payload.end_time.unwrap_or(existing.end_time);
    if effective_end <= effective_start {
        return Err(AppError::Validation(
            "end_time must be after start_time".into(),
        ));
    }

    let mut active: contest::ActiveModel = existing.into();

    if let Some(ref title) = payload.title {
        active.title = Set(title.trim().to_string());
    }
    if let Some(description) = payload.description {
        active.description = Set(description);
    }
    if let Some(start_time) = payload.start_time {
        active.start_time = Set(start_time);
    }
    if let Some(end_time) = payload.end_time {
        active.end_time = Set(end_time);
    }
    if let Some(is_public) = payload.is_public {
        active.is_public = Set(is_public);
    }
    active.updated_at = Set(chrono::Utc::now());

    let model = active.update(&txn).await?;
    txn.commit().await?;

    Ok(Json(model.into()))
}

#[instrument(skip(state, auth_user), fields(id))]
pub async fn delete_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:delete")?;

    let txn = state.db.begin().await?;
    let _contest = find_contest_for_update(&txn, id).await?;

    // TODO: check for contest-scoped submissions before deleting.

    contest_problem::Entity::delete_many()
        .filter(contest_problem::Column::ContestId.eq(id))
        .exec(&txn)
        .await?;
    contest_user::Entity::delete_many()
        .filter(contest_user::Column::ContestId.eq(id))
        .exec(&txn)
        .await?;
    contest::Entity::delete_by_id(id).exec(&txn).await?;

    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user, payload), fields(contest_id))]
pub async fn add_contest_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<AddContestProblemRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_add_contest_problem(&payload)?;

    let txn = state.db.begin().await?;
    let _contest = find_contest_for_update(&txn, contest_id).await?;

    let problem_model = problem::Entity::find_by_id(payload.problem_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Problem not found".into()))?;
    let problem_title = problem_model.title;

    if contest_problem::Entity::find_by_id((contest_id, payload.problem_id))
        .one(&txn)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(
            "Problem is already in this contest".into(),
        ));
    }

    let label = payload.label.trim().to_string();
    let existing_label = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .filter(contest_problem::Column::Label.eq(&label))
        .one(&txn)
        .await?;
    if existing_label.is_some() {
        return Err(AppError::Conflict(format!(
            "Label '{label}' is already used in this contest"
        )));
    }

    let position = match payload.position {
        Some(p) => p,
        None => next_problem_position(&txn, contest_id).await?,
    };

    let new_cp = contest_problem::ActiveModel {
        contest_id: Set(contest_id),
        problem_id: Set(payload.problem_id),
        label: Set(label),
        position: Set(position),
    };

    let model = new_cp.insert(&txn).await?;
    txn.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(contest_problem_response(model, problem_title)),
    ))
}

#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn list_contest_problems(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<Json<Vec<ContestProblemResponse>>, AppError> {
    let contest_model = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest_model).await?;

    let rows = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .find_also_related(problem::Entity)
        .order_by_asc(contest_problem::Column::Position)
        .all(&state.db)
        .await?;

    let items = rows
        .into_iter()
        .map(|(cp, prob)| contest_problem_response(cp, prob.map(|p| p.title).unwrap_or_default()))
        .collect();

    Ok(Json(items))
}

#[instrument(skip(state, auth_user, payload), fields(contest_id, problem_id))]
pub async fn update_contest_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<UpdateContestProblemRequest>,
) -> Result<Json<ContestProblemResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_update_contest_problem(&payload)?;

    if payload == UpdateContestProblemRequest::default() {
        let cp = find_contest_problem(&state.db, contest_id, problem_id).await?;
        let title = problem::Entity::find_by_id(problem_id)
            .one(&state.db)
            .await?
            .map(|p| p.title)
            .unwrap_or_default();
        return Ok(Json(contest_problem_response(cp, title)));
    }

    let txn = state.db.begin().await?;
    let _contest = find_contest_for_update(&txn, contest_id).await?;
    let existing = find_contest_problem(&txn, contest_id, problem_id).await?;

    if let Some(ref new_label) = payload.label {
        let label = new_label.trim();
        if label != existing.label {
            let dup = contest_problem::Entity::find()
                .filter(contest_problem::Column::ContestId.eq(contest_id))
                .filter(contest_problem::Column::Label.eq(label))
                .one(&txn)
                .await?;
            if dup.is_some() {
                return Err(AppError::Conflict(format!(
                    "Label '{label}' is already used in this contest"
                )));
            }
        }
    }

    let mut active: contest_problem::ActiveModel = existing.into();

    if let Some(ref label) = payload.label {
        active.label = Set(label.trim().to_string());
    }
    if let Some(position) = payload.position {
        active.position = Set(position);
    }

    let model = active.update(&txn).await?;
    let title = problem::Entity::find_by_id(model.problem_id)
        .one(&txn)
        .await?
        .map(|p| p.title)
        .unwrap_or_default();
    txn.commit().await?;

    Ok(Json(contest_problem_response(model, title)))
}

#[instrument(skip(state, auth_user), fields(contest_id, problem_id))]
pub async fn remove_contest_problem(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, problem_id)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;
    let cp = find_contest_problem(&txn, contest_id, problem_id).await?;
    let active: contest_problem::ActiveModel = cp.into();
    active.delete(&txn).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user, payload), fields(contest_id))]
pub async fn reorder_contest_problems(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<ReorderContestProblemsRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_reorder_contest_problems(&payload)?;

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;

    let existing: Vec<i32> = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .select_only()
        .column(contest_problem::Column::ProblemId)
        .into_tuple::<i32>()
        .all(&txn)
        .await?;

    let existing_set: std::collections::HashSet<i32> = existing.into_iter().collect();
    let payload_set: std::collections::HashSet<i32> = payload.problem_ids.iter().copied().collect();
    if existing_set != payload_set {
        return Err(AppError::Validation(
            "problem_ids must contain exactly the problems currently in the contest".into(),
        ));
    }

    for (i, &problem_id) in payload.problem_ids.iter().enumerate() {
        contest_problem::Entity::update_many()
            .filter(contest_problem::Column::ContestId.eq(contest_id))
            .filter(contest_problem::Column::ProblemId.eq(problem_id))
            .col_expr(
                contest_problem::Column::Position,
                Expr::value(
                    i32::try_from(i)
                        .map_err(|_| AppError::Validation("Too many problems to reorder".into()))?,
                ),
            )
            .exec(&txn)
            .await?;
    }

    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user, payload), fields(contest_id))]
pub async fn add_participant(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<AddParticipantRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;

    let target_user = user::Entity::find_by_id(payload.user_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let now = chrono::Utc::now();
    let new_cu = contest_user::ActiveModel {
        contest_id: Set(contest_id),
        user_id: Set(payload.user_id),
        registered_at: Set(now),
    };

    match new_cu.insert(&txn).await {
        Ok(model) => {
            txn.commit().await?;
            Ok((
                StatusCode::CREATED,
                Json(ContestParticipantResponse {
                    contest_id: model.contest_id,
                    user_id: model.user_id,
                    username: target_user.username,
                    registered_at: model.registered_at,
                }),
            ))
        }
        Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            Err(AppError::Conflict("Already a participant".into()))
        }
        Err(e) => Err(e.into()),
    }
}

#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn list_participants(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<Json<Vec<ContestParticipantResponse>>, AppError> {
    let contest_model = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest_model).await?;

    let rows = contest_user::Entity::find()
        .filter(contest_user::Column::ContestId.eq(contest_id))
        .find_also_related(user::Entity)
        .order_by_asc(contest_user::Column::RegisteredAt)
        .all(&state.db)
        .await?;

    let items = rows
        .into_iter()
        .map(|(cu, usr)| ContestParticipantResponse {
            contest_id: cu.contest_id,
            user_id: cu.user_id,
            username: usr.map(|u| u.username).unwrap_or_default(),
            registered_at: cu.registered_at,
        })
        .collect();

    Ok(Json(items))
}

#[instrument(skip(state, auth_user), fields(contest_id, user_id))]
pub async fn remove_participant(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, user_id)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("contest:manage")?;

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;
    let cu = contest_user::Entity::find_by_id((contest_id, user_id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Participant not found".into()))?;

    let active: contest_user::ActiveModel = cu.into();
    active.delete(&txn).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn register_for_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let now = chrono::Utc::now();
    let txn = state.db.begin().await?;
    let contest_model = find_contest_for_update(&txn, contest_id).await?;

    if !contest_model.is_public {
        return Err(AppError::NotFound("Contest not found".into())); // Prevent enumeration
    }

    if now >= contest_model.end_time {
        return Err(AppError::Validation("Contest has ended".into()));
    }
    let new_cu = contest_user::ActiveModel {
        contest_id: Set(contest_id),
        user_id: Set(auth_user.user_id),
        registered_at: Set(now),
    };

    match new_cu.insert(&txn).await {
        Ok(_) => {
            txn.commit().await?;
            Ok(StatusCode::CREATED)
        }
        Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            Err(AppError::Conflict("Already registered".into()))
        }
        Err(e) => Err(e.into()),
    }
}

#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn unregister_from_contest(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;
    let cu = contest_user::Entity::find_by_id((contest_id, auth_user.user_id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Not registered for this contest".into()))?;

    let active: contest_user::ActiveModel = cu.into();
    active.delete(&txn).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn check_contest_access<C: ConnectionTrait>(
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

async fn find_contest<C: ConnectionTrait>(db: &C, id: i32) -> Result<contest::Model, AppError> {
    contest::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))
}

async fn find_contest_for_update(
    txn: &DatabaseTransaction,
    id: i32,
) -> Result<contest::Model, AppError> {
    use sea_orm::sea_query::LockType;
    contest::Entity::find_by_id(id)
        .lock(LockType::Update)
        .one(txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest not found".into()))
}

async fn find_contest_problem<C: ConnectionTrait>(
    db: &C,
    contest_id: i32,
    problem_id: i32,
) -> Result<contest_problem::Model, AppError> {
    contest_problem::Entity::find_by_id((contest_id, problem_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contest problem not found".into()))
}

fn contest_problem_response(
    cp: contest_problem::Model,
    problem_title: String,
) -> ContestProblemResponse {
    ContestProblemResponse {
        contest_id: cp.contest_id,
        problem_id: cp.problem_id,
        label: cp.label,
        position: cp.position,
        problem_title,
    }
}

async fn next_problem_position<C: ConnectionTrait>(
    db: &C,
    contest_id: i32,
) -> Result<i32, AppError> {
    let max_pos: Option<i32> = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .select_only()
        .column_as(contest_problem::Column::Position.max(), "max_pos")
        .into_tuple::<Option<i32>>()
        .one(db)
        .await?
        .flatten();
    max_pos
        .unwrap_or(-1)
        .checked_add(1)
        .ok_or_else(|| AppError::Validation("Position overflow".into()))
}
