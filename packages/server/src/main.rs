mod database;
mod entity;
mod error;
mod extractors;
mod handlers;
mod host_funcs;
mod manager;
mod models;
mod seed;
mod state;
mod utils;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderName, HeaderValue, Method};
use axum::{
    Router,
    routing::{get, post},
};
use plugin_core::config::PluginConfig;
use tower_http::cors::CorsLayer;
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
    seed::seed_role_permissions(&db).await?;

    let config = PluginConfig::default();
    let state = AppState {
        plugins: Arc::new(ServerManager::new(config, db.clone())),
        db,
    };

    let app = Router::new()
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/me", get(handlers::auth::me))
        .route("/plugins/{id}/load", post(handlers::plugin::load_plugin))
        .route(
            "/plugins/{id}/call/{func}",
            post(handlers::plugin::call_plugin_func),
        )
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap()) // TODO: config
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([
                    HeaderName::from_static("content-type"),
                    HeaderName::from_static("authorization"),
                ])
                .allow_credentials(true)
                .max_age(Duration::from_secs(3600)),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
