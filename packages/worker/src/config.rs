use crate::error::Result;
use config::{Config, Environment, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub worker: WorkerSettings,
    pub mq: MqSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSettings {
    pub id: String,
    pub batch_size: usize,
    pub poll_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqSettings {
    pub url: String,
    pub pool_size: u8,
    /// Queue to consume jobs from.
    pub job_queue: String,
    /// Queue to publish results to.
    pub result_queue: String,
}

impl WorkerConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_string_lossy().to_string();
        let config = Config::builder()
            .add_source(File::new(&path, FileFormat::Toml))
            .add_source(Environment::with_prefix("WORKER").separator("__"))
            .build()?;
        Ok(config.try_deserialize()?)
    }

    pub fn from_env() -> Result<Self> {
        let path =
            std::env::var("WORKER_CONFIG").unwrap_or_else(|_| "./config/worker.toml".to_string());
        Self::load(path)
    }
}
