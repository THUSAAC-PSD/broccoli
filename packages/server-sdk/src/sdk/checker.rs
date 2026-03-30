use crate::error::SdkError;
#[cfg(target_arch = "wasm32")]
use crate::types::RunCheckerInput;
use crate::types::{CheckerParseInput, CheckerVerdict};

pub struct Checker {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: CheckerMock,
}

#[cfg(target_arch = "wasm32")]
impl Checker {
    pub fn run(&self, format: &str, input: &CheckerParseInput) -> Result<CheckerVerdict, SdkError> {
        let run_input = RunCheckerInput {
            format: format.to_string(),
            input: input.clone(),
        };
        let result_json =
            unsafe { crate::host::raw::run_checker(serde_json::to_string(&run_input)?)? };
        Ok(serde_json::from_str(&result_json)?)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CheckerMock;

#[cfg(not(target_arch = "wasm32"))]
impl CheckerMock {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Checker {
    pub fn run(
        &self,
        _format: &str,
        _input: &CheckerParseInput,
    ) -> Result<CheckerVerdict, SdkError> {
        Err(SdkError::Other("Mock checker not implemented".into()))
    }
}
