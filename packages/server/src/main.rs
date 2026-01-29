mod database;
mod entity;
mod handlers;
mod host_funcs;
mod manager;
mod models;
mod state;
mod utils;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, routing::post};
use plugin_core::config::PluginConfig;
use tracing::{Level, info};

use crate::manager::ServerManager;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let db = database::init_db("postgres://postgres:password@localhost:5432/broccoli").await?;

    let config = PluginConfig::default();
    let state = AppState {
        plugins: Arc::new(ServerManager::new(config, db.clone())),
        db,
    };

    let app = Router::new()
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/plugins/{id}/load", post(handlers::plugin::load_plugin))
        .route("/plugins/{id}/call/{func}", post(handlers::plugin::call_plugin_func))
        .route("/run/{id}", post(handlers::judge::execute_judge))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
