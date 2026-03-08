use crate::error::SdkError;
use crate::types::CheckerVerdict;

/// Call the run_checker host function to invoke a checker format handler.
pub fn run_checker(
    format: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    metadata: &serde_json::Value,
) -> Result<CheckerVerdict, SdkError> {
    let input = serde_json::json!({
        "format": format,
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": exit_code,
        "metadata": metadata,
    });
    let result_json = unsafe { super::raw::run_checker(serde_json::to_string(&input)?)? };
    let verdict: CheckerVerdict = serde_json::from_str(&result_json)?;
    Ok(verdict)
}
