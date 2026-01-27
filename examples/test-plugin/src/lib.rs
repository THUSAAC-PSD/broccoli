use extism_pdk::{FnResult, host_fn, plugin_fn};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct DemoInput {
    name: String,
}

#[derive(Serialize)]
struct DemoOutput {
    greeting: String,
    visit_count: u32,
}

#[host_fn]
extern "ExtismHost" {
    fn log_info(msg: String);

    fn store_set(collection: String, key: String, value: String);
    fn store_get(collection: String, key: String) -> String;
}

#[plugin_fn]
pub fn greet(input: String) -> FnResult<String> {
    let args: DemoInput = serde_json::from_str(&input)?;

    unsafe {
        log_info(format!("Guest is greeting user: {}", args.name))?;
    }

    let collection = "stats".to_string();
    let key = args.name.clone();

    let raw_value = unsafe { store_get(collection.clone(), key.clone())? };
    let mut count: u32 = if raw_value == "null" {
        0
    } else {
        serde_json::from_str(&raw_value)?
    };

    count += 1;

    let new_value = serde_json::to_string(&count)?;
    unsafe {
        store_set(collection, key, new_value)?;
    }

    let output = DemoOutput {
        greeting: format!("Hello, {}! This is from Rust Wasm.", args.name),
        visit_count: count,
    };

    Ok(serde_json::to_string(&output)?)
}
