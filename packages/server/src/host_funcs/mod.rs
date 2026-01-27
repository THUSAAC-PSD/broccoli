pub mod logger;
pub mod storage;

use extism::{Function, UserData, ValType};
use plugin_core::host::HostFunctionRegistry;
use sea_orm::DatabaseConnection;

pub fn init_host_functions(db: DatabaseConnection) -> HostFunctionRegistry {
    let mut hr = HostFunctionRegistry::new();

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
            [ValType::I64, ValType::I64, ValType::I64],
            [],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_set,
        )
    });

    let db_clone = db.clone();
    hr.register("storage", move |plugin_id| {
        Function::new(
            "store_get",
            [ValType::I64, ValType::I64],
            [ValType::I64],
            UserData::new((plugin_id.to_string(), db_clone.clone())),
            storage::store_get,
        )
    });

    hr
}
