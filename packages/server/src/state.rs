use std::sync::Arc;

use common::storage::BlobStore;
use mq::MqQueue;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    OperationBatches, OperationWaiters,
};

/// Grouped plugin registry and batch state.
#[derive(Clone)]
pub struct RegistryState {
    pub contest_type_registry: ContestTypeRegistry,
    pub evaluator_registry: EvaluatorRegistry,
    pub checker_format_registry: CheckerFormatRegistry,
    pub operation_batches: OperationBatches,
    pub operation_waiters: OperationWaiters,
    pub evaluate_batches: EvaluateBatches,
}

#[derive(Clone)]
pub struct AppState {
    pub plugins: Arc<dyn PluginManager>,
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub mq: Option<Arc<MqQueue>>,
    pub blob_store: Arc<dyn BlobStore>,
    pub registries: RegistryState,
}
