use std::path::PathBuf;

use config::{Config, ConfigError, Environment, File};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use tracing::{info, warn};

pub use common::config::MqAppConfig;
pub use common::storage::config::BlobStoreConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_database_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://postgres:password@localhost:5432/broccoli".into(),
            max_connections: default_database_max_connections(),
        }
    }
}

fn default_database_max_connections() -> u32 {
    100
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
    /// Directory containing the baked frontend `dist/` output served by the
    /// server in production.
    #[serde(default = "default_frontend_dist")]
    pub frontend_dist: PathBuf,
    /// CIDR ranges for trusted L7 proxies. Empty means no proxy headers are
    /// trusted and client IP extraction falls back to the socket address.
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    pub trusted_proxies: Vec<String>,
    /// Enables IP-based throttling on `/api/v1/auth/login`.
    #[serde(default)]
    pub rate_limit_auth: bool,
    /// Logical identity of this replica. Used to derive the per-replica
    /// operation-result queue name so multiple servers behind a load balancer
    /// each receive their own plugin-dispatch results. Empty (the default)
    /// resolves to the OS hostname; an unusable hostname falls back to a
    /// random short ID. See [`resolve_server_id`].
    #[serde(default)]
    pub id: String,
}

fn default_frontend_dist() -> PathBuf {
    PathBuf::from("/srv/dist")
}

fn parse_string_vec(value: &str) -> Result<Vec<String>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(Vec::new());
    }

    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<String>>(trimmed)
            .map_err(|err| format!("invalid JSON string array: {err}"));
    }

    Ok(trimmed
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringVecVisitor;

    impl<'de> Visitor<'de> for StringVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string, comma-separated string, JSON string array, or sequence")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_string_vec(value).map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringVecVisitor)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    #[serde(default = "default_secure_cookies")]
    pub secure_cookies: bool,
}

fn default_secure_cookies() -> bool {
    true
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct BootstrapConfig {
    #[serde(default)]
    pub admin_username: String,
    #[serde(default)]
    pub admin_password: String,
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
    #[serde(default)]
    pub bootstrap: BootstrapConfig,
}

fn default_batch_max_age_secs() -> u64 {
    600
}

/// Returns true if `id` is a safe queue-suffix (alphanumeric + `-_.`,
/// non-empty, ≤128 chars). Mirrors the worker-id rules so the same
/// validation applies to both sides of the MQ envelope.
pub fn is_valid_server_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Resolves the effective server ID from a configured value:
/// 1. If `configured` is non-empty and valid, use it.
/// 2. Else fall back to the OS hostname (sanitized — Windows hostnames may
///    contain characters Redis dislikes in queue names).
/// 3. Else generate an 8-char random ID and warn. Refusing to start would
///    be too aggressive — operators can set `BROCCOLI__SERVER__ID` explicitly
///    if they care about stable identity.
pub fn resolve_server_id(configured: &str) -> String {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        if is_valid_server_id(trimmed) {
            info!(server_id = %trimmed, "Server ID resolved from explicit configuration");
            return trimmed.to_string();
        }
        warn!(
            configured = %trimmed,
            "Configured server.id failed validation; falling back to hostname"
        );
    }

    if let Ok(host) = hostname::get() {
        let lossy = host.to_string_lossy();
        let sanitized: String = lossy
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .take(128)
            .collect();
        if is_valid_server_id(&sanitized) {
            warn!(
                server_id = %sanitized,
                "Server ID not configured; inferred from hostname. \
                 Set BROCCOLI__SERVER__ID explicitly in multi-replica deployments \
                 to avoid silent collisions between replicas with identical hostnames."
            );
            return sanitized;
        }
        warn!(
            hostname = %lossy,
            "OS hostname unsuitable as server.id; using random fallback"
        );
    } else {
        warn!("Could not read OS hostname; using random server.id fallback");
    }

    let fallback: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    warn!(
        server_id = %fallback,
        "Server ID generated as random fallback because hostname was unobtainable or invalid. \
         This ID changes every restart — set BROCCOLI__SERVER__ID explicitly so in-flight \
         operation results route correctly across restarts."
    );
    fallback
}

/// Centralized derivation of the per-replica operation-result queue name.
/// Suffixing with `server_id` ensures each replica's `consume_operation_results`
/// only receives results for tasks it dispatched.
pub fn per_replica_result_queue_name(base: &str, server_id: &str) -> String {
    format!("{}.{}", base, server_id)
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3000)?
            .set_default("server.cors.allow_origins", Vec::<String>::new())?
            .set_default("server.cors.max_age", 3600_i64)?
            .set_default("server.frontend_dist", "/srv/dist")?
            .set_default("server.id", "")?
            .set_default("server.trusted_proxies", Vec::<String>::new())?
            .set_default("server.rate_limit_auth", false)?
            .set_default(
                "database.url",
                "postgres://postgres:password@localhost:5432/broccoli",
            )?
            .set_default("database.max_connections", 100_i64)?
            .set_default("bootstrap.admin_username", "")?
            .set_default("bootstrap.admin_password", "")?
            .set_default("auth.secure_cookies", true)?
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
            .add_source(
                Environment::with_prefix("BROCCOLI")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        s.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_server_id_charset() {
        assert!(is_valid_server_id("alpha"));
        assert!(is_valid_server_id("server-01.east_2"));
        assert!(!is_valid_server_id(""));
        assert!(!is_valid_server_id("has space"));
        assert!(!is_valid_server_id("colon:bad"));
        assert!(!is_valid_server_id(&"x".repeat(129)));
    }

    #[test]
    fn resolves_explicit_server_id_when_valid() {
        assert_eq!(resolve_server_id("alpha"), "alpha");
        assert_eq!(resolve_server_id("  alpha  "), "alpha");
    }

    #[test]
    fn resolves_invalid_explicit_id_via_fallback() {
        // "has space" is invalid; we expect a non-empty fallback (hostname or random).
        let resolved = resolve_server_id("has space");
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn resolves_empty_to_hostname_or_random_fallback() {
        let resolved = resolve_server_id("");
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn resolves_whitespace_only_to_fallback() {
        // Whitespace-only configured value should behave like empty:
        // produce a non-empty, well-formed ID via the hostname/random path.
        let resolved = resolve_server_id("   ");
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn per_replica_queue_name_appends_dotted_suffix() {
        assert_eq!(
            per_replica_result_queue_name("operation_results", "alpha"),
            "operation_results.alpha"
        );
        assert_eq!(
            per_replica_result_queue_name("operation_results", "server-1"),
            "operation_results.server-1"
        );
    }

    #[derive(Debug, Deserialize)]
    struct TrustedProxyProbe {
        #[serde(default, deserialize_with = "deserialize_string_vec")]
        trusted_proxies: Vec<String>,
    }

    #[test]
    fn trusted_proxy_env_style_string_accepts_empty_json_array() {
        let probe: TrustedProxyProbe =
            serde_json::from_value(serde_json::json!({ "trusted_proxies": "[]" })).unwrap();
        assert!(probe.trusted_proxies.is_empty());
    }

    #[test]
    fn trusted_proxy_env_style_string_accepts_json_array() {
        let probe: TrustedProxyProbe = serde_json::from_value(serde_json::json!({
            "trusted_proxies": "[\"10.0.0.0/8\", \"192.168.0.0/16\"]"
        }))
        .unwrap();
        assert_eq!(probe.trusted_proxies, vec!["10.0.0.0/8", "192.168.0.0/16"]);
    }

    #[test]
    fn trusted_proxy_env_style_string_accepts_comma_list() {
        let probe: TrustedProxyProbe = serde_json::from_value(serde_json::json!({
            "trusted_proxies": "10.0.0.0/8, 192.168.0.0/16"
        }))
        .unwrap();
        assert_eq!(probe.trusted_proxies, vec!["10.0.0.0/8", "192.168.0.0/16"]);
    }

    #[test]
    fn trusted_proxy_toml_style_sequence_still_works() {
        let probe: TrustedProxyProbe = serde_json::from_value(serde_json::json!({
            "trusted_proxies": ["10.0.0.0/8"]
        }))
        .unwrap();
        assert_eq!(probe.trusted_proxies, vec!["10.0.0.0/8"]);
    }
}
