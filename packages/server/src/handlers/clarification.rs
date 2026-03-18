use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::*;
use tracing::instrument;

use crate::entity::{clarification, clarification_reply, user};
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
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ClarificationListQuery,
    ),
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

    // Visibility filter for non-admins:
    // Show threads where the user is the author, recipient, the thread is public,
    // or any reply in the thread has been made public.
    if !is_admin {
        select = select.filter(
            Condition::any()
                .add(clarification::Column::AuthorId.eq(auth_user.user_id))
                .add(clarification::Column::IsPublic.eq(true))
                .add(clarification::Column::ReplyIsPublic.eq(true))
                .add(clarification::Column::RecipientId.eq(auth_user.user_id))
                .add(
                    clarification::Column::Id.in_subquery(
                        sea_orm::sea_query::Query::select()
                            .column(clarification_reply::Column::ClarificationId)
                            .from(sea_orm::sea_query::Alias::new("clarification_reply"))
                            .and_where(
                                sea_orm::sea_query::Expr::col(
                                    clarification_reply::Column::IsPublic,
                                )
                                .eq(true),
                            )
                            .to_owned(),
                    ),
                ),
        );
    }

    select = select.order_by_desc(clarification::Column::CreatedAt);

    let rows = select.all(&state.db).await?;

    // Load all replies for these clarifications
    let clarification_ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
    let all_replies = if clarification_ids.is_empty() {
        vec![]
    } else {
        clarification_reply::Entity::find()
            .filter(clarification_reply::Column::ClarificationId.is_in(clarification_ids))
            .order_by_asc(clarification_reply::Column::CreatedAt)
            .all(&state.db)
            .await?
    };

    // Group replies by clarification_id
    let mut replies_map: std::collections::HashMap<i32, Vec<clarification_reply::Model>> =
        std::collections::HashMap::new();
    for reply in all_replies {
        replies_map
            .entry(reply.clarification_id)
            .or_default()
            .push(reply);
    }

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
        if let Some(rb) = r.resolved_by {
            user_ids.insert(rb);
        }
    }
    for replies in replies_map.values() {
        for reply in replies {
            user_ids.insert(reply.author_id);
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

            let replies = replies_map
                .get(&r.id)
                .map(|reps| {
                    reps.iter()
                        .filter(|rep| {
                            // Admins see all; others see public replies, replies they
                            // authored, or replies on their own question / DM thread.
                            is_admin
                                || rep.is_public
                                || rep.author_id == auth_user.user_id
                                || r.author_id == auth_user.user_id
                                || r.recipient_id == Some(auth_user.user_id)
                        })
                        .map(|rep| ClarificationReplyResponse {
                            id: rep.id,
                            author_id: rep.author_id,
                            author_name: user_map
                                .get(&rep.author_id)
                                .cloned()
                                .unwrap_or_else(|| "[Deleted]".into()),
                            content: rep.content.clone(),
                            is_public: rep.is_public,
                            created_at: rep.created_at,
                        })
                        .collect()
                })
                .unwrap_or_default();

            let resolved_by_name = r.resolved_by.and_then(|uid| user_map.get(&uid).cloned());

            // If the user is not a participant and the question itself is not
            // public, redact the question content (they're seeing this thread
            // only because a reply was published).
            let is_participant = is_admin
                || r.author_id == auth_user.user_id
                || r.recipient_id == Some(auth_user.user_id);
            let show_question = is_participant || r.is_public;

            ClarificationResponse {
                id: r.id,
                contest_id: r.contest_id,
                author_id: r.author_id,
                author_name: if show_question {
                    author_name
                } else {
                    "Anonymous".into()
                },
                content: if show_question {
                    r.content
                } else {
                    String::new()
                },
                clarification_type: r.clarification_type,
                recipient_id: r.recipient_id,
                recipient_name,
                is_public: r.is_public,
                reply_content: r.reply_content,
                reply_author_id: r.reply_author_id,
                reply_author_name,
                reply_is_public: r.reply_is_public,
                replied_at: r.replied_at,
                replies,
                resolved: r.resolved,
                resolved_at: r.resolved_at,
                resolved_by: r.resolved_by,
                resolved_by_name,
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
    params(("id" = i32, Path, description = "Contest ID")),
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
        replies: vec![],
        resolved: model.resolved,
        resolved_at: model.resolved_at,
        resolved_by: model.resolved_by,
        resolved_by_name: None,
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
    description = "Admin replies to a question or direct message. Multiple replies are allowed. \
                    When `is_public` is true the reply becomes visible to all participants.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("clarification_id" = i32, Path, description = "Clarification ID"),
    ),
    request_body = ReplyClarificationRequest,
    responses(
        (status = 200, description = "Reply saved", body = ClarificationResponse),
        (status = 400, description = "Validation error", body = ErrorBody),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Clarification not found", body = ErrorBody),
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
    validate_reply_clarification(&payload)?;

    let existing = clarification::Entity::find_by_id(clarification_id)
        .filter(clarification::Column::ContestId.eq(contest_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Clarification not found".into()))?;

    let is_admin = auth_user.has_permission("contest:manage");
    let is_author = existing.author_id == auth_user.user_id;
    let is_recipient = existing.recipient_id == Some(auth_user.user_id);

    // Only admins, the question author, or the DM recipient can reply
    if !is_admin && !is_author && !is_recipient {
        return Err(AppError::PermissionDenied);
    }

    let now = chrono::Utc::now();

    // Replies are always private; admins can promote them to public later
    let new_reply = clarification_reply::ActiveModel {
        clarification_id: Set(clarification_id),
        author_id: Set(auth_user.user_id),
        content: Set(payload.content.trim().to_string()),
        is_public: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    new_reply.insert(&state.db).await?;

    // Also update the legacy single-reply fields on the clarification
    let mut active: clarification::ActiveModel = existing.clone().into();
    active.reply_content = Set(Some(payload.content.trim().to_string()));
    active.reply_author_id = Set(Some(auth_user.user_id));
    active.reply_is_public = Set(false);
    active.replied_at = Set(Some(now));
    active.updated_at = Set(now);
    let model = active.update(&state.db).await?;

    // Load all replies for this clarification
    let reply_rows = clarification_reply::Entity::find()
        .filter(clarification_reply::Column::ClarificationId.eq(clarification_id))
        .order_by_asc(clarification_reply::Column::CreatedAt)
        .all(&state.db)
        .await?;

    // Resolve user names
    let mut user_ids = vec![model.author_id];
    if let Some(rid) = model.recipient_id {
        user_ids.push(rid);
    }
    if let Some(rb) = model.resolved_by {
        user_ids.push(rb);
    }
    for rep in &reply_rows {
        user_ids.push(rep.author_id);
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

    let replies = reply_rows
        .iter()
        .map(|rep| ClarificationReplyResponse {
            id: rep.id,
            author_id: rep.author_id,
            author_name: user_map
                .get(&rep.author_id)
                .cloned()
                .unwrap_or_else(|| "[Deleted]".into()),
            content: rep.content.clone(),
            is_public: rep.is_public,
            created_at: rep.created_at,
        })
        .collect();

    let resolved_by_name = model
        .resolved_by
        .and_then(|uid| user_map.get(&uid).cloned());

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
        replies,
        resolved: model.resolved,
        resolved_at: model.resolved_at,
        resolved_by: model.resolved_by,
        resolved_by_name,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }))
}

// ---------------------------------------------------------------------------
// Toggle reply visibility (admin-only)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/{clarification_id}/replies/{reply_id}/toggle-public",
    tag = "Clarifications",
    operation_id = "toggleReplyPublic",
    summary = "Toggle a reply's public visibility",
    description = "Admin can promote a private reply to a public announcement or revert it.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("clarification_id" = i32, Path, description = "Clarification ID"),
        ("reply_id" = i32, Path, description = "Reply ID"),
    ),
    responses(
        (status = 200, description = "Reply visibility toggled", body = ClarificationReplyResponse),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Reply not found", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, query))]
pub async fn toggle_reply_public(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((_contest_id, clarification_id, reply_id)): Path<(i32, i32, i32)>,
    Query(query): Query<ToggleReplyPublicQuery>,
) -> Result<Json<ClarificationReplyResponse>, AppError> {
    auth_user.require_permission("contest:manage")?;

    let reply = clarification_reply::Entity::find_by_id(reply_id)
        .filter(clarification_reply::Column::ClarificationId.eq(clarification_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Reply not found".into()))?;

    let new_is_public = !reply.is_public;
    let mut active: clarification_reply::ActiveModel = reply.clone().into();
    active.is_public = Set(new_is_public);
    let updated = active.update(&state.db).await?;

    // Sync legacy reply_is_public on parent: true if ANY reply is public
    let any_public = clarification_reply::Entity::find()
        .filter(clarification_reply::Column::ClarificationId.eq(clarification_id))
        .filter(clarification_reply::Column::IsPublic.eq(true))
        .count(&state.db)
        .await?
        > 0;
    let parent = clarification::Entity::find_by_id(clarification_id)
        .one(&state.db)
        .await?;
    if let Some(parent) = parent {
        let mut parent_active: clarification::ActiveModel = parent.into();
        parent_active.reply_is_public = Set(any_public);
        // If include_question is set and we're making the reply public,
        // also make the parent question visible to all.
        if new_is_public && query.include_question.unwrap_or(false) {
            parent_active.is_public = Set(true);
        }
        parent_active.update(&state.db).await?;
    }

    let author = user::Entity::find_by_id(updated.author_id)
        .one(&state.db)
        .await?;
    let author_name = author
        .map(|u| u.username)
        .unwrap_or_else(|| "[Deleted]".into());

    Ok(Json(ClarificationReplyResponse {
        id: updated.id,
        author_id: updated.author_id,
        author_name,
        content: updated.content,
        is_public: updated.is_public,
        created_at: updated.created_at,
    }))
}

// ---------------------------------------------------------------------------
// Resolve / reopen clarification
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/{clarification_id}/resolve",
    tag = "Clarifications",
    operation_id = "resolveClarification",
    summary = "Resolve or reopen a clarification thread",
    description = "Admins or the question author can mark a thread as resolved or reopen it.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("clarification_id" = i32, Path, description = "Clarification ID"),
    ),
    request_body = ResolveClarificationRequest,
    responses(
        (status = 200, description = "Status updated", body = ClarificationResponse),
        (status = 401, description = "Unauthorized", body = ErrorBody),
        (status = 403, description = "Forbidden", body = ErrorBody),
        (status = 404, description = "Clarification not found", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn resolve_clarification(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path((contest_id, clarification_id)): Path<(i32, i32)>,
    AppJson(payload): AppJson<ResolveClarificationRequest>,
) -> Result<Json<ClarificationResponse>, AppError> {
    let existing = clarification::Entity::find_by_id(clarification_id)
        .filter(clarification::Column::ContestId.eq(contest_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Clarification not found".into()))?;

    let is_admin = auth_user.has_permission("contest:manage");
    let is_author = existing.author_id == auth_user.user_id;

    if !is_admin && !is_author {
        return Err(AppError::PermissionDenied);
    }

    let now = chrono::Utc::now();
    let mut active: clarification::ActiveModel = existing.clone().into();
    active.resolved = Set(payload.resolved);
    active.resolved_at = Set(if payload.resolved { Some(now) } else { None });
    active.resolved_by = Set(if payload.resolved {
        Some(auth_user.user_id)
    } else {
        None
    });
    active.updated_at = Set(now);
    let model = active.update(&state.db).await?;

    // Load replies
    let reply_rows = clarification_reply::Entity::find()
        .filter(clarification_reply::Column::ClarificationId.eq(clarification_id))
        .order_by_asc(clarification_reply::Column::CreatedAt)
        .all(&state.db)
        .await?;

    // Resolve user names
    let mut user_ids = vec![model.author_id];
    if let Some(rid) = model.recipient_id {
        user_ids.push(rid);
    }
    if let Some(raid) = model.reply_author_id {
        user_ids.push(raid);
    }
    if let Some(rb) = model.resolved_by {
        user_ids.push(rb);
    }
    for rep in &reply_rows {
        user_ids.push(rep.author_id);
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
    let resolved_by_name = model
        .resolved_by
        .and_then(|uid| user_map.get(&uid).cloned());

    let replies = reply_rows
        .iter()
        .map(|rep| ClarificationReplyResponse {
            id: rep.id,
            author_id: rep.author_id,
            author_name: user_map
                .get(&rep.author_id)
                .cloned()
                .unwrap_or_else(|| "[Deleted]".into()),
            content: rep.content.clone(),
            is_public: rep.is_public,
            created_at: rep.created_at,
        })
        .collect();

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
        replies,
        resolved: model.resolved,
        resolved_at: model.resolved_at,
        resolved_by: model.resolved_by,
        resolved_by_name,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }))
}
