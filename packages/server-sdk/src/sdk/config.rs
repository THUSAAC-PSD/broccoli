use crate::error::SdkError;
use crate::types::ConfigResult;
use serde_json::Value;

pub struct Config {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) inner: ConfigMock,
}

#[cfg(target_arch = "wasm32")]
impl Config {
    pub fn get(&self, scope: &str, ref_id: &str, ns: &str) -> Result<ConfigResult, SdkError> {
        let input = serde_json::json!({
            "scope": scope,
            "ref_id": ref_id,
            "namespace": ns,
        });
        let result_json = unsafe { crate::host::raw::config_get(serde_json::to_string(&input)?)? };
        let result: Value = serde_json::from_str(&result_json)?;
        Ok(ConfigResult {
            config: result
                .get("config")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new())),
            is_default: result
                .get("is_default")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    pub fn set(&self, scope: &str, ref_id: &str, ns: &str, value: &Value) -> Result<(), SdkError> {
        let input = serde_json::json!({
            "scope": scope,
            "ref_id": ref_id,
            "namespace": ns,
            "config": value,
        });
        unsafe { crate::host::raw::config_set(serde_json::to_string(&input)?)? };
        Ok(())
    }

    pub fn get_global(&self, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("plugin", "", ns)
    }

    pub fn set_global(&self, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("plugin", "", ns, value)
    }

    pub fn get_problem(&self, problem_id: i32, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("problem", &problem_id.to_string(), ns)
    }

    pub fn set_problem(&self, problem_id: i32, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("problem", &problem_id.to_string(), ns, value)
    }

    pub fn get_contest(&self, contest_id: i32, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("contest", &contest_id.to_string(), ns)
    }

    pub fn set_contest(&self, contest_id: i32, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("contest", &contest_id.to_string(), ns, value)
    }

    pub fn get_contest_problem(
        &self,
        contest_id: i32,
        problem_id: i32,
        ns: &str,
    ) -> Result<ConfigResult, SdkError> {
        self.get("contest_problem", &format!("{contest_id}:{problem_id}"), ns)
    }

    pub fn set_contest_problem(
        &self,
        contest_id: i32,
        problem_id: i32,
        ns: &str,
        value: &Value,
    ) -> Result<(), SdkError> {
        self.set(
            "contest_problem",
            &format!("{contest_id}:{problem_id}"),
            ns,
            value,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ConfigMock {
    data: std::cell::RefCell<std::collections::HashMap<String, Value>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ConfigMock {
    pub fn new() -> Self {
        Self {
            data: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    fn key(scope: &str, ref_id: &str, ns: &str) -> String {
        format!("{scope}:{ref_id}:{ns}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Config {
    pub fn get(&self, scope: &str, ref_id: &str, ns: &str) -> Result<ConfigResult, SdkError> {
        let key = ConfigMock::key(scope, ref_id, ns);
        let config = self
            .inner
            .data
            .borrow()
            .get(&key)
            .cloned()
            .unwrap_or(Value::Null);
        Ok(ConfigResult {
            config,
            is_default: false,
        })
    }

    pub fn set(&self, scope: &str, ref_id: &str, ns: &str, value: &Value) -> Result<(), SdkError> {
        let key = ConfigMock::key(scope, ref_id, ns);
        self.inner.data.borrow_mut().insert(key, value.clone());
        Ok(())
    }

    pub fn get_global(&self, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("plugin", "", ns)
    }

    pub fn set_global(&self, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("plugin", "", ns, value)
    }

    pub fn get_problem(&self, problem_id: i32, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("problem", &problem_id.to_string(), ns)
    }

    pub fn set_problem(&self, problem_id: i32, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("problem", &problem_id.to_string(), ns, value)
    }

    pub fn get_contest(&self, contest_id: i32, ns: &str) -> Result<ConfigResult, SdkError> {
        self.get("contest", &contest_id.to_string(), ns)
    }

    pub fn set_contest(&self, contest_id: i32, ns: &str, value: &Value) -> Result<(), SdkError> {
        self.set("contest", &contest_id.to_string(), ns, value)
    }

    pub fn get_contest_problem(
        &self,
        contest_id: i32,
        problem_id: i32,
        ns: &str,
    ) -> Result<ConfigResult, SdkError> {
        self.get("contest_problem", &format!("{contest_id}:{problem_id}"), ns)
    }

    pub fn set_contest_problem(
        &self,
        contest_id: i32,
        problem_id: i32,
        ns: &str,
        value: &Value,
    ) -> Result<(), SdkError> {
        self.set(
            "contest_problem",
            &format!("{contest_id}:{problem_id}"),
            ns,
            value,
        )
    }

    /// Pre-populate a config value for testing.
    pub fn seed(&self, scope: &str, ref_id: &str, ns: &str, value: Value) {
        let key = ConfigMock::key(scope, ref_id, ns);
        self.inner.data.borrow_mut().insert(key, value);
    }
}
