use std::path::PathBuf;
use std::time::Duration;

use super::file_cacher::{BlobStoreFileCacher, FileCacher, NoopFileCacher};
use super::models::OperationTask;
use super::sandbox::SandboxManager;
use super::sandbox::isolate::IsolateSandboxManager;
use super::sandbox::mock::MockSandboxManager;
use super::task_cache::{DatabaseTaskCacheStore, NoopTaskCacheStore, TaskCacheStore};
use crate::config::WorkerAppConfig;
use crate::models::operation::handler::OperationHandler;
use anyhow::Result;
use async_trait::async_trait;
use common::storage::config::create_blob_store;
use common::worker::*;
use tracing::{error, info, warn};

pub struct OperationTaskExecutor {
    operation_executor: OperationHandler,
}

impl OperationTaskExecutor {
    pub async fn from_config(metrics: common::metrics::Metrics) -> Self {
        let config = WorkerAppConfig::load()
            .inspect_err(|e| warn!(error = %e, "Failed to load config, using defaults"))
            .ok();

        let fingerprint = String::new();

        let sandbox_manager = Self::sandbox_manager_from_config(config.as_ref());
        let (file_cacher, task_cache) = Self::caching_from_config(config.as_ref()).await;

        Self {
            operation_executor: OperationHandler::new(
                sandbox_manager,
                file_cacher,
                task_cache,
                fingerprint,
                metrics,
            ),
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
            Box::new(IsolateSandboxManager::new(isolate_bin, enable_cgroups))
        }
    }

    async fn caching_from_config(
        config: Option<&WorkerAppConfig>,
    ) -> (Box<dyn FileCacher>, Box<dyn TaskCacheStore>) {
        let noop = || -> (Box<dyn FileCacher>, Box<dyn TaskCacheStore>) {
            (Box::new(NoopFileCacher), Box::new(NoopTaskCacheStore))
        };

        let storage_config = config.map(|c| &c.storage);
        let blob_store_config = storage_config
            .map(|s| s.blob_store.clone())
            .unwrap_or_default();
        let cache_dir = storage_config
            .map(|s| s.cache_dir.clone())
            .unwrap_or_else(|| "./data/cache".into());
        let max_cache_size = storage_config
            .map(|s| s.max_cache_size)
            .unwrap_or(512 * 1024 * 1024);

        let database_config = config.map(|c| c.database.clone()).unwrap_or_default();
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
                    "Failed to connect to database, falling back to Noop cachers"
                );
                return noop();
            }
        };

        let db_for_cache = db.clone();

        let blob_store = match create_blob_store(&blob_store_config, db).await {
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
                error!(error = %e, "Failed to initialize blob store, falling back to Noop cachers");
                return noop();
            }
        };

        let file_cacher: Box<dyn FileCacher> =
            match BlobStoreFileCacher::new(blob_store, PathBuf::from(&cache_dir), max_cache_size)
                .await
            {
                Ok(cacher) => Box::new(cacher),
                Err(e) => {
                    error!(
                        error = %e,
                        "Failed to initialize BlobStoreFileCacher, falling back to Noop cachers"
                    );
                    return noop();
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

        (file_cacher, task_cache)
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
            }),
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": format!("{e:#}") }),
                error: Some(format!("{e:#}")),
            }),
        }
    }
}
