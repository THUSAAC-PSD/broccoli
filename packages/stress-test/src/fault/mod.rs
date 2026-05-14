//! Fault-injection harness scenarios.
//!
//! Originally lived in its own crate `packages/fault-harness/`; folded into
//! stress-test as `fault::*` so operators only need one binary for stack
//! validation. The `Scenario` trait, transcript shape, and existing
//! cancel-storm / kill-server-recovery scenarios moved verbatim. The CLI is
//! wired in under the `fault` subcommand (see `crate::cli`); the dispatcher
//! lives in `crate::fault::runner`.

pub mod runner;
pub mod scenarios;
pub mod transcript;

pub use scenarios::{Scenario, ScenarioContext, ScenarioOutcome};
pub use transcript::{Event, EventKind, Severity, Transcript};
