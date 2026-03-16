use crate::error::SdkError;
use crate::types::ResolvedLanguage;

/// Get language compilation/execution configuration from the host.
///
/// `extra_sources` lists additional source filenames (e.g. grader stubs) that
/// should be compiled together with the primary source.  The `{source}`
/// placeholder in `compile_cmd` will expand to all of them.
pub fn get_language_config(
    language: &str,
    submitted_filename: &str,
    extra_sources: &[String],
) -> Result<ResolvedLanguage, SdkError> {
    let input = serde_json::json!({
        "language": language,
        "submitted_filename": submitted_filename,
        "extra_sources": extra_sources,
    });
    let response_json = unsafe { super::raw::get_language_config(serde_json::to_string(&input)?)? };
    let resolved: ResolvedLanguage = serde_json::from_str(&response_json)?;
    Ok(resolved)
}
