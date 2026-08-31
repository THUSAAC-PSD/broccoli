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

#[cfg(test)]
mod tests {
    use super::*;

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
