use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::*;
use tracing::instrument;

use crate::entity::{clarification, user};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::clarification::*;
use crate::state::AppState;
use crate::utils::contest::{check_contest_access, find_contest};

// ---------------------------------------------------------------------------
// List clarifications
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/",
    tag = "Clarifications",
    operation_id = "listClarifications",
    summary = "List clarifications for a contest",
    description = "Returns clarifications visible to the current user. Admins see all; \
                    contestants see own questions, public announcements, public replies, \
                    and direct messages addressed to them.",
    params(ClarificationListQuery),
    responses(
        (status = 200, description = "List of clarifications", body = ClarificationListResponse),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 404, description = "Contest not found", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, query))]
pub async fn list_clarifications(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    Query(query): Query<ClarificationListQuery>,
) -> Result<Json<ClarificationListResponse>, AppError> {
    let contest = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest).await?;

    let is_admin = auth_user.has_permission("contest:manage");

    let mut select =
        clarification::Entity::find().filter(clarification::Column::ContestId.eq(contest_id));

    // Optional type filter
    if let Some(ref type_filter) = query.r#type {
        select = select.filter(clarification::Column::ClarificationType.eq(type_filter.as_str()));
    }

    // Visibility filter for non-admins
    if !is_admin {
        // Contestants can see:
        // 1. Their own questions
        // 2. Public clarifications (announcements, or anything with is_public=true)
        // 3. Questions with a public reply (reply_is_public = true)
        // 4. Direct messages addressed to them
        select = select.filter(
            Condition::any()
                .add(clarification::Column::AuthorId.eq(auth_user.user_id))
                .add(clarification::Column::IsPublic.eq(true))
                .add(clarification::Column::ReplyIsPublic.eq(true))
                .add(clarification::Column::RecipientId.eq(auth_user.user_id)),
        );
    }

    select = select.order_by_desc(clarification::Column::CreatedAt);

    let rows = select.all(&state.db).await?;

    // Collect unique user IDs to resolve names
    let mut user_ids = std::collections::HashSet::new();
    for r in &rows {
        user_ids.insert(r.author_id);
        if let Some(rid) = r.recipient_id {
            user_ids.insert(rid);
        }
        if let Some(raid) = r.reply_author_id {
            user_ids.insert(raid);
        }
    }

    let users: Vec<user::Model> = if user_ids.is_empty() {
        vec![]
    } else {
        user::Entity::find()
            .filter(user::Column::Id.is_in(user_ids))
            .all(&state.db)
            .await?
    };
    let user_map: std::collections::HashMap<i32, String> =
        users.into_iter().map(|u| (u.id, u.username)).collect();

    let data = rows
        .into_iter()
        .map(|r| {
            let author_name = user_map
                .get(&r.author_id)
                .cloned()
                .unwrap_or_else(|| "[Deleted]".into());
            let recipient_name = r.recipient_id.and_then(|rid| user_map.get(&rid).cloned());
            let reply_author_name = r
                .reply_author_id
                .and_then(|raid| user_map.get(&raid).cloned());

            ClarificationResponse {
                id: r.id,
                contest_id: r.contest_id,
                author_id: r.author_id,
                author_name,
                content: r.content,
                clarification_type: r.clarification_type,
                recipient_id: r.recipient_id,
                recipient_name,
                is_public: r.is_public,
                reply_content: r.reply_content,
                reply_author_id: r.reply_author_id,
                reply_author_name,
                reply_is_public: r.reply_is_public,
                replied_at: r.replied_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect();

    Ok(Json(ClarificationListResponse { data }))
}

// ---------------------------------------------------------------------------
// Create clarification
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/",
    tag = "Clarifications",
    operation_id = "createClarification",
    summary = "Create a clarification",
    description = "Contestants can create questions. Admins can also create announcements \
                    and direct messages to specific participants.",
    request_body = CreateClarificationRequest,
    responses(
        (status = 201, description = "Clarification created", body = ClarificationResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Contest not found", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn create_clarification(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(contest_id): Path<i32>,
    AppJson(payload): AppJson<CreateClarificationRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_create_clarification(&payload)?;

    let contest = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest).await?;

    let is_admin = auth_user.has_permission("contest:manage");

    // Non-admins can only create questions
    if !is_admin && payload.clarification_type != "question" {
        return Err(AppError::PermissionDenied);
    }

    // For direct messages, validate the recipient exists
    if let Some(recipient_id) = payload.recipient_id {
        user::Entity::find_by_id(recipient_id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Recipient user not found".into()))?;
    }

    let is_public = if payload.clarification_type == "announcement" {
        true
    } else {
        payload.is_public.unwrap_or(false)
    };

    let now = chrono::Utc::now();
    let new = clarification::ActiveModel {
        contest_id: Set(contest_id),
        author_id: Set(auth_user.user_id),
        content: Set(payload.content.trim().to_string()),
        clarification_type: Set(payload.clarification_type.clone()),
        recipient_id: Set(payload.recipient_id),
        is_public: Set(is_public),
        reply_is_public: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let model = new.insert(&state.db).await?;

    let author_name = auth_user.username.clone();
    let recipient_name = if let Some(rid) = model.recipient_id {
        user::Entity::find_by_id(rid)
            .one(&state.db)
            .await?
            .map(|u| u.username)
    } else {
        None
    };

    let resp = ClarificationResponse {
        id: model.id,
        contest_id: model.contest_id,
        author_id: model.author_id,
        author_name,
        content: model.content,
        clarification_type: model.clarification_type,
        recipient_id: model.recipient_id,
        recipient_name,
        is_public: model.is_public,
        reply_content: model.reply_content,
        reply_author_id: model.reply_author_id,
        reply_author_name: None,
        reply_is_public: model.reply_is_public,
        replied_at: model.replied_at,
        created_at: model.created_at,
        updated_at: model.updated_at,
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

// ---------------------------------------------------------------------------
// Reply to clarification
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/{clarification_id}/reply",
    tag = "Clarifications",
    operation_id = "replyClarification",
    summary = "Reply to a clarification",
    description = "Admin replies to a question or direct message. \
                    When `is_public` is true the reply becomes visible to all participants.",
    request_body = ReplyClarificationRequest,
    responses(
        (status = 200, description = "Reply saved", body = ClarificationResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Clarification not found", body = ErrorBody),
        (status = 409, description = "Already replied", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn reply_clarification(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, clarification_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<ReplyClarificationRequest>,
) -> Result<Json<ClarificationResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;
    validate_reply_clarification(&payload)?;

    let existing = clarification::Entity::find_by_id(clarification_id)
        .filter(clarification::Column::ContestId.eq(contest_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Clarification not found".into()))?;

    if existing.reply_content.is_some() {
        return Err(AppError::Conflict(
            "This clarification has already been replied to".into(),
        ));
    }

    let now = chrono::Utc::now();
    let mut active: clarification::ActiveModel = existing.clone().into();
    active.reply_content = Set(Some(payload.content.trim().to_string()));
    active.reply_author_id = Set(Some(auth_user.user_id));
    active.reply_is_public = Set(payload.is_public);
    active.replied_at = Set(Some(now));
    active.updated_at = Set(now);

    let model = active.update(&state.db).await?;

    // Resolve user names
    let mut user_ids = vec![model.author_id];
    if let Some(rid) = model.recipient_id {
        user_ids.push(rid);
    }
    if let Some(raid) = model.reply_author_id {
        user_ids.push(raid);
    }
    let users: Vec<user::Model> = user::Entity::find()
        .filter(user::Column::Id.is_in(user_ids))
        .all(&state.db)
        .await?;
    let user_map: std::collections::HashMap<i32, String> =
        users.into_iter().map(|u| (u.id, u.username)).collect();

    let author_name = user_map
        .get(&model.author_id)
        .cloned()
        .unwrap_or_else(|| "[Deleted]".into());
    let recipient_name = model
        .recipient_id
        .and_then(|rid| user_map.get(&rid).cloned());
    let reply_author_name = model
        .reply_author_id
        .and_then(|raid| user_map.get(&raid).cloned());

    Ok(Json(ClarificationResponse {
        id: model.id,
        contest_id: model.contest_id,
        author_id: model.author_id,
        author_name,
        content: model.content,
        clarification_type: model.clarification_type,
        recipient_id: model.recipient_id,
        recipient_name,
        is_public: model.is_public,
        reply_content: model.reply_content,
        reply_author_id: model.reply_author_id,
        reply_author_name,
        reply_is_public: model.reply_is_public,
        replied_at: model.replied_at,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }))
}
