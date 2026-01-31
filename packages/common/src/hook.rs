use anyhow::Result;
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

use crate::event::{Event, GenericEvent};

/// Typed hook, used for specific event type
#[async_trait]
pub trait Hook<E: Event>: Send + Sync {
    /// Hook identifier
    fn id(&self) -> &str;
    /// Get the topics this hook is interested in
    fn topics(&self) -> &[&str];

    async fn on_register(&self) -> Result<()> {
        Ok(())
    }
    async fn on_unregister(&self) -> Result<()> {
        Ok(())
    }
    async fn on_event(&self, e: &E) -> Result<HookAction<E>>;
}

/// A hook can pass, stop, or modify the event
#[derive(Debug)]
pub enum HookAction<E: Event> {
    Pass,
    Stop,
    Modified(E),
}

pub type GenericHookAction = HookAction<GenericEvent>;

/// Generic hook trait object for dynamic dispatch
#[async_trait]
pub trait GenericHook: Send + Sync {
    /// Hook identifier
    fn id(&self) -> &str;
    /// Get the topics this hook is interested in
    fn topics(&self) -> &[&str];

    async fn on_register(&self) -> Result<()> {
        Ok(())
    }
    async fn on_unregister(&self) -> Result<()> {
        Ok(())
    }
    async fn on_event(&self, e: &GenericEvent) -> Result<GenericHookAction>;
}

/// Adapter to convert typed Hook<E> into GenericHook
pub struct HookAdapter<E: Event, H: Hook<E>> {
    hook: Arc<H>,
    _phantom: std::marker::PhantomData<E>,
}

#[async_trait]
impl<E: Event, H: Hook<E>> GenericHook for HookAdapter<E, H> {
    fn id(&self) -> &str {
        self.hook.id()
    }
    fn topics(&self) -> &[&str] {
        self.hook.topics()
    }
    async fn on_event(&self, generic_event: &GenericEvent) -> Result<GenericHookAction> {
        let typed_event: E = E::from_generic_event(generic_event)?;
        let action = self.hook.on_event(&typed_event).await?;
        match action {
            HookAction::Pass => Ok(GenericHookAction::Pass),
            HookAction::Stop => Ok(GenericHookAction::Stop),
            HookAction::Modified(modified_event) => Ok(GenericHookAction::Modified(
                modified_event.to_generic_event(),
            )),
        }
    }
}

/// Generic hook registry to manage hooks
#[derive(Clone)]
pub struct HookRegistry {
    hooks: HashMap<String, Vec<Arc<dyn GenericHook>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Add a typed hook to the registry
    pub fn add_hook<E: Event + 'static, H: Hook<E> + 'static>(&mut self, hook: Arc<H>) {
        let adapter = Arc::new(HookAdapter::<E, H> {
            hook,
            _phantom: std::marker::PhantomData,
        });

        for &topic in adapter.topics() {
            self.hooks
                .entry(topic.to_string())
                .or_default()
                .push(adapter.clone());
        }
    }

    /// Add a generic hook to the registry
    pub fn add_generic_hook<H: GenericHook + 'static>(&mut self, hook: Arc<H>) {
        for &topic in hook.topics() {
            self.hooks
                .entry(topic.to_string())
                .or_default()
                .push(hook.clone());
        }
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
            let action = hook.on_event(&generic_event).await?;

            match action {
                HookAction::Pass => {}
                HookAction::Modified(new_event) => {
                    generic_event = new_event;
                }
                HookAction::Stop => {
                    return Ok(HookAction::Stop);
                }
            }
        }

        Ok(HookAction::Modified(E::from_generic_event(&generic_event)?))
    }
}
