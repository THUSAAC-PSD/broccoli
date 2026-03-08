use crate::error::SdkError;

/// Get a plugin config value by scope, ref_id, and namespace.
pub fn get_config(
    scope: &str,
    ref_id: &str,
    namespace: &str,
) -> Result<Option<serde_json::Value>, SdkError> {
    let input = serde_json::json!({
        "scope": scope,
        "ref_id": ref_id,
        "namespace": namespace,
    });
    let result_json = unsafe { super::raw::config_get(serde_json::to_string(&input)?)? };
    let result: serde_json::Value = serde_json::from_str(&result_json)?;
    if result.is_null() {
        Ok(None)
    } else {
        Ok(result.get("config").cloned())
    }
}

/// Set a plugin config value by scope, ref_id, and namespace.
pub fn set_config(
    scope: &str,
    ref_id: &str,
    namespace: &str,
    config: &serde_json::Value,
) -> Result<(), SdkError> {
    let input = serde_json::json!({
        "scope": scope,
        "ref_id": ref_id,
        "namespace": namespace,
        "config": config,
    });
    unsafe { super::raw::config_set(serde_json::to_string(&input)?)? };
    Ok(())
}

pub fn get_global_config(namespace: &str) -> Result<Option<serde_json::Value>, SdkError> {
    get_config("plugin", "", namespace)
}

pub fn set_global_config(namespace: &str, config: &serde_json::Value) -> Result<(), SdkError> {
    set_config("plugin", "", namespace, config)
}

pub fn get_problem_config(
    problem_id: i32,
    namespace: &str,
) -> Result<Option<serde_json::Value>, SdkError> {
    get_config("problem", &problem_id.to_string(), namespace)
}

pub fn set_problem_config(
    problem_id: i32,
    namespace: &str,
    config: &serde_json::Value,
) -> Result<(), SdkError> {
    set_config("problem", &problem_id.to_string(), namespace, config)
}

pub fn get_contest_problem_config(
    contest_id: i32,
    problem_id: i32,
    namespace: &str,
) -> Result<Option<serde_json::Value>, SdkError> {
    get_config(
        "contest_problem",
        &format!("{contest_id}:{problem_id}"),
        namespace,
    )
}

pub fn set_contest_problem_config(
    contest_id: i32,
    problem_id: i32,
    namespace: &str,
    config: &serde_json::Value,
) -> Result<(), SdkError> {
    set_config(
        "contest_problem",
        &format!("{contest_id}:{problem_id}"),
        namespace,
        config,
    )
}

pub fn get_contest_config(
    contest_id: i32,
    namespace: &str,
) -> Result<Option<serde_json::Value>, SdkError> {
    get_config("contest", &contest_id.to_string(), namespace)
}

pub fn set_contest_config(
    contest_id: i32,
    namespace: &str,
    config: &serde_json::Value,
) -> Result<(), SdkError> {
    set_config("contest", &contest_id.to_string(), namespace, config)
}
