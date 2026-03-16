use std::collections::HashMap;

use extism::Function;

/// A factory closure that produces a single Extism Function instance.
type SingleFactory = Box<dyn Fn(&str) -> Function + Send + Sync>;

/// A factory closure that produces multiple Extism Function instances.
type MultiFactory = Box<dyn Fn(&str) -> Vec<Function> + Send + Sync>;

enum FunctionFactory {
    Single(SingleFactory),
    Multi(MultiFactory),
}

/// A registry that holds available host functions, grouped by permission keys.
pub struct HostFunctionRegistry {
    factories: HashMap<String, Vec<FunctionFactory>>,
}

impl HostFunctionRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Registers a single host function under a specific permission group.
    pub fn register<F>(&mut self, permission: &str, factory: F)
    where
        F: Fn(&str) -> Function + Send + Sync + 'static,
    {
        self.factories
            .entry(permission.to_string())
            .or_default()
            .push(FunctionFactory::Single(Box::new(factory)));
    }

    /// Registers multiple host functions under a specific permission group.
    ///
    /// The factory is called once per plugin and all returned functions are added.
    pub fn register_many<F>(&mut self, permission: &str, factory: F)
    where
        F: Fn(&str) -> Vec<Function> + Send + Sync + 'static,
    {
        self.factories
            .entry(permission.to_string())
            .or_default()
            .push(FunctionFactory::Multi(Box::new(factory)));
    }

    /// Resolves a list of permissions into a concrete list of Extism Functions.
    pub fn resolve(&self, plugin_id: &str, permissions: &[String]) -> Vec<Function> {
        let mut functions = Vec::new();
        for perm in permissions {
            if let Some(facts) = self.factories.get(perm) {
                for factory in facts {
                    match factory {
                        FunctionFactory::Single(f) => functions.push(f(plugin_id)),
                        FunctionFactory::Multi(f) => functions.extend(f(plugin_id)),
                    }
                }
            } else {
                tracing::warn!(
                    permission = %perm,
                    "Plugin requested unknown or empty permission"
                );
            }
        }
        functions
    }
}

impl Default for HostFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
