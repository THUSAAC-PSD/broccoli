use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

pub use common::config::MqAppConfig;

/// Worker-specific configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct WorkerConfig {
    /// Unique identifier for this worker instance. Default: "worker-1".
    #[serde(default = "default_worker_id")]
    pub id: String,
    /// Number of jobs to fetch per batch. Default: 10.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Isolate executable path. Default: "isolate".
    #[serde(default = "default_isolate_bin")]
    pub isolate_bin: String,
}

fn default_worker_id() -> String {
    "worker-1".into()
}
fn default_batch_size() -> usize {
    10
}
fn default_isolate_bin() -> String {
    "isolate".into()
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            id: default_worker_id(),
            batch_size: default_batch_size(),
            isolate_bin: default_isolate_bin(),
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
}

impl WorkerAppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path =
            std::env::var("BROCCOLI_CONFIG").unwrap_or_else(|_| "config/config".to_string());

        let s = Config::builder()
            .set_default("worker.id", "worker-1")?
            .set_default("worker.batch_size", 10_i64)?
            .set_default("worker.isolate_bin", "isolate")?
            .set_default("mq.enabled", true)?
            .set_default("mq.url", "redis://localhost:6379")?
            .set_default("mq.pool_size", 5_i64)?
            .set_default("mq.queue_name", "judge_jobs")?
            .set_default("mq.result_queue_name", "judge_results")?
            .add_source(File::with_name(&config_path).required(false))
            .add_source(Environment::with_prefix("BROCCOLI").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
