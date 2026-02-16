use std::sync::Arc;

use common::storage::BlobStore;
use mq::MqQueue;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub plugins: Arc<dyn PluginManager>,
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub mq: Option<Arc<MqQueue>>,
    pub blob_store: Arc<dyn BlobStore>,
}
