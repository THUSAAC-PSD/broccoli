use std::collections::HashMap;
use std::path::PathBuf;

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
use common::language::LanguageDefinition;
use common::storage::config::create_blob_store;
use common::worker::*;
use futures::future::join_all;
use tracing::{error, info, warn};

/// Executor for running operations with isolated sandboxes
pub struct OperationTaskExecutor {
    operation_executor: OperationHandler,
}

impl OperationTaskExecutor {
    pub async fn from_config() -> Self {
        let config = WorkerAppConfig::load()
            .inspect_err(|e| warn!(error = %e, "Failed to load config, using defaults"))
            .ok();

        let languages = config
            .as_ref()
            .map(|c| c.languages.clone())
            .unwrap_or_default();
        let fingerprint = Self::probe_toolchain_fingerprint(&languages).await;
        info!(fingerprint = %fingerprint, "Toolchain fingerprint computed");

        let sandbox_manager = Self::sandbox_manager_from_config(config.as_ref());
        let (file_cacher, task_cache) = Self::caching_from_config(config.as_ref()).await;

        Self {
            operation_executor: OperationHandler::new(
                sandbox_manager,
                file_cacher,
                task_cache,
                fingerprint,
            ),
        }
    }

    /// Create with a specific sandbox manager (uses NoopFileCacher and NoopTaskCacheStore; for tests).
    pub fn new_with_sandbox_manager(
        sandbox_manager: Box<dyn SandboxManager + Send + Sync>,
    ) -> Self {
        Self {
            operation_executor: OperationHandler::new(
                sandbox_manager,
                Box::new(NoopFileCacher),
                Box::new(NoopTaskCacheStore),
                String::new(),
            ),
        }
    }

    /// Probe installed toolchain versions and return a deterministic fingerprint.
    ///
    /// For each language with a `version_cmd`, runs the command, captures the first
    /// non-empty output line, and incorporates it into a SHA-256 hash. Returns the
    /// hex fingerprint, or `""` if all probes fail or no `version_cmd` is configured.
    async fn probe_toolchain_fingerprint(
        languages: &HashMap<String, LanguageDefinition>,
    ) -> String {
        use sha2::{Digest, Sha256};

        // Collect (lang_id, version_cmd) pairs sorted for determinism.
        let mut probes: Vec<(String, Vec<String>)> = languages
            .iter()
            .filter_map(|(lang_id, def)| {
                def.version_cmd
                    .as_ref()
                    .map(|cmd| (lang_id.clone(), cmd.clone()))
            })
            .collect();
        probes.sort_by(|a, b| a.0.cmp(&b.0));

        let futures: Vec<_> = probes
            .into_iter()
            .map(|(lang_id, cmd)| async move {
                let version = run_version_probe(&cmd).await;
                (lang_id, version)
            })
            .collect();

        // join_all preserves the order of `futures`, which was built from the
        // pre-sorted `probes` vec — so `entries` is already in sorted order.
        let entries: Vec<String> = join_all(futures)
            .await
            .into_iter()
            .filter_map(|(lang_id, version)| version.map(|v| format!("{}:{}", lang_id, v)))
            .collect();

        if entries.is_empty() {
            return String::new();
        }

        let joined = entries.join("\n");
        let hash = Sha256::digest(joined.as_bytes());
        hex::encode(hash)
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

        let database_url = config
            .map(|c| c.database.url.clone())
            .unwrap_or_else(|| "postgres://localhost/broccoli".into());

        let db = match sea_orm::Database::connect(&database_url).await {
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

impl Default for OperationTaskExecutor {
    fn default() -> Self {
        // Sync default uses NoopFileCacher, NoopTaskCacheStore, and empty fingerprint.
        //
        // For production, use `from_config().await` which probes toolchain versions.
        let config = WorkerAppConfig::load()
            .inspect_err(|e| warn!(error = %e, "Failed to load config, using defaults"))
            .ok();
        Self {
            operation_executor: OperationHandler::new(
                Self::sandbox_manager_from_config(config.as_ref()),
                Box::new(NoopFileCacher),
                Box::new(NoopTaskCacheStore),
                String::new(),
            ),
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
        match self.operation_executor.execute(&operation).await {
            Ok(result) => Ok(TaskResult {
                task_id: task.id,
                success: result.success,
                output: serde_json::to_value(result)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize result: {}", e))?,
            }),
            Err(e) => Ok(TaskResult {
                task_id: task.id,
                success: false,
                output: serde_json::json!({ "error": format!("{e:#}") }),
            }),
        }
    }
}

/// Run a version probe command and return the first non-empty trimmed output line.
///
/// Merges stdout and stderr. Returns `None` on I/O error, timeout, or empty output.
/// A 5-second timeout prevents a hung binary (e.g. on a network filesystem) from
/// blocking worker startup indefinitely.
async fn run_version_probe(cmd: &[String]) -> Option<String> {
    use std::process::Stdio;
    use std::time::Duration;

    if cmd.is_empty() {
        return None;
    }

    let probe = tokio::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = tokio::time::timeout(Duration::from_secs(5), probe)
        .await
        .ok()? // timeout → None
        .ok()?; // IO error → None

    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));

    combined
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}
