use broccoli_server_sdk::types::*;
use serde::Deserialize;

use crate::util::truncate;

#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::Host;

/// Compiler configuration for checker binaries (`standard-languages` plugin).
#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct CheckerCompilerConfig {
    pub compiler: String,
    pub flags: Vec<String>,
}

impl Default for CheckerCompilerConfig {
    fn default() -> Self {
        Self {
            compiler: "/usr/bin/g++".into(),
            flags: vec!["-O2".into(), "-std=c++17".into()],
        }
    }
}

/// Testlib checker configuration, loaded from plugin global config.
#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct TestlibConfig {
    pub cpp: CheckerCompilerConfig,
    pub c: CheckerCompilerConfig,
    pub compile_time_limit_s: f64,
    pub compile_memory_limit_kb: u32,
    pub run_time_limit_s: f64,
    pub run_memory_limit_kb: u32,
}

impl Default for TestlibConfig {
    fn default() -> Self {
        Self {
            cpp: CheckerCompilerConfig {
                compiler: "/usr/bin/g++".into(),
                flags: vec!["-O2".into(), "-std=c++17".into()],
            },
            c: CheckerCompilerConfig {
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

/// Map checker source file extension to a language ID for the resolver.
pub fn checker_language_id(primary_filename: &str) -> Result<&str, String> {
    if primary_filename.ends_with(".cpp")
        || primary_filename.ends_with(".cc")
        || primary_filename.ends_with(".cxx")
    {
        Ok("cpp")
    } else if primary_filename.ends_with(".c") {
        Ok("c")
    } else {
        Err(format!(
            "Unsupported checker source language for '{}'. Only C (.c) and C++ (.cpp/.cc/.cxx) are supported.",
            primary_filename
        ))
    }
}

#[cfg(target_arch = "wasm32")]
fn load_testlib_config(host: &Host) -> TestlibConfig {
    match host.config.get_global("testlib") {
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
#[cfg(target_arch = "wasm32")]
pub fn dispatch_testlib_checker(host: &Host, req: &CheckerParseInput) -> CheckerVerdict {
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

    let filenames: Vec<String> = checker_source.iter().map(|f| f.filename.clone()).collect();
    let primary_file = &filenames[0];

    let lang_id = match checker_language_id(primary_file) {
        Ok(id) => id,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(e),
            };
        }
    };

    let config = load_testlib_config(host);

    let checker_compiler = match lang_id {
        "cpp" => &config.cpp,
        "c" => &config.c,
        _ => &config.cpp, // unreachable for testlib (always C/C++)
    };

    let resolved = match host.language.resolve(&ResolveLanguageInput {
        language_id: lang_id.to_string(),
        submitted_files: filenames,
        additional_files: vec![],
        problem_id: None,
        contest_id: None,
        overrides: Some(serde_json::json!({
            "compiler": checker_compiler.compiler,
            "flags": checker_compiler.flags,
        })),
    }) {
        Ok(r) => r,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(format!("Failed to resolve checker language: {e}")),
            };
        }
    };

    let mut files_in = Vec::new();
    for source in checker_source {
        files_in.push((
            source.filename.clone(),
            SessionFile::Content {
                content: source.content.clone(),
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

    let mut steps = Vec::new();

    if let Some(compile) = &resolved.compile {
        let cache_outputs: Vec<String> = compile
            .outputs
            .iter()
            .map(|o| match o {
                OutputSpec::File(f) => f.clone(),
                OutputSpec::Glob(g) => g.clone(),
            })
            .collect();
        let mut collect = cache_outputs.clone();
        collect.push("checker_compile.log".to_string());
        collect.push("checker_compile_err.log".to_string());

        steps.push(Step {
            id: "compile_checker".to_string(),
            env_ref: "checker_sandbox".to_string(),
            argv: compile.command.clone(),
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
            collect,
            depends_on: vec![],
            cache: Some(StepCacheConfig {
                key_inputs: compile.cache_inputs.clone(),
                outputs: cache_outputs,
            }),
        });
    }

    let checker_binary = resolved
        .compile
        .as_ref()
        .and_then(|c| c.outputs.first())
        .map(|o| match o {
            OutputSpec::File(f) => format!("./{f}"),
            OutputSpec::Glob(_) => "./checker".to_string(), // unreachable since it's always C/C++
        })
        .unwrap_or_else(|| "./checker".to_string());

    steps.push(Step {
        id: "check".to_string(),
        env_ref: "checker_sandbox".to_string(),
        argv: vec![
            checker_binary,
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
        depends_on: if resolved.compile.is_some() {
            vec!["compile_checker".to_string()]
        } else {
            vec![]
        },
        cache: None,
    });

    let operations = vec![OperationTask {
        environments: vec![Environment {
            id: "checker_sandbox".to_string(),
            files_in,
        }],
        tasks: steps,
        channels: vec![],
        priority: None,
    }];

    let batch_id = match host.operations.start_batch(&operations) {
        Ok(id) => id,
        Err(e) => {
            return CheckerVerdict {
                verdict: Verdict::SystemError,
                score: 0.0,
                message: Some(format!("Failed to dispatch checker operation: {:?}", e)),
            };
        }
    };

    let op_result = match host.operations.next_result(&batch_id, 30000) {
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
            let rest = line.get(first_token.len()..).unwrap_or("").trim();
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
    fn checker_language_id_cpp() {
        assert_eq!(checker_language_id("checker.cpp").unwrap(), "cpp");
        assert_eq!(checker_language_id("checker.cc").unwrap(), "cpp");
        assert_eq!(checker_language_id("checker.cxx").unwrap(), "cpp");
    }

    #[test]
    fn checker_language_id_c() {
        assert_eq!(checker_language_id("checker.c").unwrap(), "c");
    }

    #[test]
    fn checker_language_id_unsupported() {
        let err = checker_language_id("checker.py").unwrap_err();
        assert!(err.contains("Unsupported"));
    }

    #[test]
    fn testlib_config_partial_deserialize() {
        let json = serde_json::json!({"compile_time_limit_s": 20.0});
        let config: TestlibConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.compile_time_limit_s, 20.0);
        // Unspecified fields use defaults
        assert_eq!(config.run_time_limit_s, 5.0);
        assert_eq!(config.compile_memory_limit_kb, 512 * 1024);
    }
}
