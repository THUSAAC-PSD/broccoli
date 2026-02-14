use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use common::{DlqMessageType, SubmissionStatus, judge_job::JudgeJob, worker::Task};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use tracing::{info, instrument, warn};

use crate::dlq::{DlqService, ResolveResult, dlq_service};
use crate::entity::{dead_letter_message, submission};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::models::dlq::*;
use crate::models::shared::Pagination;
use crate::state::AppState;

/// List dead letter messages.
#[utoipa::path(
    get,
    path = "",
    tag = "Dead Letter Queue",
    operation_id = "listDlqMessages",
    summary = "List dead letter messages",
    description = "Returns a paginated list of dead letter messages. Requires `dlq:manage` permission.",
    params(ListDlqParams),
    responses(
        (status = 200, description = "List of DLQ messages", body = DlqListResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn list_dlq_messages(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ListDlqParams>,
) -> Result<Json<DlqListResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let message_type = params
        .message_type
        .map(|mt| mt.parse::<DlqMessageType>())
        .transpose()
        .map_err(AppError::Validation)?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);

    let dlq = dlq_service(&state.db);
    let (messages, total) = dlq
        .list(message_type, params.resolved, page, per_page)
        .await?;

    let data: Vec<DlqMessageResponse> = messages.into_iter().map(Into::into).collect();
    let total_pages = total.div_ceil(per_page);

    Ok(Json(DlqListResponse {
        data,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages,
        },
    }))
}

/// Get DLQ statistics.
#[utoipa::path(
    get,
    path = "/stats",
    tag = "Dead Letter Queue",
    operation_id = "getDlqStats",
    summary = "Get DLQ statistics",
    description = "Returns statistics about the dead letter queue. Requires `dlq:manage` permission.",
    responses(
        (status = 200, description = "DLQ statistics", body = DlqStatsResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user))]
pub async fn get_dlq_stats(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<DlqStatsResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let dlq = dlq_service(&state.db);
    let stats = dlq.stats().await?;

    Ok(Json(stats.into()))
}

/// Get a single DLQ message by ID.
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Dead Letter Queue",
    operation_id = "getDlqMessage",
    summary = "Get DLQ message details",
    description = "Returns full details of a DLQ message including payload and retry history. Requires `dlq:manage` permission.",
    params(("id" = i32, Path, description = "DLQ message ID")),
    responses(
        (status = 200, description = "DLQ message details", body = DlqMessageDetailResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Message not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn get_dlq_message(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<DlqMessageDetailResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let dlq = dlq_service(&state.db);
    let message = dlq
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("DLQ message {} not found", id)))?;

    Ok(Json(message.into()))
}

/// Retry a DLQ message by re-enqueuing it.
#[utoipa::path(
    post,
    path = "/{id}/retry",
    tag = "Dead Letter Queue",
    operation_id = "retryDlqMessage",
    summary = "Retry a DLQ message",
    description = "Re-enqueues a dead letter message for processing. Only works for judge_job message type. Marks the DLQ entry as resolved. Requires `dlq:manage` permission.",
    params(("id" = i32, Path, description = "DLQ message ID")),
    responses(
        (status = 200, description = "Message requeued", body = DlqRetryResponse),
        (status = 400, description = "Cannot retry judge_result messages (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Message not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Message already resolved (CONFLICT)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn retry_dlq_message(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<DlqRetryResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let txn = state.db.begin().await?;

    let dlq = crate::dlq::DlqService::new(&txn);
    let message = dlq
        .get_by_id_for_update(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("DLQ message {} not found", id)))?;

    if message.resolved {
        return Err(AppError::Conflict("Message already resolved".into()));
    }

    if message.message_type != DlqMessageType::JudgeJob.as_str() {
        return Err(AppError::Validation(
            "Only judge_job messages can be retried. judge_result messages require manual intervention.".into(),
        ));
    }

    let Some(submission_id) = message.submission_id else {
        return Err(AppError::Validation(
            "Cannot retry: submission_id is unknown (message had deserialization failure)".into(),
        ));
    };

    let Some(ref mq) = state.mq else {
        return Err(AppError::Internal("Message queue not available".into()));
    };

    let job: JudgeJob = serde_json::from_value(message.payload.clone())
        .map_err(|e| AppError::Internal(format!("Failed to deserialize job payload: {}", e)))?;

    let task = Task {
        id: job.job_id.clone(),
        task_type: "judge".into(),
        payload: message.payload.clone(),
    };

    let submission_update = submission::ActiveModel {
        id: Set(submission_id),
        status: Set(SubmissionStatus::Pending),
        error_code: Set(None),
        error_message: Set(None),
        ..Default::default()
    };
    submission_update
        .update(&txn)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to reset submission status: {}", e)))?;

    match dlq.resolve(id, Some(auth_user.user_id)).await? {
        ResolveResult::Resolved => {} // Expected
        ResolveResult::AlreadyResolved => {
            tracing::warn!(id, "DLQ message was resolved concurrently during retry");
        }
        ResolveResult::NotFound => {
            return Err(AppError::Internal(
                "DLQ message disappeared during retry".into(),
            ));
        }
    }

    mq.publish(&state.config.mq.queue_name, None, &task, None)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to re-enqueue message: {}", e)))?;

    txn.commit().await.map_err(|e| {
        tracing::error!(
            id,
            submission_id,
            error = %e,
            "CRITICAL: MQ message published but DB commit failed. \
             Message is in worker queue but DLQ entry remains unresolved."
        );
        AppError::Internal(format!("DB commit failed after MQ publish: {}", e))
    })?;

    info!(id, submission_id, "DLQ message retried");

    Ok(Json(DlqRetryResponse {
        message: format!("Message requeued for submission {}", submission_id),
    }))
}

/// Delete (resolve) a DLQ message.
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Dead Letter Queue",
    operation_id = "deleteDlqMessage",
    summary = "Delete (resolve) a DLQ message",
    description = "Marks a DLQ message as resolved without retrying. Use this to acknowledge messages that don't need to be reprocessed. Requires `dlq:manage` permission.",
    params(("id" = i32, Path, description = "DLQ message ID")),
    responses(
        (status = 204, description = "Message resolved"),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Message not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn delete_dlq_message(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let dlq = dlq_service(&state.db);
    let result = dlq.resolve(id, Some(auth_user.user_id)).await?;

    match result {
        ResolveResult::Resolved => {
            info!(id, "DLQ message resolved");
            Ok(StatusCode::NO_CONTENT)
        }
        ResolveResult::NotFound => Err(AppError::NotFound(format!("DLQ message {} not found", id))),
        ResolveResult::AlreadyResolved => {
            info!(id, "DLQ message already resolved");
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

/// Bulk-retry DLQ messages.
#[utoipa::path(
    post,
    path = "/bulk-retry",
    tag = "Dead Letter Queue",
    operation_id = "bulkRetryDlq",
    summary = "Bulk-retry DLQ messages",
    description = "Re-enqueues multiple dead letter messages for processing. Supports either specific message IDs or filter-based selection. Only judge_job messages with a known submission_id are retryable. Requires `dlq:manage` permission.",
    request_body = BulkRetryDlqRequest,
    responses(
        (status = 200, description = "Bulk retry result", body = BulkRetryDlqResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 500, description = "MQ unavailable (INTERNAL_ERROR)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn bulk_retry_dlq(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<BulkRetryDlqRequest>,
) -> Result<Json<BulkRetryDlqResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;
    validate_bulk_retry_dlq(&payload)?;

    let Some(ref mq) = state.mq else {
        return Err(AppError::Internal("Message queue not available".into()));
    };

    let message_ids: Vec<i32> = if let Some(ref ids) = payload.message_ids {
        ids.clone()
    } else {
        let mut query = dead_letter_message::Entity::find()
            .filter(dead_letter_message::Column::Resolved.eq(false));

        if let Some(ref mt) = payload.message_type {
            query = query.filter(dead_letter_message::Column::MessageType.eq(mt.as_str()));
        }
        if let Some(ref ec) = payload.error_code {
            query = query.filter(dead_letter_message::Column::ErrorCode.eq(ec.as_str()));
        }

        let ids: Vec<i32> = query
            .select_only()
            .column(dead_letter_message::Column::Id)
            .order_by_asc(dead_letter_message::Column::CreatedAt)
            .limit(10001)
            .into_tuple::<i32>()
            .all(&state.db)
            .await?;

        if ids.len() > 10_000 {
            return Err(AppError::Validation(
                "Filter matches more than 10,000 messages. Narrow your filters.".into(),
            ));
        }

        ids
    };

    let txn = state.db.begin().await?;
    let dlq = DlqService::new(&txn);

    let mut retried = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();
    let mut tasks_to_publish: Vec<Task> = Vec::new();

    for id in &message_ids {
        let message = match dlq.get_by_id_for_update(*id).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                errors.push(BulkRetryError {
                    id: *id,
                    error: "Message not found".into(),
                });
                continue;
            }
            Err(e) => {
                errors.push(BulkRetryError {
                    id: *id,
                    error: format!("DB error: {e}"),
                });
                continue;
            }
        };

        if message.resolved {
            skipped += 1;
            continue;
        }

        if message.message_type != DlqMessageType::JudgeJob.as_str() {
            skipped += 1;
            continue;
        }

        let Some(submission_id) = message.submission_id else {
            skipped += 1;
            continue;
        };

        let job: JudgeJob = match serde_json::from_value(message.payload.clone()) {
            Ok(j) => j,
            Err(e) => {
                errors.push(BulkRetryError {
                    id: *id,
                    error: format!("Failed to deserialize payload: {e}"),
                });
                continue;
            }
        };

        let submission_update = submission::ActiveModel {
            id: Set(submission_id),
            status: Set(SubmissionStatus::Pending),
            error_code: Set(None),
            error_message: Set(None),
            ..Default::default()
        };
        if let Err(e) = submission_update.update(&txn).await {
            errors.push(BulkRetryError {
                id: *id,
                error: format!("Failed to reset submission: {e}"),
            });
            continue;
        }

        match dlq.resolve(*id, Some(auth_user.user_id)).await {
            Ok(ResolveResult::Resolved | ResolveResult::AlreadyResolved) => {}
            Ok(ResolveResult::NotFound) => {
                errors.push(BulkRetryError {
                    id: *id,
                    error: "DLQ message disappeared during retry".into(),
                });
                continue;
            }
            Err(e) => {
                errors.push(BulkRetryError {
                    id: *id,
                    error: format!("Failed to resolve: {e}"),
                });
                continue;
            }
        }

        tasks_to_publish.push(Task {
            id: job.job_id.clone(),
            task_type: "judge".into(),
            payload: message.payload.clone(),
        });

        retried += 1;
    }

    txn.commit().await?;

    for task in &tasks_to_publish {
        if let Err(e) = mq
            .publish(&state.config.mq.queue_name, None, task, None)
            .await
        {
            warn!(task_id = %task.id, error = %e, "Failed to publish retried task to MQ");
        }
    }

    info!(
        retried,
        skipped,
        errors = errors.len(),
        user_id = auth_user.user_id,
        "Bulk retried DLQ messages"
    );

    Ok(Json(BulkRetryDlqResponse {
        retried,
        skipped,
        errors,
    }))
}

/// Bulk-delete (resolve) DLQ messages.
#[utoipa::path(
    delete,
    path = "/bulk",
    tag = "Dead Letter Queue",
    operation_id = "bulkDeleteDlq",
    summary = "Bulk-delete (resolve) DLQ messages",
    description = "Marks multiple DLQ messages as resolved without retrying. Requires `dlq:manage` permission.",
    request_body = BulkDeleteDlqRequest,
    responses(
        (status = 200, description = "Messages resolved", body = BulkDeleteDlqResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user, payload))]
pub async fn bulk_delete_dlq(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppJson(payload): AppJson<BulkDeleteDlqRequest>,
) -> Result<Json<BulkDeleteDlqResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;
    validate_bulk_delete_dlq(&payload)?;

    let dlq = dlq_service(&state.db);
    let rows_affected = dlq
        .resolve_many(&payload.message_ids, Some(auth_user.user_id))
        .await?;

    info!(
        deleted = rows_affected,
        user_id = auth_user.user_id,
        "Bulk resolved DLQ messages"
    );

    Ok(Json(BulkDeleteDlqResponse {
        deleted: rows_affected as usize,
    }))
}
