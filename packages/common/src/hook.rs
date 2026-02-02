use anyhow::Result;
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

use crate::event::{Event, GenericEvent};

// TODO: add hook priorities?

/// Typed hook, used for specific event type
#[async_trait]
pub trait Hook<E: Event>: Send + Sync {
    type Output: Event;
    type Context: Send + Sync + 'static;

    /// Hook identifier
    fn id(&self) -> &str;
    /// Get the topics this hook is interested in
    fn topics(&self) -> &[&str];

    async fn on_register(&self, _ctx: Self::Context) -> Result<()> {
        Ok(())
    }
    async fn on_unregister(&self, _ctx: Self::Context) -> Result<()> {
        Ok(())
    }
    async fn on_event(&self, ctx: Self::Context, e: &E) -> Result<HookAction<E, Self::Output>>;
}

/// E: input event type
/// O: output event type (for chaining)
pub enum HookAction<E: Event, O: Event = E> {
    Pass,
    Stop,
    Modified(E),
    Reject(String),
    Chain(Vec<O>),
}

pub type GenericHookAction = HookAction<GenericEvent>;

/// Generic hook trait object for dynamic dispatch
#[async_trait]
pub trait GenericHook: Send + Sync {
    type Context: Send + Sync + 'static;

    /// Hook identifier
    fn id(&self) -> &str;
    /// Get the topics this hook is interested in
    fn topics(&self) -> &[&str];

    async fn on_register(&self, _ctx: Self::Context) -> Result<()> {
        Ok(())
    }
    async fn on_unregister(&self, _ctx: Self::Context) -> Result<()> {
        Ok(())
    }
    async fn on_event(&self, ctx: Self::Context, e: &GenericEvent) -> Result<GenericHookAction>;
}

/// Adapter to convert typed Hook<E> into GenericHook
pub struct HookAdapter<E: Event, H: Hook<E>> {
    hook: Arc<H>,
    _phantom: std::marker::PhantomData<E>,
}

#[async_trait]
impl<E: Event, H: Hook<E>> GenericHook for HookAdapter<E, H> {
    type Context = H::Context;

    fn id(&self) -> &str {
        self.hook.id()
    }
    fn topics(&self) -> &[&str] {
        self.hook.topics()
    }
    async fn on_event(
        &self,
        ctx: H::Context,
        generic_event: &GenericEvent,
    ) -> Result<GenericHookAction> {
        let typed_event: E = E::from_generic_event(generic_event)?;
        let action = self.hook.on_event(ctx, &typed_event).await?;
        match action {
            HookAction::Pass => Ok(GenericHookAction::Pass),
            HookAction::Stop => Ok(GenericHookAction::Stop),
            HookAction::Modified(modified_event) => Ok(GenericHookAction::Modified(
                modified_event.to_generic_event(),
            )),
            HookAction::Reject(reason) => Ok(GenericHookAction::Reject(reason)),
            HookAction::Chain(events) => Ok(GenericHookAction::Chain(
                events.into_iter().map(|e| e.to_generic_event()).collect(),
            )),
        }
    }

    async fn on_register(&self, ctx: H::Context) -> Result<()> {
        self.hook.on_register(ctx).await
    }

    async fn on_unregister(&self, ctx: H::Context) -> Result<()> {
        self.hook.on_unregister(ctx).await
    }
}

/// Generic hook registry to manage hooks
/// All hooks share the same context in a single registry
#[derive(Clone)]
pub struct HookRegistry<Context: Send + Sync + Copy + 'static = ()> {
    ctx: Context,
    hooks: HashMap<String, Vec<Arc<dyn GenericHook<Context = Context>>>>,
}

impl<C: Send + Sync + Copy + 'static> HookRegistry<C> {
    pub fn new(ctx: C) -> Self {
        Self {
            ctx,
            hooks: HashMap::new(),
        }
    }

    /// Add a typed hook to the registry
    pub async fn add_hook<E: Event + 'static, H: Hook<E, Context = C> + 'static>(
        &mut self,
        hook: H,
    ) -> Result<()> {
        let adapter = Arc::new(HookAdapter::<E, H> {
            hook: Arc::new(hook),
            _phantom: std::marker::PhantomData,
        });
        adapter.on_register(self.ctx).await?;

        for &topic in adapter.topics() {
            self.hooks
                .entry(topic.to_string())
                .or_default()
                .push(adapter.clone());
        }
        Ok(())
    }

    /// Add a generic hook to the registry
    pub async fn add_generic_hook(
        &mut self,
        hook: Arc<dyn GenericHook<Context = C>>,
    ) -> Result<()> {
        hook.on_register(self.ctx).await?;
        for &topic in hook.topics() {
            self.hooks
                .entry(topic.to_string())
                .or_default()
                .push(hook.clone());
        }
        Ok(())
    }

    /// Remove a hook by its ID, only removes the first
    pub async fn remove_hook(&mut self, hook_id: &str) -> Result<()> {
        let hooks = &mut self.hooks;

        for hooks_list in hooks.values_mut() {
            if let Some(pos) = hooks_list.iter().position(|h| h.id() == hook_id) {
                let hook = hooks_list.remove(pos);
                hook.on_unregister(self.ctx).await?;
                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Hook not found: {}", hook_id))
    }

    /// Trigger all hooks for an event
    /// @return true if event is allowed to proceed, false if stopped
    pub async fn trigger<E: Event>(&self, event: &E) -> Result<HookAction<E>> {
        let topic = event.topic();
        let hooks = match self.hooks.get(topic) {
            Some(h) if !h.is_empty() => h,
            _ => return Ok(HookAction::Pass),
        };

        let mut generic_event = event.to_generic_event();
        for hook in hooks {
            let action = hook.on_event(self.ctx, &generic_event).await?;

            match action {
                HookAction::Pass => {}
                HookAction::Modified(new_event) => {
                    generic_event = new_event;
                }
                HookAction::Stop => {
                    return Ok(HookAction::Stop);
                }
                HookAction::Reject(reason) => {
                    return Err(anyhow::anyhow!(
                        "Event rejected by hook {}: {}",
                        hook.id(),
                        reason
                    ));
                }
                HookAction::Chain(events) => {
                    // TODO: whether to auto trigger these chained events
                    return Ok(HookAction::Chain(
                        events
                            .into_iter()
                            .map(|e| E::from_generic_event(&e))
                            .collect::<Result<Vec<E>>>()?,
                    ));
                }
            }
        }

        Ok(HookAction::Modified(E::from_generic_event(&generic_event)?))
    }
}
