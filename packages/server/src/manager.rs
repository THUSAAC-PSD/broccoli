use plugin_core::config::PluginConfig;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::manager::PluginBaseState;
use plugin_core::manifest::PluginManifest;
use plugin_core::traits::{PluginManager, PluginMap};
use sea_orm::DatabaseConnection;

use crate::host_funcs::init_host_functions;

pub struct ServerManager {
    state: PluginBaseState,
    host_functions: HostFunctionRegistry,
}

impl ServerManager {
    pub fn new(config: PluginConfig, db: DatabaseConnection) -> Self {
        Self {
            state: PluginBaseState::new(config),
            host_functions: init_host_functions(db),
        }
    }
}

impl PluginManager for ServerManager {
    fn get_config(&self) -> &PluginConfig {
        &self.state.config
    }
    fn get_registry(&self) -> &PluginMap {
        &self.state.registry
    }
    fn get_host_functions(&self) -> &HostFunctionRegistry {
        &self.host_functions
    }

    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
        manifest
            .server
            .as_ref()
            .map(|s| (s.entry.clone(), s.permissions.clone()))
    }
}
