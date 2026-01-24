use extism_pdk::{FnResult, host_fn, plugin_fn};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct DemoInput {
    name: String,
}

#[derive(Serialize)]
struct DemoOutput {
    greeting: String,
}

#[host_fn]
extern "ExtismHost" {
    fn log_info(msg: String);
}

#[plugin_fn]
pub fn greet(input: String) -> FnResult<String> {
    let args: DemoInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!("Guest is greeting user: {}", args.name))?;
    }

    let output = DemoOutput {
        greeting: format!("Hello, {}! This is from Rust Wasm.", args.name),
    };

    Ok(serde_json::to_string(&output)?)
}
