use std::collections::HashMap;

use extism::Function;

type SingleFactory = Box<dyn Fn(&str) -> Function + Send + Sync>;

type MultiFactory = Box<dyn Fn(&str) -> Vec<Function> + Send + Sync>;

enum FunctionFactory {
    Single(SingleFactory),
    Multi(MultiFactory),
}

pub struct HostFunctionRegistry {
    factories: HashMap<String, Vec<FunctionFactory>>,
}

impl HostFunctionRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, permission: &str, factory: F)
    where
        F: Fn(&str) -> Function + Send + Sync + 'static,
    {
        self.factories
            .entry(permission.to_string())
            .or_default()
            .push(FunctionFactory::Single(Box::new(factory)));
    }

    pub fn register_many<F>(&mut self, permission: &str, factory: F)
    where
        F: Fn(&str) -> Vec<Function> + Send + Sync + 'static,
    {
        self.factories
            .entry(permission.to_string())
            .or_default()
            .push(FunctionFactory::Multi(Box::new(factory)));
    }

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
