use extism_pdk::{FnResult, plugin_fn};

/// Returns the input unchanged.
#[plugin_fn]
pub fn echo(input: String) -> FnResult<String> {
    Ok(input)
}

/// Returns a greeting.
#[plugin_fn]
pub fn greet(name: String) -> FnResult<String> {
    let response = serde_json::json!({ "message": format!("Hello, {}!", name) });
    Ok(serde_json::to_string(&response)?)
}
