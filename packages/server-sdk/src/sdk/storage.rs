use crate::error::SdkError;

pub struct Storage {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: StorageMock,
}

#[cfg(target_arch = "wasm32")]
impl Storage {
    pub fn get(&self, key: &str) -> Result<Option<String>, SdkError> {
        let input = serde_json::json!({ "key": key });
        let result_json = unsafe { crate::host::raw::store_get(serde_json::to_string(&input)?)? };
        let result: serde_json::Value = serde_json::from_str(&result_json)?;
        Ok(result
            .get("value")
            .and_then(|v| v.as_str())
            .map(String::from))
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), SdkError> {
        let input = serde_json::json!({ "key": key, "value": value });
        unsafe { crate::host::raw::store_set(serde_json::to_string(&input)?)? };
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct StorageMock {
    data: std::cell::RefCell<std::collections::HashMap<String, String>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl StorageMock {
    pub fn new() -> Self {
        Self {
            data: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage {
    pub fn get(&self, key: &str) -> Result<Option<String>, SdkError> {
        Ok(self.inner.data.borrow().get(key).cloned())
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), SdkError> {
        self.inner
            .data
            .borrow_mut()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn data(&self) -> std::collections::HashMap<String, String> {
        self.inner.data.borrow().clone()
    }
}
