use axum::Json;
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Serialize, ToSchema)]
pub struct VersionResponse {
    /// Server version, matching the `Cargo.toml` `[package].version`.
    #[schema(example = "0.2.0")]
    pub version: String,
    /// Short Git SHA captured at build time (or `"unknown"` if not built from a git checkout).
    #[schema(example = "abc1234")]
    pub git_sha: String,
}

#[utoipa::path(
    get,
    path = "/version",
    tag = "Meta",
    operation_id = "getVersion",
    summary = "Server version and build info",
    description = "Public, unauthenticated. Used by the stress-test CLI for an advisory \
                   client/server version check.",
    responses(
        (status = 200, description = "Server version info", body = VersionResponse),
    ),
)]
#[instrument]
pub async fn get_version() -> Result<Json<VersionResponse>, AppError> {
    Ok(Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("BROCCOLI_GIT_SHA")
            .unwrap_or("unknown")
            .to_string(),
    }))
}
