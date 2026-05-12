use std::sync::Arc;

use common::storage::BlobStore;
use mq::MqQueue;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::config::AppConfig;
use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    LanguageResolverRegistry, OperationBatches, OperationWaiters,
};

#[derive(Clone)]
pub struct HostFunctionSystemDeps {
    pub db: DatabaseConnection,
    pub mq: Option<Arc<MqQueue>>,
    pub operation_batches: OperationBatches,
    pub operation_waiters: OperationWaiters,
    pub contest_type_registry: ContestTypeRegistry,
    pub evaluator_registry: EvaluatorRegistry,
    pub checker_format_registry: CheckerFormatRegistry,
    pub language_resolver_registry: LanguageResolverRegistry,
    pub evaluate_batches: EvaluateBatches,
    pub blob_store: Arc<dyn BlobStore>,
    pub config: AppConfig,
    pub metrics: Option<common::metrics::Metrics>,
}

#[derive(Clone)]
pub struct HostFunctionDeps {
    pub system: HostFunctionSystemDeps,
    pub plugin_manager: Arc<dyn PluginManager>,
}

#[derive(Clone)]
pub struct OperationHostDeps {
    pub mq: Option<Arc<MqQueue>>,
    pub operation_batches: OperationBatches,
    pub operation_waiters: OperationWaiters,
    pub operation_queue_name: String,
    pub operation_result_queue_name: String,
    pub blob_store: Arc<dyn BlobStore>,
    pub metrics: Option<common::metrics::Metrics>,
}

#[derive(Clone)]
pub struct EvaluateHostDeps {
    pub db: DatabaseConnection,
    pub evaluator_registry: EvaluatorRegistry,
    pub evaluate_batches: EvaluateBatches,
    pub evaluator_slots: Arc<Semaphore>,
    pub plugin_manager: Arc<dyn PluginManager>,
    pub blob_store: Arc<dyn BlobStore>,
    pub metrics: Option<common::metrics::Metrics>,
}

impl HostFunctionSystemDeps {
    pub fn with_plugin_manager(self, plugin_manager: Arc<dyn PluginManager>) -> HostFunctionDeps {
        HostFunctionDeps {
            system: self,
            plugin_manager,
        }
    }

    pub fn operation_deps(&self) -> OperationHostDeps {
        OperationHostDeps {
            mq: self.mq.clone(),
            operation_batches: self.operation_batches.clone(),
            operation_waiters: self.operation_waiters.clone(),
            operation_queue_name: self.config.mq.operation_queue_name.clone(),
            operation_result_queue_name: self.config.mq.operation_result_queue_name.clone(),
            blob_store: self.blob_store.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl HostFunctionDeps {
    pub fn evaluate_deps(&self, evaluator_slots: Arc<Semaphore>) -> EvaluateHostDeps {
        EvaluateHostDeps {
            db: self.system.db.clone(),
            evaluator_registry: self.system.evaluator_registry.clone(),
            evaluate_batches: self.system.evaluate_batches.clone(),
            evaluator_slots,
            plugin_manager: self.plugin_manager.clone(),
            blob_store: self.system.blob_store.clone(),
            metrics: self.system.metrics.clone(),
        }
    }
}
