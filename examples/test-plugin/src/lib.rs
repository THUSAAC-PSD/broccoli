use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct DemoInput {
    name: String,
}

#[derive(Serialize)]
struct DemoOutput {
    greeting: String,
}

#[plugin_fn]
pub fn greet(input: String) -> FnResult<String> {
    let args: DemoInput = serde_json::from_str(&input)?;

    let output = DemoOutput {
        greeting: format!("Hello, {}! This is from Rust Wasm.", args.name),
    };

    Ok(serde_json::to_string(&output)?)
}
