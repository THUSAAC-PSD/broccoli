use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::prelude::Expr;
use sea_orm::sea_query::{Func, LikeExpr, Query as SeaQuery};
use sea_orm::*;
use tracing::instrument;

use crate::entity::{contest, contest_problem, contest_user, problem, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::contest::*;
use crate::models::shared::{Pagination, escape_like};
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/",
    tag = "Contests",
    operation_id = "createContest",
    summary = "Create a new contest",
    description = "Creates a new contest. Requires `contest:create` permission.",
    request_body = CreateContestRequest,
    responses(
        (status = 201, description = "Contest created", body = ContestResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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
        submissions_visible: Set(payload.submissions_visible.unwrap_or(false)),
        show_compile_output: Set(payload.show_compile_output.unwrap_or(true)),
        show_participants_list: Set(payload.show_participants_list.unwrap_or(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = new_contest.insert(&state.db).await?;

    Ok((StatusCode::CREATED, Json(ContestResponse::from(model))))
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Contests",
    operation_id = "listContests",
    summary = "List contests with pagination and search",
    description = "Returns a paginated list of contests with optional search and sorting. Users with `contest:manage` see all contests; others only see public contests and those they are enrolled in. Supports sorting by `created_at`, `updated_at`, `start_time`, or `title`.",
    params(ContestListQuery),
    responses(
        (status = 200, description = "List of contests", body = ContestListResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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
        .column(contest::Column::SubmissionsVisible)
        .column(contest::Column::ShowCompileOutput)
        .column(contest::Column::ShowParticipantsList)
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

#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Contests",
    operation_id = "getContest",
    summary = "Get a contest by ID",
    description = "Returns the full details of a contest. Users with `contest:manage` can view any contest; others can view public contests or those they are enrolled in. Returns 404 (not 403) for inaccessible contests to prevent enumeration.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 200, description = "Contest details", body = ContestResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    patch,
    path = "/{id}",
    tag = "Contests",
    operation_id = "updateContest",
    summary = "Update an existing contest",
    description = "Partially updates a contest using PATCH semantics. Requires `contest:manage` permission. An empty payload returns the current resource unchanged. Cross-field validation ensures end_time stays after start_time even when updating one of the two.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = UpdateContestRequest,
    responses(
        (status = 200, description = "Contest updated", body = ContestResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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
    if let Some(submissions_visible) = payload.submissions_visible {
        active.submissions_visible = Set(submissions_visible);
    }
    if let Some(show_compile_output) = payload.show_compile_output {
        active.show_compile_output = Set(show_compile_output);
    }
    if let Some(show_participants_list) = payload.show_participants_list {
        active.show_participants_list = Set(show_participants_list);
    }
    active.updated_at = Set(chrono::Utc::now());

    let model = active.update(&txn).await?;
    txn.commit().await?;

    Ok(Json(model.into()))
}

#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Contests",
    operation_id = "deleteContest",
    summary = "Delete a contest by ID",
    description = "Permanently deletes a contest and cascade-deletes its problem associations and participant records. Requires `contest:delete` permission.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 204, description = "Contest deleted"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/",
    tag = "Contest Problems",
    operation_id = "addContestProblem",
    summary = "Add a problem to a contest",
    description = "Associates an existing problem with the contest under a given label. Requires `contest:manage` permission. Labels must be unique within the contest. Position is auto-assigned if omitted. Returns 409 if the problem ID or label is already present.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = AddContestProblemRequest,
    responses(
        (status = 201, description = "Problem added to contest", body = ContestProblemResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Problem already in contest (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    get,
    path = "/",
    tag = "Contest Problems",
    operation_id = "listContestProblems",
    summary = "List problems in a contest",
    description = "Returns all problems in the contest, ordered by position. Same visibility rules as getContest apply.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 200, description = "List of contest problems", body = Vec<ContestProblemResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    patch,
    path = "/{problem_id}",
    tag = "Contest Problems",
    operation_id = "updateContestProblem",
    summary = "Update a contest problem's label or position",
    description = "Updates the label or position of a problem within a contest. Requires `contest:manage` permission. Returns 409 CONFLICT on duplicate labels.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
    ),
    request_body = UpdateContestProblemRequest,
    responses(
        (status = 200, description = "Contest problem updated", body = ContestProblemResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Duplicate label in contest (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    delete,
    path = "/{problem_id}",
    tag = "Contest Problems",
    operation_id = "removeContestProblem",
    summary = "Remove a problem from a contest",
    description = "Removes the association between a problem and the contest. Requires `contest:manage` permission. The problem itself is not deleted.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
    ),
    responses(
        (status = 204, description = "Problem removed from contest"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest problem not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    put,
    path = "/reorder",
    tag = "Contest Problems",
    operation_id = "reorderContestProblems",
    summary = "Reorder problems in a contest",
    description = "Replaces the ordering of all problems in a contest. Requires `contest:manage` permission. The ID array must contain exactly all problems currently in the contest. Positions are assigned by array index starting at 0.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = ReorderContestProblemsRequest,
    responses(
        (status = 204, description = "Contest problems reordered"),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/",
    tag = "Contest Participants",
    operation_id = "addParticipant",
    summary = "Add a participant to a contest",
    description = "Adds a user to the contest as a participant (admin action). Requires `contest:manage` permission. Returns 409 if the user is already a participant.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = AddParticipantRequest,
    responses(
        (status = 201, description = "Participant added", body = ContestParticipantResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or user not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "User already a participant (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    get,
    path = "/",
    tag = "Contest Participants",
    operation_id = "listParticipants",
    summary = "List participants of a contest",
    description = "Returns all participants in the contest, ordered by registration time. Requires `contest:manage` permission if `show_participants_list` is false.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 200, description = "List of participants", body = Vec<ContestParticipantResponse>),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden when show_participants_list is false (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(contest_id))]
pub async fn list_participants(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
) -> Result<Json<Vec<ContestParticipantResponse>>, AppError> {
    let contest_model = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest_model).await?;

    if !contest_model.show_participants_list && !auth_user.has_permission("contest:manage") {
        return Err(AppError::PermissionDenied);
    }

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

#[utoipa::path(
    delete,
    path = "/{user_id}",
    tag = "Contest Participants",
    operation_id = "removeParticipant",
    summary = "Remove a participant from a contest",
    description = "Removes a participant from the contest (admin action). Requires `contest:manage` permission.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("user_id" = i32, Path, description = "User ID"),
    ),
    responses(
        (status = 204, description = "Participant removed"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Participant not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/{id}/register",
    tag = "Contests",
    operation_id = "registerForContest",
    summary = "Self-register for a public contest",
    description = "Registers the authenticated user for a public contest. Non-public contests return 404 to prevent enumeration. Blocked after the contest ends. Returns 409 if already registered.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 201, description = "Registered for contest"),
        (status = 400, description = "Contest has ended (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Already registered (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

#[utoipa::path(
    delete,
    path = "/{id}/register",
    tag = "Contests",
    operation_id = "unregisterFromContest",
    summary = "Self-unregister from a contest",
    description = "Removes the authenticated user's registration from a contest. Returns 404 if the caller is not registered.",
    params(("id" = i32, Path, description = "Contest ID")),
    responses(
        (status = 204, description = "Unregistered from contest"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 404, description = "Not registered or contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
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

/// Bulk-delete problems from a contest.
#[utoipa::path(
    delete,
    path = "/bulk",
    tag = "Contest Problems",
    operation_id = "bulkDeleteContestProblems",
    summary = "Bulk-remove problems from a contest",
    description = "Removes multiple problems from a contest in a single operation. Requires `contest:manage` permission. All provided problem IDs must be currently in the contest.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = BulkDeleteContestProblemsRequest,
    responses(
        (status = 200, description = "Problems removed", body = BulkDeleteContestProblemsResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found or problem IDs not in contest (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(contest_id))]
pub async fn bulk_delete_contest_problems(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<BulkDeleteContestProblemsRequest>,
) -> Result<Json<BulkDeleteContestProblemsResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_bulk_delete_contest_problems(&payload)?;

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;

    let existing_ids: Vec<i32> = contest_problem::Entity::find()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .filter(contest_problem::Column::ProblemId.is_in(payload.problem_ids.clone()))
        .select_only()
        .column(contest_problem::Column::ProblemId)
        .into_tuple::<i32>()
        .all(&txn)
        .await?;

    let existing_set: std::collections::HashSet<i32> = existing_ids.into_iter().collect();
    let missing: Vec<i32> = payload
        .problem_ids
        .iter()
        .filter(|id| !existing_set.contains(id))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(AppError::NotFound(format!(
            "Problem IDs not found in contest {contest_id}: {missing:?}"
        )));
    }

    let result = contest_problem::Entity::delete_many()
        .filter(contest_problem::Column::ContestId.eq(contest_id))
        .filter(contest_problem::Column::ProblemId.is_in(payload.problem_ids))
        .exec(&txn)
        .await?;

    txn.commit().await?;

    tracing::info!(
        contest_id,
        removed = result.rows_affected,
        user_id = auth_user.user_id,
        "Bulk removed contest problems"
    );

    Ok(Json(BulkDeleteContestProblemsResponse {
        removed: result.rows_affected as usize,
    }))
}

/// Bulk-add participants to a contest, with optional account creation.
#[utoipa::path(
    post,
    path = "/bulk",
    tag = "Contest Participants",
    operation_id = "bulkAddParticipants",
    summary = "Bulk-add participants to a contest",
    description = "Enrolls multiple users in a contest. Existing users are looked up by username; new users can be created with auto-generated or custom passwords. Requires `contest:manage` permission. Partial success model: missing usernames are reported in `not_found`, already-enrolled users in `already_enrolled`.",
    params(("id" = i32, Path, description = "Contest ID")),
    request_body = BulkAddParticipantsRequest,
    responses(
        (status = 200, description = "Participants added", body = BulkAddParticipantsResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload), fields(contest_id))]
pub async fn bulk_add_participants(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<BulkAddParticipantsRequest>,
) -> Result<Json<BulkAddParticipantsResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_bulk_add_participants(&payload)?;

    let mut hashed_entries: Vec<(String, String, String)> = Vec::new(); // (username, plaintext, hash)
    if !payload.create_users.is_empty() {
        let entries: Vec<(String, String)> = payload
            .create_users
            .iter()
            .map(|e| {
                let username = e.username.trim().to_string();
                let plaintext = e
                    .password
                    .clone()
                    .unwrap_or_else(|| crate::utils::password::generate_password(12));
                (username, plaintext)
            })
            .collect();

        hashed_entries = tokio::task::spawn_blocking(move || {
            entries
                .into_iter()
                .map(|(username, plaintext)| {
                    let hash = crate::utils::hash::hash_password(&plaintext)
                        .map_err(|e| format!("Password hash error for '{username}': {e}"))?;
                    Ok((username, plaintext, hash))
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .await
        .map_err(|e| AppError::Internal(format!("Password hashing task failed: {e}")))?
        .map_err(AppError::Internal)?;
    }

    let txn = state.db.begin().await?;
    find_contest_for_update(&txn, contest_id).await?;

    let mut added = Vec::new();
    let mut created_response = Vec::new();
    let mut already_enrolled = Vec::new();
    let mut not_found = Vec::new();

    let mut users_to_enroll: Vec<(i32, String)> = Vec::new();

    for (username, plaintext, hash) in hashed_entries {
        let new_user = user::ActiveModel {
            username: Set(username.clone()),
            password: Set(hash),
            role: Set(crate::entity::role::DEFAULT_ROLE.to_string()),
            created_at: Set(chrono::Utc::now()),
            ..Default::default()
        };

        match new_user.insert(&txn).await {
            Ok(m) => {
                created_response.push(BulkParticipantCreated {
                    user_id: m.id,
                    username: username.clone(),
                    password: plaintext,
                });
                users_to_enroll.push((m.id, username));
            }
            Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                let existing = user::Entity::find()
                    .filter(user::Column::Username.eq(&username))
                    .one(&txn)
                    .await?
                    .ok_or_else(|| {
                        AppError::Internal(format!(
                            "User '{username}' caused UniqueConstraintViolation but not found"
                        ))
                    })?;
                users_to_enroll.push((existing.id, username));
            }
            Err(e) => return Err(e.into()),
        }
    }

    if !payload.usernames.is_empty() {
        let trimmed_usernames: Vec<String> = payload
            .usernames
            .iter()
            .map(|u| u.trim().to_string())
            .collect();

        let found_users: Vec<user::Model> = user::Entity::find()
            .filter(user::Column::Username.is_in(trimmed_usernames.clone()))
            .all(&txn)
            .await?;

        let found_map: std::collections::HashMap<String, i32> = found_users
            .iter()
            .map(|u| (u.username.clone(), u.id))
            .collect();

        for name in &trimmed_usernames {
            if let Some(&uid) = found_map.get(name) {
                users_to_enroll.push((uid, name.clone()));
            } else {
                not_found.push(name.clone());
            }
        }
    }

    let user_ids_to_check: Vec<i32> = users_to_enroll.iter().map(|(id, _)| *id).collect();
    let already_enrolled_ids: std::collections::HashSet<i32> = if !user_ids_to_check.is_empty() {
        contest_user::Entity::find()
            .filter(contest_user::Column::ContestId.eq(contest_id))
            .filter(contest_user::Column::UserId.is_in(user_ids_to_check))
            .select_only()
            .column(contest_user::Column::UserId)
            .into_tuple::<i32>()
            .all(&txn)
            .await?
            .into_iter()
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    let now = chrono::Utc::now();
    let created_user_ids: std::collections::HashSet<i32> =
        created_response.iter().map(|c| c.user_id).collect();

    for (uid, name) in users_to_enroll {
        if already_enrolled_ids.contains(&uid) {
            if !created_user_ids.contains(&uid) {
                already_enrolled.push(BulkParticipantAdded {
                    user_id: uid,
                    username: name,
                });
            }
        } else {
            let new_cu = contest_user::ActiveModel {
                contest_id: Set(contest_id),
                user_id: Set(uid),
                registered_at: Set(now),
            };
            match new_cu.insert(&txn).await {
                Ok(_) => {
                    if !created_user_ids.contains(&uid) {
                        added.push(BulkParticipantAdded {
                            user_id: uid,
                            username: name,
                        });
                    }
                }
                Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                    if !created_user_ids.contains(&uid) {
                        already_enrolled.push(BulkParticipantAdded {
                            user_id: uid,
                            username: name,
                        });
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    txn.commit().await?;

    tracing::info!(
        contest_id,
        added = added.len(),
        created = created_response.len(),
        already_enrolled = already_enrolled.len(),
        not_found = not_found.len(),
        user_id = auth_user.user_id,
        "Bulk added participants"
    );

    Ok(Json(BulkAddParticipantsResponse {
        added,
        created: created_response,
        already_enrolled,
        not_found,
    }))
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
