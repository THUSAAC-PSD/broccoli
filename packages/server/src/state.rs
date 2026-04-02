use std::sync::Arc;
use std::time::Instant;

use common::storage::BlobStore;
use dashmap::DashMap;
use mq::MqQueue;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::hooks::SharedHookRegistry;
use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    LanguageResolverRegistry, OperationBatches, OperationWaiters,
};

/// A pending device authorization request (RFC 8628).
pub struct PendingDeviceAuth {
    pub user_code: String,
    pub token: Option<String>,
    pub created_at: Instant,
    pub expires_at: Instant,
    pub last_poll: Option<Instant>,
}

/// Keyed by device_code (32-byte hex secret).
pub type DeviceCodeStore = Arc<DashMap<String, PendingDeviceAuth>>;

/// Grouped plugin registry and batch state.
#[derive(Clone)]
pub struct RegistryState {
    pub contest_type_registry: ContestTypeRegistry,
    pub evaluator_registry: EvaluatorRegistry,
    pub checker_format_registry: CheckerFormatRegistry,
    pub language_resolver_registry: LanguageResolverRegistry,
    pub operation_batches: OperationBatches,
    pub operation_waiters: OperationWaiters,
    pub evaluate_batches: EvaluateBatches,
    pub hook_registry: SharedHookRegistry,
}

#[derive(Clone)]
pub struct AppState {
    pub plugins: Arc<dyn PluginManager>,
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub mq: Option<Arc<MqQueue>>,
    pub blob_store: Arc<dyn BlobStore>,
    pub registries: RegistryState,
    pub device_codes: DeviceCodeStore,
}
