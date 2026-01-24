use crate::config::PluginConfig;
use crate::traits::PluginMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct PluginBaseState {
    pub config: PluginConfig,
    pub registry: PluginMap,
}

impl PluginBaseState {
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
