use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PluginConfig {
    pub plugins_dir: PathBuf,
    pub enable_wasi: bool,
    #[serde(default = "default_call_timeout")]
    pub call_timeout_secs: u64,
    #[serde(default = "default_pool_max_instances")]
    pub pool_max_instances: usize,
}

fn default_call_timeout() -> u64 {
    300
}

fn default_pool_max_instances() -> usize {
    32
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            plugins_dir: PathBuf::from("./plugins"),
            enable_wasi: true,
            call_timeout_secs: default_call_timeout(),
            pool_max_instances: default_pool_max_instances(),
        }
    }
}

impl PluginConfig {
    pub fn check_plugins_dir(&self) -> bool {
        self.plugins_dir.exists() && self.plugins_dir.is_dir()
    }
}
