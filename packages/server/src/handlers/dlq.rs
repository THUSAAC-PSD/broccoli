use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use common::{DlqMessageType, SubmissionStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use tracing::{info, instrument, warn};

use crate::dispatcher::queue_depth::enforce_queue_depth_admission;
use crate::dlq::{DlqService, ResolveResult, dlq_service};
use crate::entity::{dead_letter_message, submission};
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::json::AppJson;
use crate::extractors::path::AppPath;
use crate::models::dlq::*;
use crate::models::shared::Pagination;
use crate::state::AppState;

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
    AppPath(id): AppPath<i32>,
) -> Result<Json<DlqMessageDetailResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;

    let dlq = dlq_service(&state.db);
    let message = dlq
        .get_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("DLQ message {} not found", id)))?;

    Ok(Json(message.into()))
}

#[utoipa::path(
    post,
    path = "/{id}/retry",
    tag = "Dead Letter Queue",
    operation_id = "retryDlqMessage",
    summary = "Retry a DLQ message",
    description = "Retries a dead letter message by resetting the submission to Pending and re-dispatching it to the plugin-based judging system. Only stuck_submission messages can be retried; operation_task, stuck_code_run, and stuck_submission_judgement messages are visibility-only from here. Marks the DLQ entry as resolved. Requires `dlq:manage` permission.",
    params(("id" = i32, Path, description = "DLQ message ID")),
    responses(
        (status = 200, description = "Submission re-dispatched", body = DlqRetryResponse),
        (status = 400, description = "Only stuck_submission messages can be retried, or submission is not in a retryable state (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Message or submission not found (NOT_FOUND)", body = ErrorBody),
        (status = 409, description = "Message already resolved (CONFLICT)", body = ErrorBody),
        (status = 503, description = "Durable queue depth exceeded (QUEUE_OVERLOADED)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
#[instrument(skip(state, auth_user), fields(id))]
pub async fn retry_dlq_message(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<i32>,
) -> Result<Json<DlqRetryResponse>, AppError> {
    auth_user.require_permission("dlq:manage")?;
    // UP#39 backpressure-on-post: DLQ retry flips the submission back
    // into `Queued` (see comment below the update). Treat it as a
    // fresh durable-accept and apply the same cap.
    enforce_queue_depth_admission(&state).await?;

    let txn = state.db.begin().await?;

    let dlq = DlqService::new(&txn);
    let message = dlq
        .get_by_id_for_update(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("DLQ message {} not found", id)))?;

    if message.resolved {
        return Err(AppError::Conflict("Message already resolved".into()));
    }

    if message.message_type != DlqMessageType::StuckSubmission.as_str() {
        return Err(AppError::Validation(
            "Only stuck_submission messages can be retried. operation_task, stuck_code_run, and stuck_submission_judgement messages are visibility-only from this endpoint.".into(),
        ));
    }

    let Some(submission_id) = message.submission_id else {
        return Err(AppError::Validation(
            "Cannot retry: submission_id is unknown (message had deserialization failure)".into(),
        ));
    };

    let sub = submission::Entity::find_by_id(submission_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Submission {} not found", submission_id)))?;

    if sub.status != SubmissionStatus::SystemError && sub.status != SubmissionStatus::Pending {
        return Err(AppError::Validation(format!(
            "Submission {} is in '{}' state and cannot be retried. Only SystemError or Pending submissions can be retried.",
            submission_id, sub.status
        )));
    }

    let submission_update = submission::ActiveModel {
        id: Set(submission_id),
        // UP#37: DLQ retry resets the submission into the durable-accept
        // state. The claim fiber (UP#38) will promote `Queued` to
        // `Pending` and re-dispatch via the plugin path — no need to
        // spawn here, which also avoids losing the retry if the api
        // crashes between txn commit and the spawned task running.
        status: Set(SubmissionStatus::Queued),
        error_code: Set(None),
        error_message: Set(None),
        ..Default::default()
    };
    submission_update
        .update(&txn)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to reset submission status: {}", e)))?;

    match dlq.resolve(id, Some(auth_user.user_id)).await? {
        ResolveResult::Resolved => {}
        ResolveResult::AlreadyResolved => {
            warn!(id, "DLQ message was resolved concurrently during retry");
        }
        ResolveResult::NotFound => {
            return Err(AppError::Internal(
                "DLQ message disappeared during retry".into(),
            ));
        }
    }

    txn.commit().await?;

    info!(
        id,
        submission_id, "DLQ message reset to Queued for claim-fiber dispatch"
    );

    Ok(Json(DlqRetryResponse {
        message: format!("Submission {} re-queued for judging", submission_id),
    }))
}

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
    AppPath(id): AppPath<i32>,
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

#[utoipa::path(
    post,
    path = "/bulk-retry",
    tag = "Dead Letter Queue",
    operation_id = "bulkRetryDlq",
    summary = "Bulk-retry DLQ messages",
    description = "Retries multiple dead letter messages by resetting their submissions to Pending and re-dispatching to the plugin-based judging system. Supports either specific message IDs or filter-based selection. Only stuck_submission messages with a known submission_id in SystemError or Pending state are retryable; other message types are skipped. Requires `dlq:manage` permission.",
    request_body = BulkRetryDlqRequest,
    responses(
        (status = 200, description = "Bulk retry result", body = BulkRetryDlqResponse),
        (status = 400, description = "Validation error (VALIDATION_ERROR)", body = ErrorBody),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 503, description = "Durable queue depth exceeded (QUEUE_OVERLOADED)", body = ErrorBody),
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
    // UP#39 backpressure-on-post: bulk DLQ retry flips many
    // submissions back to `Queued` in a single call. The check here
    // samples the depth **once at the start** of the bulk insert,
    // which is intentionally permissive: the cap is a steady-state
    // circuit-breaker, not a strict per-row gate. A DLQ retry storm
    // that arrives just below the cap can take the depth meaningfully
    // past it for the rest of that call's duration; the next caller
    // (or the next bulk-retry) will be rejected, restoring
    // equilibrium. The alternative — per-chunk recheck — would
    // serialize the bulk path into hundreds of COUNT round-trips and
    // defeat the bulk endpoint, so we accept bounded overshoot under
    // a retry storm in exchange for keeping bulk retry a single
    // logical operation. Mirrors `bulk_rejudge_submissions`
    // (`submission.rs`) by design.
    enforce_queue_depth_admission(&state).await?;

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

    let mut retried = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();
    let mut submissions_to_dispatch: Vec<i32> = Vec::new();

    const BULK_RETRY_CHUNK_SIZE: usize = 100;

    for chunk in message_ids.chunks(BULK_RETRY_CHUNK_SIZE) {
        let txn = state.db.begin().await?;
        let dlq = DlqService::new(&txn);

        for id in chunk {
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

            if message.message_type != DlqMessageType::StuckSubmission.as_str() {
                skipped += 1;
                continue;
            }

            let Some(submission_id) = message.submission_id else {
                skipped += 1;
                continue;
            };

            let sub = match submission::Entity::find_by_id(submission_id)
                .one(&txn)
                .await
            {
                Ok(Some(s)) => s,
                Ok(None) => {
                    errors.push(BulkRetryError {
                        id: *id,
                        error: format!("Submission {submission_id} not found"),
                    });
                    continue;
                }
                Err(e) => {
                    errors.push(BulkRetryError {
                        id: *id,
                        error: format!("Failed to load submission: {e}"),
                    });
                    continue;
                }
            };

            if sub.status != SubmissionStatus::SystemError
                && sub.status != SubmissionStatus::Pending
            {
                skipped += 1;
                continue;
            }

            let submission_update = submission::ActiveModel {
                id: Set(submission_id),
                // UP#37: durable-accept reset — see single-message
                // `retry_dlq_message` for the gap being closed.
                status: Set(SubmissionStatus::Queued),
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

            submissions_to_dispatch.push(submission_id);
            retried += 1;
        }

        txn.commit().await?;
    }

    // UP#37: each reset submission is now in `Queued`; the claim fiber
    // (UP#38, `dispatcher/claim.rs`) takes it from there. We no longer
    // reload + spawn per row — that path was the very silent-loss
    // vector this PR closes.

    info!(
        retried,
        skipped,
        errors = errors.len(),
        user_id = auth_user.user_id,
        queued_for_claim = submissions_to_dispatch.len(),
        "Bulk retried DLQ messages reset to Queued for claim-fiber dispatch"
    );

    Ok(Json(BulkRetryDlqResponse {
        retried,
        skipped,
        errors,
    }))
}

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
