use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sea_orm::*;
use tracing::{error, warn};

use crate::entity::idempotency_key;
use crate::error::AppError;
use crate::state::AppState;

/// Maximum size for caching response bodies (1 MB).
const MAX_RESPONSE_BODY_SIZE: usize = 1_048_576;

/// Stale pending key threshold (2 minutes). If a key is pending for longer
/// than this, we assume the server crashed mid-request and reclaim it.
const STALE_PENDING_SECS: i64 = 120;

/// Idempotency middleware for POST requests.
///
/// When an `Idempotency-Key` header is present on a POST request:
/// 1. Claims the key atomically via INSERT ON CONFLICT DO NOTHING
/// 2. If already claimed and completed, returns the cached response
/// 3. If already claimed and pending (< 2min), returns 409
/// 4. If already claimed and pending (>= 2min, stale), reclaims and proceeds
/// 5. After handler returns 2xx, caches the response; on 4xx/5xx, deletes the key
pub async fn idempotency_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }

    let key = match request.headers().get("idempotency-key") {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                return AppError::Validation("Idempotency-Key header is not valid UTF-8".into())
                    .into_response();
            }
        },
        None => return next.run(request).await, // No header then pass through
    };

    if key.is_empty() || key.len() > 255 {
        return AppError::Validation("Idempotency-Key must be 1-255 characters".into())
            .into_response();
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return AppError::Validation(
            "Idempotency-Key must contain only alphanumeric characters, hyphens, and underscores"
                .into(),
        )
        .into_response();
    }

    let user_id = match extract_user_id(&request, &state) {
        Some(id) => id,
        None => return next.run(request).await, // Unauthenticated then pass through
    };

    let request_path = request.uri().path().to_string();
    let request_method = request.method().to_string();
    let db = &state.db;

    match try_claim_key(db, &key, user_id, &request_path, &request_method).await {
        Ok(ClaimResult::Claimed) => {
            let response = next.run(request).await;
            complete_key(db, &key, user_id, response).await
        }
        Ok(ClaimResult::AlreadyExists(existing)) => {
            handle_existing_key(
                db,
                existing,
                &key,
                user_id,
                &request_path,
                &request_method,
                request,
                next,
            )
            .await
        }
        Err(e) => {
            // Fail-open: DB error -> pass through without idempotency
            warn!(error = %e, key = %key, "Idempotency middleware DB error, passing through");
            next.run(request).await
        }
    }
}

/// Extract user_id from the Authorization header without consuming the request.
fn extract_user_id(request: &Request<Body>, state: &AppState) -> Option<i32> {
    let auth_header = request.headers().get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let claims = crate::utils::jwt::verify(token, &state.config.auth.jwt_secret).ok()?;
    Some(claims.uid)
}

enum ClaimResult {
    Claimed,
    AlreadyExists(idempotency_key::Model),
}

/// Try to claim a key atomically.
async fn try_claim_key(
    db: &DatabaseConnection,
    key: &str,
    user_id: i32,
    request_path: &str,
    request_method: &str,
) -> Result<ClaimResult, DbErr> {
    let now = Utc::now();
    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"INSERT INTO idempotency_key (key, user_id, request_path, request_method, status, created_at)
               VALUES ($1, $2, $3, $4, 'pending', $5)
               ON CONFLICT (key, user_id) DO NOTHING"#,
            [
                key.into(),
                user_id.into(),
                request_path.into(),
                request_method.into(),
                now.into(),
            ],
        ))
        .await?;

    if result.rows_affected() == 1 {
        return Ok(ClaimResult::Claimed);
    }

    // Key already exists -- fetch it
    let existing = idempotency_key::Entity::find_by_id((key.to_string(), user_id))
        .one(db)
        .await?
        .ok_or_else(|| {
            DbErr::Custom("Idempotency key vanished between INSERT and SELECT".into())
        })?;

    Ok(ClaimResult::AlreadyExists(existing))
}

/// Handle an existing idempotency key (completed or pending).
async fn handle_existing_key(
    db: &DatabaseConnection,
    existing: idempotency_key::Model,
    key: &str,
    user_id: i32,
    request_path: &str,
    request_method: &str,
    request: Request<Body>,
    next: Next,
) -> Response {
    match existing.status.as_str() {
        "completed" => {
            // Validate path/method match
            if existing.request_path != request_path || existing.request_method != request_method {
                return AppError::IdempotencyKeyMismatch(format!(
                    "This key was used with {} {} but this request is {} {}",
                    existing.request_method, existing.request_path, request_method, request_path
                ))
                .into_response();
            }

            // Return cached response
            let status = existing
                .response_status
                .and_then(|s| StatusCode::from_u16(s as u16).ok())
                .unwrap_or(StatusCode::OK);
            let body = existing.response_body.unwrap_or_default();

            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        "pending" => {
            let age_secs = (Utc::now() - existing.created_at).num_seconds();
            if age_secs < STALE_PENDING_SECS {
                AppError::IdempotencyKeyInProgress.into_response()
            } else {
                // Stale. The server probably crashed. Delete and reclaim.
                warn!(
                    key = %key,
                    age_secs,
                    "Reclaiming stale pending idempotency key"
                );
                match reclaim_stale_key(db, key, user_id, request_path, request_method).await {
                    Ok(true) => {
                        let response = next.run(request).await;
                        complete_key(db, key, user_id, response).await
                    }
                    Ok(false) => {
                        // Another request beat us to reclaim, so treat as in-progress
                        AppError::IdempotencyKeyInProgress.into_response()
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to reclaim stale key, passing through");
                        next.run(request).await
                    }
                }
            }
        }
        _ => {
            // Unknown status, just pass through
            warn!(key = %key, status = %existing.status, "Unknown idempotency key status");
            next.run(request).await
        }
    }
}

/// Atomically reclaim a stale pending key. Returns true if we successfully reclaimed.
async fn reclaim_stale_key(
    db: &DatabaseConnection,
    key: &str,
    user_id: i32,
    request_path: &str,
    request_method: &str,
) -> Result<bool, DbErr> {
    let now = Utc::now();
    let stale_cutoff = now - chrono::Duration::seconds(STALE_PENDING_SECS);

    let result = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"UPDATE idempotency_key
               SET status = 'pending', created_at = $1, completed_at = NULL,
                   response_status = NULL, response_body = NULL,
                   request_path = $2, request_method = $3
               WHERE key = $4 AND user_id = $5
                 AND status = 'pending' AND created_at < $6"#,
            [
                now.into(),
                request_path.into(),
                request_method.into(),
                key.into(),
                user_id.into(),
                stale_cutoff.into(),
            ],
        ))
        .await?;

    Ok(result.rows_affected() == 1)
}

/// After handler returns: cache 2xx responses, delete key for errors.
async fn complete_key(
    db: &DatabaseConnection,
    key: &str,
    user_id: i32,
    response: Response,
) -> Response {
    let status = response.status();
    let is_success = status.is_success();

    if is_success {
        // Cache the response body
        let (parts, body) = response.into_parts();
        match to_bytes(body, MAX_RESPONSE_BODY_SIZE).await {
            Ok(bytes) => {
                let body_str = match String::from_utf8(bytes.to_vec()) {
                    Ok(s) => s,
                    Err(_) => {
                        warn!("Response body is not valid UTF-8, skipping idempotency cache");
                        let _ = delete_key(db, key, user_id).await;
                        return Response::from_parts(parts, Body::from(bytes.to_vec()));
                    }
                };
                if let Err(e) =
                    mark_completed(db, key, user_id, parts.status.as_u16() as i16, &body_str).await
                {
                    error!(error = %e, "Failed to cache idempotency response");
                }
                // Reconstruct response
                Response::from_parts(parts, Body::from(bytes.to_vec()))
            }
            Err(e) => {
                // Body too large to cache. Mark the key as completed WITHOUT a
                // cached body so retries get 409 instead of creating duplicates.
                warn!(error = %e, "Response body too large to cache for idempotency");
                if let Err(e) =
                    mark_completed(db, key, user_id, parts.status.as_u16() as i16, "{}").await
                {
                    error!(error = %e, "Failed to mark oversized idempotency key as completed");
                }
                Response::builder()
                    .status(parts.status)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            }
        }
    } else {
        if let Err(e) = delete_key(db, key, user_id).await {
            warn!(error = %e, "Failed to delete idempotency key after error response");
        }
        response
    }
}

/// Mark a key as completed with the cached response.
async fn mark_completed(
    db: &DatabaseConnection,
    key: &str,
    user_id: i32,
    response_status: i16,
    response_body: &str,
) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"UPDATE idempotency_key
           SET status = 'completed', response_status = $1, response_body = $2, completed_at = $3
           WHERE key = $4 AND user_id = $5 AND status = 'pending'"#,
        [
            response_status.into(),
            response_body.into(),
            Utc::now().into(),
            key.into(),
            user_id.into(),
        ],
    ))
    .await?;
    Ok(())
}

/// Delete an idempotency key.
async fn delete_key(db: &DatabaseConnection, key: &str, user_id: i32) -> Result<(), DbErr> {
    idempotency_key::Entity::delete_by_id((key.to_string(), user_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Delete idempotency keys older than 24 hours. Called by background cleanup task.
pub async fn cleanup_expired_keys(db: &DatabaseConnection) {
    let cutoff = Utc::now() - chrono::Duration::hours(24);
    match db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM idempotency_key WHERE created_at < $1",
            [cutoff.into()],
        ))
        .await
    {
        Ok(result) => {
            let deleted = result.rows_affected();
            if deleted > 0 {
                tracing::info!(deleted, "Cleaned up expired idempotency keys");
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to cleanup expired idempotency keys");
        }
    }
}
