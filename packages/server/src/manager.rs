use plugin_core::config::PluginConfig;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::i18n::I18nRegistry;
use plugin_core::manager::PluginManagerState;
use plugin_core::manifest::PluginManifest;
use plugin_core::registry::PluginRegistry;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;

use crate::host_funcs::init_host_functions;

pub struct ServerManager {
    state: PluginManagerState,
    host_functions: HostFunctionRegistry,
    i18n: I18nRegistry,
}

impl ServerManager {
    pub fn new(config: PluginConfig, db: DatabaseConnection) -> Self {
        Self {
            state: PluginManagerState::new(config),
            host_functions: init_host_functions(db),
            i18n: I18nRegistry::new(),
        }
    }
}

impl PluginManager for ServerManager {
    fn get_config(&self) -> &PluginConfig {
        &self.state.config
    }
    fn get_registry(&self) -> &PluginRegistry {
        &self.state.registry
    }
    fn get_host_functions(&self) -> &HostFunctionRegistry {
        &self.host_functions
    }
    fn get_i18n_registry(&self) -> &I18nRegistry {
        &self.i18n
    }

    fn resolve(&self, manifest: &PluginManifest) -> Option<(String, Vec<String>)> {
        manifest
            .server
            .as_ref()
            .map(|s| (s.entry.clone(), s.permissions.clone()))
    }
}
