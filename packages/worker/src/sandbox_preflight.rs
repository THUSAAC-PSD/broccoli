//! Fail-closed sandbox preflight.
//!
//! The batch evaluator derives MemoryLimitExceeded solely from
//! `cg_oom_killed` (server-sdk `interpret.rs`), and isolate only emits that
//! meta key under `--cg`. With cgroups disabled an over-limit run is reported
//! as RuntimeError instead of MLE — a silent, permanent misjudgement. Rather
//! than paper over the verdict, the worker refuses to start in configurations
//! that produce it.

/// Reject the isolate backend when cgroups are off, unless the operator has
/// explicitly opted into the insecure mode (development only).
pub fn validate_sandbox_config(
    backend: &str,
    enable_cgroups: bool,
    allow_insecure: bool,
) -> Result<(), String> {
    // Mirror OperationTaskExecutor::sandbox_manager_from_config: any backend
    // that isn't case-insensitively "mock" resolves to the isolate sandbox
    // (unknown values fall back to isolate with only a warning). The gate
    // must refuse cgroups-off for every backend that will actually boot
    // isolate, not just the literal string "isolate".
    let uses_isolate = !backend.eq_ignore_ascii_case("mock");
    if uses_isolate && !enable_cgroups && !allow_insecure {
        return Err("isolate sandbox requires cgroups; refusing to start with \
             worker.enable_cgroups=false (over-limit runs would be misreported \
             as RuntimeError instead of MemoryLimitExceeded). Set \
             worker.allow_insecure_no_cgroups=true to override for development."
            .to_string());
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
}
