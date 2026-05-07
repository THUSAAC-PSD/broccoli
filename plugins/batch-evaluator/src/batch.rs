use broccoli_server_sdk::types::{
    BuildEvalOpsInput, Environment, EvaluationTimeoutBudget, IOConfig, IOTarget, JudgeFile,
    OperationTask, OutputSpec, ResolveLanguageOutput, ResourceLimits, RunOptions, SessionFile,
    Step, StepCacheConfig, seconds_from_ms,
};
use serde::Deserialize;
use std::collections::HashSet;

/// Admin-configurable sandbox resource limits.
/// All fields have sensible defaults so zero-config deployments work unchanged.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    pub compile_time_limit_s: f64,
    pub compile_wall_time_multiplier: f64,
    pub compile_extra_time_s: f64,
    pub compile_memory_limit_kb: u32,
    pub compile_stack_limit_kb: u32,
    pub compile_process_limit: u32,
    pub compile_open_files_limit: u32,
    pub compile_file_size_limit_kb: u32,
    pub exec_extra_time_s: f64,
    pub exec_stack_limit_kb: u32,
    pub exec_process_limit: u32,
    pub exec_open_files_limit: u32,
    pub exec_file_size_limit_kb: u32,
    pub exec_wall_time_multiplier: f64,
    pub result_timeout_ms: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            compile_time_limit_s: 30.0,
            compile_wall_time_multiplier: 2.0,
            compile_extra_time_s: 0.0,
            compile_memory_limit_kb: 524_288, // 512 MB
            compile_stack_limit_kb: 0,
            compile_process_limit: 32,
            compile_open_files_limit: 256,
            compile_file_size_limit_kb: 524_288, // 512 MB
            exec_extra_time_s: 0.0,
            exec_stack_limit_kb: 0,
            exec_process_limit: 1,
            exec_open_files_limit: 64,
            exec_file_size_limit_kb: 65_536, // 64 MB
            exec_wall_time_multiplier: 3.0,
            result_timeout_ms: EvaluationTimeoutBudget::default_for_time_limit_ms(0)
                .minimum_timeout_ms,
        }
    }
}

impl SandboxConfig {
    /// Build ResourceLimits for the compilation step.
    pub fn compile_limits(&self) -> ResourceLimits {
        ResourceLimits {
            time_limit: Some(self.compile_time_limit_s),
            wall_time_limit: Some(self.compile_time_limit_s * self.compile_wall_time_multiplier),
            extra_time: if self.compile_extra_time_s > 0.0 {
                Some(self.compile_extra_time_s)
            } else {
                None
            },
            memory_limit: Some(self.compile_memory_limit_kb),
            stack_limit: limit_if_positive(self.compile_stack_limit_kb),
            process_limit: Some(self.compile_process_limit),
            open_files_limit: Some(self.compile_open_files_limit),
            file_size_limit: Some(self.compile_file_size_limit_kb),
            ..Default::default()
        }
    }

    /// Build ResourceLimits for the execution step.
    pub fn exec_limits(&self, time_limit_s: f64, memory_limit_kb: u32) -> ResourceLimits {
        ResourceLimits {
            time_limit: Some(time_limit_s),
            wall_time_limit: Some(time_limit_s * self.exec_wall_time_multiplier),
            extra_time: if self.exec_extra_time_s > 0.0 {
                Some(self.exec_extra_time_s)
            } else {
                None
            },
            memory_limit: Some(memory_limit_kb),
            stack_limit: limit_if_positive(self.exec_stack_limit_kb),
            process_limit: Some(self.exec_process_limit),
            open_files_limit: Some(self.exec_open_files_limit),
            file_size_limit: Some(self.exec_file_size_limit_kb),
            ..Default::default()
        }
    }

    pub fn result_timeout_ms_for(&self, time_limit_ms: i32, compile_units: u32) -> u64 {
        EvaluationTimeoutBudget {
            compile_units,
            compile_time_limit_s: self.compile_time_limit_s,
            compile_wall_time_multiplier: self.compile_wall_time_multiplier,
            compile_extra_time_s: self.compile_extra_time_s,
            exec_time_limit_s: seconds_from_ms(time_limit_ms),
            exec_wall_time_multiplier: self.exec_wall_time_multiplier,
            exec_extra_time_s: self.exec_extra_time_s,
            minimum_timeout_ms: self.result_timeout_ms.max(
                EvaluationTimeoutBudget::default_for_time_limit_ms(time_limit_ms)
                    .minimum_timeout_ms,
            ),
            maximum_timeout_ms: self.result_timeout_ms.max(
                EvaluationTimeoutBudget::default_for_time_limit_ms(time_limit_ms)
                    .maximum_timeout_ms,
            ),
            ..EvaluationTimeoutBudget::default_for_time_limit_ms(time_limit_ms)
        }
        .timeout_ms()
    }
}

fn limit_if_positive(value: u32) -> Option<u32> {
    if value > 0 { Some(value) } else { None }
}

/// Build a sandbox OperationTask from enriched evaluator input.
///
/// Returns `Vec<OperationTask>` ready for `host.operations.start_batch()`.
pub fn build_operation(
    req: &BuildEvalOpsInput,
    lang: &ResolveLanguageOutput,
    config: &SandboxConfig,
) -> Result<Vec<OperationTask>, String> {
    if req.solution_source.is_empty() {
        return Err("No source file provided".into());
    }

    let mut files_in = Vec::new();
    let mut seen_filenames = HashSet::new();

    for af in &req.additional_file_refs {
        if seen_filenames.insert(af.filename.clone()) {
            files_in.push((
                af.filename.clone(),
                SessionFile::Blob {
                    hash: af.blob_hash.clone(),
                },
            ));
        }
    }

    for source in &req.solution_source {
        if seen_filenames.insert(source.filename.clone()) {
            files_in.push((
                source.filename.clone(),
                SessionFile::Content {
                    content: source.content.clone(),
                },
            ));
        }
    }

    files_in.push((
        "input.txt".to_string(),
        session_file_from_judge_file(&req.test_input),
    ));

    let env = Environment {
        id: "sandbox".to_string(),
        files_in,
    };

    let time_limit_ms = u32::try_from(req.time_limit_ms)
        .map_err(|_| format!("Invalid time_limit_ms: {}", req.time_limit_ms))?;
    let time_limit_s = time_limit_ms as f64 / 1000.0;
    let memory_limit_kb = u32::try_from(req.memory_limit_kb)
        .map_err(|_| format!("Invalid memory_limit_kb: {}", req.memory_limit_kb))?;

    let mut steps = Vec::new();

    // Compile step (only for compiled languages)
    if let Some(compile) = &lang.compile {
        let cache_outputs: Vec<String> = compile
            .outputs
            .iter()
            .map(|o| match o {
                OutputSpec::File(f) => f.clone(),
                OutputSpec::Glob(g) => g.clone(),
            })
            .collect();

        let mut collect = cache_outputs.clone();
        collect.push("compile_stderr.txt".to_string());

        let compile_step = Step {
            id: "compile".to_string(),
            env_ref: "sandbox".to_string(),
            argv: compile.command.clone(),
            conf: RunOptions {
                resource_limits: compile
                    .resource_limits
                    .clone()
                    .unwrap_or_else(|| config.compile_limits()),
                wait: true,
                env_rules: vec![],
                ..Default::default()
            },
            io: IOConfig {
                stdin: IOTarget::Null,
                stdout: IOTarget::Null,
                stderr: IOTarget::File {
                    path: "compile_stderr.txt".to_string(),
                },
            },
            collect,
            depends_on: vec![],
            cache: Some(StepCacheConfig {
                key_inputs: compile.cache_inputs.clone(),
                outputs: cache_outputs,
            }),
        };
        steps.push(compile_step);
    }

    // Exec step
    let exec_depends = if lang.compile.is_some() {
        vec!["compile".to_string()]
    } else {
        vec![]
    };

    let exec_step = Step {
        id: "exec".to_string(),
        env_ref: "sandbox".to_string(),
        argv: lang.run.command.clone(),
        conf: RunOptions {
            resource_limits: config.exec_limits(time_limit_s, memory_limit_kb),
            wait: true,
            env_rules: vec![],
            ..Default::default()
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
        priority: None,
        target_worker_id: req.target_worker_id.clone(),
    };

    Ok(vec![op])
}

fn session_file_from_judge_file(file: &JudgeFile) -> SessionFile {
    match file {
        JudgeFile::Blob { file } => SessionFile::Blob {
            hash: file.blob_hash.clone(),
        },
        JudgeFile::Inline { text } => SessionFile::Content {
            content: text.clone(),
        },
        JudgeFile::Missing => SessionFile::Content {
            content: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broccoli_server_sdk::types::{CompileSpec, FileRef, JudgeFile, RunSpec, SourceFile};

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
            contest_id: None,
            test_input: JudgeFile::inline("hello\n"),
            expected_output: JudgeFile::inline("world\n"),
            checker_format: Some("exact".to_string()),
            checker_config: None,
            checker_source: None,
            additional_file_refs: vec![],
            target_worker_id: None,
        }
    }

    fn compiled_lang() -> ResolveLanguageOutput {
        ResolveLanguageOutput {
            compile: Some(CompileSpec {
                command: vec![
                    "/usr/bin/g++".to_string(),
                    "-O2".to_string(),
                    "solution.cpp".to_string(),
                    "-o".to_string(),
                    "solution".to_string(),
                ],
                cache_inputs: vec!["main.cpp".to_string(), "solution.cpp".to_string()],
                outputs: vec![OutputSpec::File("solution".to_string())],
                resource_limits: None,
            }),
            run: RunSpec {
                command: vec!["./solution".to_string()],
                extra_files: vec![],
            },
        }
    }

    fn interpreted_lang() -> ResolveLanguageOutput {
        ResolveLanguageOutput {
            compile: None,
            run: RunSpec {
                command: vec!["/usr/bin/python3".to_string(), "solution.py".to_string()],
                extra_files: vec!["solution.py".to_string()],
            },
        }
    }

    fn default_config() -> SandboxConfig {
        SandboxConfig::default()
    }

    #[test]
    fn compiled_language_produces_compile_and_exec_steps() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        assert_eq!(ops.len(), 1);
        let tasks = &ops[0].tasks;
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "compile");
        assert_eq!(tasks[1].id, "exec");
        assert_eq!(tasks[1].depends_on, vec!["compile"]);
    }

    #[test]
    fn interpreted_language_produces_only_exec_step() {
        let ops = build_operation(&make_req(), &interpreted_lang(), &default_config()).unwrap();

        assert_eq!(ops.len(), 1);
        let tasks = &ops[0].tasks;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "exec");
        assert!(tasks[0].depends_on.is_empty());
    }

    #[test]
    fn test_input_wired_to_environment_files() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

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
            _ => panic!("expected inline content for input.txt"),
        }
    }

    #[test]
    fn test_input_blob_ref_wired_to_environment_files() {
        let mut req = make_req();
        req.test_input = JudgeFile::blob(FileRef {
            filename: "input.txt".to_string(),
            content_type: Some("text/plain".to_string()),
            blob_hash: "abc123".to_string(),
            read_token: None,
        });

        let ops = build_operation(&req, &compiled_lang(), &default_config()).unwrap();

        let env = &ops[0].environments[0];
        let input_file = env
            .files_in
            .iter()
            .find(|(name, _)| name == "input.txt")
            .expect("input.txt not found in environment");
        match &input_file.1 {
            SessionFile::Blob { hash } => {
                assert_eq!(hash, "abc123");
            }
            _ => panic!("expected blob ref for input.txt"),
        }
    }

    #[test]
    fn source_file_placed_with_correct_filename() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let env = &ops[0].environments[0];
        let source_file = env
            .files_in
            .iter()
            .find(|(name, _)| name == "main.cpp")
            .expect("source file not found");
        match &source_file.1 {
            SessionFile::Content { content } => {
                assert_eq!(content, "int main() {}");
            }
            _ => panic!("expected inline content for source file"),
        }
    }

    #[test]
    fn multi_file_submissions_keep_all_files_in_the_environment() {
        let mut req = make_req();
        req.solution_source.push(SourceFile {
            filename: "helper.hpp".to_string(),
            content: "// helper".to_string(),
        });

        let ops = build_operation(&req, &compiled_lang(), &default_config()).unwrap();
        let env = &ops[0].environments[0];

        assert!(env.files_in.iter().any(|(name, _)| name == "main.cpp"));
        assert!(env.files_in.iter().any(|(name, _)| name == "helper.hpp"));

        let compile = &ops[0].tasks[0];
        let cache = compile
            .cache
            .as_ref()
            .expect("compile step missing cache config");
        assert_eq!(
            cache.key_inputs,
            vec!["main.cpp".to_string(), "solution.cpp".to_string(),]
        );
    }

    #[test]
    fn compile_step_has_cache_config() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let compile = &ops[0].tasks[0];
        let cache = compile
            .cache
            .as_ref()
            .expect("compile step missing cache config");
        assert_eq!(
            cache.key_inputs,
            vec!["main.cpp".to_string(), "solution.cpp".to_string()]
        );
        assert_eq!(cache.outputs, vec!["solution"]);
    }

    #[test]
    fn time_limit_converted_from_ms_to_seconds() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.time_limit, Some(1.0));
    }

    #[test]
    fn memory_limit_passed_through() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.memory_limit, Some(262144));
    }

    #[test]
    fn no_source_file_returns_error() {
        let mut req = make_req();
        req.solution_source.clear();
        let result = build_operation(&req, &compiled_lang(), &default_config());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No source file"));
    }

    #[test]
    fn exec_step_collects_stdout_and_stderr() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert!(exec.collect.contains(&"output.txt".to_string()));
        assert!(exec.collect.contains(&"stderr.txt".to_string()));
    }

    #[test]
    fn env_rules_are_empty() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        assert!(ops[0].tasks[0].conf.env_rules.is_empty());
        assert!(ops[0].tasks[1].conf.env_rules.is_empty());
    }

    #[test]
    fn exec_step_has_process_limit() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.process_limit, Some(1));
    }

    #[test]
    fn compile_step_has_process_limit() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.process_limit, Some(32));
    }

    #[test]
    fn negative_memory_limit_returns_error() {
        let mut req = make_req();
        req.memory_limit_kb = -1;
        let result = build_operation(&req, &compiled_lang(), &default_config());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid memory_limit_kb"));
    }

    #[test]
    fn negative_time_limit_returns_error() {
        let mut req = make_req();
        req.time_limit_ms = -1;
        let result = build_operation(&req, &compiled_lang(), &default_config());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid time_limit_ms"));
    }

    #[test]
    fn exec_step_has_file_size_limit() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.file_size_limit, Some(65_536));
    }

    #[test]
    fn exec_step_has_open_files_limit() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.open_files_limit, Some(64));
    }

    #[test]
    fn compile_step_has_file_and_fd_limits() {
        let ops = build_operation(&make_req(), &compiled_lang(), &default_config()).unwrap();

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.file_size_limit, Some(524_288));
        assert_eq!(compile.conf.resource_limits.open_files_limit, Some(256));
    }

    #[test]
    fn partial_config_deserializes_with_defaults() {
        let json = r#"{ "exec_process_limit": 4 }"#;
        let config: SandboxConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.exec_process_limit, 4);
        // All other fields should be their defaults
        assert_eq!(config.compile_time_limit_s, 30.0);
        assert_eq!(config.compile_wall_time_multiplier, 2.0);
        assert_eq!(config.compile_extra_time_s, 0.0);
        assert_eq!(config.compile_memory_limit_kb, 524_288);
        assert_eq!(config.exec_extra_time_s, 0.0);
        assert_eq!(config.exec_wall_time_multiplier, 3.0);
        assert_eq!(config.compile_stack_limit_kb, 0);
        assert_eq!(config.exec_stack_limit_kb, 0);
        assert_eq!(config.result_timeout_ms, 900_000);
    }

    #[test]
    fn empty_config_deserializes_to_defaults() {
        let json = "{}";
        let config: SandboxConfig = serde_json::from_str(json).unwrap();
        let default = SandboxConfig::default();
        assert_eq!(config.compile_time_limit_s, default.compile_time_limit_s);
        assert_eq!(config.exec_process_limit, default.exec_process_limit);
        assert_eq!(config.result_timeout_ms, default.result_timeout_ms);
    }

    #[test]
    fn custom_config_overrides_defaults() {
        let config = SandboxConfig {
            exec_process_limit: 4,
            exec_wall_time_multiplier: 5.0,
            compile_time_limit_s: 60.0,
            ..SandboxConfig::default()
        };
        let ops = build_operation(&make_req(), &compiled_lang(), &config).unwrap();

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.time_limit, Some(60.0));

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.process_limit, Some(4));
        // 1000ms = 1.0s, wall_time = 1.0 * 5.0 = 5.0s
        assert_eq!(exec.conf.resource_limits.wall_time_limit, Some(5.0));
    }

    #[test]
    fn compile_limits_use_configured_stack_limit() {
        let config = SandboxConfig {
            compile_stack_limit_kb: 262_144,
            ..SandboxConfig::default()
        };
        let ops = build_operation(&make_req(), &compiled_lang(), &config).unwrap();

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.stack_limit, Some(262_144));
    }

    #[test]
    fn exec_limits_use_configured_stack_limit() {
        let config = SandboxConfig {
            exec_stack_limit_kb: 262_144,
            ..SandboxConfig::default()
        };
        let ops = build_operation(&make_req(), &compiled_lang(), &config).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.stack_limit, Some(262_144));
    }

    #[test]
    fn compile_limits_use_configured_wall_time_multiplier_and_extra_time() {
        let config = SandboxConfig {
            compile_time_limit_s: 40.0,
            compile_wall_time_multiplier: 3.5,
            compile_extra_time_s: 1.5,
            ..SandboxConfig::default()
        };
        let ops = build_operation(&make_req(), &compiled_lang(), &config).unwrap();

        let compile = &ops[0].tasks[0];
        assert_eq!(compile.conf.resource_limits.time_limit, Some(40.0));
        assert_eq!(compile.conf.resource_limits.wall_time_limit, Some(140.0));
        assert_eq!(compile.conf.resource_limits.extra_time, Some(1.5));
    }

    #[test]
    fn exec_limits_use_configured_extra_time() {
        let config = SandboxConfig {
            exec_extra_time_s: 2.5,
            ..SandboxConfig::default()
        };
        let ops = build_operation(&make_req(), &compiled_lang(), &config).unwrap();

        let exec = &ops[0].tasks[1];
        assert_eq!(exec.conf.resource_limits.extra_time, Some(2.5));
    }

    #[test]
    fn result_timeout_uses_configured_value_as_floor() {
        let config = SandboxConfig {
            result_timeout_ms: 1_200_000,
            ..SandboxConfig::default()
        };

        assert_eq!(config.result_timeout_ms_for(1000, 1), 1_200_000);
    }

    #[test]
    fn result_timeout_scales_with_worst_case_wall_budget() {
        let config = SandboxConfig {
            compile_time_limit_s: 120.0,
            compile_wall_time_multiplier: 3.0,
            exec_wall_time_multiplier: 5.0,
            ..SandboxConfig::default()
        };

        assert!(config.result_timeout_ms_for(300_000, 1) > config.result_timeout_ms);
    }
}
