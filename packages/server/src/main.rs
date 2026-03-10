use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{HeaderName, HeaderValue, Method};
use common::storage::BlobStore;
use common::storage::database::DatabaseBlobStore;
use common::storage::filesystem::FilesystemBlobStore;
use common::storage::object_storage::{ObjectStorageBlobStore, ObjectStorageConfig};
use dashmap::DashMap;
use mq::{MqConfig as MqConnConfig, init_mq};
use std::path::PathBuf;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{Level, info, warn};

use server::build_router;
use server::config::AppConfig;
use server::consumers::{consume_judge_results, consume_operation_results, consume_worker_dlq};
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

    let blob_store: Arc<dyn BlobStore> = match app_config.storage.backend.as_str() {
        "filesystem" => {
            let blob_path = PathBuf::from(&app_config.storage.data_dir).join("blobs");
            Arc::new(
                FilesystemBlobStore::new(blob_path, app_config.storage.max_blob_size)
                    .await
                    .context("Failed to initialize filesystem blob storage")?,
            )
        }
        "object_storage" => {
            let os_toml = app_config
                .storage
                .object_storage
                .as_ref()
                .context("storage.backend is 'object_storage' but [storage.object_storage] section is missing")?;
            Arc::new(
                ObjectStorageBlobStore::new(ObjectStorageConfig {
                    bucket: os_toml.bucket.clone(),
                    region: os_toml.region.clone(),
                    endpoint: os_toml.endpoint.clone(),
                    access_key: os_toml.access_key.clone(),
                    secret_key: os_toml.secret_key.clone(),
                    path_style: os_toml.path_style,
                    max_size: app_config.storage.max_blob_size,
                    temp_dir: os_toml
                        .temp_dir
                        .as_ref()
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from),
                })
                .context("Failed to initialize object storage")?,
            )
        }
        "database" | _ => Arc::new(DatabaseBlobStore::new(
            db.clone(),
            app_config.storage.max_blob_size,
        )),
    };
    info!(
        "Blob storage initialized (backend: {})",
        app_config.storage.backend
    );

    let contest_type_registry = Arc::new(RwLock::new(HashMap::new()));
    let evaluator_registry = Arc::new(RwLock::new(HashMap::new()));
    let checker_format_registry = Arc::new(RwLock::new(HashMap::new()));
    let operation_batches = Arc::new(DashMap::new());
    let operation_waiters = Arc::new(DashMap::new());
    let evaluate_batches = Arc::new(DashMap::new());

    let batch_max_age = Duration::from_secs(app_config.batch_max_age_secs);
    registry::spawn_batch_reaper("operation", operation_batches.clone(), batch_max_age);
    registry::spawn_batch_reaper("evaluate", evaluate_batches.clone(), batch_max_age);

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
        evaluate_batches.clone(),
        app_config.clone(),
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
            operation_batches: operation_batches.clone(),
            operation_waiters: operation_waiters.clone(),
            evaluate_batches: evaluate_batches.clone(),
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
