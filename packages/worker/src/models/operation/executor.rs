use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::file_cacher::{BlobStoreFileCacher, FileCacher, NoopFileCacher};
use super::models::OperationTask;
use super::sandbox::SandboxManager;
use super::sandbox::isolate::IsolateSandboxManager;
use super::sandbox::mock::MockSandboxManager;
use crate::config::WorkerAppConfig;
use crate::models::operation::handler::OperationHandler;
use anyhow::Result;
use async_trait::async_trait;
use common::storage::database::DatabaseBlobStore;
use common::worker::*;
use tracing::{error, info, warn};

/// Executor for running operations with isolated sandboxes
pub struct OperationTaskExecutor {
    operation_executor: Mutex<OperationHandler>,
}

impl OperationTaskExecutor {
    /// Create from config, initializing DatabaseBlobStore + BlobStoreFileCacher.
    pub async fn from_config() -> Self {
        let config = WorkerAppConfig::load().ok();

        let sandbox_manager = Self::sandbox_manager_from_config(config.as_ref());
        let file_cacher = Self::file_cacher_from_config(config.as_ref()).await;

        Self {
            operation_executor: Mutex::new(OperationHandler::new(sandbox_manager, file_cacher)),
        }
    }

    /// Create with a specific sandbox manager (uses NoopFileCacher — for tests).
    pub fn new_with_sandbox_manager(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
    ) -> Self {
        Self {
            operation_executor: Mutex::new(OperationHandler::new(
                sandbox_manager,
                Box::new(NoopFileCacher),
            )),
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
            Box::new(IsolateSandboxManager)
        }
    }

    async fn file_cacher_from_config(config: Option<&WorkerAppConfig>) -> Box<dyn FileCacher> {
        let storage_config = config.map(|c| &c.storage);

        let database_url = config
            .map(|c| c.database.url.clone())
            .unwrap_or_else(|| "postgres://localhost/broccoli".into());
        let cache_dir = storage_config
            .map(|s| s.cache_dir.clone())
            .unwrap_or_else(|| "./data/cache".into());
        let max_cache_size = storage_config
            .map(|s| s.max_cache_size)
            .unwrap_or(512 * 1024 * 1024);

        // Connect to database for DatabaseBlobStore.
        let db = match sea_orm::Database::connect(&database_url).await {
            Ok(db) => db,
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to connect to database for blob store, falling back to NoopFileCacher"
                );
                return Box::new(NoopFileCacher) as Box<dyn FileCacher>;
            }
        };

        // Ensure blob_data table exists.
        if let Err(e) = DatabaseBlobStore::ensure_table(&db).await {
            error!(error = %e, "Failed to ensure blob_data table");
            return Box::new(NoopFileCacher) as Box<dyn FileCacher>;
        }

        let blob_store = Arc::new(DatabaseBlobStore::new(db, 128 * 1024 * 1024));

        match BlobStoreFileCacher::new(blob_store, PathBuf::from(&cache_dir), max_cache_size).await
        {
            Ok(cacher) => {
                info!(
                    database_url = %database_url,
                    cache_dir = %cache_dir,
                    max_cache_size,
                    "DatabaseBlobStore + BlobStoreFileCacher initialized"
                );
                Box::new(cacher) as Box<dyn FileCacher>
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to initialize BlobStoreFileCacher, falling back to NoopFileCacher"
                );
                Box::new(NoopFileCacher) as Box<dyn FileCacher>
            }
        }
    }
}

impl Default for OperationTaskExecutor {
    fn default() -> Self {
        // Sync default uses NoopFileCacher. For production, use `from_config().await`.
        let config = WorkerAppConfig::load().ok();
        Self {
            operation_executor: Mutex::new(OperationHandler::new(
                Self::sandbox_manager_from_config(config.as_ref()),
                Box::new(NoopFileCacher),
            )),
        }
    }
}

#[async_trait]
impl Executor for OperationTaskExecutor {
    fn if_accept(&self, task_type: &str) -> bool {
        task_type == "operation"
    }
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        // Deserialize the payload into an Operation
        let operation: OperationTask = serde_json::from_value(task.payload.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialize operation config: {}", e))?;

        // Execute the operation
        let mut operation_executor = self.operation_executor.lock().await;
        match operation_executor.execute(&operation).await {
            Ok(result) => Ok(TaskResult {
                task_id: task.id,
                success: result.success,
                output: serde_json::to_value(result)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))?,
            }),
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }
}
