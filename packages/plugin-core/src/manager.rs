use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::config::PluginConfig;
use crate::registry::PluginRegistry;

pub struct PluginManagerState {
    pub config: PluginConfig,
    pub registry: PluginRegistry,
}

impl PluginManagerState {
    pub fn new(config: PluginConfig) -> Self {
        if !config.check_plugins_dir() {
            let _ = std::fs::create_dir_all(&config.plugins_dir);
        }

        Self {
            config,
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
