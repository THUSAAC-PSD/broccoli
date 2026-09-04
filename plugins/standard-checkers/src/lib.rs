#[cfg(target_arch = "wasm32")]
use broccoli_server_sdk::prelude::*;
#[cfg(target_arch = "wasm32")]
use extism_pdk::{FnResult, plugin_fn};

pub mod checkers;
pub mod resolve;
pub mod util;

/// Formats registered on the fused resolve/interpret path (checker fusion), and
/// thus the selectable checker formats surfaced to problem authors. The built-in
/// comparison formats run via the native broccoli-compare binary; `testlib` runs
/// the compiled checker. `none` is registered for discoverability/validation but
/// performs NO comparison: the evaluator schedules no checker step and interprets
/// it inline (a clean run -> Accepted, output retained for display), so its
/// resolve/interpret handlers are never reached on the hot path.
#[cfg(target_arch = "wasm32")]
const RESOLVABLE_CHECKER_FORMATS: &[&str] = &[
    "exact",
    "lines",
    "tokens",
    "tokens-case-insensitive",
    "tokens-float",
    "testlib",
    "none",
];

#[cfg(target_arch = "wasm32")]
#[plugin_fn]
pub fn init() -> FnResult<String> {
    let host = Host::new();
    // Checker fusion: register the resolve + interpret handlers so the
    // batch-evaluator can splice a checker stage into the run op instead of
    // streaming the full output through the coordinator. `none` is registered
    // here too (discoverable/selectable) but is interpreted inline by the
    // evaluator - it schedules no checker step.
    for format in RESOLVABLE_CHECKER_FORMATS {
        host.registry.register_checker_resolver(
            format,
            "resolve_standard_checker",
            "interpret_standard_checker_result",
        )?;
    }
    host.log.info("Standard checkers registered")?;
    Ok("ok".to_string())
}
