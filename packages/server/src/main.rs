use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{HeaderName, HeaderValue, Method};
use server::build_router;
use tower_http::cors::CorsLayer;
use tracing::{Level, info};

use server::config::AppConfig;
use server::manager::ServerManager;
use server::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let app_config = AppConfig::load().context("Failed to load configuration")?;

    let db = server::database::init_db(&app_config.database.url).await?;
    server::seed::seed_role_permissions(&db).await?;

    let state = AppState {
        plugins: Arc::new(ServerManager::new(app_config.plugin.clone(), db.clone())),
        db,
        config: app_config.clone(),
    };

    let mut allow_origins = Vec::new();
    for origin in &app_config.server.cors.allow_origins {
        allow_origins.push(
            origin
                .parse::<HeaderValue>()
                .with_context(|| format!("Invalid CORS origin: {}", origin))?,
        );
    }

    let app = build_router(state).layer(
        CorsLayer::new()
            .allow_origin(allow_origins)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
            ])
            .allow_credentials(true)
            .max_age(Duration::from_secs(app_config.server.cors.max_age)),
    );

    let addr_str = format!("{}:{}", app_config.server.host, app_config.server.port);
    let addr: SocketAddr = addr_str
        .parse()
        .with_context(|| format!("Invalid server address: {}", addr_str))?;

    info!("Server running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;
    axum::serve(listener, app)
        .await
        .context("Server runtime error")?;

    Ok(())
}
