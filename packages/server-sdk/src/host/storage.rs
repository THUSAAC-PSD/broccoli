use crate::error::SdkError;

/// Get a value from plugin key-value storage.
pub fn store_get(key: &str) -> Result<Option<String>, SdkError> {
    let input = serde_json::json!({ "key": key });
    let result_json = unsafe { super::raw::store_get(serde_json::to_string(&input)?)? };
    let result: serde_json::Value = serde_json::from_str(&result_json)?;
    Ok(result
        .get("value")
        .and_then(|v| v.as_str())
        .map(String::from))
}

/// Set a value in plugin key-value storage.
pub fn store_set(key: &str, value: &str) -> Result<(), SdkError> {
    let input = serde_json::json!({ "key": key, "value": value });
    unsafe { super::raw::store_set(serde_json::to_string(&input)?)? };
    Ok(())
}
