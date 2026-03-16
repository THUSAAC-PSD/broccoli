use broccoli_server_sdk::types::{OperationResult, TestCaseVerdict, Verdict};

/// Interpret the communication operation result into a TestCaseVerdict.
pub fn interpret_result(
    test_case_id: i32,
    result: &OperationResult,
    num_processes: usize,
    req_memory_limit_kb: u32,
) -> TestCaseVerdict {
    // Operation-level failure with no results
    if !result.success && result.task_results.is_empty() {
        return TestCaseVerdict {
            test_case_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: result.error.clone().or(Some("Operation failed".into())),
            stdout: None,
            stderr: None,
        };
    }

    if let Some(compile_mgr) = result.task_results.get("compile_manager") {
        if !compile_mgr.success {
            return TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: Some(truncate(
                    &compile_mgr.sandbox_result.stderr,
                    "Manager compilation failed",
                )),
                stdout: None,
                stderr: None,
            };
        }
    }

    for i in 0..num_processes {
        let step_id = format!("compile_contestant_{i}");
        if let Some(compile_c) = result.task_results.get(&step_id) {
            if !compile_c.success {
                return TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::CompileError,
                    score: 0.0,
                    time_used_ms: None,
                    memory_used_kb: None,
                    message: Some(truncate(
                        &compile_c.sandbox_result.stderr,
                        "Compilation failed",
                    )),
                    stdout: None,
                    stderr: None,
                };
            }
        }
    }

    let mut total_time_s: f64 = 0.0;
    let mut max_memory_kb: Option<u32> = None;

    for i in 0..num_processes {
        let step_id = format!("run_contestant_{i}");
        if let Some(run_c) = result.task_results.get(&step_id) {
            let sandbox = &run_c.sandbox_result;

            total_time_s += sandbox.time_used;
            if let Some(mem) = sandbox.memory_used {
                max_memory_kb = Some(max_memory_kb.map_or(mem, |m: u32| m.max(mem)));
            }

            if !run_c.success || sandbox.exit_code != Some(0) {
                let mem_exceeded = sandbox.memory_used.map_or(false, |m| {
                    req_memory_limit_kb > 0 && m >= req_memory_limit_kb
                });
                if sandbox.cg_oom_killed || (sandbox.killed && mem_exceeded) {
                    return TestCaseVerdict {
                        test_case_id,
                        verdict: Verdict::MemoryLimitExceeded,
                        score: 0.0,
                        time_used_ms: time_to_ms(total_time_s),
                        memory_used_kb: max_memory_kb.map(|m| m as i64),
                        message: Some(format!(
                            "Memory limit exceeded (contestant {i}, {}KB)",
                            sandbox.memory_used.unwrap_or(0)
                        )),
                        stdout: None,
                        stderr: opt_nonempty(&sandbox.stderr),
                    };
                }

                match sandbox.status.as_str() {
                    "TO" => {
                        return TestCaseVerdict {
                            test_case_id,
                            verdict: Verdict::TimeLimitExceeded,
                            score: 0.0,
                            time_used_ms: time_to_ms(total_time_s),
                            memory_used_kb: max_memory_kb.map(|m| m as i64),
                            message: Some(format!("Time limit exceeded (contestant {i})")),
                            stdout: None,
                            stderr: opt_nonempty(&sandbox.stderr),
                        };
                    }
                    "SG" => {
                        return TestCaseVerdict {
                            test_case_id,
                            verdict: Verdict::RuntimeError,
                            score: 0.0,
                            time_used_ms: time_to_ms(total_time_s),
                            memory_used_kb: max_memory_kb.map(|m| m as i64),
                            message: Some(format!(
                                "Signal received (contestant {i}): {}",
                                sandbox.message
                            )),
                            stdout: None,
                            stderr: opt_nonempty(&sandbox.stderr),
                        };
                    }
                    _ => {
                        // RE or unknown failure
                        return TestCaseVerdict {
                            test_case_id,
                            verdict: Verdict::RuntimeError,
                            score: 0.0,
                            time_used_ms: time_to_ms(total_time_s),
                            memory_used_kb: max_memory_kb.map(|m| m as i64),
                            message: Some(format!(
                                "Runtime error (contestant {i}, exit code: {})",
                                sandbox.exit_code.unwrap_or(-1)
                            )),
                            stdout: None,
                            stderr: opt_nonempty(&sandbox.stderr),
                        };
                    }
                }
            }
        } else {
            return TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: Some(format!("Contestant {i} run step was skipped")),
                stdout: None,
                stderr: None,
            };
        }
    }

    let run_mgr = match result.task_results.get("run_manager") {
        Some(r) => r,
        None => {
            return TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: time_to_ms(total_time_s),
                memory_used_kb: max_memory_kb.map(|m| m as i64),
                message: Some("Manager run step missing".into()),
                stdout: None,
                stderr: None,
            };
        }
    };

    let mgr_sandbox = &run_mgr.sandbox_result;

    if mgr_sandbox.exit_code != Some(0) {
        return TestCaseVerdict {
            test_case_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: time_to_ms(total_time_s),
            memory_used_kb: max_memory_kb.map(|m| m as i64),
            message: Some(format!(
                "Manager exited with code {} — {}",
                mgr_sandbox.exit_code.unwrap_or(-1),
                truncate(&mgr_sandbox.stderr, "manager error")
            )),
            stdout: None,
            stderr: opt_nonempty(&mgr_sandbox.stderr),
        };
    }

    let score_str = mgr_sandbox.stdout.lines().next().unwrap_or("").trim();
    let score: f64 = match score_str.parse::<f64>() {
        Ok(s) if s.is_finite() => s,
        _ => {
            return TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: time_to_ms(total_time_s),
                memory_used_kb: max_memory_kb.map(|m| m as i64),
                message: Some(format!(
                    "Manager stdout is not a valid score: '{}'",
                    score_str
                )),
                stdout: opt_nonempty(&mgr_sandbox.stdout),
                stderr: opt_nonempty(&mgr_sandbox.stderr),
            };
        }
    };

    let message = opt_nonempty(mgr_sandbox.stderr.trim());

    let capped_score = score.min(1.0).max(0.0);
    let verdict = if capped_score >= 1.0 {
        Verdict::Accepted
    } else {
        Verdict::WrongAnswer
    };

    TestCaseVerdict {
        test_case_id,
        verdict,
        score: capped_score,
        time_used_ms: time_to_ms(total_time_s),
        memory_used_kb: max_memory_kb.map(|m| m as i64),
        message,
        stdout: opt_nonempty(&mgr_sandbox.stdout),
        stderr: opt_nonempty(&mgr_sandbox.stderr),
    }
}

fn time_to_ms(secs: f64) -> Option<i64> {
    if secs >= 0.0 && secs.is_finite() && secs < (i64::MAX as f64 / 1000.0) {
        Some((secs * 1000.0) as i64)
    } else {
        None
    }
}

fn opt_nonempty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn truncate(s: &str, fallback: &str) -> String {
    if s.is_empty() {
        return fallback.to_string();
    }
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(4096).collect();
    if chars.next().is_some() {
        format!("{head}... (truncated)")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broccoli_server_sdk::types::{ExecutionResult, TaskExecutionResult};
    use std::collections::HashMap;

    const MEM_LIMIT: u32 = 262_144; // 256 MB

    fn ok_sandbox(exit_code: i32, time: f64, memory: u32) -> ExecutionResult {
        ExecutionResult {
            exit_code: Some(exit_code),
            time_used: time,
            wall_time_used: time * 2.0,
            memory_used: Some(memory),
            status: if exit_code == 0 {
                "OK".to_string()
            } else {
                "RE".to_string()
            },
            ..Default::default()
        }
    }

    fn task_result(
        id: &str,
        success: bool,
        sandbox: ExecutionResult,
    ) -> (String, TaskExecutionResult) {
        (
            id.to_string(),
            TaskExecutionResult {
                task_id: id.to_string(),
                success,
                sandbox_result: sandbox,
                collected_outputs: HashMap::new(),
            },
        )
    }

    fn mgr_result(score: &str, message: &str) -> ExecutionResult {
        ExecutionResult {
            exit_code: Some(0),
            time_used: 0.5,
            wall_time_used: 1.0,
            memory_used: Some(4096),
            status: "OK".to_string(),
            stdout: score.to_string(),
            stderr: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn accepted_when_score_is_1() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("compile_manager", true, ok_sandbox(0, 1.0, 8192)),
                task_result("compile_contestant_0", true, ok_sandbox(0, 1.0, 8192)),
                task_result("run_manager", true, mgr_result("1.0\n", "Correct\n")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.5, 4096)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::Accepted);
        assert_eq!(verdict.score, 1.0);
        assert_eq!(verdict.message, Some("Correct".into()));
    }

    #[test]
    fn partial_score_when_between_0_and_1() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("0.75\n", "Partial\n")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::WrongAnswer);
        assert_eq!(verdict.score, 0.75);
    }

    #[test]
    fn wrong_answer_when_score_is_0() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("0.0\n", "Wrong\n")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.2, 1024)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::WrongAnswer);
        assert_eq!(verdict.score, 0.0);
    }

    #[test]
    fn contestant_tle() {
        let mut tle_sandbox = ok_sandbox(0, 2.0, 4096);
        tle_sandbox.killed = true;
        tle_sandbox.status = "TO".to_string();
        tle_sandbox.exit_code = None;

        let result = OperationResult {
            success: false,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("0.0", "")),
                task_result("run_contestant_0", false, tle_sandbox),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::TimeLimitExceeded);
    }

    #[test]
    fn contestant_mle_via_memory_exceeded_without_oom_kill() {
        // Killed + memory >= limit but cg_oom_killed is false
        let mut mle_sandbox = ok_sandbox(0, 0.5, MEM_LIMIT);
        mle_sandbox.killed = true;
        mle_sandbox.exit_code = None;
        mle_sandbox.status = "SG".to_string();

        let result = OperationResult {
            success: false,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("0.0", "")),
                task_result("run_contestant_0", false, mle_sandbox),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::MemoryLimitExceeded);
    }

    #[test]
    fn contestant_runtime_error() {
        let result = OperationResult {
            success: false,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("0.0", "")),
                task_result("run_contestant_0", false, ok_sandbox(11, 0.1, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::RuntimeError);
    }

    #[test]
    fn contestant_compile_error() {
        let mut ce = ok_sandbox(1, 1.0, 8192);
        ce.stderr = "error: expected ';'".to_string();

        let result = OperationResult {
            success: false,
            task_results: HashMap::from([
                task_result("compile_manager", true, ok_sandbox(0, 1.0, 8192)),
                task_result("compile_contestant_0", false, ce),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::CompileError);
        assert!(verdict.message.unwrap().contains("expected ';'"));
    }

    #[test]
    fn manager_compile_failure_is_system_error() {
        let mut ce = ok_sandbox(1, 1.0, 8192);
        ce.stderr = "manager.cpp: error".to_string();

        let result = OperationResult {
            success: false,
            task_results: HashMap::from([task_result("compile_manager", false, ce)]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::SystemError);
    }

    #[test]
    fn manager_nonzero_exit_is_system_error() {
        let mut mgr = mgr_result("0.5", "internal error");
        mgr.exit_code = Some(1);
        mgr.status = "RE".to_string();

        let result = OperationResult {
            success: false,
            task_results: HashMap::from([
                task_result("run_manager", false, mgr),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::SystemError);
    }

    #[test]
    fn invalid_manager_score_is_system_error() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("not_a_number", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::SystemError);
        assert!(verdict.message.unwrap().contains("not a valid score"));
    }

    #[test]
    fn nan_score_is_system_error() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("NaN", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::SystemError);
        assert!(verdict.message.unwrap().contains("not a valid score"));
    }

    #[test]
    fn inf_score_is_system_error() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("inf", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::SystemError);
    }

    #[test]
    fn n2_aggregates_time_and_memory() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("1.0\n", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.3, 2048)),
                task_result("run_contestant_1", true, ok_sandbox(0, 0.7, 4096)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 2, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::Accepted);
        // Time = sum: 0.3 + 0.7 = 1.0s = 1000ms
        assert_eq!(verdict.time_used_ms, Some(1000));
        // Memory = max: 4096
        assert_eq!(verdict.memory_used_kb, Some(4096));
    }

    #[test]
    fn score_capped_at_1() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("1.5\n", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.1, 1024)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::Accepted);
        assert_eq!(verdict.score, 1.0);
    }

    #[test]
    fn negative_score_clamped_to_0() {
        let result = OperationResult {
            success: true,
            task_results: HashMap::from([
                task_result("run_manager", true, mgr_result("-0.5\n", "")),
                task_result("run_contestant_0", true, ok_sandbox(0, 0.1, 1024)),
            ]),
            error: None,
        };

        let verdict = interpret_result(42, &result, 1, MEM_LIMIT);
        assert_eq!(verdict.verdict, Verdict::WrongAnswer);
        assert_eq!(verdict.score, 0.0);
    }
}
