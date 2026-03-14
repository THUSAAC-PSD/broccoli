use broccoli_server_sdk::types::*;
use serde::Deserialize;

use crate::util::truncate;

#[cfg(feature = "wasm")]
use broccoli_server_sdk::host;

// submission_files (language: cpp17): ("Shape.cpp", "Shape.h")   additional_files: ("cpp17" -> [(language: "cpp17" source: "main.cpp"), (language: "make" source: "Makefile")])

/// Compiler configuration for a single language.
#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct CompilerConfig {
    pub compiler: String,
    pub flags: Vec<String>,
}

/// Testlib checker configuration, loaded from plugin global config.
/// All fields have sensible defaults so zero-config deployments work unchanged.
#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct TestlibConfig {
    pub cpp: CompilerConfig,
    pub c: CompilerConfig,
    pub compile_time_limit_s: f64,
    pub compile_memory_limit_kb: u32,
    pub run_time_limit_s: f64,
    pub run_memory_limit_kb: u32,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        // Defaults to C++ since it's the most common case.
        // Callers building a TestlibConfig override both `c` and `cpp` via
        // the TestlibConfig Default impl anyway.
        Self {
            compiler: "/usr/bin/g++".into(),
            flags: vec!["-O2".into(), "-std=c++17".into()],
        }
    }
}

impl Default for TestlibConfig {
    fn default() -> Self {
        Self {
            cpp: CompilerConfig {
                compiler: "/usr/bin/g++".into(),
                flags: vec!["-O2".into(), "-std=c++17".into()],
            },
            c: CompilerConfig {
                compiler: "/usr/bin/gcc".into(),
                flags: vec!["-O2".into(), "-std=c11".into()],
            },
            compile_time_limit_s: 10.0,
            compile_memory_limit_kb: 512 * 1024,
            run_time_limit_s: 5.0,
            run_memory_limit_kb: 256 * 1024,
        }
    }
}

#[cfg(feature = "wasm")]
fn load_testlib_config() -> TestlibConfig {
    match host::config::get_global_config("testlib") {
        Ok(r) => serde_json::from_value(r.config).unwrap_or_default(),
        Err(_) => TestlibConfig::default(),
    }
}

/// Pure interpretation of testlib exit codes.
pub fn interpret_testlib_exit_code(exit_code: i32, stderr: &str) -> CheckerVerdict {
    match exit_code {
        0 => CheckerVerdict {
            verdict: Verdict::Accepted,
            score: 1.0,
            message: extract_testlib_message(stderr),
        },
        1 => CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: extract_testlib_message(stderr),
        },
        2 => CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: extract_testlib_message(stderr).or_else(|| Some("Presentation error".into())),
        },
        3 => CheckerVerdict {
            verdict: Verdict::SystemError,
            score: 0.0,
            message: extract_testlib_message(stderr)
                .or_else(|| Some("Checker reported judge failure (exit code 3)".into())),
        },
        7 => {
            let (score, msg) = parse_testlib_partial(stderr);
            let verdict = if score >= 1.0 {
                Verdict::Accepted
            } else {
                Verdict::WrongAnswer
            };
            CheckerVerdict {
                verdict,
                score,
                message: msg,
            }
        }
        other => CheckerVerdict {
            verdict: Verdict::SystemError,
            score: 0.0,
            message: Some(format!("Checker exited with unexpected code {}", other)),
        },
    }
}

/// Dispatch testlib checker.
#[cfg(feature = "wasm")]
pub fn dispatch_testlib_checker(req: &CheckerParseInput) -> CheckerVerdict {
    let test_input = req.test_input.as_str();
    let expected_output = req.expected_output.as_str();

    let checker_source = match req.checker_source.as_ref() {
        Some(files) => files,
        None => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some("Testlib checker requires checker_source in metadata".into()),
            };
        }
    };

    if checker_source.is_empty() {
        return CheckerVerdict {
            verdict: Verdict::SystemError,
            score: 0.0,
            message: Some("checker_source is empty".into()),
        };
    }

    let files: Vec<(&str, &str)> = checker_source
        .iter()
        .map(|f| (f.filename.as_str(), f.content.as_str()))
        .collect();

    let config = load_testlib_config();

    let primary_file = files[0].0;
    let compile_cmd = match get_checker_compile_config(&config, primary_file) {
        Ok(cmd) => cmd,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(e),
            };
        }
    };

    let mut files_in = Vec::new();
    for (filename, content) in &files {
        files_in.push((
            (*filename).to_string(),
            SessionFile::Content {
                content: (*content).to_string(),
            },
        ));
    }
    files_in.push((
        "input.txt".to_string(),
        SessionFile::Content {
            content: test_input.to_string(),
        },
    ));
    files_in.push((
        "output.txt".to_string(),
        SessionFile::Content {
            content: req.stdout.clone(),
        },
    ));
    files_in.push((
        "answer.txt".to_string(),
        SessionFile::Content {
            content: expected_output.to_string(),
        },
    ));

    let key_inputs: Vec<String> = files.iter().map(|(f, _)| (*f).to_string()).collect();

    let operations = vec![OperationTask {
        environments: vec![Environment {
            id: "checker_sandbox".to_string(),
            files_in,
        }],
        tasks: vec![
            Step {
                id: "compile_checker".to_string(),
                env_ref: "checker_sandbox".to_string(),
                argv: compile_cmd,
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        time_limit: Some(config.compile_time_limit_s),
                        memory_limit: Some(config.compile_memory_limit_kb),
                        process_limit: Some(64),
                        ..Default::default()
                    },
                    env_rules: vec![EnvRule::FullEnv],
                    ..Default::default()
                },
                io: IOConfig {
                    stdin: IOTarget::Null,
                    stdout: IOTarget::File {
                        path: "checker_compile.log".to_string(),
                    },
                    stderr: IOTarget::File {
                        path: "checker_compile_err.log".to_string(),
                    },
                },
                collect: vec![
                    "checker".to_string(),
                    "checker_compile.log".to_string(),
                    "checker_compile_err.log".to_string(),
                ],
                depends_on: vec![],
                cache: Some(StepCacheConfig {
                    key_inputs,
                    outputs: vec!["checker".to_string()],
                }),
            },
            Step {
                id: "check".to_string(),
                env_ref: "checker_sandbox".to_string(),
                argv: vec![
                    "./checker".to_string(),
                    "input.txt".to_string(),
                    "output.txt".to_string(),
                    "answer.txt".to_string(),
                ],
                conf: RunOptions {
                    resource_limits: ResourceLimits {
                        time_limit: Some(config.run_time_limit_s),
                        memory_limit: Some(config.run_memory_limit_kb),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                io: IOConfig {
                    stdin: IOTarget::Null,
                    stdout: IOTarget::File {
                        path: "checker_out.txt".to_string(),
                    },
                    stderr: IOTarget::File {
                        path: "checker_err.txt".to_string(),
                    },
                },
                collect: vec!["checker_out.txt".to_string(), "checker_err.txt".to_string()],
                depends_on: vec!["compile_checker".to_string()],
                cache: None,
            },
        ],
        channels: vec![],
    }];

    let batch_id = match host::operations::start_batch_tasks(&operations) {
        Ok(id) => id,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(format!("Failed to dispatch checker operation: {:?}", e)),
            };
        }
    };

    let op_result = match host::operations::wait_for_result(&batch_id, 30000) {
        Ok(r) => r,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(format!("Failed to get checker result: {:?}", e)),
            };
        }
    };

    if !op_result.success && op_result.task_results.is_empty() {
        return CheckerVerdict {
            verdict: Verdict::SystemError,
            score: 0.0,
            message: op_result
                .error
                .or_else(|| Some("Checker operation failed".into())),
        };
    }

    if let Some(cc_result) = op_result.task_results.get("compile_checker") {
        if let Some(exit_code) = cc_result.sandbox_result.exit_code {
            if exit_code != 0 {
                let msg = if cc_result.sandbox_result.stderr.is_empty() {
                    "Checker compilation failed".to_string()
                } else {
                    truncate(&cc_result.sandbox_result.stderr, 4096)
                };
                return CheckerVerdict {
                    verdict: Verdict::SystemError,
                    score: 0.0,
                    message: Some(msg),
                };
            }
        }
    }

    match op_result.task_results.get("check") {
        Some(check_result) => interpret_testlib_exit_code(
            check_result.sandbox_result.exit_code.unwrap_or(-1),
            &check_result.sandbox_result.stderr,
        ),
        None => CheckerVerdict {
            verdict: Verdict::SystemError,
            score: 0.0,
            message: Some("Checker operation completed but no check result found".into()),
        },
    }
}

/// Resolve compile command from config and primary source filename.
pub fn get_checker_compile_config(
    config: &TestlibConfig,
    primary_filename: &str,
) -> Result<Vec<String>, String> {
    let lang = if primary_filename.ends_with(".cpp")
        || primary_filename.ends_with(".cc")
        || primary_filename.ends_with(".cxx")
    {
        &config.cpp
    } else if primary_filename.ends_with(".c") {
        &config.c
    } else {
        return Err(format!(
            "Unsupported checker source language for '{}'. Only C (.c) and C++ (.cpp/.cc/.cxx) are supported.",
            primary_filename
        ));
    };

    let mut cmd = vec![lang.compiler.clone()];
    cmd.extend(lang.flags.iter().cloned());
    cmd.push("-o".into());
    cmd.push("checker".into());
    cmd.push(primary_filename.into());
    Ok(cmd)
}

fn extract_testlib_message(stderr: &str) -> Option<String> {
    let msg = stderr.trim();
    if msg.is_empty() {
        None
    } else {
        Some(truncate(msg, 1024))
    }
}

pub fn parse_testlib_partial(stderr: &str) -> (f64, Option<String>) {
    let line = stderr.lines().next().unwrap_or("").trim();

    if let Some(rest) = line
        .strip_prefix("points ")
        .or_else(|| line.strip_prefix("points\t"))
    {
        let parts: Vec<&str> = rest.splitn(2, |c: char| c.is_whitespace()).collect();
        if let Some(score) = parts.first().and_then(|s| s.parse::<f64>().ok()) {
            let score = if score.is_finite() {
                score.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let message = parts.get(1).map(|m| truncate(m.trim(), 1024));
            return (score, message);
        }
    }

    if let Some(first_token) = line.split_whitespace().next() {
        if let Ok(score) = first_token.parse::<f64>() {
            let score = if score.is_finite() {
                score.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let rest = line[first_token.len()..].trim();
            let message = if rest.is_empty() {
                None
            } else {
                Some(truncate(rest, 1024))
            };
            return (score, message);
        }
    }

    (0.0, extract_testlib_message(stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_0_accepted() {
        let v = interpret_testlib_exit_code(0, "ok answer is correct\n");
        assert_eq!(v.verdict, Verdict::Accepted);
        assert_eq!(v.score, 1.0);
    }

    #[test]
    fn exit_1_wrong_answer() {
        let v = interpret_testlib_exit_code(1, "wrong answer expected 42, got 43\n");
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert_eq!(v.score, 0.0);
        assert!(v.message.unwrap().contains("expected 42"));
    }

    #[test]
    fn exit_2_presentation_error() {
        let v = interpret_testlib_exit_code(2, "");
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert!(v.message.unwrap().contains("Presentation error"));
    }

    #[test]
    fn exit_3_system_error() {
        let v = interpret_testlib_exit_code(3, "FAIL checker bug\n");
        assert_eq!(v.verdict, Verdict::SystemError);
        assert_eq!(v.score, 0.0);
    }

    #[test]
    fn exit_7_partial() {
        let v = interpret_testlib_exit_code(7, "points 0.5 partially correct\n");
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert_eq!(v.score, 0.5);
        assert_eq!(v.message.unwrap(), "partially correct");
    }

    #[test]
    fn exit_7_full_score() {
        let v = interpret_testlib_exit_code(7, "points 1.0 perfect\n");
        assert_eq!(v.verdict, Verdict::Accepted);
        assert_eq!(v.score, 1.0);
    }

    #[test]
    fn compile_config_cpp_defaults() {
        let config = TestlibConfig::default();
        let cmd = get_checker_compile_config(&config, "checker.cpp").unwrap();
        assert_eq!(cmd[0], "/usr/bin/g++");
        assert!(cmd.contains(&"checker.cpp".to_string()));
    }

    #[test]
    fn compile_config_c_defaults() {
        let config = TestlibConfig::default();
        let cmd = get_checker_compile_config(&config, "checker.c").unwrap();
        assert_eq!(cmd[0], "/usr/bin/gcc");
    }

    #[test]
    fn compile_config_unsupported() {
        let config = TestlibConfig::default();
        let err = get_checker_compile_config(&config, "checker.py").unwrap_err();
        assert!(err.contains("Unsupported"));
    }

    #[test]
    fn compile_config_custom_compiler() {
        let config = TestlibConfig {
            cpp: CompilerConfig {
                compiler: "/usr/local/bin/g++-13".into(),
                flags: vec!["-O3".into(), "-std=c++20".into()],
            },
            ..Default::default()
        };
        let cmd = get_checker_compile_config(&config, "checker.cpp").unwrap();
        assert_eq!(cmd[0], "/usr/local/bin/g++-13");
        assert!(cmd.contains(&"-O3".to_string()));
        assert!(cmd.contains(&"-std=c++20".to_string()));
        assert!(cmd.contains(&"checker.cpp".to_string()));
    }

    #[test]
    fn compile_config_cc_extension() {
        let config = TestlibConfig::default();
        let cmd = get_checker_compile_config(&config, "checker.cc").unwrap();
        assert_eq!(cmd[0], "/usr/bin/g++");
    }

    #[test]
    fn testlib_config_partial_deserialize() {
        let json = serde_json::json!({"cpp": {"compiler": "/opt/g++"}});
        let config: TestlibConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.cpp.compiler, "/opt/g++");
        // Unspecified fields use defaults
        assert_eq!(config.c.compiler, "/usr/bin/gcc");
        assert_eq!(config.compile_time_limit_s, 10.0);
    }
}
