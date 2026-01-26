use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use plugin_core::traits::PluginManagerExt;

use crate::models::judge::{JudgeResult, Submission};
use crate::state::AppState;

pub async fn execute_judge(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(submission): Json<Submission>,
) -> Result<Json<JudgeResult>, StatusCode> {
    let result: JudgeResult = state
        .plugins
        .call(&plugin_id, "greet", submission)
        .await
        .map_err(|e| {
            tracing::error!("Plugin execution error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}
