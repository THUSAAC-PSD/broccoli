use common::storage::BlobStore;
use mq::MqQueue;
use plugin_core::config::PluginConfig;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::i18n::I18nRegistry;
use plugin_core::manager::PluginManagerState;
use plugin_core::manifest::PluginManifest;
use plugin_core::registry::PluginRegistry;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;
use std::sync::{Arc, OnceLock};

use crate::config::AppConfig;
use crate::host_funcs;
use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    OperationBatches, OperationWaiters,
};

pub struct ServerManager {
    state: PluginManagerState,
    host_functions: OnceLock<HostFunctionRegistry>,
    i18n: I18nRegistry,
}

impl ServerManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: PluginConfig,
        db: DatabaseConnection,
        mq: Option<Arc<MqQueue>>,
        operation_batches: OperationBatches,
        operation_waiters: OperationWaiters,
        contest_type_registry: ContestTypeRegistry,
        evaluator_registry: EvaluatorRegistry,
        checker_format_registry: CheckerFormatRegistry,
        evaluate_batches: EvaluateBatches,
        app_config: AppConfig,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<Arc<Self>, anyhow::Error> {
        let manager = Arc::new(Self {
            state: PluginManagerState::new(config),
            host_functions: OnceLock::new(),
            i18n: I18nRegistry::new(),
        });

        let host_functions = host_funcs::init_host_functions(
            db,
            mq,
            operation_batches,
            operation_waiters,
            contest_type_registry,
            evaluator_registry,
            checker_format_registry,
            evaluate_batches,
            manager.clone() as Arc<dyn PluginManager>,
            app_config,
            blob_store,
        );

        manager
            .host_functions
            .set(host_functions)
            .map_err(|_| anyhow::anyhow!("Host functions already initialized"))?;

        Ok(manager)
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
        self.host_functions
            .get()
            .expect("Host functions not initialized")
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
