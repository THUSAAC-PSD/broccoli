
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StressError {
    #[error("HTTP {status}: {code} — {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
    },

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("decode error: {0}")]
    Decode(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type StressResult<T> = Result<T, StressError>;
