use crate::error::SdkError;
use crate::types::{ResolveLanguageInput, ResolveLanguageOutput, ResolvedLanguage};

pub struct Language {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: LanguageMock,
}

#[cfg(target_arch = "wasm32")]
impl Language {
    pub fn get_config(
        &self,
        language: &str,
        submitted_filename: &str,
        extra_sources: &[String],
    ) -> Result<ResolvedLanguage, SdkError> {
        let input = serde_json::json!({
            "language": language,
            "submitted_filename": submitted_filename,
            "extra_sources": extra_sources,
        });
        let response_json =
            unsafe { crate::host::raw::get_language_config(serde_json::to_string(&input)?)? };
        Ok(serde_json::from_str(&response_json)?)
    }

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
    pub fn get_config(
        &self,
        _language: &str,
        _submitted_filename: &str,
        _extra_sources: &[String],
    ) -> Result<ResolvedLanguage, SdkError> {
        Err(SdkError::Other("Mock language not implemented".into()))
    }

    pub fn resolve(
        &self,
        _input: &ResolveLanguageInput,
    ) -> Result<ResolveLanguageOutput, SdkError> {
        Err(SdkError::Other("Mock language not implemented".into()))
    }
}
