pub mod checker;
pub mod config;
pub mod context;
pub mod dispatch;
pub mod evaluate;
pub mod language;
pub mod logger;
pub mod registry;
pub mod sql;
pub mod storage;

use crate::host_funcs::context::HostFunctionDeps;
use extism::{Function, UserData, ValType};
use plugin_core::host::HostFunctionRegistry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Semaphore;

pub fn init_host_functions(deps: HostFunctionDeps) -> HostFunctionRegistry {
    let mut hr = HostFunctionRegistry::new();
    let blob_read_grants: storage::BlobReadGrants = Arc::new(dashmap::DashMap::new());
    let db = deps.system.db.clone();
    let blob_store = deps.system.blob_store.clone();

    hr.register("logger", |plugin_id| {
        Function::new(
            "log_info",
            [ValType::I64],
            [],
            UserData::new(plugin_id.to_string()),
            logger::log_info,
        )
    });

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
    hr.register("storage", move |plugin_id| {
        Function::new(
            "store_compare_and_set",
            [ValType::I64],
            [ValType::I64],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_compare_and_set,
        )
    });

    let db_clone = db.clone();
    hr.register("storage", move |plugin_id| {
        Function::new(
            "store_delete",
            [ValType::I64],
            [],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_delete,
        )
    });

    let blob_store_for_storage = blob_store.clone();
    let blob_read_grants_for_storage = blob_read_grants.clone();
    hr.register("blob:read", move |plugin_id| {
        Function::new(
            "blob_read_range",
            [ValType::I64],
            [ValType::I64],
            UserData::new((
                plugin_id.to_string(),
                blob_store_for_storage.clone(),
                blob_read_grants_for_storage.clone(),
            )),
            storage::blob_read_range,
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

    let contest_reg = deps.system.contest_type_registry.clone();
    let eval_reg = deps.system.evaluator_registry.clone();
    let checker_reg = deps.system.checker_format_registry.clone();
    let lang_reg = deps.system.language_resolver_registry.clone();
    hr.register_many("plugin:register", move |plugin_id| {
        registry::create_registry_functions(
            plugin_id.to_string(),
            contest_reg.clone(),
            eval_reg.clone(),
            checker_reg.clone(),
            lang_reg.clone(),
        )
    });

    // Cap concurrent evaluator plugin calls per server. Default = number of cores,
    // but on hosts with spare RAM and a high-fan-out workload (Signpost: 20 testcases
    // per submission, ICPC fans them out as parallel tokio tasks), this should be
    // bumped well above core count via `BROCCOLI__PLUGIN__EVALUATOR_PARALLELISM`.
    let evaluator_parallelism = deps
        .system
        .config
        .plugin
        .evaluator_parallelism
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
        })
        .max(1);
    tracing::info!(evaluator_parallelism, "evaluator semaphore configured");
    let evaluator_slots = Arc::new(Semaphore::new(evaluator_parallelism));
    let eval_deps = deps.evaluate_deps(evaluator_slots);
    hr.register_many("evaluator:evaluate", move |plugin_id| {
        evaluate::create_evaluate_functions(plugin_id.to_string(), eval_deps.clone())
    });

    let dispatch_deps = deps.system.operation_deps();
    hr.register_many("operations:dispatch", move |plugin_id| {
        dispatch::create_dispatch_functions(plugin_id.to_string(), dispatch_deps.clone())
    });

    let checker_reg = deps.system.checker_format_registry;
    let pm = deps.plugin_manager.clone();
    let blob_read_grants_for_checker = blob_read_grants;
    hr.register("checker:run", move |plugin_id| {
        checker::create_checker_function(
            plugin_id.to_string(),
            pm.clone(),
            checker_reg.clone(),
            blob_read_grants_for_checker.clone(),
        )
    });

    let lang_reg = deps.system.language_resolver_registry;
    let pm_for_resolve = deps.plugin_manager.clone();
    hr.register("language:resolve", move |plugin_id| {
        language::create_resolve_language_function(
            plugin_id.to_string(),
            lang_reg.clone(),
            pm_for_resolve.clone(),
        )
    });

    let db_clone = db.clone();
    let registry = deps.plugin_manager.get_registry().clone();
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
