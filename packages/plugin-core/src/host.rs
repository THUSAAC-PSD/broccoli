use std::collections::HashMap;

use extism::Function;

/// A factory closure that produces a new Extism Function instance.
pub type FunctionFactory = Box<dyn Fn(&str) -> Function + Send + Sync>;

/// A registry that holds available host functions, grouped by permission keys.
pub struct HostFunctionRegistry {
    // Key: Permission name
    // Value: List of function factories associated with that permission
    factories: HashMap<String, Vec<FunctionFactory>>,
}

impl HostFunctionRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Registers a host function under a specific permission group.
    ///
    /// * `permission` - The permission string.
    /// * `factory` - A closure that returns a new `extism::Function`.
    pub fn register<F>(&mut self, permission: &str, factory: F)
    where
        F: Fn(&str) -> Function + Send + Sync + 'static,
    {
        self.factories
            .entry(permission.to_string())
            .or_default()
            .push(Box::new(factory));
    }

    /// Resolves a list of permissions into a concrete list of Extism Functions.
    ///
    /// This iterates through the requested permissions and calls the factories
    /// to generate the actual function instances.
    pub fn resolve(&self, plugin_id: &str, permissions: &[String]) -> Vec<Function> {
        let mut functions = Vec::new();
        for perm in permissions {
            if let Some(facts) = self.factories.get(perm) {
                for factory in facts {
                    functions.push(factory(plugin_id));
                }
            } else {
                eprintln!(
                    "Warning: Plugin requested unknown or empty permission '{}'",
                    perm
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
