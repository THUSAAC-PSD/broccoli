use serde::de::DeserializeOwned;

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

    /// Set `key` to `new` only if the current value equals `expected`.
    ///
    /// Returns `true` if the swap succeeded, `false` if the value changed.
    /// `expected = None` means "only set if the key doesn't exist yet".
    pub fn compare_and_set(
        &self,
        key: &str,
        expected: Option<&str>,
        new: &str,
    ) -> Result<bool, SdkError> {
        let input = serde_json::json!({
            "key": key,
            "expected": expected,
            "new": new,
        });
        let result_json =
            unsafe { crate::host::raw::store_compare_and_set(serde_json::to_string(&input)?)? };
        let result: serde_json::Value = serde_json::from_str(&result_json)?;
        Ok(result["swapped"].as_bool().unwrap_or(false))
    }

    /// Atomically read-modify-write a JSON value.
    ///
    /// Reads the current value (or `T::default()` if absent), calls `f` to
    /// modify it, and writes back. Retries automatically on contention.
    ///
    /// ```ignore
    /// let state = host.storage.modify::<TokenState>(&key, |state| {
    ///     if state.available == 0 {
    ///         return Err(SdkError::Other("No tokens".into()));
    ///     }
    ///     state.used += 1;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn modify<T, F>(&self, key: &str, f: F) -> Result<T, SdkError>
    where
        T: serde::Serialize + DeserializeOwned + Default,
        F: Fn(&mut T) -> Result<(), SdkError>,
    {
        loop {
            let old_raw = self.get(key)?;
            let mut val: T = match &old_raw {
                Some(json) => serde_json::from_str(json)?,
                None => T::default(),
            };
            f(&mut val)?;
            let new_raw = serde_json::to_string(&val)?;
            if self.compare_and_set(key, old_raw.as_deref(), &new_raw)? {
                return Ok(val);
            }
        }
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

    pub fn compare_and_set(
        &self,
        key: &str,
        expected: Option<&str>,
        new: &str,
    ) -> Result<bool, SdkError> {
        let mut data = self.inner.data.borrow_mut();
        let current = data.get(key).map(|s| s.as_str());
        if current == expected {
            data.insert(key.to_string(), new.to_string());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Atomically read-modify-write a JSON value.
    pub fn modify<T, F>(&self, key: &str, f: F) -> Result<T, SdkError>
    where
        T: serde::Serialize + DeserializeOwned + Default,
        F: Fn(&mut T) -> Result<(), SdkError>,
    {
        loop {
            let old_raw = self.get(key)?;
            let mut val: T = match &old_raw {
                Some(json) => serde_json::from_str(json)?,
                None => T::default(),
            };
            f(&mut val)?;
            let new_raw = serde_json::to_string(&val)?;
            if self.compare_and_set(key, old_raw.as_deref(), &new_raw)? {
                return Ok(val);
            }
        }
    }

    pub fn data(&self) -> std::collections::HashMap<String, String> {
        self.inner.data.borrow().clone()
    }
}
