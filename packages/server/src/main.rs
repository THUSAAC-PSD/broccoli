use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde::{Deserialize, Serialize};
use tracing::{Level, info};

mod plugins;
use plugins::traits::PluginManagerExt;
use plugins::{ExtismPluginManager, PluginConfig, PluginManager};

#[derive(Clone)]
struct AppState {
    plugins: Arc<dyn PluginManager>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Submission {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JudgeResult {
    greeting: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let config = PluginConfig::default();
    let manager = ExtismPluginManager::new(config);

    let state = AppState {
        plugins: Arc::new(manager),
    };

    let app = Router::new()
        .route("/plugins/{id}/load", post(load_plugin))
        .route("/run/{id}", post(execute_judge))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_plugin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, StatusCode> {
    state.plugins.load_plugin(&id).map_err(|e| {
        tracing::error!("Failed to load plugin: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(format!("Plugin '{}' loaded successfully", id))
}

async fn execute_judge(
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
