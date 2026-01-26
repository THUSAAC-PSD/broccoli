use std::sync::Arc;

use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub plugins: Arc<dyn PluginManager>,
    pub db: DatabaseConnection,
}
