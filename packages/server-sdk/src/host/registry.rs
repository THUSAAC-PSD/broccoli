use crate::error::SdkError;

/// Register a contest type with both submission and code_run handlers.
pub fn register_contest_type(
    contest_type: &str,
    submission_handler: &str,
    code_run_handler: &str,
) -> Result<(), SdkError> {
    let input = serde_json::json!({
        "type": contest_type,
        "submission_handler": submission_handler,
        "code_run_handler": code_run_handler,
    });
    unsafe { super::raw::register_contest_type(serde_json::to_string(&input)?)? };
    Ok(())
}

/// Register an evaluator handler with the plugin registry.
pub fn register_evaluator(evaluator_type: &str, handler: &str) -> Result<(), SdkError> {
    let input = serde_json::json!({
        "type": evaluator_type,
        "handler": handler,
    });
    unsafe { super::raw::register_evaluator(serde_json::to_string(&input)?)? };
    Ok(())
}

/// Register a checker format handler with the plugin registry.
pub fn register_checker_format(format: &str, handler: &str) -> Result<(), SdkError> {
    let input = serde_json::json!({
        "format": format,
        "handler": handler,
    });
    unsafe { super::raw::register_checker_format(serde_json::to_string(&input)?)? };
    Ok(())
}
