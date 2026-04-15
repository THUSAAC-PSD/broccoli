use anyhow::Result;
use async_trait::async_trait;
use common::event::GenericEvent;
use common::hook::{GenericHook, GenericHookAction, HookAction};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::traits::{PluginManager, PluginManagerExt};

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

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum HookResponse {
    Pass,
    Stop,
    Reject {
        #[serde(default = "default_reject_code")]
        code: String,
        #[serde(default = "default_reject_message")]
        message: String,
        #[serde(default = "default_reject_status")]
        status_code: u16,
        #[serde(default)]
        details: Option<serde_json::Value>,
    },
    Modified {
        event: serde_json::Value,
    },
}

fn default_reject_code() -> String {
    "PLUGIN_REJECTED".into()
}
fn default_reject_message() -> String {
    "Request rejected by plugin".into()
}
fn default_reject_status() -> u16 {
    400
}

impl HookResponse {
    fn into_hook_action(self, original_topic: &str) -> GenericHookAction {
        match self {
            HookResponse::Pass => HookAction::Pass,
            HookResponse::Stop => HookAction::Stop,
            HookResponse::Reject {
                code,
                message,
                status_code,
                details,
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
        match self
            .plugin_manager
            .call::<_, serde_json::Value>(&self.plugin_id, &self.function_name, &event.payload)
            .await
        {
            Ok(response_value) => {
                match serde_json::from_value::<HookResponse>(response_value) {
                    Ok(hook_response) => Ok(hook_response.into_hook_action(&event.topic)),
                    Err(e) => {
                        tracing::warn!(
                            plugin_id = %self.plugin_id,
                            function = %self.function_name,
                            "Hook returned unparseable response, treating as pass: {e}",
                        );

                        Ok(GenericHookAction::Pass)
                    }
                }
            }
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
