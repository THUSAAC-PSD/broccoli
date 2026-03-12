use std::collections::HashMap;
use std::sync::Arc;

use broccoli_server_sdk::types::HookEvent;
use common::event::GenericEvent;
use common::hook::{GenericHook, HookAction};
use plugin_core::hook::{HookMode, HookScope, PluginHook};
use plugin_core::traits::PluginManager;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use tokio::sync::RwLock;

use crate::entity::plugin_config;
use crate::error::AppError;
use crate::host_funcs::config::extract_plugin_id;

/// Max recursion depth for Chain events to prevent infinite loops.
const MAX_CHAIN_DEPTH: u8 = 3;

/// Entry in the server hook registry (wraps a PluginHook with metadata).
struct HookEntry {
    plugin_id: String,
    scope: HookScope,
    mode: HookMode,
    hook: Arc<dyn GenericHook<Context = ()>>,
}

/// Server-side hook registry. Stores hooks indexed by topic.
pub struct ServerHookRegistry {
    /// topic -> list of hook entries
    hooks: HashMap<String, Vec<HookEntry>>,
}

impl Default for ServerHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerHookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Register a plugin hook. Called when a plugin is loaded.
    pub fn register<M: PluginManager + Send + Sync + ?Sized + 'static>(
        &mut self,
        hook: Arc<PluginHook<M>>,
    ) {
        let plugin_id = hook.plugin_id().to_string();
        let scope = hook.scope;
        let mode = *hook.mode();

        for topic in hook.topics() {
            let entry = HookEntry {
                plugin_id: plugin_id.clone(),
                scope,
                mode,
                hook: hook.clone(),
            };
            self.hooks.entry(topic.clone()).or_default().push(entry);
        }
    }

    /// Remove all hooks for a given plugin_id. Called when a plugin is unloaded.
    pub fn unregister_plugin(&mut self, plugin_id: &str) {
        for entries in self.hooks.values_mut() {
            entries.retain(|e| e.plugin_id != plugin_id);
        }
    }

    /// Get all hook entries for a topic.
    fn get_hooks(&self, topic: &str) -> Option<&Vec<HookEntry>> {
        self.hooks.get(topic)
    }
}

pub type SharedHookRegistry = Arc<RwLock<ServerHookRegistry>>;

/// Create a new shared hook registry.
pub fn new_shared_registry() -> SharedHookRegistry {
    Arc::new(RwLock::new(ServerHookRegistry::new()))
}

/// Enabled plugin_id -> position map for the applicable resource scopes.
///
/// Fetched once per request and threaded through hook dispatch calls.
pub type ResourceEnablements = HashMap<String, i32>;

/// Scope priority for cascading enablement resolution.
/// Higher value = more specific = wins when a plugin appears at multiple scopes.
fn scope_priority(scope: &str) -> u8 {
    match scope {
        "contest_problem" => 3,
        "contest" => 2,
        "problem" => 1,
        _ => 0,
    }
}

/// Fetch plugin enablement data for the applicable resource scopes.
///
/// Queries all relevant scopes and aggregates with scope priority: contest_problem > contest > problem.
/// If a plugin appears at multiple scopes, the most specific scope wins
/// (its `enabled` flag and `position` take precedence).
///
/// Within a single scope, multiple config rows per plugin are aggregated:
/// the plugin is enabled if *any* namespace is enabled, and its position is
/// the minimum across namespaces.
pub async fn fetch_resource_enablements<C: ConnectionTrait>(
    problem_id: i32,
    contest_id: Option<i32>,
    db: &C,
) -> Result<ResourceEnablements, AppError> {
    use sea_orm::sea_query::Condition;

    let problem_ref = crate::models::plugin_config::config_key::problem(problem_id);

    let mut condition = Condition::any().add(
        Condition::all()
            .add(plugin_config::Column::Scope.eq("problem"))
            .add(plugin_config::Column::RefId.eq(&problem_ref)),
    );

    if let Some(cid) = contest_id {
        let contest_ref = crate::models::plugin_config::config_key::contest(cid);
        let cp_ref = crate::models::plugin_config::config_key::contest_problem(cid, problem_id);

        condition = condition
            .add(
                Condition::all()
                    .add(plugin_config::Column::Scope.eq("contest"))
                    .add(plugin_config::Column::RefId.eq(&contest_ref)),
            )
            .add(
                Condition::all()
                    .add(plugin_config::Column::Scope.eq("contest_problem"))
                    .add(plugin_config::Column::RefId.eq(&cp_ref)),
            );
    }

    let rows = plugin_config::Entity::find()
        .filter(condition)
        .all(db)
        .await?;

    let mut best: HashMap<String, (u8, bool, i32)> = HashMap::new(); // pid -> (priority, enabled, position)

    for r in rows {
        let pid = extract_plugin_id(&r.namespace).to_string();
        let pri = scope_priority(&r.scope);

        best.entry(pid)
            .and_modify(|(cur_pri, enabled, pos)| {
                match pri.cmp(cur_pri) {
                    std::cmp::Ordering::Greater => {
                        // More specific scope: replace entirely
                        *cur_pri = pri;
                        *enabled = r.enabled;
                        *pos = r.position;
                    }
                    std::cmp::Ordering::Equal => {
                        // Same scope: aggregate (any-enabled, min-position)
                        *enabled = *enabled || r.enabled;
                        *pos = (*pos).min(r.position);
                    }
                    std::cmp::Ordering::Less => {
                        // Less specific: ignore
                    }
                }
            })
            .or_insert((pri, r.enabled, r.position));
    }

    Ok(best
        .into_iter()
        .filter(|(_, (_, enabled, _))| *enabled)
        .map(|(pid, (_, _, pos))| (pid, pos))
        .collect())
}

/// Outcome of dispatching hooks for an event.
#[derive(Debug)]
pub enum HookOutcome {
    /// All hooks passed (or returned Modified). Contains the possibly-modified event payload.
    Allowed(serde_json::Value),
    /// A hook rejected the event.
    Rejected {
        code: String,
        message: String,
        status_code: u16,
        /// Optional structured data from the plugin (e.g. remaining seconds, submission counts).
        details: Option<serde_json::Value>,
    },
    /// A hook returned Stop.
    Stopped,
}

/// Parse a Reject reason string (JSON) into structured fields.
fn parse_reject_detail(reason: &str) -> (String, String, u16, Option<serde_json::Value>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(reason) {
        let code = v
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("PLUGIN_REJECTED")
            .to_string();
        let message = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Request rejected by plugin")
            .to_string();
        let status_code = v.get("status_code").and_then(|s| s.as_u64()).unwrap_or(400) as u16;
        let details = v.get("details").cloned();
        (code, message, status_code, details)
    } else {
        ("PLUGIN_REJECTED".into(), reason.to_string(), 400, None)
    }
}

/// Dispatch hooks for an event.
pub async fn dispatch_hooks(
    topic: &str,
    event_payload: serde_json::Value,
    enabled_plugins: Option<&ResourceEnablements>,
    hook_registry: &SharedHookRegistry,
) -> Result<HookOutcome, AppError> {
    dispatch_hooks_inner(topic, event_payload, enabled_plugins, hook_registry, 0).await
}

/// Type-safe hook dispatch. Serializes the event struct and uses its TOPIC constant.
pub async fn dispatch_hooks_typed<E: HookEvent>(
    event: &E,
    enabled_plugins: Option<&ResourceEnablements>,
    hook_registry: &SharedHookRegistry,
) -> Result<HookOutcome, AppError> {
    let payload = serde_json::to_value(event)
        .map_err(|e| AppError::Internal(format!("Failed to serialize hook event: {e}")))?;
    dispatch_hooks(E::TOPIC, payload, enabled_plugins, hook_registry).await
}

/// Dispatch hooks in a background task. Returns immediately.
pub fn dispatch_hooks_background(
    topic: String,
    event_payload: serde_json::Value,
    enabled_plugins: Option<ResourceEnablements>,
    hook_registry: SharedHookRegistry,
) {
    tokio::spawn(async move {
        match dispatch_hooks(
            &topic,
            event_payload,
            enabled_plugins.as_ref(),
            &hook_registry,
        )
        .await
        {
            Ok(outcome) => match outcome {
                HookOutcome::Allowed(_) => {}
                HookOutcome::Rejected { code, message, .. } => {
                    tracing::warn!(
                        topic,
                        code,
                        message,
                        "Background hook returned Reject (no effect — response already sent)",
                    );
                }
                HookOutcome::Stopped => {
                    tracing::warn!(
                        topic,
                        "Background hook returned Stop (no effect — response already sent)",
                    );
                }
            },
            Err(e) => {
                tracing::warn!(topic, "Background hook dispatch failed: {e:?}");
            }
        }
    });
}

/// Type-safe background dispatch. Serializes the event and fires in a background task.
pub fn dispatch_hooks_background_typed<E: HookEvent + Send + 'static>(
    event: E,
    enabled_plugins: Option<ResourceEnablements>,
    hook_registry: SharedHookRegistry,
) {
    let topic = E::TOPIC.to_string();
    let payload = match serde_json::to_value(&event) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to serialize background hook event: {e}");
            return;
        }
    };
    dispatch_hooks_background(topic, payload, enabled_plugins, hook_registry);
}

async fn dispatch_hooks_inner(
    topic: &str,
    mut event_payload: serde_json::Value,
    enabled_plugins: Option<&ResourceEnablements>,
    hook_registry: &SharedHookRegistry,
    depth: u8,
) -> Result<HookOutcome, AppError> {
    if depth > MAX_CHAIN_DEPTH {
        tracing::warn!(
            topic,
            depth,
            "Hook chain depth exceeded, ignoring chained events"
        );
        return Ok(HookOutcome::Allowed(event_payload));
    }

    struct DispatchEntry {
        position: i32,
        is_global: bool,
        mode: HookMode,
        hook: Arc<dyn GenericHook<Context = ()>>,
    }

    let mut dispatch_list: Vec<DispatchEntry> = {
        let registry = hook_registry.read().await;
        let entries = match registry.get_hooks(topic) {
            Some(e) if !e.is_empty() => e,
            _ => return Ok(HookOutcome::Allowed(event_payload)),
        };

        let mut list: Vec<DispatchEntry> = Vec::new();

        for entry in entries {
            match entry.scope {
                HookScope::Global => {
                    list.push(DispatchEntry {
                        position: i32::MIN, // globals first
                        is_global: true,
                        mode: entry.mode,
                        hook: entry.hook.clone(),
                    });
                }
                HookScope::Resource => {
                    if let Some(ref enabled) = enabled_plugins
                        && let Some(&pos) = enabled.get(&entry.plugin_id)
                    {
                        list.push(DispatchEntry {
                            position: pos,
                            is_global: false,
                            mode: entry.mode,
                            hook: entry.hook.clone(),
                        });
                    }
                    // If no enablements provided, skip resource-scoped hooks
                }
            }
        }

        list
    }; // registry read lock dropped here

    // Sort globals first, then blocking before notify, then by position.
    dispatch_list.sort_by(|a, b| {
        b.is_global
            .cmp(&a.is_global)
            .then(a.mode.cmp(&b.mode))
            .then(a.position.cmp(&b.position))
    });

    for entry in &dispatch_list {
        let generic_event = GenericEvent {
            topic: topic.to_string(),
            payload: event_payload.clone(),
        };

        match entry.mode {
            HookMode::Blocking => {
                let action = entry.hook.on_event((), &generic_event).await.map_err(|e| {
                    tracing::error!(topic, "Hook execution error: {e}");
                    AppError::Internal(format!("Hook execution failed: {e}"))
                })?;

                match action {
                    HookAction::Pass => {}
                    HookAction::Modified(new_event) => {
                        event_payload = new_event.payload;
                    }
                    HookAction::Stop => {
                        return Ok(HookOutcome::Stopped);
                    }
                    HookAction::Reject(reason) => {
                        let (code, message, status_code, details) = parse_reject_detail(&reason);
                        return Ok(HookOutcome::Rejected {
                            code,
                            message,
                            status_code,
                            details,
                        });
                    }
                    HookAction::Chain(events) => {
                        for chained in events {
                            let result = Box::pin(dispatch_hooks_inner(
                                &chained.topic,
                                chained.payload,
                                enabled_plugins,
                                hook_registry,
                                depth + 1,
                            ))
                            .await?;
                            if let HookOutcome::Rejected { .. } | HookOutcome::Stopped = result {
                                return Ok(result);
                            }
                        }
                        return Ok(HookOutcome::Allowed(event_payload));
                    }
                }
            }
            HookMode::Notify => {
                match entry.hook.on_event((), &generic_event).await {
                    Ok(action) => match &action {
                        HookAction::Pass => {}
                        HookAction::Reject(reason) => {
                            tracing::debug!(
                                topic,
                                "Notify hook returned Reject (ignored): {reason}",
                            );
                        }
                        HookAction::Stop => {
                            tracing::debug!(topic, "Notify hook returned Stop (ignored)");
                        }
                        HookAction::Modified(_) => {
                            tracing::debug!(topic, "Notify hook returned Modified (ignored)");
                        }
                        HookAction::Chain(_) => {
                            tracing::debug!(topic, "Notify hook returned Chain (ignored)");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(topic, "Notify hook error (ignored): {e}");
                    }
                }
                // Always continue to next hook regardless of response
            }
        }
    }

    Ok(HookOutcome::Allowed(event_payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::async_trait::async_trait;
    use common::event::GenericEvent;
    use common::hook::GenericHookAction;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A test hook that returns a fixed action and records how many times it was called.
    struct MockHook {
        id: String,
        topics: Vec<String>,
        action: GenericHookAction,
        call_count: AtomicUsize,
    }

    impl MockHook {
        fn new(id: &str, topic: &str, action: GenericHookAction) -> Self {
            Self {
                id: id.into(),
                topics: vec![topic.into()],
                action,
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl GenericHook for MockHook {
        type Context = ();

        fn id(&self) -> &str {
            &self.id
        }

        fn topics(&self) -> &[String] {
            &self.topics
        }

        async fn on_event(
            &self,
            _ctx: (),
            _event: &GenericEvent,
        ) -> anyhow::Result<GenericHookAction> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(match &self.action {
                HookAction::Pass => HookAction::Pass,
                HookAction::Stop => HookAction::Stop,
                HookAction::Reject(r) => HookAction::Reject(r.clone()),
                HookAction::Modified(e) => HookAction::Modified(e.clone()),
                HookAction::Chain(v) => HookAction::Chain(v.clone()),
            })
        }
    }

    /// A hook that always fails with an error.
    struct FailingHook {
        id: String,
        topics: Vec<String>,
    }

    #[async_trait]
    impl GenericHook for FailingHook {
        type Context = ();

        fn id(&self) -> &str {
            &self.id
        }

        fn topics(&self) -> &[String] {
            &self.topics
        }

        async fn on_event(
            &self,
            _ctx: (),
            _event: &GenericEvent,
        ) -> anyhow::Result<GenericHookAction> {
            Err(anyhow::anyhow!("WASM crash"))
        }
    }

    /// Register a mock hook directly into the ServerHookRegistry with specified scope and mode.
    fn register_direct(
        registry: &mut ServerHookRegistry,
        hook: Arc<dyn GenericHook<Context = ()>>,
        scope: HookScope,
        mode: HookMode,
    ) {
        let plugin_id = hook.id().to_string();
        for topic in hook.topics() {
            registry
                .hooks
                .entry(topic.clone())
                .or_default()
                .push(HookEntry {
                    plugin_id: plugin_id.clone(),
                    scope,
                    mode,
                    hook: hook.clone(),
                });
        }
    }

    /// Helper: dispatch with no contest (global hooks only).
    async fn dispatch_global(
        registry: &SharedHookRegistry,
        topic: &str,
        payload: serde_json::Value,
    ) -> Result<HookOutcome, AppError> {
        dispatch_hooks(topic, payload, None, registry).await
    }

    #[tokio::test]
    async fn blocking_hook_reject_returns_rejected_with_parsed_code() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new(
            "rejector",
            "test",
            HookAction::Reject(r#"{"code":"NO","message":"nope","status_code":429}"#.into()),
        ));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Global,
            HookMode::Blocking,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Rejected { ref code, .. } if code == "NO"));
        assert_eq!(hook.calls(), 1);
    }

    #[tokio::test]
    async fn blocking_stop_halts_chain_and_skips_remaining_hooks() {
        let registry = new_shared_registry();
        let stop = Arc::new(MockHook::new("stopper", "test", HookAction::Stop));
        let pass = Arc::new(MockHook::new("passer", "test", HookAction::Pass));
        {
            let mut r = registry.write().await;
            register_direct(&mut r, stop.clone(), HookScope::Global, HookMode::Blocking);
            register_direct(&mut r, pass.clone(), HookScope::Global, HookMode::Blocking);
        }

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Stopped));
        assert_eq!(stop.calls(), 1);
        // The pass hook should NOT have been called (stop short-circuits)
        assert_eq!(pass.calls(), 0);
    }

    #[tokio::test]
    async fn notify_hook_reject_does_not_block_event() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new(
            "notify_rejector",
            "test",
            HookAction::Reject("denied".into()),
        ));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Global,
            HookMode::Notify,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        // Should be Allowed despite the hook returning Reject
        assert!(matches!(result, HookOutcome::Allowed(_)));
        assert_eq!(hook.calls(), 1);
    }

    #[tokio::test]
    async fn notify_hook_stop_does_not_halt_chain() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new("notify_stopper", "test", HookAction::Stop));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Global,
            HookMode::Notify,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Allowed(_)));
        assert_eq!(hook.calls(), 1);
    }

    #[tokio::test]
    async fn notify_hook_wasm_crash_does_not_propagate_error() {
        let registry = new_shared_registry();
        let hook = Arc::new(FailingHook {
            id: "crasher".into(),
            topics: vec!["test".into()],
        });
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Notify,
        );

        // Should not return an error — notify hooks swallow errors
        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Allowed(_)));
    }

    #[tokio::test]
    async fn blocking_hook_wasm_crash_propagates_as_app_error() {
        let registry = new_shared_registry();
        let hook = Arc::new(FailingHook {
            id: "crasher".into(),
            topics: vec!["test".into()],
        });
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Blocking,
        );

        // Blocking hook errors should propagate as AppError
        let result = dispatch_global(&registry, "test", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn blocking_hooks_execute_before_notify_hooks_regardless_of_registration_order() {
        let registry = new_shared_registry();
        // Register notify first, then blocking — but blocking should execute first
        let notify = Arc::new(MockHook::new("notify", "test", HookAction::Pass));
        let blocking = Arc::new(MockHook::new("blocker", "test", HookAction::Stop));
        {
            let mut r = registry.write().await;
            register_direct(&mut r, notify.clone(), HookScope::Global, HookMode::Notify);
            register_direct(
                &mut r,
                blocking.clone(),
                HookScope::Global,
                HookMode::Blocking,
            );
        }

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        // Blocking hook returns Stop, which short-circuits before the notify hook runs
        assert!(matches!(result, HookOutcome::Stopped));
        assert_eq!(blocking.calls(), 1);
        assert_eq!(notify.calls(), 0);
    }

    #[tokio::test]
    async fn topic_with_no_hooks_returns_allowed_with_original_payload() {
        let registry = new_shared_registry();
        let result = dispatch_global(&registry, "nonexistent", serde_json::json!({"x": 1}))
            .await
            .unwrap();
        match result {
            HookOutcome::Allowed(payload) => assert_eq!(payload["x"], 1),
            other => panic!("Expected Allowed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn unregistered_plugin_hooks_no_longer_fire() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new("plugin_a", "test", HookAction::Stop));
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Blocking,
        );

        // Verify hook exists
        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Stopped));

        // Unregister
        registry.write().await.unregister_plugin("plugin_a");

        // Hook should no longer fire
        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Allowed(_)));
    }

    #[tokio::test]
    async fn modified_payload_replaces_original_for_subsequent_hooks() {
        let registry = new_shared_registry();
        let modifier = Arc::new(MockHook::new(
            "modifier",
            "test",
            HookAction::Modified(GenericEvent {
                topic: "test".into(),
                payload: serde_json::json!({"modified": true}),
            }),
        ));
        let pass = Arc::new(MockHook::new("passer", "test", HookAction::Pass));
        {
            let mut r = registry.write().await;
            register_direct(&mut r, modifier, HookScope::Global, HookMode::Blocking);
            register_direct(&mut r, pass, HookScope::Global, HookMode::Blocking);
        }

        let result = dispatch_global(&registry, "test", serde_json::json!({"original": true}))
            .await
            .unwrap();
        match result {
            HookOutcome::Allowed(payload) => {
                assert_eq!(payload["modified"], true);
                // original field should be gone (payload was replaced)
                assert!(payload.get("original").is_none());
            }
            other => panic!("Expected Allowed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn reject_with_details_propagates_structured_data() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new(
            "limiter",
            "test",
            HookAction::Reject(
                r#"{"code":"LIMIT","message":"limit hit","status_code":429,"details":{"remaining":5,"total":10}}"#.into(),
            ),
        ));
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Blocking,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        match result {
            HookOutcome::Rejected { code, details, .. } => {
                assert_eq!(code, "LIMIT");
                let details = details.expect("details should be Some");
                assert_eq!(details["remaining"], 5);
                assert_eq!(details["total"], 10);
            }
            other => panic!("Expected Rejected, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn reject_without_details_returns_none() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new(
            "rejector",
            "test",
            HookAction::Reject(r#"{"code":"NO","message":"nope","status_code":400}"#.into()),
        ));
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Blocking,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        match result {
            HookOutcome::Rejected { code, details, .. } => {
                assert_eq!(code, "NO");
                assert!(details.is_none(), "details should be None when absent");
            }
            other => panic!("Expected Rejected, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn reject_with_plain_string_returns_no_details() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new(
            "rejector",
            "test",
            HookAction::Reject("plain text rejection".into()),
        ));
        register_direct(
            &mut *registry.write().await,
            hook,
            HookScope::Global,
            HookMode::Blocking,
        );

        let result = dispatch_global(&registry, "test", serde_json::json!({}))
            .await
            .unwrap();
        match result {
            HookOutcome::Rejected {
                code,
                message,
                details,
                ..
            } => {
                assert_eq!(code, "PLUGIN_REJECTED");
                assert_eq!(message, "plain text rejection");
                assert!(details.is_none());
            }
            other => panic!("Expected Rejected, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn resource_scoped_hook_fires_when_plugin_is_in_enablements() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new("cooldown", "test", HookAction::Stop));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Resource,
            HookMode::Blocking,
        );

        // With enablements containing this plugin — should fire
        let mut enabled = HashMap::new();
        enabled.insert("cooldown".to_string(), 0);
        let result = dispatch_hooks("test", serde_json::json!({}), Some(&enabled), &registry)
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Stopped));
        assert_eq!(hook.calls(), 1);
    }

    #[tokio::test]
    async fn resource_scoped_hook_skipped_when_no_enablements() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new("cooldown", "test", HookAction::Stop));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Resource,
            HookMode::Blocking,
        );

        // With None enablements — resource-scoped hook should NOT fire
        let result = dispatch_hooks("test", serde_json::json!({}), None, &registry)
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Allowed(_)));
        assert_eq!(hook.calls(), 0);
    }

    #[tokio::test]
    async fn resource_scoped_hook_skipped_when_plugin_not_in_enablements() {
        let registry = new_shared_registry();
        let hook = Arc::new(MockHook::new("cooldown", "test", HookAction::Stop));
        register_direct(
            &mut *registry.write().await,
            hook.clone(),
            HookScope::Resource,
            HookMode::Blocking,
        );

        // Enablements present but for a different plugin
        let mut enabled = HashMap::new();
        enabled.insert("other-plugin".to_string(), 0);
        let result = dispatch_hooks("test", serde_json::json!({}), Some(&enabled), &registry)
            .await
            .unwrap();
        assert!(matches!(result, HookOutcome::Allowed(_)));
        assert_eq!(hook.calls(), 0);
    }
}
