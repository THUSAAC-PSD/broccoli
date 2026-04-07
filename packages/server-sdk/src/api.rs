use crate::error::SdkError;
use crate::types::PluginHttpResponse;

/// Error type for plugin API handlers.
///
/// ```ignore
/// fn handle(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
///     let id: i32 = req.param("contest_id")?;      // SdkError -> ApiError
///     let info = contest::check_access(host, req, id)?; // ApiError passthrough
///     // ...
/// }
/// ```
#[derive(Debug)]
pub enum ApiError {
    /// An HTTP response to return directly (e.g. 404 Not Found).
    Response(PluginHttpResponse),
    /// An SDK error that will be converted to a 500 response.
    Sdk(SdkError),
}

impl ApiError {
    /// Convert into an HTTP response. SDK errors become 500.
    pub fn into_response(self) -> PluginHttpResponse {
        match self {
            Self::Response(r) => r,
            Self::Sdk(e) => PluginHttpResponse::error(500, format!("{e:?}")),
        }
    }
}

impl From<SdkError> for ApiError {
    fn from(e: SdkError) -> Self {
        Self::Sdk(e)
    }
}

impl From<PluginHttpResponse> for ApiError {
    fn from(r: PluginHttpResponse) -> Self {
        Self::Response(r)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::Sdk(SdkError::Serialization(e.to_string()))
    }
}

/// Run a plugin API handler with standard boilerplate:
/// creates `Host`, deserializes `PluginHttpRequest`, catches errors into responses.
///
/// ```ignore
/// #[plugin_fn]
/// pub fn api_standings(input: String) -> FnResult<String> {
///     run_api_handler(&input, handle_standings)
/// }
///
/// fn handle_standings(host: &Host, req: &PluginHttpRequest) -> Result<PluginHttpResponse, ApiError> {
///     let contest_id: i32 = req.param("contest_id")?;
///     // ...
///     Ok(PluginHttpResponse { status: 200, headers: None, body: Some(json) })
/// }
/// ```
#[cfg(target_arch = "wasm32")]
pub fn run_api_handler(
    input: &str,
    handler: impl FnOnce(
        &crate::sdk::Host,
        &crate::types::PluginHttpRequest,
    ) -> Result<PluginHttpResponse, ApiError>,
) -> extism_pdk::FnResult<String> {
    let host = crate::sdk::Host::new();
    let req: crate::types::PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;
    let resp = match handler(&host, &req) {
        Ok(r) => r,
        Err(e) => e.into_response(),
    };
    Ok(serde_json::to_string(&resp)?)
}
