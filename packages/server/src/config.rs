use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};

pub use common::config::MqAppConfig;
pub use common::storage::config::BlobStoreConfig;

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
    pub max_size: usize,
    pub rate_limit_per_minute: u32,
}

impl Default for SubmissionConfig {
    fn default() -> Self {
        Self {
            max_size: 1_048_576,
            rate_limit_per_minute: 10,
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
    pub storage: BlobStoreConfig,
    #[serde(default)]
    pub mq: MqAppConfig,
    #[serde(default)]
    pub observability: common::config::ObservabilityConfig,
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
            .set_default("server.cors.allow_origins", Vec::<String>::new())?
            .set_default("server.cors.max_age", 3600_i64)?
            .set_default("plugin.plugins_dir", "./plugins")?
            .set_default("plugin.enable_wasi", true)?
            .set_default("submission.max_size", 1_048_576_i64)?
            .set_default("submission.rate_limit_per_minute", 10_i64)?
            .set_default("mq.enabled", true)?
            .set_default("mq.url", "redis://localhost:6379")?
            .set_default("mq.pool_size", 5_i64)?
            .set_default("mq.operation_queue_name", "operation_tasks")?
            .set_default("mq.operation_result_queue_name", "operation_results")?
            .set_default("mq.operation_dlq_queue_name", "operation_tasks_dlq")?
            .set_default("observability.log_format", "pretty")?
            .set_default("observability.log_filter", "info")?
            .set_default("observability.otlp.service_name", "broccoli-server")?
            .add_source(File::with_name("config/config").required(false))
            .add_source(Environment::with_prefix("BROCCOLI").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
