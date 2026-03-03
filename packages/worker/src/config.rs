use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

pub use common::config::MqAppConfig;

/// Worker-specific configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct WorkerConfig {
    /// Unique identifier for this worker instance. Default: "worker-1".
    #[serde(default = "default_worker_id")]
    pub id: String,
    /// Isolate executable path. Default: "isolate".
    #[serde(default = "default_isolate_bin")]
    pub isolate_bin: String,
    /// Enable control groups (cgroup) mode for isolate. Default: false.
    #[serde(default = "default_enable_cgroups")]
    pub enable_cgroups: bool,
    /// Sandbox backend for operation execution. Supported: "isolate", "mock".
    #[serde(default = "default_sandbox_backend")]
    pub sandbox_backend: String,
}

fn default_worker_id() -> String {
    "worker-1".into()
}
fn default_isolate_bin() -> String {
    "isolate".into()
}
fn default_enable_cgroups() -> bool {
    true
}
fn default_sandbox_backend() -> String {
    "isolate".into()
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            id: default_worker_id(),
            isolate_bin: default_isolate_bin(),
            enable_cgroups: default_enable_cgroups(),
            sandbox_backend: default_sandbox_backend(),
        }
    }
}

/// Storage configuration for database-backed blob store and local cache.
#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    /// PostgreSQL connection URL for DatabaseBlobStore.
    #[serde(default = "default_database_url")]
    pub database_url: String,
    /// Local directory for the file cache. Default: "./data/cache".
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// Maximum total cache size in bytes. Default: 512 MB.
    #[serde(default = "default_max_cache_size")]
    pub max_cache_size: u64,
}

fn default_database_url() -> String {
    "postgres://localhost/broccoli".into()
}
fn default_cache_dir() -> String {
    "./data/cache".into()
}
fn default_max_cache_size() -> u64 {
    512 * 1024 * 1024
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_url: default_database_url(),
            cache_dir: default_cache_dir(),
            max_cache_size: default_max_cache_size(),
        }
    }
}

/// Worker application configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct WorkerAppConfig {
    #[serde(default)]
    pub worker: WorkerConfig,
    #[serde(default)]
    pub mq: MqAppConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

impl WorkerAppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path =
            std::env::var("BROCCOLI_CONFIG").unwrap_or_else(|_| "config/config".to_string());

        let s = Config::builder()
            .set_default("worker.id", "worker-1")?
            .set_default("worker.batch_size", 4_i64)?
            .set_default("worker.isolate_bin", "isolate")?
            .set_default("worker.enable_cgroups", true)?
            .set_default("worker.sandbox_backend", "isolate")?
            .set_default("mq.enabled", true)?
            .set_default("mq.url", "redis://localhost:6379")?
            .set_default("mq.pool_size", 5_i64)?
            .set_default("mq.queue_name", "judge_jobs")?
            .set_default("mq.result_queue_name", "judge_results")?
            .set_default("storage.database_url", "postgres://localhost/broccoli")?
            .set_default("storage.cache_dir", "./data/cache")?
            .set_default("storage.max_cache_size", 512 * 1024 * 1024_i64)?
            .add_source(File::with_name(&config_path).required(false))
            .add_source(Environment::with_prefix("BROCCOLI").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
