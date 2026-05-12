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
use tracing::{info, warn};

use crate::build_router;
use crate::config::{AppConfig, per_replica_result_queue_name, resolve_server_id};
use crate::consumers::{consume_operation_dlq, consume_operation_results};
use crate::dlq::run_stuck_job_detector;
use crate::host_funcs::context::HostFunctionSystemDeps;
use crate::manager::ServerManager;
use crate::registry;
use crate::serve::serve_with_shutdown;
use crate::state::AppState;
use crate::utils::plugin::sync_plugins;

pub struct ServerRuntime {
    listener: tokio::net::TcpListener,
    app: axum::Router,
    addr: SocketAddr,
    _telemetry_guard: common::observability::TelemetryGuard,
}

impl ServerRuntime {
    pub async fn build(mut app_config: AppConfig) -> anyhow::Result<Self> {
        let telemetry_guard = common::observability::init_tracing(&app_config.observability);

        let server_id = resolve_server_id(&app_config.server.id);
        app_config.server.id = server_id.clone();
        let per_replica_op_result_queue =
            per_replica_result_queue_name(&app_config.mq.operation_result_queue_name, &server_id);
        app_config.mq.operation_result_queue_name = per_replica_op_result_queue.clone();
        info!(
            server_id = %server_id,
            result_queue = %per_replica_op_result_queue,
            "Resolved server identity"
        );

        let (metrics, prometheus_registry) =
            common::observability::init_metrics(&app_config.observability.otlp.service_name);

        let db = crate::database::init_db_with_max_connections(
            &app_config.database.url,
            app_config.database.max_connections,
        )
        .await?;
        let blob_store = create_blob_store(&app_config.storage, db.clone(), Some(metrics.clone()))
            .await
            .context("Failed to initialize blob storage")?;
        info!(
            "Blob storage initialized (backend: {})",
            app_config.storage.backend
        );

        crate::seed::seed_role_permissions(&db).await?;
        crate::seed::ensure_bootstrap_admin(
            &db,
            &app_config.bootstrap.admin_username,
            &app_config.bootstrap.admin_password,
        )
        .await?;
        crate::seed::ensure_indexes(&db).await?;
        crate::seed::backfill_submission_judgements(&db).await?;
        crate::seed::spawn_large_test_case_body_backfill(db.clone(), Arc::clone(&blob_store));

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

        let redis_client = if app_config.mq.enabled {
            match redis::Client::open(app_config.mq.url.as_str()) {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => {
                    warn!(
                        "Redis admin client init failed, /admin/system endpoints will degrade: {}",
                        e
                    );
                    None
                }
            }
        } else {
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
            Some(metrics.clone()),
            move |_batch_id, batch| {
                for key in batch.cleanup_keys.iter() {
                    operation_waiters_for_reaper.remove(key);

                    if let Some(ref mq) = reaper_mq {
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
            Some(metrics.clone()),
            |_batch_id, _batch| {},
        );

        if let Some(ref mq_arc) = mq {
            let op_consumer_mq = Arc::clone(mq_arc);
            let op_result_queue = app_config.mq.operation_result_queue_name.clone();
            let op_waiters = operation_waiters.clone();
            let op_result_metrics = metrics.clone();
            tokio::spawn(async move {
                consume_operation_results(
                    op_consumer_mq,
                    op_waiters,
                    op_result_queue,
                    op_result_metrics,
                )
                .await;
            });
            info!(
                queue = %app_config.mq.operation_result_queue_name,
                "Operation result consumer started (per-replica)"
            );
        }

        let manager = ServerManager::new(
            app_config.plugin.clone(),
            HostFunctionSystemDeps {
                db: db.clone(),
                mq: mq.clone(),
                operation_batches: operation_batches.clone(),
                operation_waiters: operation_waiters.clone(),
                contest_type_registry: contest_type_registry.clone(),
                evaluator_registry: evaluator_registry.clone(),
                checker_format_registry: checker_format_registry.clone(),
                language_resolver_registry: language_resolver_registry.clone(),
                evaluate_batches: evaluate_batches.clone(),
                blob_store: blob_store.clone(),
                config: app_config.clone(),
                metrics: Some(metrics.clone()),
            },
            Some(metrics.clone()),
        )
        .context("Failed to initialize plugin manager")?;

        let device_codes: crate::state::DeviceCodeStore = Arc::new(DashMap::new());

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

        {
            let cleanup_db = db.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(3600));
                loop {
                    interval.tick().await;
                    crate::middleware::idempotency::cleanup_expired_keys(&cleanup_db).await;
                }
            });
        }

        let state = AppState {
            plugins: manager,
            db: db.clone(),
            config: app_config.clone(),
            mq: mq.clone(),
            redis_client,
            blob_store,
            registries: crate::state::RegistryState {
                contest_type_registry: contest_type_registry.clone(),
                evaluator_registry: evaluator_registry.clone(),
                checker_format_registry: checker_format_registry.clone(),
                language_resolver_registry: language_resolver_registry.clone(),
                operation_batches: operation_batches.clone(),
                operation_waiters: operation_waiters.clone(),
                evaluate_batches: evaluate_batches.clone(),
                hook_registry: crate::hooks::new_shared_registry(),
            },
            device_codes,
            metrics,
            prometheus_registry,
        };

        crate::handlers::system::spawn_queue_depth_sampler(state.clone(), Duration::from_secs(5));

        let _failures = sync_plugins(&state).await?;

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
                    HeaderName::from_static("x-request-id"),
                    HeaderName::from_static("x-broccoli-stress-run-id"),
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

        Ok(Self {
            listener,
            app,
            addr,
            _telemetry_guard: telemetry_guard,
        })
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        serve_with_shutdown(self.listener, self.app)
            .await
            .with_context(|| format!("Server runtime error at http://{}", self.addr))
    }
}
