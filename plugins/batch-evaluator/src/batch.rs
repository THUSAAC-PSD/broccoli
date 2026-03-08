use broccoli_server_sdk::types::BuildEvalOpsInput;
use serde::{Deserialize, Serialize};

/// Language configuration resolved by the host.
/// Mirrors `broccoli_server_sdk::host::language::ResolvedLanguage`
/// but defined locally so batch.rs stays testable without the `guest` feature.
#[derive(Debug, Clone, Deserialize)]
pub struct ResolvedLanguage {
    pub compile_cmd: Option<Vec<String>>,
    pub run_cmd: Vec<String>,
    pub source_filename: String,
    pub binary_name: String,
}

/// Environment configuration — represents a sandbox instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub files_in: Vec<(String, SessionFile)>,
}

/// File source for initial environment setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionFile {
    #[serde(rename = "content")]
    Content { content: String },
}

/// IO target for task stdin/stdout/stderr.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum IOTarget {
    #[serde(rename = "null")]
    Null,
    #[default]
    #[serde(rename = "inherit")]
    Inherit,
    #[serde(rename = "file")]
    File { path: String },
}

/// IO configuration for task execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IOConfig {
    pub stdin: IOTarget,
    pub stdout: IOTarget,
    pub stderr: IOTarget,
}

/// Resource limits for sandbox execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_time_limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_files_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_limit: Option<u32>,
}

/// Run configuration for a step.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunConfig {
    pub resource_limits: ResourceLimits,
    #[serde(default)]
    pub wait: bool,
    #[serde(default)]
    pub env_rules: Vec<serde_json::Value>,
}

/// Step-level cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCacheConfig {
    pub key_inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// A step (task) within an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub env_ref: String,
    pub argv: Vec<String>,
    pub conf: RunConfig,
    pub io: IOConfig,
    pub collect: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<StepCacheConfig>,
}

/// A single operation task dispatched to the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTask {
    pub environments: Vec<Environment>,
    pub tasks: Vec<Step>,
    #[serde(default)]
    pub channels: Vec<serde_json::Value>,
}

/// Build a sandbox OperationTask from enriched evaluator input.
///
/// Returns JSON string of `Vec<OperationTask>` (single-element array)
/// ready for `host::operations::start_batch()`.
pub fn build_operation(
    req: &BuildEvalOpsInput,
    lang: &ResolvedLanguage,
) -> Result<String, String> {
    let source = req
        .solution_source
        .first()
        .ok_or("No source file provided")?;

    // Environment: source file + test input
    let env = Environment {
        id: "sandbox".to_string(),
        files_in: vec![
            (
                lang.source_filename.clone(),
                SessionFile::Content {
                    content: source.content.clone(),
                },
            ),
            (
                "input.txt".to_string(),
                SessionFile::Content {
                    content: req.test_input.clone(),
                },
            ),
        ],
    };

    let time_limit_s = req.time_limit_ms as f64 / 1000.0;
    let memory_limit_kb = u32::try_from(req.memory_limit_kb)
        .map_err(|_| format!("Invalid memory_limit_kb: {}", req.memory_limit_kb))?;

    let mut steps = Vec::new();

    // Compile step (only for compiled languages)
    if let Some(compile_cmd) = &lang.compile_cmd {
        let compile_step = Step {
            id: "compile".to_string(),
            env_ref: "sandbox".to_string(),
            argv: compile_cmd.clone(),
            conf: RunConfig {
                resource_limits: ResourceLimits {
                    time_limit: Some(30.0),
                    wall_time_limit: Some(60.0),
                    memory_limit: Some(524_288), // 512 MB for compilation
                    process_limit: Some(32),     // compilers may fork
                    ..Default::default()
                },
                wait: true,
                env_rules: vec![],
            },
            io: IOConfig {
                stdin: IOTarget::Null,
                stdout: IOTarget::Null,
                stderr: IOTarget::File {
                    path: "compile_stderr.txt".to_string(),
                },
            },
            collect: vec![lang.binary_name.clone(), "compile_stderr.txt".to_string()],
            depends_on: vec![],
            cache: Some(StepCacheConfig {
                key_inputs: vec![lang.source_filename.clone()],
                outputs: vec![lang.binary_name.clone()],
            }),
        };
        steps.push(compile_step);
    }

    // Exec step
    let exec_depends = if lang.compile_cmd.is_some() {
        vec!["compile".to_string()]
    } else {
        vec![]
    };

    let exec_step = Step {
        id: "exec".to_string(),
        env_ref: "sandbox".to_string(),
        argv: lang.run_cmd.clone(),
        conf: RunConfig {
            resource_limits: ResourceLimits {
                time_limit: Some(time_limit_s),
                wall_time_limit: Some(time_limit_s * 3.0),
                memory_limit: Some(memory_limit_kb),
                process_limit: Some(1), // no forking for user code
                ..Default::default()
            },
            wait: true,
            env_rules: vec![],
        },
        io: IOConfig {
            stdin: IOTarget::File {
                path: "input.txt".to_string(),
            },
            stdout: IOTarget::File {
                path: "output.txt".to_string(),
            },
            stderr: IOTarget::File {
                path: "stderr.txt".to_string(),
            },
        },
        collect: vec!["output.txt".to_string(), "stderr.txt".to_string()],
        depends_on: exec_depends,
        cache: None,
    };
    steps.push(exec_step);

    let op = OperationTask {
        environments: vec![env],
        tasks: steps,
        channels: vec![],
    };

    serde_json::to_string(&vec![op]).map_err(|e| format!("Failed to serialize operation: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use broccoli_server_sdk::types::SourceFile;

    fn make_req() -> BuildEvalOpsInput {
        BuildEvalOpsInput {
            problem_id: 1,
            test_case_id: 42,
            solution_source: vec![SourceFile {
                filename: "main.cpp".to_string(),
                content: "int main() {}".to_string(),
            }],
            solution_language: "cpp".to_string(),
            time_limit_ms: 1000,
            memory_limit_kb: 262144,
            test_input: "hello\n".to_string(),
            expected_output: "world\n".to_string(),
            checker_format: Some("exact".to_string()),
            checker_config: None,
            checker_source: None,
        }
    }

    fn compiled_lang() -> ResolvedLanguage {
        ResolvedLanguage {
            compile_cmd: Some(vec![
                "/usr/bin/g++".to_string(),
                "-O2".to_string(),
                "solution.cpp".to_string(),
                "-o".to_string(),
                "solution".to_string(),
            ]),
            run_cmd: vec!["./solution".to_string()],
            source_filename: "solution.cpp".to_string(),
            binary_name: "solution".to_string(),
        }
    }

    fn interpreted_lang() -> ResolvedLanguage {
        ResolvedLanguage {
            compile_cmd: None,
            run_cmd: vec!["/usr/bin/python3".to_string(), "solution.py".to_string()],
            source_filename: "solution.py".to_string(),
            binary_name: "solution.py".to_string(),
        }
    }

    fn parse_ops(json: &str) -> Vec<OperationTask> {
        serde_json::from_str(json).expect("Failed to parse operation JSON")
    }

    #[test]
    fn compiled_language_produces_compile_and_exec_steps() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        assert_eq!(ops.len(), 1);
        let tasks = &ops[0].tasks;
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "compile");
        assert_eq!(tasks[1].id, "exec");
        assert_eq!(tasks[1].depends_on, vec!["compile"]);
    }

    #[test]
    fn interpreted_language_produces_only_exec_step() {
        let req = make_req();
        let lang = interpreted_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        assert_eq!(ops.len(), 1);
        let tasks = &ops[0].tasks;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "exec");
        assert!(tasks[0].depends_on.is_empty());
    }

    #[test]
    fn test_input_wired_to_environment_files() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let env = &ops[0].environments[0];
        let input_file = env
            .files_in
            .iter()
            .find(|(name, _)| name == "input.txt")
            .expect("input.txt not found in environment");
        match &input_file.1 {
            SessionFile::Content { content } => {
                assert_eq!(content, "hello\n");
            }
        }
    }

    #[test]
    fn source_file_placed_with_correct_filename() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let env = &ops[0].environments[0];
        let source_file = env
            .files_in
            .iter()
            .find(|(name, _)| name == "solution.cpp")
            .expect("source file not found");
        match &source_file.1 {
            SessionFile::Content { content } => {
                assert_eq!(content, "int main() {}");
            }
        }
    }

    #[test]
    fn compile_step_has_cache_config() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let compile = &ops[0].tasks[0];
        let cache = compile.cache.as_ref().expect("compile step missing cache config");
        assert_eq!(cache.key_inputs, vec!["solution.cpp"]);
        assert_eq!(cache.outputs, vec!["solution"]);
    }

    #[test]
    fn time_limit_converted_from_ms_to_seconds() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.time_limit, Some(1.0));
    }

    #[test]
    fn memory_limit_passed_through() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.memory_limit, Some(262144));
    }

    #[test]
    fn no_source_file_returns_error() {
        let mut req = make_req();
        req.solution_source.clear();
        let lang = compiled_lang();
        let result = build_operation(&req, &lang);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No source file"));
    }

    #[test]
    fn exec_step_collects_stdout_and_stderr() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let exec = &ops[0].tasks[1];
        assert!(exec.collect.contains(&"output.txt".to_string()));
        assert!(exec.collect.contains(&"stderr.txt".to_string()));
    }

    #[test]
    fn env_rules_are_empty() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        // Compile step
        assert!(ops[0].tasks[0].conf.env_rules.is_empty(), "compile step must not leak env vars");
        // Exec step
        assert!(ops[0].tasks[1].conf.env_rules.is_empty(), "exec step must not leak env vars");
    }

    #[test]
    fn exec_step_has_process_limit() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.process_limit, Some(1));
    }

    #[test]
    fn compile_step_has_process_limit() {
        let req = make_req();
        let lang = compiled_lang();
        let json = build_operation(&req, &lang).unwrap();
        let ops = parse_ops(&json);

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.process_limit, Some(32));
    }

    #[test]
    fn negative_memory_limit_returns_error() {
        let mut req = make_req();
        req.memory_limit_kb = -1;
        let lang = compiled_lang();
        let result = build_operation(&req, &lang);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory_limit_kb"));
    }
}
