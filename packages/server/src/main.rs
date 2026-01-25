mod handlers;
mod host_funcs;
mod manager;
mod models;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, routing::post};
use plugin_core::config::PluginConfig;
use tracing::{Level, info};

use crate::handlers::{judge::execute_judge, plugin::load_plugin};
use crate::manager::ServerManager;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let config = PluginConfig::default();
    let state = AppState {
        plugins: Arc::new(ServerManager::new(config)),
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
