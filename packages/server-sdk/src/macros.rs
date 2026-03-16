/// Generates the `#[plugin_fn]` wrapper for a plugin API endpoint.
///
/// Usage: `api_handler!(api_use_token, handle_use_token);`
///
/// Expands to a `#[plugin_fn]` function that deserializes `PluginHttpRequest`,
/// calls the handler, catches errors as 500, and serializes the response.
///
/// The handler must have signature:
///   `fn handler(req: PluginHttpRequest) -> Result<PluginHttpResponse, SdkError>`
#[macro_export]
macro_rules! api_handler {
    ($name:ident, $handler:path) => {
        #[extism_pdk::plugin_fn]
        pub fn $name(input: String) -> extism_pdk::FnResult<String> {
            let req: $crate::PluginHttpRequest = match serde_json::from_str(&input) {
                Ok(r) => r,
                Err(e) => {
                    let resp =
                        $crate::PluginHttpResponse::error(400, format!("Invalid request: {e}"));
                    return Ok(serde_json::to_string(&resp)?);
                }
            };
            let resp = match $handler(req) {
                Ok(r) => r,
                Err(e) => $crate::PluginHttpResponse::error(500, format!("{e}")),
            };
            Ok(serde_json::to_string(&resp)?)
        }
    };
}
