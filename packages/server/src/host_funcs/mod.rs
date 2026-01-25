pub mod logger;

use extism::{Function, UserData, ValType};
use plugin_core::host::HostFunctionRegistry;

pub fn init_host_functions() -> HostFunctionRegistry {
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

    hr
}
