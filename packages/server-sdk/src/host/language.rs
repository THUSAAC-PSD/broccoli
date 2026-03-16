use crate::error::SdkError;
use crate::types::ResolvedLanguage;

/// Get language compilation/execution configuration from the host.
pub fn get_language_config(
    language: &str,
    submitted_filename: &str,
) -> Result<ResolvedLanguage, SdkError> {
    let input = serde_json::json!({
        "language": language,
        "submitted_filename": submitted_filename,
    });
    let response_json = unsafe { super::raw::get_language_config(serde_json::to_string(&input)?)? };
    let resolved: ResolvedLanguage = serde_json::from_str(&response_json)?;
    Ok(resolved)
}
