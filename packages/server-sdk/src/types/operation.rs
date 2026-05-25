use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const DEFAULT_OPERATION_RESULT_TIMEOUT_MS: u64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionFile {
    #[serde(rename = "path")]
    Path { path: String },
    #[serde(rename = "content")]
    Content { content: String },
    #[serde(rename = "blob")]
    Blob { hash: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub files_in: Vec<(String, SessionFile)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum IOTarget {
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "inherit")]
    #[default]
    Inherit,
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "pipe")]
    Pipe { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IOConfig {
    pub stdin: IOTarget,
    pub stdout: IOTarget,
    pub stderr: IOTarget,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryOptions {
    pub read_write: bool,
    pub allow_devices: bool,
    pub no_exec: bool,
    pub is_filesystem: bool,
    pub is_tmp: bool,
    pub no_recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryRule {
    pub inside_path: PathBuf,
    pub outside_path: Option<PathBuf>,
    pub options: DirectoryOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvRule {
    Inherit(String),
    Set(String, String),
    FullEnv,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub time_limit: Option<f64>,
    pub wall_time_limit: Option<f64>,
    pub extra_time: Option<f64>,
    pub memory_limit: Option<u32>,
    pub stack_limit: Option<u32>,
    pub open_files_limit: Option<u32>,
    pub file_size_limit: Option<u32>,
    pub process_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunOptions {
    pub resource_limits: ResourceLimits,
    pub wait: bool,
    pub as_uid: Option<u32>,
    pub as_gid: Option<u32>,
    pub stdin: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
    pub env_rules: Vec<EnvRule>,
    pub directory_rules: Vec<DirectoryRule>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            resource_limits: ResourceLimits {
                time_limit: None,
                wall_time_limit: None,
                extra_time: None,
                memory_limit: None,
                stack_limit: None,
                open_files_limit: None,
                file_size_limit: None,
                process_limit: Some(1),
            },
            wait: true,
            as_uid: None,
            as_gid: None,
            stdin: None,
            stdout: None,
            stderr: None,
            env_rules: vec![EnvRule::FullEnv],
            directory_rules: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCacheConfig {
    pub key_inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    #[default]
    Generic,
    Compile,
    Testcase,
    CheckerCompile,
    Checker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    #[serde(default)]
    pub kind: StepKind,
    pub env_ref: String,
    pub argv: Vec<String>,
    pub conf: RunOptions,
    pub io: IOConfig,
    pub collect: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub cache: Option<StepCacheConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub buffer_size: Option<usize>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            name: String::new(),
            buffer_size: Some(8192),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTask {
    pub environments: Vec<Environment>,
    pub tasks: Vec<Step>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    /// When set, the host routes this operation to the worker's private
    /// queue (`<operation_queue_name>:worker:<id>`) instead of the shared
    /// pool. Used by admin probe / pinned-rejudge flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_worker_id: Option<String>,
    /// Evaluator-owned correlation: the evaluate batch this operation belongs
    /// to. When set together with `test_case_id`, the host records the
    /// dispatched task in its cancel registry so cancel_test_cases can later
    /// short-circuit pending work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluate_batch_id: Option<String>,
    /// Test case this operation evaluates. Paired with `evaluate_batch_id`
    /// to scope cancellation to a subset of in-flight work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_case_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDetachedWindowedOperationInput {
    pub operations: Vec<OperationTask>,
    pub concurrency: usize,
    pub result_timeout_ms: u64,
    pub callback_fn: String,
    #[serde(default)]
    pub state: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachedOperationSession {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachedOperationCallbackInput {
    pub session_id: String,
    #[serde(default)]
    pub state: serde_json::Value,
    pub event: DetachedOperationCallbackEvent,
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetachedOperationCallbackEvent {
    Result {
        operation_index: usize,
        result: OperationResult,
    },
    Timeout {
        message: String,
    },
    Exhausted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetachedOperationCallbackOutput {
    #[serde(default)]
    pub state: serde_json::Value,
    #[serde(default)]
    pub action: DetachedOperationCallbackAction,
    #[serde(default = "default_refill")]
    pub refill: bool,
    #[serde(default)]
    pub cancel_operation_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetachedOperationCallbackAction {
    #[default]
    Continue,
    Finish,
    Cancel,
}

impl DetachedOperationCallbackInput {
    pub fn result(&self) -> Option<(usize, &OperationResult)> {
        match &self.event {
            DetachedOperationCallbackEvent::Result {
                operation_index,
                result,
            } => Some((*operation_index, result)),
            DetachedOperationCallbackEvent::Timeout { .. }
            | DetachedOperationCallbackEvent::Exhausted => None,
        }
    }
}

impl DetachedOperationCallbackOutput {
    pub fn continue_with(state: serde_json::Value) -> Self {
        Self {
            state,
            action: DetachedOperationCallbackAction::Continue,
            refill: true,
            cancel_operation_indices: Vec::new(),
        }
    }

    pub fn finish(state: serde_json::Value) -> Self {
        Self {
            state,
            action: DetachedOperationCallbackAction::Finish,
            refill: false,
            cancel_operation_indices: Vec::new(),
        }
    }

    pub fn cancel(state: serde_json::Value) -> Self {
        Self {
            state,
            action: DetachedOperationCallbackAction::Cancel,
            refill: false,
            cancel_operation_indices: Vec::new(),
        }
    }

    pub fn refill(mut self, refill: bool) -> Self {
        self.refill = refill;
        self
    }

    pub fn refill_while(
        mut self,
        input: &DetachedOperationCallbackInput,
        predicate: impl FnOnce(usize, &OperationResult) -> bool,
    ) -> Self {
        self.refill = input
            .result()
            .is_some_and(|(operation_index, result)| predicate(operation_index, result));
        self
    }

    pub fn cancel_operation_indices(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        self.cancel_operation_indices = indices.into_iter().collect();
        self
    }
}

fn default_refill() -> bool {
    true
}

#[cfg(test)]
mod detached_tests {
    use super::*;

    #[test]
    fn detached_operation_output_refill_while_uses_result_predicate() {
        let input = DetachedOperationCallbackInput {
            session_id: "session".to_string(),
            state: serde_json::json!({}),
            event: DetachedOperationCallbackEvent::Result {
                operation_index: 2,
                result: OperationResult {
                    success: true,
                    task_results: HashMap::new(),
                    error: None,
                },
            },
            completed: 1,
            total: 3,
        };

        let output = DetachedOperationCallbackOutput::continue_with(serde_json::json!({}))
            .refill_while(&input, |operation_index, result| {
                operation_index == 2 && result.success
            });

        assert!(output.refill);
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub task_results: HashMap<String, TaskExecutionResult>,
    pub error: Option<String>,
}

impl OperationResult {
    pub const CANCELLED_BY_HOST: &'static str = "Cancelled by host";

    pub fn cancelled_by_host() -> Self {
        Self {
            success: false,
            task_results: HashMap::new(),
            error: Some(Self::CANCELLED_BY_HOST.to_string()),
        }
    }

    pub fn is_cancelled_by_host(&self) -> bool {
        !self.success
            && self.task_results.is_empty()
            && self.error.as_deref() == Some(Self::CANCELLED_BY_HOST)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub success: bool,
    pub sandbox_result: ExecutionResult,
    #[serde(default)]
    pub collected_outputs: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub signal: Option<i32>,
    #[serde(default)]
    pub time_used: f64,
    #[serde(default)]
    pub wall_time_used: f64,
    #[serde(default)]
    pub memory_used: Option<u32>,
    #[serde(default)]
    pub killed: bool,
    #[serde(default)]
    pub cg_oom_killed: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            exit_code: None,
            signal: None,
            time_used: 0.0,
            wall_time_used: 0.0,
            memory_used: None,
            killed: false,
            cg_oom_killed: false,
            status: "UNKNOWN".to_string(),
            message: String::new(),
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

pub type SandboxResult = ExecutionResult;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_kind_defaults_to_generic_for_legacy_payloads() {
        let step: Step = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "env_ref": "sandbox",
            "argv": ["echo", "ok"],
            "conf": {},
            "io": {
                "stdin": {"type": "null"},
                "stdout": {"type": "null"},
                "stderr": {"type": "null"}
            },
            "collect": []
        }))
        .unwrap();

        assert_eq!(step.kind, StepKind::Generic);
    }

    #[test]
    fn step_kind_uses_snake_case_wire_values() {
        let value = serde_json::to_value(StepKind::CheckerCompile).unwrap();
        assert_eq!(value, serde_json::json!("checker_compile"));
    }
}
