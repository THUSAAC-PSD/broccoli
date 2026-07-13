//! Config in the `print` namespace at contest and plugin (global) scope.

use broccoli_server_sdk::Host;
use serde::Deserialize;

pub const NAMESPACE: &str = "print";

fn default_true() -> bool {
    true
}

fn default_max_pages() -> i32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrintConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default = "default_max_pages")]
    pub max_pages: i32,
    #[serde(default)]
    pub banner: String,
    #[serde(default)]
    pub station_tokens: Vec<String>,
}

impl Default for PrintConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: false,
            max_pages: default_max_pages(),
            banner: String::new(),
            station_tokens: Vec::new(),
        }
    }
}

/// Missing or unparseable config falls back to defaults.
pub fn load_contest_config(host: &Host, contest_id: i32) -> PrintConfig {
    match host.config.get_contest(contest_id, NAMESPACE) {
        Ok(result) => serde_json::from_value(result.config).unwrap_or_default(),
        Err(_) => PrintConfig::default(),
    }
}

pub fn load_global_config(host: &Host) -> PrintConfig {
    match host.config.get_global(NAMESPACE) {
        Ok(result) => serde_json::from_value(result.config).unwrap_or_default(),
        Err(_) => PrintConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_enable_printing_without_approval() {
        let cfg = PrintConfig::default();
        assert!(cfg.enabled);
        assert!(!cfg.require_approval);
        assert_eq!(cfg.max_pages, 10);
        assert!(cfg.station_tokens.is_empty());
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let cfg: PrintConfig = serde_json::from_value(json!({ "require_approval": true })).unwrap();
        assert!(cfg.enabled); // still defaults to true
        assert!(cfg.require_approval);
        assert_eq!(cfg.max_pages, 10);
    }

    #[test]
    fn reads_contest_config_from_host() {
        let host = Host::mock();
        host.config.seed(
            "contest",
            "5",
            NAMESPACE,
            json!({ "require_approval": true, "max_pages": 3, "station_tokens": ["t1"] }),
        );
        let cfg = load_contest_config(&host, 5);
        assert!(cfg.require_approval);
        assert_eq!(cfg.max_pages, 3);
        assert_eq!(cfg.station_tokens, vec!["t1".to_string()]);
    }

    #[test]
    fn missing_contest_config_is_default() {
        let host = Host::mock();
        let cfg = load_contest_config(&host, 9);
        assert!(cfg.enabled);
        assert_eq!(cfg.max_pages, 10);
    }
}
