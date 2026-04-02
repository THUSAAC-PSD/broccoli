use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{HeaderName, HeaderValue, Method};
use common::storage::config::create_blob_store;
use dashmap::DashMap;
use mq::{MqConfig as MqConnConfig, init_mq};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{Level, info, warn};

use server::build_router;
use server::config::AppConfig;
use server::consumers::{consume_operation_dlq, consume_operation_results};
use server::dlq::run_stuck_job_detector;
use server::manager::ServerManager;
use server::registry;
use server::state::AppState;
use server::utils::plugin::sync_plugins;

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
        let op_dlq_consumer_db = db.clone();
        let op_dlq_consumer_mq = Arc::clone(mq_arc);
        let op_dlq_queue = app_config.mq.operation_dlq_queue_name.clone();
        tokio::spawn(async move {
            consume_operation_dlq(op_dlq_consumer_db, op_dlq_consumer_mq, op_dlq_queue).await;
        });
        info!("Operation DLQ consumer started");
    }

    {
        let detector_db = db.clone();
        let detector_config = app_config.mq.dlq.clone();
        tokio::spawn(async move {
            run_stuck_job_detector(detector_db, detector_config).await;
        });
        info!("Stuck job detector started");
    }

    let blob_store = create_blob_store(&app_config.storage, db.clone())
        .await
        .context("Failed to initialize blob storage")?;
    info!(
        "Blob storage initialized (backend: {})",
        app_config.storage.backend
    );

    let contest_type_registry = Arc::new(RwLock::new(HashMap::new()));
    let evaluator_registry = Arc::new(RwLock::new(HashMap::new()));
    let checker_format_registry = Arc::new(RwLock::new(HashMap::new()));
    let language_resolver_registry = Arc::new(RwLock::new(HashMap::new()));
    let operation_batches = Arc::new(DashMap::new());
    let operation_waiters = Arc::new(DashMap::new());
    let evaluate_batches = Arc::new(DashMap::new());

    let batch_max_age = Duration::from_secs(app_config.batch_max_age_secs);
    let reaper_mq = mq.clone();
    let reaper_op_dlq_queue = app_config.mq.operation_dlq_queue_name.clone();
    let operation_waiters_for_reaper = operation_waiters.clone();
    registry::spawn_batch_reaper(
        "operation",
        operation_batches.clone(),
        batch_max_age,
        move |_batch_id, batch| {
            for key in batch.cleanup_keys.iter() {
                operation_waiters_for_reaper.remove(key);

                // Publish DLQ envelope for admin visibility
                if let Some(ref mq) = reaper_mq {
                    // MQ None only in dev (disabled config)
                    let mq = Arc::clone(mq);
                    let queue = reaper_op_dlq_queue.clone();
                    let key = key.clone();
                    tokio::spawn(async move {
                        let envelope = common::DlqEnvelope {
                            message_id: key.clone(),
                            message_type: common::DlqMessageType::OperationTask,
                            submission_id: None,
                            payload: serde_json::json!({ "task_id": key }),
                            error_code: common::DlqErrorCode::StuckJob,
                            error_message: "Operation batch timed out".into(),
                            retry_history: vec![],
                        };
                        if let Err(e) = mq.publish(&queue, None, &envelope, None).await {
                            tracing::error!(%key, error = %e, "Failed to publish stale op to DLQ");
                        }
                    });
                }
            }
        },
    );
    registry::spawn_batch_reaper(
        "evaluate",
        evaluate_batches.clone(),
        batch_max_age,
        |_batch_id, _batch| {},
    );

    if let Some(ref mq_arc) = mq {
        let op_consumer_mq = Arc::clone(mq_arc);
        let op_result_queue = app_config.mq.operation_result_queue_name.clone();
        let op_waiters = operation_waiters.clone();
        tokio::spawn(async move {
            consume_operation_results(op_consumer_mq, op_waiters, op_result_queue).await;
        });
        info!("Operation result consumer started");
    }

    let manager = ServerManager::new(
        app_config.plugin.clone(),
        db.clone(),
        mq.clone(),
        operation_batches.clone(),
        operation_waiters.clone(),
        contest_type_registry.clone(),
        evaluator_registry.clone(),
        checker_format_registry.clone(),
        language_resolver_registry.clone(),
        evaluate_batches.clone(),
        app_config.clone(),
        blob_store.clone(),
    )
    .context("Failed to initialize plugin manager")?;

    let device_codes: server::state::DeviceCodeStore = Arc::new(DashMap::new());

    // Spawn reaper for expired device codes (runs every 60s, removes entries older than 15min)
    {
        let codes = device_codes.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = std::time::Instant::now();
                codes.retain(|_code, entry| entry.expires_at > now);
            }
        });
    }

    // Spawn hourly cleanup for expired idempotency keys (24h TTL)
    {
        let cleanup_db = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            loop {
                interval.tick().await;
                server::middleware::idempotency::cleanup_expired_keys(&cleanup_db).await;
            }
        });
    }

    let state = AppState {
        plugins: manager,
        db: db.clone(),
        config: app_config.clone(),
        mq: mq.clone(),
        blob_store,
        registries: server::state::RegistryState {
            contest_type_registry: contest_type_registry.clone(),
            evaluator_registry: evaluator_registry.clone(),
            checker_format_registry: checker_format_registry.clone(),
            language_resolver_registry: language_resolver_registry.clone(),
            operation_batches: operation_batches.clone(),
            operation_waiters: operation_waiters.clone(),
            evaluate_batches: evaluate_batches.clone(),
            hook_registry: server::hooks::new_shared_registry(),
        },
        device_codes,
    };

    sync_plugins(&state).await?;

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
                HeaderName::from_static("idempotency-key"),
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
