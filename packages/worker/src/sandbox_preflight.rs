//! Fail-closed sandbox preflight.
//!
//! The batch evaluator derives MemoryLimitExceeded solely from
//! `cg_oom_killed` (server-sdk `interpret.rs`), and isolate only emits that
//! meta key under `--cg`. With cgroups disabled an over-limit run is reported
//! as RuntimeError instead of MLE — a silent, permanent misjudgement. Rather
//! than paper over the verdict, the worker refuses to start in configurations
//! that produce it.

/// The isolate sandbox is used for every backend that is not case-insensitively
/// "mock" — unknown/typo/wrong-case values fall back to isolate at runtime
/// (see OperationTaskExecutor::sandbox_manager_from_config), so the safety
/// gates must treat them as isolate too.
pub fn uses_isolate(backend: &str) -> bool {
    !backend.eq_ignore_ascii_case("mock")
}

/// Reject the isolate backend when cgroups are off, unless the operator has
/// explicitly opted into the insecure mode (development only).
pub fn validate_sandbox_config(
    backend: &str,
    enable_cgroups: bool,
    allow_insecure: bool,
) -> Result<(), String> {
    // isolate iff not "mock" — see uses_isolate.
    if uses_isolate(backend) && !enable_cgroups && !allow_insecure {
        return Err("isolate sandbox requires cgroups; refusing to start with \
             worker.enable_cgroups=false (over-limit runs would be misreported \
             as RuntimeError instead of MemoryLimitExceeded). Set \
             worker.allow_insecure_no_cgroups=true to override for development."
            .to_string());
    }
    Ok(())
}

/// Ensure the unified cgroup v2 hierarchy exposes the controllers isolate
/// needs. `contents` is the text of `/sys/fs/cgroup/cgroup.controllers`.
pub fn controllers_present(contents: &str) -> Result<(), String> {
    if contents.trim().is_empty() {
        return Err("cgroup v2 controllers are empty".to_string());
    }
    let have: std::collections::HashSet<&str> = contents.split_whitespace().collect();
    for required in ["cpu", "memory", "pids"] {
        if !have.contains(required) {
            return Err(format!(
                "cgroup v2 controller '{required}' is not delegated \
                 (found: {})",
                contents.trim()
            ));
        }
    }
    Ok(())
}

/// Result of the live isolate self-test.
#[derive(Debug)]
pub enum ProbeOutcome {
    /// isolate ran an over-limit allocation and reported cg-oom-killed.
    Ok,
    /// isolate ran but did NOT report cg-oom-killed — MLE would be misjudged.
    OomNotReported,
    /// The probe could not be executed (isolate error, spawn failure, ...).
    Failed(String),
}

/// Turn a probe outcome into a fail-closed startup decision.
pub fn gate_on_probe(outcome: ProbeOutcome) -> Result<(), String> {
    match outcome {
        ProbeOutcome::Ok => Ok(()),
        ProbeOutcome::OomNotReported => Err(
            "sandbox self-test: isolate did not report cg-oom-killed for an \
             over-limit allocation; MemoryLimitExceeded would be misclassified \
             as RuntimeError"
                .to_string(),
        ),
        ProbeOutcome::Failed(e) => Err(format!("sandbox self-test failed: {e}")),
    }
}

/// Run a real over-limit allocation in isolate and confirm the cg-oom-killed
/// signal reaches us. Any error is reported as `Failed` so the caller fails
/// closed rather than guessing.
///
/// The probe must be given an `IsolateSandboxManager` built with cgroups
/// ENABLED: isolate only emits the `cg-oom-killed` meta key under `--cg`, and
/// `--cg-mem` (not `--mem`) is what actually OOM-kills the box, so a
/// cgroups-off manager would always report `Ok(false)` regardless of the host.
pub async fn probe_isolate_oom(
    mgr: &crate::models::operation::sandbox::isolate::IsolateSandboxManager,
) -> ProbeOutcome {
    // Python that grabs far more than the cgroup memory cap, in 10 MiB chunks,
    // so it blows past the 64 MiB `--cg-mem` cap within a handful of iterations.
    const OOM_SRC: &str = "b=bytearray()\nwhile True:\n b.extend(bytearray(10*1024*1024))\n";
    match run_oom_probe(mgr, OOM_SRC).await {
        Ok(true) => ProbeOutcome::Ok,
        Ok(false) => ProbeOutcome::OomNotReported,
        Err(e) => ProbeOutcome::Failed(e),
    }
}

/// Returns `Ok(true)` if the run reported `cg_oom_killed`, `Ok(false)` if it
/// ran but did not, `Err(msg)` if the box could not be initialised or run.
///
/// Self-contained: it touches only the sandbox manager (no queue/db/executor).
/// A dedicated box id (`990`) is used so the probe never collides with the
/// operation boxes (`0`..). The box is torn down on EVERY path — success,
/// `Ok(false)`, and the error paths — via an explicit cleanup before each
/// return (no scopeguard / new crates).
async fn run_oom_probe(
    mgr: &crate::models::operation::sandbox::isolate::IsolateSandboxManager,
    src: &str,
) -> Result<bool, String> {
    use crate::models::operation::sandbox::{ResourceLimits, RunOptions, SandboxManager};

    const PROBE_BOX_ID: &str = "990";

    // Best-effort: clear any stale box left by a crashed prior probe. A missing
    // box makes `--cleanup` a no-op / harmless error, so ignore the result.
    let _ = mgr.remove_sandbox(PROBE_BOX_ID).await;

    if let Err(e) = mgr.create_sandbox(Some(PROBE_BOX_ID)).await {
        // If init failed because a stale box `990` still exists (best-effort
        // pre-cleanup silently failed), reclaim it here so the NEXT boot's init
        // can succeed — otherwise every subsequent boot hits the same stale box
        // and the probe can never self-heal. Keeps the doc-comment invariant:
        // every return path attempts cleanup.
        let _ = mgr.remove_sandbox(PROBE_BOX_ID).await;
        return Err(format!("failed to init probe sandbox: {e}"));
    }

    // 64 MiB cgroup cap (KiB) with tight CPU/wall bounds — the allocator trips
    // the cap long before either clock elapses.
    let run_options = RunOptions {
        resource_limits: ResourceLimits {
            memory_limit: Some(65536),
            time_limit: Some(5.0),
            wall_time_limit: Some(10.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let argv = vec![
        "/usr/bin/python3".to_string(),
        "-c".to_string(),
        src.to_string(),
    ];

    let exec_result = mgr.execute(PROBE_BOX_ID, argv, &run_options).await;

    // Always tear the probe box down, whether the run succeeded or errored, so a
    // refused boot never leaks an isolate box. Cleanup failure is non-fatal to
    // the probe verdict itself.
    let _ = mgr.remove_sandbox(PROBE_BOX_ID).await;

    match exec_result {
        Ok(result) => Ok(result.cg_oom_killed),
        Err(e) => Err(format!("failed to run probe allocation: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_ok_passes_the_gate() {
        assert!(gate_on_probe(ProbeOutcome::Ok).is_ok());
    }

    #[test]
    fn probe_without_oom_report_fails_closed() {
        let err = gate_on_probe(ProbeOutcome::OomNotReported).unwrap_err();
        assert!(
            err.contains("cg-oom-killed") || err.contains("MemoryLimitExceeded"),
            "gate should explain the misclassification risk: {err}"
        );
    }

    #[test]
    fn probe_failure_fails_closed() {
        assert!(gate_on_probe(ProbeOutcome::Failed("boom".into())).is_err());
    }

    #[test]
    fn isolate_without_cgroups_is_refused() {
        let err = validate_sandbox_config("isolate", false, false).unwrap_err();
        assert!(
            err.contains("cgroups"),
            "message should explain the refusal: {err}"
        );
    }

    #[test]
    fn isolate_with_cgroups_is_allowed() {
        assert!(validate_sandbox_config("isolate", true, false).is_ok());
    }

    #[test]
    fn explicit_override_allows_cgroups_off() {
        assert!(validate_sandbox_config("isolate", false, true).is_ok());
    }

    #[test]
    fn mock_backend_is_never_gated() {
        assert!(validate_sandbox_config("mock", false, false).is_ok());
    }

    #[test]
    fn wrong_case_isolate_without_cgroups_is_refused() {
        // Mirrors OperationTaskExecutor::sandbox_manager_from_config, which
        // resolves any backend that isn't case-insensitively "mock" to the
        // isolate sandbox. "Isolate" must be refused just like "isolate".
        let err = validate_sandbox_config("Isolate", false, false).unwrap_err();
        assert!(
            err.contains("cgroups"),
            "message should explain the refusal: {err}"
        );
    }

    #[test]
    fn unknown_backend_without_cgroups_is_refused() {
        // Unknown/typo backends fall back to isolate at runtime (with only a
        // warning), so the gate must refuse them too, not silently pass.
        let err = validate_sandbox_config("docker", false, false).unwrap_err();
        assert!(
            err.contains("cgroups"),
            "message should explain the refusal: {err}"
        );
    }

    #[test]
    fn case_insensitive_mock_backend_is_never_gated() {
        assert!(validate_sandbox_config("MOCK", false, false).is_ok());
    }

    #[test]
    fn controllers_ok_when_required_present() {
        assert!(controllers_present("cpuset cpu io memory pids").is_ok());
    }

    #[test]
    fn controllers_empty_is_rejected() {
        assert!(controllers_present("   ").is_err());
    }

    #[test]
    fn controllers_missing_memory_is_rejected() {
        let err = controllers_present("cpu pids").unwrap_err();
        assert!(
            err.contains("memory"),
            "should name the missing controller: {err}"
        );
    }

    #[test]
    fn uses_isolate_true_for_isolate_and_unknown_but_false_for_mock() {
        assert!(uses_isolate("isolate"));
        assert!(uses_isolate("docker"));
        assert!(uses_isolate("Isolate"));
        assert!(!uses_isolate("mock"));
        assert!(!uses_isolate("MOCK"));
    }
}
