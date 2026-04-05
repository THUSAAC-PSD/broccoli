use crate::error::SdkError;
use crate::types::{ResolveLanguageInput, ResolveLanguageOutput};

pub struct Language {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: LanguageMock,
}

#[cfg(target_arch = "wasm32")]
impl Language {
    pub fn resolve(&self, input: &ResolveLanguageInput) -> Result<ResolveLanguageOutput, SdkError> {
        let response_json =
            unsafe { crate::host::raw::resolve_language(serde_json::to_string(input)?)? };
        Ok(serde_json::from_str(&response_json)?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct LanguageMock;

#[cfg(not(target_arch = "wasm32"))]
impl LanguageMock {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Language {
    pub fn resolve(
        &self,
        _input: &ResolveLanguageInput,
    ) -> Result<ResolveLanguageOutput, SdkError> {
        Err(SdkError::Other("Mock language not implemented".into()))
    }
}
