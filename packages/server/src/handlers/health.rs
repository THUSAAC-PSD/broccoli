use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde::Serialize;
use tokio::time::timeout;
use tracing::{instrument, warn};
use utoipa::ToSchema;

use crate::state::AppState;

const PING_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Aggregate status: `"ok"` when every probed component is healthy,
    /// `"degraded"` otherwise.
    #[schema(example = "ok")]
    pub status: String,
    /// Database probe result: `"ok"` or `"down"`.
    #[schema(example = "ok")]
    pub db: String,
    /// Message-queue probe result: `"ok"`, `"down"`, or `"disabled"` when
    /// MQ is not configured.
    #[schema(example = "ok")]
    pub mq: String,
    /// Server version, matching the `Cargo.toml` `[package].version`.
    #[schema(example = "0.2.0")]
    pub version: String,
    /// Short Git SHA captured at build time (or `"unknown"` if not built
    /// from a git checkout).
    #[schema(example = "abc1234")]
    pub git_sha: String,
}

/// Probes the database and MQ backends and returns a populated
/// [`HealthResponse`]. Each backend is probed with a 2-second hard timeout.
///
/// Shared between the documented `/api/v1/health` route and the plain
/// `/healthz` route mounted on the outer Router.
pub async fn compute_health(
    db: &DatabaseConnection,
    redis_client: Option<&redis::Client>,
    mq_enabled: bool,
) -> HealthResponse {
    let db_status = match timeout(PING_TIMEOUT, ping_db(db)).await {
        Ok(Ok(())) => "ok",
        Ok(Err(e)) => {
            warn!(error = %e, "Health check: DB ping failed");
            "down"
        }
        Err(_) => {
            warn!("Health check: DB ping timed out");
            "down"
        }
    };

    let mq_status = if !mq_enabled {
        "disabled"
    } else {
        match redis_client {
            Some(client) => match timeout(PING_TIMEOUT, ping_redis(client)).await {
                Ok(Ok(())) => "ok",
                Ok(Err(e)) => {
                    warn!(error = %e, "Health check: MQ ping failed");
                    "down"
                }
                Err(_) => {
                    warn!("Health check: MQ ping timed out");
                    "down"
                }
            },
            None => {
                warn!("Health check: MQ enabled but no Redis client available");
                "down"
            }
        }
    };

    let status = if db_status == "ok" && (mq_status == "ok" || mq_status == "disabled") {
        "ok"
    } else {
        "degraded"
    };

    HealthResponse {
        status: status.to_string(),
        db: db_status.to_string(),
        mq: mq_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("BROCCOLI_GIT_SHA")
            .unwrap_or("unknown")
            .to_string(),
    }
}

async fn ping_db(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    db.execute_raw(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT 1".to_string(),
    ))
    .await
    .map(|_| ())
}

async fn ping_redis(client: &redis::Client) -> Result<(), redis::RedisError> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _pong: String = redis::cmd("PING").query_async(&mut conn).await?;
    Ok(())
}

fn status_code(body: &HealthResponse) -> StatusCode {
    if body.status == "ok" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// Plain-router handler for `GET /healthz`. Not part of the OpenAPI surface.
#[instrument(skip(state))]
pub async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    let body = compute_health(&state.db, state.redis_client.as_deref(), state.mq.is_some()).await;
    (status_code(&body), Json(body))
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "Meta",
    operation_id = "getHealth",
    summary = "Server liveness and dependency health",
    description = "Public, unauthenticated. Pings the database and message queue with a 2s \
                   timeout each. Returns 200 with `status: \"ok\"` when all probed components \
                   are healthy; 503 with `status: \"degraded\"` otherwise. Used by load \
                   balancers and Docker `HEALTHCHECK` (via the `--healthcheck` CLI flag).",
    responses(
        (status = 200, description = "All components healthy", body = HealthResponse),
        (status = 503, description = "One or more components are down", body = HealthResponse),
    ),
)]
#[instrument(skip(state))]
pub async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let body = compute_health(&state.db, state.redis_client.as_deref(), state.mq.is_some()).await;
    (status_code(&body), Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    /// Constructs a DB pool pointed at an unreachable address; any operation
    /// will fail (or, with our 2s timeout, time out). Verifies the degraded
    /// path returns 503 with `db: "down"` without needing a live database.
    #[tokio::test]
    async fn compute_health_reports_db_down_when_db_unreachable() {
        // Lazy-connect to a port that refuses connections so the call to
        // `Database::connect` itself succeeds, then the SELECT fails.
        let mut opts = ConnectOptions::new("postgres://nobody:nobody@127.0.0.1:1/nope");
        opts.max_connections(1)
            .min_connections(0)
            .connect_timeout(Duration::from_millis(500))
            .acquire_timeout(Duration::from_millis(500))
            .connect_lazy(true)
            .sqlx_logging(false);
        let bad_db = Database::connect(opts)
            .await
            .expect("lazy connect should succeed");

        let body = compute_health(&bad_db, None, false).await;

        assert_eq!(body.db, "down", "expected db=down, got {body:?}");
        assert_eq!(body.mq, "disabled");
        assert_eq!(body.status, "degraded");
        assert_eq!(status_code(&body), StatusCode::SERVICE_UNAVAILABLE);
    }
}
