use crate::error::SdkError;
use crate::types::{CheckerParseInput, CheckerVerdict, RunCheckerInput};

/// Call the run_checker host function to invoke a checker format handler.
pub fn run_checker(format: &str, input: &CheckerParseInput) -> Result<CheckerVerdict, SdkError> {
    let run_input = RunCheckerInput {
        format: format.to_string(),
        input: input.clone(),
    };
    let result_json = unsafe { super::raw::run_checker(serde_json::to_string(&run_input)?)? };
    let verdict: CheckerVerdict = serde_json::from_str(&result_json)?;
    Ok(verdict)
}
