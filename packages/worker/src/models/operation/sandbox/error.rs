use thiserror::Error;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Environment initialization failed: {0}")]
    Initialization(String),

    #[error("execution error: {0}")]
    Execution(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
