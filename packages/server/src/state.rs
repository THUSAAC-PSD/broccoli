use std::sync::Arc;

use plugin_core::traits::PluginManager;

#[derive(Clone)]
pub struct AppState {
    pub plugins: Arc<dyn PluginManager>,
}
