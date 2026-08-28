use anyhow::Result;
use async_trait::async_trait;
use broccoli_types::types::HookResponse;
use common::event::GenericEvent;
use common::hook::{GenericHook, GenericHookAction, HookAction};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::retry::{PoolRetryPolicy, call_raw_with_pool_retry};
use crate::traits::PluginManager;

pub const PLUGIN_RUNTIME_ERROR_CODE: &str = "__BROCCOLI_PLUGIN_RUNTIME_ERROR";

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookScope {
    #[default]
    Resource,
    Global,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum HookMode {
    #[default]
    Blocking,
    Notify,
}

/// Map a plugin's `HookResponse` onto the host's hook action. Free function
/// (rather than an inherent impl) because `HookResponse` is the shared
/// guest<->host contract type owned by `broccoli-types` (re-exported via `broccoli-server-sdk`).
fn into_hook_action(response: HookResponse, original_topic: &str) -> GenericHookAction {
    match response {
        HookResponse::Pass => HookAction::Pass,
        HookResponse::Stop => HookAction::Stop,
        HookResponse::Reject {
            code,
            details,
            message,
            status_code,
        } => {
            let mut detail = serde_json::json!({
                "code": code,
                "message": message,
                "status_code": status_code,
            });
            if let Some(d) = details {
                detail["details"] = d;
            }
            HookAction::Reject(detail.to_string())
        }
        HookResponse::Modified { event } => {
            let generic = GenericEvent {
                topic: event
                    .get("topic")
                    .and_then(|t| t.as_str())
                    .unwrap_or(original_topic)
                    .to_string(),
                payload: event,
            };
            HookAction::Modified(generic)
        }
    }
}

pub struct PluginHook<M: PluginManager + ?Sized> {
    plugin_manager: Arc<M>,
    plugin_id: String,
    function_name: String,
    topics: Vec<String>,
    pub scope: HookScope,
    pub mode: HookMode,
}

impl<M: PluginManager + ?Sized> PluginHook<M> {
    pub fn new(
        plugin_manager: Arc<M>,
        plugin_id: String,
        function_name: String,
        topics: Vec<String>,
        scope: HookScope,
        mode: HookMode,
    ) -> Self {
        Self {
            plugin_manager,
            plugin_id,
            function_name,
            topics,
            scope,
            mode,
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn mode(&self) -> &HookMode {
        &self.mode
    }
}

#[async_trait]
impl<M: PluginManager + Send + Sync + ?Sized + 'static> GenericHook for PluginHook<M> {
    type Context = ();

    fn id(&self) -> &str {
        &self.plugin_id
    }

    fn topics(&self) -> &[String] {
        &self.topics
    }

    async fn on_event(&self, _ctx: (), event: &GenericEvent) -> Result<GenericHookAction> {
        // Serialize the payload once outside the retry loop. A serialization
        // failure here is a genuine error (not transient backpressure), so we
        // fail-closed with Reject - matching the prior `call::<_, _>` path
        // which would have surfaced this as `PluginError::Serialization`.
        let input_bytes = match serde_json::to_vec(&event.payload) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!(
                    plugin_id = %self.plugin_id,
                    function = %self.function_name,
                    "Failed to serialize hook payload (fail-closed): {e}",
                );
                let detail = serde_json::json!({
                    "code": PLUGIN_RUNTIME_ERROR_CODE,
                    "message": format!("Plugin '{}' hook '{}' failed: {e}", self.plugin_id, self.function_name),
                    "status_code": 500,
                });
                return Ok(GenericHookAction::Reject(detail.to_string()));
            }
        };

        // Pool timeouts are transient backpressure: every WASM instance is
        // busy but a free slot will appear shortly. Retrying transparently
        // here matches the synchronous evaluator path and prevents a
        // fail-closed Reject (HTTP 500) for what is purely a server-side
        // overload signal (UP#14h).
        let call_result = call_raw_with_pool_retry(
            self.plugin_manager.as_ref(),
            &self.plugin_id,
            &self.function_name,
            input_bytes,
            PoolRetryPolicy::default(),
        )
        .await;

        match call_result {
            Ok(output_bytes) => match serde_json::from_slice::<serde_json::Value>(&output_bytes) {
                // Unparseable output from a broken guest: fail closed so a Blocking
                // gate (cooldown / submission-limit) rejects rather than passes.
                Err(e) => {
                    tracing::error!(
                        plugin_id = %self.plugin_id,
                        function = %self.function_name,
                        "Hook returned non-JSON output (fail-closed): {e}",
                    );
                    let detail = serde_json::json!({
                        "code": PLUGIN_RUNTIME_ERROR_CODE,
                        "message": format!("Plugin '{}' hook '{}' returned a non-JSON response", self.plugin_id, self.function_name),
                        "status_code": 500,
                    });
                    Ok(GenericHookAction::Reject(detail.to_string()))
                }
                // Valid JSON of the wrong shape is a pass: not every plugin
                // implements every hook, and that must not block the request.
                Ok(value) => match serde_json::from_value::<HookResponse>(value) {
                    Ok(hook_response) => Ok(into_hook_action(hook_response, &event.topic)),
                    Err(e) => {
                        tracing::warn!(
                            plugin_id = %self.plugin_id,
                            function = %self.function_name,
                            "Hook returned a wrong-shape response, treating as pass: {e}",
                        );
                        Ok(GenericHookAction::Pass)
                    }
                },
            },
            Err(e) => {
                tracing::error!(
                    plugin_id = %self.plugin_id,
                    function = %self.function_name,
                    "Hook WASM call failed (fail-closed): {e}",
                );
                let detail = serde_json::json!({
                    "code": PLUGIN_RUNTIME_ERROR_CODE,
                    "message": format!("Plugin '{}' hook '{}' failed: {e}", self.plugin_id, self.function_name),
                    "status_code": 500,
                });
                Ok(GenericHookAction::Reject(detail.to_string()))
            }
        }
    }
}
