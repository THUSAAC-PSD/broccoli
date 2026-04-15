use crate::error::SdkError;
use crate::types::PluginHttpResponse;

#[derive(Debug)]
pub enum ApiError {
    Response(PluginHttpResponse),
    Sdk(SdkError),
}

impl ApiError {
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
