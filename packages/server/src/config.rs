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
    20
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
    /// Multi-replica deployments require a stable explicit server ID; when
    /// true, hostname/random fallbacks are rejected.
    #[serde(default)]
    pub expects_multi_replica: bool,
    /// Master switch for lease refresh, steal scanning, and reply-queue
    /// sweeping. Defaults off for opt-in rollout.
    #[serde(default)]
    pub dispatcher_lease_steal_enabled: bool,
    /// Admission-control switch for plugin-dispatch background tasks.
    #[serde(default = "default_dispatcher_semaphore_enabled")]
    pub dispatcher_semaphore_enabled: bool,
    #[serde(default = "default_dispatcher_concurrency")]
    pub dispatcher_concurrency: u32,
    /// Per-server cap on dispatch tasks waiting for a semaphore permit.
    /// Multi-replica deployments get an aggregate cap of roughly
    /// `max_queued_submissions * live_server_replicas`.
    #[serde(default = "default_max_queued_submissions")]
    pub max_queued_submissions: u32,
    #[serde(default = "default_lease_ttl_secs")]
    pub lease_ttl_secs: u64,
    #[serde(default = "default_lease_refresh_interval_secs")]
    pub lease_refresh_interval_secs: u64,
    #[serde(default = "default_steal_scan_interval_secs")]
    pub steal_scan_interval_secs: u64,
    #[serde(default = "default_steal_batch_size")]
    pub steal_batch_size: u32,
    #[serde(default = "default_sweep_interval_secs")]
    pub sweep_interval_secs: u64,
    #[serde(default = "default_max_dispatch_retries")]
    pub max_dispatch_retries: u32,
    /// Initially dry-run only: log ghost reply queues and debounce them, but
    /// do not delete Redis keys until operators explicitly enable deletion.
    #[serde(default = "default_sweeper_dry_run")]
    pub sweeper_dry_run: bool,
    /// Master switch for Redis-backed operation cancellation keys. Defaults
    /// off so worker-side cancellation checks can soak independently.
    #[serde(default)]
    pub cancel_primitive_enabled: bool,
    /// Admission switch for sizing evaluator slots from live worker
    /// heartbeats instead of local CPU count.
    #[serde(default)]
    pub fleet_aware_admission_enabled: bool,
    #[serde(default = "default_fleet_capacity_poll_interval_secs")]
    pub fleet_capacity_poll_interval_secs: u64,
}

fn default_dispatcher_semaphore_enabled() -> bool {
    true
}

fn default_dispatcher_concurrency() -> u32 {
    16
}

fn default_max_queued_submissions() -> u32 {
    100
}

fn default_lease_ttl_secs() -> u64 {
    60
}

fn default_lease_refresh_interval_secs() -> u64 {
    10
}

fn default_steal_scan_interval_secs() -> u64 {
    15
}

fn default_steal_batch_size() -> u32 {
    8
}

fn default_sweep_interval_secs() -> u64 {
    300
}

fn default_max_dispatch_retries() -> u32 {
    5
}

fn default_sweeper_dry_run() -> bool {
    true
}

fn default_fleet_capacity_poll_interval_secs() -> u64 {
    5
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
    7200
}

/// Returns true if `id` is a safe queue-suffix (alphanumeric + `-_.`,
/// non-empty, ≤128 chars). Mirrors the worker-id rules so the same
/// validation applies to both sides of the MQ envelope.
pub fn is_valid_server_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.ends_with("_processing")
        && !id.ends_with("_failed")
        && !id.ends_with("_fairness_set")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ServerIdResolveError {
    #[error(
        "server.id must be explicitly set to a valid value when server.expects_multi_replica is true"
    )]
    MultiReplicaServerIdRequired,
}

/// Resolves the effective server ID from a configured value:
/// 1. If `configured` is non-empty and valid, use it.
/// 2. If multi-replica mode is expected, reject missing/invalid configured IDs.
/// 3. Else fall back to the OS hostname (sanitized — Windows hostnames may
///    contain characters Redis dislikes in queue names).
/// 4. Else generate an 8-char random ID and warn.
pub fn resolve_server_id(
    configured: &str,
    expects_multi_replica: bool,
) -> Result<String, ServerIdResolveError> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        if is_valid_server_id(trimmed) {
            info!(server_id = %trimmed, "Server ID resolved from explicit configuration");
            return Ok(trimmed.to_string());
        }
        if expects_multi_replica {
            return Err(ServerIdResolveError::MultiReplicaServerIdRequired);
        }
        warn!(
            configured = %trimmed,
            "Configured server.id failed validation; falling back to hostname"
        );
    } else if expects_multi_replica {
        return Err(ServerIdResolveError::MultiReplicaServerIdRequired);
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
            return Ok(sanitized);
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
    Ok(fallback)
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
            .set_default("server.expects_multi_replica", false)?
            .set_default("server.dispatcher_lease_steal_enabled", false)?
            .set_default("server.dispatcher_semaphore_enabled", true)?
            .set_default("server.dispatcher_concurrency", 16_i64)?
            .set_default("server.max_queued_submissions", 100_i64)?
            .set_default("server.lease_ttl_secs", 60_i64)?
            .set_default("server.lease_refresh_interval_secs", 10_i64)?
            .set_default("server.steal_scan_interval_secs", 15_i64)?
            .set_default("server.steal_batch_size", 8_i64)?
            .set_default("server.sweep_interval_secs", 300_i64)?
            .set_default("server.max_dispatch_retries", 5_i64)?
            .set_default("server.sweeper_dry_run", true)?
            .set_default("server.cancel_primitive_enabled", false)?
            .set_default("server.fleet_aware_admission_enabled", false)?
            .set_default("server.fleet_capacity_poll_interval_secs", 5_i64)?
            .set_default("server.trusted_proxies", Vec::<String>::new())?
            .set_default("server.rate_limit_auth", false)?
            .set_default(
                "database.url",
                "postgres://postgres:password@localhost:5432/broccoli",
            )?
            .set_default("database.max_connections", 20_i64)?
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
        assert!(!is_valid_server_id("server_processing"));
        assert!(!is_valid_server_id("server_failed"));
        assert!(!is_valid_server_id("server_fairness_set"));
        assert!(!is_valid_server_id(&"x".repeat(129)));
    }

    #[test]
    fn resolves_explicit_server_id_when_valid() {
        assert_eq!(resolve_server_id("alpha", false).unwrap(), "alpha");
        assert_eq!(resolve_server_id("  alpha  ", false).unwrap(), "alpha");
    }

    #[test]
    fn resolves_invalid_explicit_id_via_fallback() {
        // "has space" is invalid; we expect a non-empty fallback (hostname or random).
        let resolved = resolve_server_id("has space", false).unwrap();
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn resolves_empty_to_hostname_or_random_fallback() {
        let resolved = resolve_server_id("", false).unwrap();
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn resolves_whitespace_only_to_fallback() {
        // Whitespace-only configured value should behave like empty:
        // produce a non-empty, well-formed ID via the hostname/random path.
        let resolved = resolve_server_id("   ", false).unwrap();
        assert!(!resolved.is_empty());
        assert!(is_valid_server_id(&resolved));
    }

    #[test]
    fn multi_replica_server_id_requires_explicit_valid_id() {
        assert!(resolve_server_id("", true).is_err());
        assert!(resolve_server_id("   ", true).is_err());
        assert!(resolve_server_id("has space", true).is_err());
        assert_eq!(resolve_server_id("server-1", true).unwrap(), "server-1");
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
