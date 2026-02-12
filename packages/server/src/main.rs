use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{HeaderName, HeaderValue, Method};
use mq::{MqConfig as MqConnConfig, init_mq};
use plugin_core::traits::PluginManager;
use tower_http::cors::CorsLayer;
use tracing::{Level, info, warn};

use server::build_router;
use server::config::AppConfig;
use server::consumers::{consume_judge_results, consume_worker_dlq};
use server::dlq::run_stuck_job_detector;
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
    server::seed::ensure_indexes(&db).await?;

    let mq = if app_config.mq.enabled {
        match init_mq(MqConnConfig {
            url: app_config.mq.url.clone(),
            pool_size: app_config.mq.pool_size,
        })
        .await
        {
            Ok(queue) => {
                info!("MQ connected to {}", app_config.mq.url);
                Some(Arc::new(queue))
            }
            Err(e) => {
                warn!("MQ connection failed, submissions won't be queued: {}", e);
                None
            }
        }
    } else {
        info!("MQ disabled by configuration");
        None
    };

    if let Some(ref mq_arc) = mq {
        let consumer_db = db.clone();
        let consumer_mq = Arc::clone(mq_arc);
        let result_queue = app_config.mq.result_queue_name.clone();
        let dlq_config = app_config.mq.dlq.clone();
        tokio::spawn(async move {
            consume_judge_results(consumer_db, consumer_mq, result_queue, dlq_config).await;
        });
        info!("Judge result consumer started");

        let dlq_consumer_db = db.clone();
        let dlq_consumer_mq = Arc::clone(mq_arc);
        let dlq_queue = app_config.mq.dlq_queue_name.clone();
        tokio::spawn(async move {
            consume_worker_dlq(dlq_consumer_db, dlq_consumer_mq, dlq_queue).await;
        });
        info!("Worker DLQ consumer started");
    }

    {
        let detector_db = db.clone();
        let detector_config = app_config.mq.dlq.clone();
        tokio::spawn(async move {
            run_stuck_job_detector(detector_db, detector_config).await;
        });
        info!("Stuck job detector started");
    }

    let plugin_manager = ServerManager::new(app_config.plugin.clone(), db.clone());
    plugin_manager.discover_plugins()?;
    let state = AppState {
        plugins: Arc::new(plugin_manager),
        db,
        config: app_config.clone(),
        mq,
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
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
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
