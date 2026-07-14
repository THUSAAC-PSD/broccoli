//! Shared host<->guest wire and domain types for the Broccoli online judge.
//!
//! This crate sits at the bottom of the workspace graph: it carries the serde
//! contracts (verdicts, the operation/step model, checker/evaluate types, hook
//! events, filter IO) with no heavy dependencies, so base crates (`common`,
//! `worker`, `plugin-core`) and the published guest SDK (`broccoli-server-sdk`)
//! can both depend on it downward instead of the base crates depending upward on
//! the guest SDK.
pub mod error;
pub mod types;
