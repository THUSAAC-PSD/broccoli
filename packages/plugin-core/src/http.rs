//! HTTP proxy DTOs for the plugin host<->guest boundary.
//!
//! The request/response/auth shapes are the shared host<->guest contract, so
//! they live in `broccoli-types` (single source, drift-pinned) rather than being
//! redefined here. Re-exported for the `plugin_core::http` path the host proxy
//! imports.
pub use broccoli_types::types::{PluginHttpAuth, PluginHttpRequest, PluginHttpResponse};
