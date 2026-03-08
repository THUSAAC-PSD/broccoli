use crate::error::SdkError;
use crate::host;
use crate::types::*;

/// Interpret a sandbox operation result into a TestCaseVerdict.
pub fn interpret_sandbox_result(
    test_case_id: i32,
    result: &OperationResult,
    checker_format: &str,
    checker_input: &CheckerParseInput,
) -> Result<TestCaseVerdict, SdkError> {
    if !result.success && result.task_results.is_empty() {
        return Ok(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::JudgeError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: result.error.clone(),
        });
    }

    if let Some(compile_result) = result.task_results.get("compile") {
        if let Some(exit_code) = compile_result.sandbox_result.exit_code {
            if exit_code != 0 {
                return Ok(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::CompileError,
                    score: 0.0,
                    time_used_ms: None,
                    memory_used_kb: None,
                    message: truncate_stderr(
                        &compile_result.sandbox_result.stderr,
                        "Compilation failed",
                    ),
                });
            }
        } else if !compile_result.success {
            // Sandbox failure before execution (no exit code available)
            return Ok(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::JudgeError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: truncate_stderr(
                    &compile_result.sandbox_result.stderr,
                    "Compilation step failed (sandbox error)",
                ),
            });
        }
    }

    if let Some(exec_result) = result.task_results.get("exec") {
        let sandbox = &exec_result.sandbox_result;

        if !exec_result.success && sandbox.status.is_empty() {
            return Ok(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::JudgeError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: Some("Execution step was skipped".into()),
            });
        }

        if sandbox.cg_oom_killed {
            return Ok(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::MemoryLimitExceeded,
                score: 0.0,
                time_used_ms: extract_time_used(result),
                memory_used_kb: extract_memory_used(result),
                message: Some(format!(
                    "Memory limit exceeded ({}KB)",
                    sandbox.memory_used.unwrap_or(0)
                )),
            });
        }

        match sandbox.status.as_str() {
            "TO" => {
                let time_ms = extract_time_used(result);
                return Ok(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::TimeLimitExceeded,
                    score: 0.0,
                    time_used_ms: time_ms,
                    memory_used_kb: extract_memory_used(result),
                    message: Some(format!(
                        "Time limit exceeded ({}ms)",
                        time_ms.map_or("?".into(), |t| t.to_string())
                    )),
                });
            }
            "SG" => {
                return Ok(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::RuntimeError,
                    score: 0.0,
                    time_used_ms: extract_time_used(result),
                    memory_used_kb: extract_memory_used(result),
                    message: Some(sandbox.message.clone()),
                });
            }
            "RE" => {
                return Ok(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::RuntimeError,
                    score: 0.0,
                    time_used_ms: extract_time_used(result),
                    memory_used_kb: extract_memory_used(result),
                    message: Some(format!("Exit code: {}", sandbox.exit_code.unwrap_or(-1))),
                });
            }
            _ => {} // "OK" or other, continue to checker
        }
    }

    let exec_result = match result.task_results.get("exec") {
        Some(r) => r,
        None => {
            return Ok(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::JudgeError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: Some("No exec result found".into()),
            });
        }
    };

    let mut input = checker_input.clone();
    input.stdout = exec_result.sandbox_result.stdout.clone();
    input.stderr = exec_result.sandbox_result.stderr.clone();
    input.exit_code = exec_result.sandbox_result.exit_code.unwrap_or(-1);

    let time_used_ms = extract_time_used(result);
    let memory_used_kb = extract_memory_used(result);

    match host::checker::run_checker(checker_format, &input) {
        Ok(v) => Ok(TestCaseVerdict {
            test_case_id,
            verdict: v.verdict,
            score: v.score,
            time_used_ms,
            memory_used_kb,
            message: v.message,
        }),
        Err(e) => Ok(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::JudgeError,
            score: 0.0,
            time_used_ms,
            memory_used_kb,
            message: Some(format!("Checker call failed: {:?}", e)),
        }),
    }
}

fn extract_time_used(result: &OperationResult) -> Option<i64> {
    result.task_results.get("exec").and_then(|exec_result| {
        let t = exec_result.sandbox_result.time_used;
        if t > 0.0 && t.is_finite() && t < (i64::MAX as f64 / 1000.0) {
            Some((t * 1000.0) as i64)
        } else {
            None
        }
    })
}

fn extract_memory_used(result: &OperationResult) -> Option<i64> {
    result
        .task_results
        .get("exec")
        .and_then(|exec_result| exec_result.sandbox_result.memory_used.map(|m| m as i64))
}

fn truncate_stderr(stderr: &str, fallback: &str) -> Option<String> {
    if stderr.is_empty() {
        Some(fallback.into())
    } else if stderr.chars().count() <= 4096 {
        Some(stderr.to_string())
    } else {
        Some(format!(
            "{}... (truncated)",
            stderr.chars().take(4096).collect::<String>()
        ))
    }
}
