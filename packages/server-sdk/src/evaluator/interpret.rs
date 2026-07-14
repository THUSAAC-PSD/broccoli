use crate::Checker;
use crate::error::SdkError;
use crate::types::*;

/// Interpret a FUSED operation result: the checker ran worker-side as a `check`
/// step in the same op, so its small result (exit code + message + sandbox
/// status) is read from `task_results` and turned into a verdict via the
/// checker plugin's `interpret`. Outcome precedence (TLE/MLE/RE/compile) is
/// identical to the legacy path — a failed run wins and the checker is ignored.
///
/// The checker message is sourced generically: prefer the check step's stderr
/// (testlib convention), falling back to its stdout (broccoli-compare writes its
/// message there). Worker File-redirects are read back into both fields.
pub fn interpret_fused_result(
    checker: &Checker,
    test_case_id: i32,
    result: &OperationResult,
    checker_format: &str,
    check_step_id: &str,
) -> Result<TestCaseVerdict, SdkError> {
    // precheck first: it applies the correct precedence for a FAILED solution
    // step (host cancellation, compile error, and exec TLE/MLE/RE). A skipped
    // check step that was skipped *because* the solution failed must be handled
    // as that solution failure, not as an infrastructure error -- so this must
    // run before we inspect the check step below.
    if let Some(verdict) = precheck_verdict(test_case_id, result) {
        return Ok(verdict);
    }

    let time_used_ms = extract_time_used(result);
    let memory_used_kb = extract_memory_used(result);
    let exec_result = result
        .task_results
        .get("exec")
        .expect("precheck guarantees a clean exec result");
    let exec_stdout = opt_nonempty(&exec_result.sandbox_result.stdout);
    let exec_stderr = opt_nonempty(&exec_result.sandbox_result.stderr);

    // `none`: no output comparison is performed, so the evaluator splices NO
    // check step (no worker-side comparator process — this is the hot path for
    // custom "run code", which has no expected output). precheck above already
    // cleared TLE/MLE/RE/CompileError, so a surviving result is a clean run →
    // Accepted, with the solution's own stdout retained for display. There is no
    // check-step result to look up.
    if checker_format == "none" {
        return Ok(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::Accepted,
            score: 1.0,
            time_used_ms,
            memory_used_kb,
            message: None,
            stdout: exec_stdout,
            stderr: exec_stderr,
        });
    }

    // At this point precheck confirmed a clean solution run, so the check step
    // MUST have produced a result. Treat a missing check step -- or one that is
    // present but never actually ran (not successful and with no sandbox status,
    // e.g. the platform comparator tool failed to launch) -- as an infrastructure
    // failure, never a contestant verdict.
    let check_result = match result.task_results.get(check_step_id) {
        Some(c) if c.success || !c.sandbox_result.status.is_empty() => c,
        _ => {
            return Ok(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms,
                memory_used_kb,
                message: Some(format!(
                    "checker step '{check_step_id}' produced no result (infrastructure failure)"
                )),
                stdout: exec_stdout,
                stderr: exec_stderr,
            });
        }
    };

    let sandbox = &check_result.sandbox_result;
    // Both checkers write their verdict message to stderr (broccoli-compare's
    // stdout carries the display preview, testlib's stderr carries quitf output).
    let message = sandbox.stderr.clone();
    // Display preview: the run output preview (exec stdout, File mode) or the
    // comparator's stdout preview (Stream mode, where exec stdout is a FIFO and
    // thus empty). 64 KiB cap applied worker-side.
    let display_stdout = exec_stdout.or_else(|| opt_nonempty(&sandbox.stdout));
    let small = CheckerSmallResult {
        exit_code: sandbox.exit_code,
        stderr: message,
        sandbox_status: Some(sandbox.status.clone()),
    };

    match checker.interpret(checker_format, &small) {
        Ok(v) => Ok(TestCaseVerdict {
            test_case_id,
            verdict: v.verdict,
            score: v.score,
            time_used_ms,
            memory_used_kb,
            message: v.message,
            stdout: display_stdout,
            stderr: exec_stderr,
        }),
        Err(e) => Ok(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms,
            memory_used_kb,
            message: Some(format!("Checker interpret failed: {e:?}")),
            stdout: display_stdout,
            stderr: exec_stderr,
        }),
    }
}

/// Determine the verdict that applies BEFORE consulting the checker: host
/// cancellation, system errors, compile errors, and execution failures
/// (skipped / OOM→MLE / TO→TLE / SG,RE→RE). Returns `None` only when a clean
/// `exec` result is present, meaning the caller should consult the checker.
///
/// Used by `interpret_fused_result` (worker-side check step) to apply outcome
/// precedence before consulting the checker.
pub(crate) fn precheck_verdict(
    test_case_id: i32,
    result: &OperationResult,
) -> Option<TestCaseVerdict> {
    if result.is_cancelled_by_host() {
        return Some(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::Cancelled,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: result.error.clone(),
            stdout: None,
            stderr: None,
        });
    }

    if !result.success && result.task_results.is_empty() {
        return Some(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            // Never surface a null-message SystemError -- an operation that
            // produced no step results is opaque enough already.
            message: Some(result.error.clone().unwrap_or_else(|| {
                "Operation produced no step results (system fault; no error detail reported)".into()
            })),
            stdout: None,
            stderr: None,
        });
    }

    if let Some(compile_result) = result.task_results.get("compile") {
        if let Some(exit_code) = compile_result.sandbox_result.exit_code {
            if exit_code != 0 {
                return Some(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::CompileError,
                    score: 0.0,
                    time_used_ms: None,
                    memory_used_kb: None,
                    message: truncate_stderr(
                        &compile_result.sandbox_result.stderr,
                        "Compilation failed",
                    ),
                    stdout: None,
                    stderr: None,
                });
            }
        } else if !compile_result.success {
            return Some(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: truncate_stderr(
                    &compile_result.sandbox_result.stderr,
                    "Compilation step failed (sandbox error)",
                ),
                stdout: None,
                stderr: None,
            });
        }
    }

    if let Some(exec_result) = result.task_results.get("exec") {
        let sandbox = &exec_result.sandbox_result;
        let exec_stdout = opt_nonempty(&sandbox.stdout);
        let exec_stderr = opt_nonempty(&sandbox.stderr);

        if !exec_result.success && sandbox.status.is_empty() {
            return Some(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::SystemError,
                score: 0.0,
                time_used_ms: None,
                memory_used_kb: None,
                message: Some("Execution step was skipped".into()),
                stdout: None,
                stderr: None,
            });
        }

        if sandbox.cg_oom_killed {
            return Some(TestCaseVerdict {
                test_case_id,
                verdict: Verdict::MemoryLimitExceeded,
                score: 0.0,
                time_used_ms: extract_time_used(result),
                memory_used_kb: extract_memory_used(result),
                message: Some(format!(
                    "Memory limit exceeded ({}KB)",
                    sandbox.memory_used.unwrap_or(0)
                )),
                stdout: exec_stdout,
                stderr: exec_stderr,
            });
        }

        // Classify the sandbox termination once, via the normalized status.
        // `status_kind()` derives from the raw string when the typed field is
        // absent, so results predating `sandbox_status` still classify.
        match sandbox.status_kind() {
            SandboxStatus::TimedOut => {
                let time_ms = extract_time_used(result);
                return Some(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::TimeLimitExceeded,
                    score: 0.0,
                    time_used_ms: time_ms,
                    memory_used_kb: extract_memory_used(result),
                    message: Some(format!(
                        "Time limit exceeded ({}ms)",
                        time_ms.map_or("?".into(), |t| t.to_string())
                    )),
                    stdout: exec_stdout,
                    stderr: exec_stderr,
                });
            }
            SandboxStatus::Signaled => {
                return Some(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::RuntimeError,
                    score: 0.0,
                    time_used_ms: extract_time_used(result),
                    memory_used_kb: extract_memory_used(result),
                    message: Some(sandbox.message.clone()),
                    stdout: exec_stdout,
                    stderr: exec_stderr,
                });
            }
            SandboxStatus::NonZeroExit => {
                return Some(TestCaseVerdict {
                    test_case_id,
                    verdict: Verdict::RuntimeError,
                    score: 0.0,
                    time_used_ms: extract_time_used(result),
                    memory_used_kb: extract_memory_used(result),
                    message: Some(format!("Exit code: {}", sandbox.exit_code.unwrap_or(-1))),
                    stdout: exec_stdout,
                    stderr: exec_stderr,
                });
            }
            SandboxStatus::Ok | SandboxStatus::InternalError | SandboxStatus::Unknown => {}
        }
    }

    // A clean exec result is required to consult the checker; its absence is a
    // system error.
    match result.task_results.get("exec") {
        Some(_) => None,
        None => Some(TestCaseVerdict {
            test_case_id,
            verdict: Verdict::SystemError,
            score: 0.0,
            time_used_ms: None,
            memory_used_kb: None,
            message: Some("No exec result found".into()),
            stdout: None,
            stderr: None,
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

fn opt_nonempty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Host;
    use crate::types::{ExecutionResult, TaskExecutionResult};
    use std::collections::HashMap;

    fn task(success: bool, sandbox: ExecutionResult) -> TaskExecutionResult {
        TaskExecutionResult {
            task_id: "t".into(),
            success,
            sandbox_result: sandbox,
            collected_outputs: HashMap::new(),
        }
    }

    fn op(entries: Vec<(&str, TaskExecutionResult)>) -> OperationResult {
        let mut task_results = HashMap::new();
        for (id, t) in entries {
            task_results.insert(id.to_string(), t);
        }
        OperationResult {
            success: true,
            task_results,
            error: None,
        }
    }

    fn exec_ok() -> TaskExecutionResult {
        task(
            true,
            ExecutionResult {
                exit_code: Some(0),
                status: "OK".into(),
                ..Default::default()
            },
        )
    }

    fn compile_ok() -> TaskExecutionResult {
        task(
            true,
            ExecutionResult {
                exit_code: Some(0),
                status: "OK".into(),
                ..Default::default()
            },
        )
    }

    // ----- precheck_verdict: shared outcome precedence -----

    #[test]
    fn precheck_passes_through_clean_exec() {
        let result = op(vec![("compile", compile_ok()), ("exec", exec_ok())]);
        assert!(precheck_verdict(1, &result).is_none());
    }

    #[test]
    fn precheck_compile_error() {
        let compile = task(
            false,
            ExecutionResult {
                exit_code: Some(1),
                status: "OK".into(),
                stderr: "error: expected ';'".into(),
                ..Default::default()
            },
        );
        let v = precheck_verdict(1, &op(vec![("compile", compile)])).unwrap();
        assert_eq!(v.verdict, Verdict::CompileError);
        assert!(v.message.unwrap().contains("expected"));
    }

    #[test]
    fn precheck_time_limit_exceeded() {
        let exec = task(
            false,
            ExecutionResult {
                exit_code: None,
                status: "TO".into(),
                ..Default::default()
            },
        );
        let v = precheck_verdict(1, &op(vec![("compile", compile_ok()), ("exec", exec)])).unwrap();
        assert_eq!(v.verdict, Verdict::TimeLimitExceeded);
    }

    #[test]
    fn precheck_memory_limit_exceeded() {
        let exec = task(
            false,
            ExecutionResult {
                exit_code: None,
                status: "SG".into(),
                cg_oom_killed: true,
                memory_used: Some(999),
                ..Default::default()
            },
        );
        let v = precheck_verdict(1, &op(vec![("exec", exec)])).unwrap();
        assert_eq!(v.verdict, Verdict::MemoryLimitExceeded);
    }

    #[test]
    fn precheck_runtime_error_on_signal() {
        let exec = task(
            false,
            ExecutionResult {
                exit_code: None,
                status: "SG".into(),
                message: "Caught fatal signal 11".into(),
                ..Default::default()
            },
        );
        let v = precheck_verdict(1, &op(vec![("exec", exec)])).unwrap();
        assert_eq!(v.verdict, Verdict::RuntimeError);
    }

    #[test]
    fn precheck_cancelled() {
        let v = precheck_verdict(1, &OperationResult::cancelled_by_host()).unwrap();
        assert_eq!(v.verdict, Verdict::Cancelled);
    }

    #[test]
    fn precheck_missing_exec_is_system_error() {
        let v = precheck_verdict(1, &op(vec![("compile", compile_ok())])).unwrap();
        assert_eq!(v.verdict, Verdict::SystemError);
        assert!(v.message.unwrap().contains("No exec result"));
    }

    #[test]
    fn precheck_no_results_has_non_null_message() {
        // An operation that produced no step results (error unset) must still carry
        // a diagnostic; a null-message SystemError is opaque to admins/contestants.
        let result = OperationResult {
            success: false,
            task_results: HashMap::new(),
            error: None,
        };
        let v = precheck_verdict(1, &result).unwrap();
        assert_eq!(v.verdict, Verdict::SystemError);
        let msg = v.message.expect("SystemError must carry a message");
        assert!(msg.contains("no step results"), "got: {msg}");
    }

    // ----- interpret_fused_result -----

    #[test]
    fn fused_propagates_run_failure_without_calling_checker() {
        // A TLE must win regardless of the checker (mock checker would Err).
        let host = Host::mock();
        let exec = task(
            false,
            ExecutionResult {
                exit_code: None,
                status: "TO".into(),
                ..Default::default()
            },
        );
        let result = op(vec![("compile", compile_ok()), ("exec", exec)]);
        let v = interpret_fused_result(&host.checker, 1, &result, "tokens", "check").unwrap();
        assert_eq!(v.verdict, Verdict::TimeLimitExceeded);
    }

    #[test]
    fn fused_missing_check_step_is_system_error() {
        let host = Host::mock();
        let result = op(vec![("compile", compile_ok()), ("exec", exec_ok())]);
        let v = interpret_fused_result(&host.checker, 1, &result, "tokens", "check").unwrap();
        assert_eq!(v.verdict, Verdict::SystemError);
        assert!(v.message.unwrap().contains("produced no result"));
    }

    #[test]
    fn fused_check_present_but_never_ran_is_system_error() {
        // Clean exec, but the check step exists yet never ran (launch failure:
        // not successful, empty sandbox status). This is infrastructure, not a
        // contestant verdict.
        let host = Host::mock();
        let dead_check = task(false, ExecutionResult::default());
        let result = op(vec![
            ("compile", compile_ok()),
            ("exec", exec_ok()),
            ("check", dead_check),
        ]);
        let v = interpret_fused_result(&host.checker, 1, &result, "tokens", "check").unwrap();
        assert_eq!(v.verdict, Verdict::SystemError);
    }

    #[test]
    fn fused_clean_run_consults_the_checker() {
        // Clean exec + a check step present → interpret is consulted. The mock
        // checker returns Err, surfaced as SystemError "Checker interpret failed"
        // (the real verdict mapping is unit-tested in standard-checkers).
        let host = Host::mock();
        let check = task(
            true,
            ExecutionResult {
                exit_code: Some(1),
                status: "OK".into(),
                stdout: "solution-output-preview".into(),
                stderr: "token 2 mismatch".into(),
                ..Default::default()
            },
        );
        let result = op(vec![
            ("compile", compile_ok()),
            ("exec", exec_ok()),
            ("check", check),
        ]);
        let v = interpret_fused_result(&host.checker, 1, &result, "tokens", "check").unwrap();
        assert_eq!(v.verdict, Verdict::SystemError);
        assert!(v.message.unwrap().contains("Checker interpret failed"));
        // Stream-mode preview fallback: exec stdout is empty (FIFO), so the
        // display preview comes from the check step's stdout.
        assert_eq!(v.stdout.as_deref(), Some("solution-output-preview"));
    }

    // ----- interpret_fused_result: 'none' (no comparison, no check step) -----

    #[test]
    fn fused_none_accepts_clean_run_without_a_check_step() {
        // `none` schedules NO checker step (no worker process); a clean exec is
        // Accepted and the solution's own stdout is retained for display. The
        // mock checker would Err if consulted — proving none never consults it.
        let host = Host::mock();
        let exec = task(
            true,
            ExecutionResult {
                exit_code: Some(0),
                status: "OK".into(),
                stdout: "hello world\n".into(),
                ..Default::default()
            },
        );
        let result = op(vec![("compile", compile_ok()), ("exec", exec)]);
        let v = interpret_fused_result(&host.checker, 7, &result, "none", "check").unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
        assert_eq!(v.score, 1.0);
        assert_eq!(v.test_case_id, 7);
        // Output retained for display (the custom "run code" path relies on this).
        assert_eq!(v.stdout.as_deref(), Some("hello world\n"));
    }

    #[test]
    fn fused_none_does_not_mask_a_run_failure() {
        // `none` must not turn a TLE/RE into Accepted — precheck still wins.
        let host = Host::mock();
        let exec = task(
            false,
            ExecutionResult {
                exit_code: None,
                status: "TO".into(),
                ..Default::default()
            },
        );
        let result = op(vec![("compile", compile_ok()), ("exec", exec)]);
        let v = interpret_fused_result(&host.checker, 1, &result, "none", "check").unwrap();
        assert_eq!(v.verdict, Verdict::TimeLimitExceeded);
    }
}
