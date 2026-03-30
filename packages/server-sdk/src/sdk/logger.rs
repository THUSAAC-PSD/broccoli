use crate::error::SdkError;

pub struct Logger {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: LoggerMock,
}

#[cfg(target_arch = "wasm32")]
impl Logger {
    pub fn info(&self, msg: &str) -> Result<(), SdkError> {
        unsafe { crate::host::raw::log_info(msg.to_string())? };
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct LoggerMock {
    messages: std::cell::RefCell<Vec<String>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LoggerMock {
    pub fn new() -> Self {
        Self {
            messages: std::cell::RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Logger {
    pub fn info(&self, msg: &str) -> Result<(), SdkError> {
        self.inner.messages.borrow_mut().push(msg.to_string());
        Ok(())
    }

    pub fn messages(&self) -> Vec<String> {
        self.inner.messages.borrow().clone()
    }
}
