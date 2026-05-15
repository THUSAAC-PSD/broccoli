use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::cache_leader::{
    CacheLeaderElector, CacheLeaderOpts, NoopCacheLeaderElector, RedisCacheLeaderElector,
};
use super::file_cacher::{BlobStoreFileCacher, FileCacher, NoopFileCacher};
use super::models::OperationTask;
use super::sandbox::SandboxManager;
use super::sandbox::isolate::IsolateSandboxManager;
use super::sandbox::mock::MockSandboxManager;
use super::task_cache::{DatabaseTaskCacheStore, NoopTaskCacheStore, TaskCacheStore};
use crate::config::WorkerAppConfig;
use crate::models::operation::handler::OperationHandler;
use crate::toolchain_fingerprint;
use anyhow::Result;
use async_trait::async_trait;
use common::storage::config::create_blob_store;
use common::worker::*;
use tracing::{error, info, warn};

pub struct OperationTaskExecutor {
    operation_executor: OperationHandler,
}

impl OperationTaskExecutor {
    pub async fn from_app_config(
        config: &WorkerAppConfig,
        metrics: common::metrics::Metrics,
    ) -> Result<Self> {
        let fingerprint =
            toolchain_fingerprint::compute(toolchain_fingerprint::default_probes()).await;
        info!(
            worker_toolchain_fingerprint = %fingerprint,
            "Computed toolchain fingerprint for compile cache key"
        );
        let sandbox_manager =
            Self::sandbox_manager_from_config(Some(config), Some(metrics.clone()));
        let (file_cacher, task_cache) = Self::caching_from_config(config, metrics.clone()).await?;
        let cache_leader = Self::cache_leader_from_config(config);
        let follower_poll = Duration::from_millis(config.worker.cache_follower_poll_interval_ms);
        let follower_max_wait = Duration::from_secs(config.worker.cache_follower_max_wait_secs);

        Ok(Self {
            operation_executor: OperationHandler::with_cache_leader(
                sandbox_manager,
                file_cacher,
                Arc::from(task_cache),
                cache_leader,
                follower_poll,
                follower_max_wait,
                fingerprint,
                metrics,
            ),
        })
    }

    fn cache_leader_from_config(config: &WorkerAppConfig) -> Arc<dyn CacheLeaderElector> {
        if !config.worker.cache_leader_election_enabled || !config.mq.enabled {
            info!("cache-leader-election disabled, using NoopCacheLeaderElector");
            return Arc::new(NoopCacheLeaderElector);
        }
        let opts = CacheLeaderOpts {
            ttl_secs: config.worker.cache_leader_ttl_secs,
            heartbeat_interval_secs: config.worker.cache_leader_heartbeat_interval_secs,
        };
        match RedisCacheLeaderElector::from_url(&config.mq.url, opts) {
            Ok(elector) => {
                info!(
                    redis_url = %config.mq.url,
                    ttl_secs = opts.ttl_secs,
                    heartbeat_interval_secs = opts.heartbeat_interval_secs,
                    "RedisCacheLeaderElector initialized"
                );
                Arc::new(elector)
            }
            Err(e) => {
                warn!(
                    redis_url = %config.mq.url,
                    error = %e,
                    "Failed to build RedisCacheLeaderElector, falling back to no-op"
                );
                Arc::new(NoopCacheLeaderElector)
            }
        }
    }

    #[allow(dead_code)]
    pub fn new_with_sandbox_manager(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
        metrics: common::metrics::Metrics,
    ) -> Self {
        Self {
            operation_executor: OperationHandler::new(
                sandbox_manager,
                Box::new(NoopFileCacher),
                Box::new(NoopTaskCacheStore),
                String::new(),
                metrics,
            ),
        }
    }

    fn sandbox_manager_from_config(
        config: Option<&WorkerAppConfig>,
        metrics: Option<common::metrics::Metrics>,
    ) -> Box<dyn SandboxManager + Send + Sync> {
        let backend = config
            .map(|c| c.worker.sandbox_backend.clone())
            .unwrap_or_else(|| {
                warn!("No config available, fallback to isolate sandbox");
                "isolate".to_string()
            });

        if backend.eq_ignore_ascii_case("mock") {
            info!(sandbox_backend = "mock", "Using operation sandbox backend");
            Box::new(MockSandboxManager::default())
        } else {
            if !backend.eq_ignore_ascii_case("isolate") {
                warn!(sandbox_backend = %backend, "Unknown sandbox backend, fallback to isolate");
            }
            info!(
                sandbox_backend = "isolate",
                "Using operation sandbox backend"
            );
            let isolate_bin = config
                .map(|c| c.worker.isolate_bin.clone())
                .unwrap_or_else(|| "isolate".to_string());
            let enable_cgroups = config.map(|c| c.worker.enable_cgroups).unwrap_or(false);
            Box::new(IsolateSandboxManager::new_with_metrics(
                isolate_bin,
                enable_cgroups,
                metrics,
            ))
        }
    }

    async fn caching_from_config(
        config: &WorkerAppConfig,
        metrics: common::metrics::Metrics,
    ) -> Result<(Box<dyn FileCacher>, Box<dyn TaskCacheStore>)> {
        let blob_store_config = config.storage.blob_store.clone();
        let cache_dir = config.storage.cache_dir.clone();
        let max_cache_size = config.storage.max_cache_size;

        let database_config = config.database.clone();
        let mut connect_options = sea_orm::ConnectOptions::new(database_config.url.clone());
        connect_options
            .max_connections(database_config.max_connections)
            .min_connections(database_config.max_connections.min(1))
            .connect_timeout(Duration::from_secs(30))
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800));

        let db = match sea_orm::Database::connect(connect_options).await {
            Ok(db) => db,
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to connect to database"
                );
                return Err(e.into());
            }
        };

        let db_for_cache = db.clone();

        let blob_store =
            match create_blob_store(&blob_store_config, db, Some(metrics.clone())).await {
                Ok(store) => {
                    info!(
                        backend = %blob_store_config.backend,
                        cache_dir = %cache_dir,
                        max_cache_size,
                        "Blob store initialized"
                    );
                    store
                }
                Err(e) => {
                    error!(error = %e, "Failed to initialize blob store");
                    return Err(e.into());
                }
            };

        let file_cacher: Box<dyn FileCacher> = match BlobStoreFileCacher::new(
            blob_store,
            PathBuf::from(&cache_dir),
            max_cache_size,
            Some(metrics.clone()),
        )
        .await
        {
            Ok(cacher) => Box::new(cacher),
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to initialize BlobStoreFileCacher"
                );
                return Err(anyhow::anyhow!(e));
            }
        };

        let task_cache: Box<dyn TaskCacheStore> = match DatabaseTaskCacheStore::ensure_table(
            &db_for_cache,
        )
        .await
        {
            Ok(()) => {
                info!("DatabaseTaskCacheStore initialized");
                Box::new(DatabaseTaskCacheStore::new(db_for_cache))
            }
            Err(e) => {
                warn!(error = %e, "Failed to ensure task_cache table, using NoopTaskCacheStore");
                Box::new(NoopTaskCacheStore)
            }
        };

        Ok((file_cacher, task_cache))
    }
}

#[async_trait]
impl Executor for OperationTaskExecutor {
    fn if_accept(&self, task_type: &str) -> bool {
        task_type == "operation"
    }
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let operation: OperationTask = serde_json::from_value(task.payload.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialize operation config: {}", e))?;

        match self.operation_executor.execute(&operation).await {
            Ok(result) => Ok(TaskResult {
                task_id: task.id,
                success: result.success,
                output: serde_json::to_value(&result)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))?,
                error: if result.success {
                    None
                } else {
                    Some(
                        result
                            .error
                            .clone()
                            .unwrap_or_else(|| "Operation failed".into()),
                    )
                },
                task_type: None,
                operation: None,
                worker_id: None,
                enqueued_at_unix_ms: None,
            }),
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": format!("{e:#}") }),
                error: Some(format!("{e:#}")),
                task_type: None,
                operation: None,
                worker_id: None,
                enqueued_at_unix_ms: None,
            }),
        }
    }
}
