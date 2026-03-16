use crate::error::SdkError;

/// Log an informational message via the host logger.
pub fn log_info(msg: impl Into<String>) -> Result<(), SdkError> {
    unsafe { super::raw::log_info(msg.into())? };
    Ok(())
}
