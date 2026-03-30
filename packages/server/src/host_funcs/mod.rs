pub mod checker;
pub mod config;
pub mod dispatch;
pub mod evaluate;
pub mod language;
pub mod logger;
pub mod registry;
pub mod sql;
pub mod storage;

use crate::config::AppConfig;
use crate::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    OperationBatches, OperationWaiters,
};
use common::storage::BlobStore;
use extism::{Function, UserData, ValType};
use mq::MqQueue;
use plugin_core::host::HostFunctionRegistry;
use plugin_core::traits::PluginManager;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Semaphore;

#[allow(clippy::too_many_arguments)]
pub fn init_host_functions(
    db: DatabaseConnection,
    mq: Option<Arc<MqQueue>>,
    operation_batches: OperationBatches,
    operation_waiters: OperationWaiters,
    contest_type_registry: ContestTypeRegistry,
    evaluator_registry: EvaluatorRegistry,
    checker_format_registry: CheckerFormatRegistry,
    evaluate_batches: EvaluateBatches,
    plugin_manager: Arc<dyn PluginManager>,
    config: AppConfig,
    blob_store: Arc<dyn BlobStore>,
) -> HostFunctionRegistry {
    let mut hr = HostFunctionRegistry::new();

    // Logger permission
    hr.register("logger", |plugin_id| {
        Function::new(
            "log_info",
            [ValType::I64],
            [],
            UserData::new(plugin_id.to_string()),
            logger::log_info,
        )
    });

    // Storage permission (single JSON input, matching SDK)
    let db_clone = db.clone();
    hr.register("storage", move |plugin_id| {
        Function::new(
            "store_set",
            [ValType::I64],
            [],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_set,
        )
    });

    let db_clone = db.clone();
    hr.register("storage", move |plugin_id| {
        Function::new(
            "store_get",
            [ValType::I64],
            [ValType::I64],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_get,
        )
    });

    let db_clone = db.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_execute",
            [ValType::I64, ValType::I64],
            [ValType::I64],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            sql::db_execute,
        )
    });

    let db_clone = db.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_query",
            [ValType::I64, ValType::I64],
            [ValType::I64],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            sql::db_query,
        )
    });

    let txn_map: sql::TransactionMap = Arc::new(StdMutex::new(HashMap::new()));

    let db_clone = db.clone();
    let txn_map_clone = txn_map.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_begin",
            [ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                db_clone.clone(),
                txn_map_clone.clone(),
            )),
            sql::db_begin,
        )
    });

    let db_clone = db.clone();
    let txn_map_clone = txn_map.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_query_in",
            [ValType::I64, ValType::I64, ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                db_clone.clone(),
                txn_map_clone.clone(),
            )),
            sql::db_query_in,
        )
    });

    let db_clone = db.clone();
    let txn_map_clone = txn_map.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_execute_in",
            [ValType::I64, ValType::I64, ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                db_clone.clone(),
                txn_map_clone.clone(),
            )),
            sql::db_execute_in,
        )
    });

    let db_clone = db.clone();
    let txn_map_clone = txn_map.clone();
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_commit",
            [ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                db_clone.clone(),
                txn_map_clone.clone(),
            )),
            sql::db_commit,
        )
    });

    let db_clone = db.clone();
    let txn_map_clone = txn_map;
    hr.register("sql", move |plugin_id| {
        Function::new(
            "db_rollback",
            [ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                db_clone.clone(),
                txn_map_clone.clone(),
            )),
            sql::db_rollback,
        )
    });

    let contest_reg = contest_type_registry.clone();
    let eval_reg = evaluator_registry.clone();
    let checker_reg = checker_format_registry.clone();
    hr.register_many("plugin:register", move |plugin_id| {
        registry::create_registry_functions(
            plugin_id.to_string(),
            contest_reg.clone(),
            eval_reg.clone(),
            checker_reg.clone(),
        )
    });

    let eval_reg = evaluator_registry.clone();
    let pm = plugin_manager.clone();
    let eval_batches = evaluate_batches;
    let evaluator_parallelism = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .max(1);
    let evaluator_slots = Arc::new(Semaphore::new(evaluator_parallelism));
    let db_for_eval = db.clone();
    let blob_store_for_eval = blob_store;
    hr.register_many("evaluator:evaluate", move |plugin_id| {
        evaluate::create_evaluate_functions(
            plugin_id.to_string(),
            pm.clone(),
            eval_reg.clone(),
            eval_batches.clone(),
            evaluator_slots.clone(),
            db_for_eval.clone(),
            blob_store_for_eval.clone(),
        )
    });

    let mq_clone = mq;
    let batches = operation_batches;
    let waiters = operation_waiters;
    let op_queue = config.mq.operation_queue_name.clone();
    let res_queue = config.mq.operation_result_queue_name.clone();
    hr.register_many("operations:dispatch", move |plugin_id| {
        dispatch::create_dispatch_functions(
            plugin_id.to_string(),
            mq_clone.clone(),
            batches.clone(),
            waiters.clone(),
            op_queue.clone(),
            res_queue.clone(),
        )
    });

    let checker_reg = checker_format_registry;
    let pm = plugin_manager.clone();
    hr.register("checker:run", move |plugin_id| {
        checker::create_checker_function(plugin_id.to_string(), pm.clone(), checker_reg.clone())
    });

    let languages = config.languages;
    hr.register("language:config", move |plugin_id| {
        language::create_language_function(plugin_id.to_string(), languages.clone())
    });

    let db_clone = db.clone();
    let registry = plugin_manager.get_registry().clone();
    hr.register("config:read", move |plugin_id| {
        config::create_config_get_function(
            plugin_id.to_string(),
            db_clone.clone(),
            registry.clone(),
        )
    });

    let db_clone = db;
    hr.register("config:write", move |plugin_id| {
        config::create_config_set_function(plugin_id.to_string(), db_clone.clone())
    });

    hr
}
