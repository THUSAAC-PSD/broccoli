use std::collections::HashMap;

use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};

pub use common::config::MqAppConfig;

/// Database configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://postgres:password@localhost:5432/broccoli".into(),
        }
    }
}

/// Blob storage configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    /// Base directory for blob storage data.
    #[serde(default = "default_storage_data_dir")]
    pub data_dir: String,
    /// Maximum size per blob in bytes. Default: 128MB.
    #[serde(default = "default_storage_max_blob_size")]
    pub max_blob_size: u64,
}

fn default_storage_data_dir() -> String {
    "./data".into()
}

fn default_storage_max_blob_size() -> u64 {
    128 * 1024 * 1024 // 128MB
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_storage_data_dir(),
            max_blob_size: default_storage_max_blob_size(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CorsConfig {
    pub allow_origins: Vec<String>,
    pub max_age: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors: CorsConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubmissionConfig {
    /// Maximum total size of all files in a submission (in bytes).
    /// Default: 1MB (1048576 bytes).
    pub max_size: usize,
    /// Maximum submissions per user per minute. 0 = disabled.
    /// Default: 10.
    pub rate_limit_per_minute: u32,
}

impl Default for SubmissionConfig {
    fn default() -> Self {
        Self {
            max_size: 1_048_576,       // 1 MB
            rate_limit_per_minute: 10, // 10 per minute
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub plugin: plugin_core::config::PluginConfig,
    #[serde(default)]
    pub submission: SubmissionConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub mq: MqAppConfig,
    #[serde(default)]
    pub languages: HashMap<String, common::language::LanguageDefinition>,
    /// Maximum age for plugin batch operations in seconds before reaping. Default: 600 (10 min).
    #[serde(default = "default_batch_max_age_secs")]
    pub batch_max_age_secs: u64,
}

fn default_batch_max_age_secs() -> u64 {
    600
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3000)?
            .set_default("plugin.plugins_dir", "./plugins")?
            .set_default("plugin.enable_wasi", true)?
            .set_default("submission.max_size", 1_048_576_i64)?
            .set_default("submission.rate_limit_per_minute", 10_i64)?
            .set_default("mq.enabled", true)?
            .set_default("mq.url", "redis://localhost:6379")?
            .set_default("mq.pool_size", 5_i64)?
            .set_default("mq.queue_name", "judge_jobs")?
            .set_default("mq.result_queue_name", "judge_results")?
            .set_default("mq.operation_queue_name", "operation_tasks")?
            .set_default("mq.operation_result_queue_name", "operation_results")?
            // Load from config/config.toml
            .add_source(File::with_name("config/config").required(false))
            // Override from environment (e.g., BROCCOLI__AUTH__JWT_SECRET)
            .add_source(Environment::with_prefix("BROCCOLI").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
