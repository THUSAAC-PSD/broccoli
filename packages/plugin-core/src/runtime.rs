use std::path::PathBuf;

use extism::{Function, Manifest, Plugin, Wasm};

use crate::error::PluginError;
use crate::host::HostFunctionRegistry;

/// Builder for creating Extism Plugin instances.
pub struct PluginBuilder {
    wasm_path: PathBuf,
    wasi_enabled: bool,
    host_functions: Vec<Function>,
}

impl PluginBuilder {
    /// Creates a new builder for the specified Wasm file.
    pub fn new(wasm_path: PathBuf) -> Self {
        Self {
            wasm_path,
            wasi_enabled: false,
            host_functions: vec![],
        }
    }

    /// Enables or disables WASI support.
    pub fn with_wasi(mut self, enable: bool) -> Self {
        self.wasi_enabled = enable;
        self
    }

    /// Manually adds a single host function to the plugin.
    /// Useful for adding functions that are always available regardless of permissions.
    pub fn with_function(mut self, f: Function) -> Self {
        self.host_functions.push(f);
        self
    }

    /// Automatically injects host functions based on requested permissions using the registry.
    pub fn with_permissions(
        mut self,
        plugin_id: &str,
        permissions: &[String],
        registry: &HostFunctionRegistry,
    ) -> Self {
        let funcs = registry.resolve(plugin_id, permissions);
        self.host_functions.extend(funcs);
        self
    }

    /// Builds the final Extism Plugin instance.
    pub fn build(self) -> Result<Plugin, PluginError> {
        if !self.wasm_path.exists() {
            return Err(PluginError::NotFound(format!(
                "Wasm binary not found at {:?}",
                self.wasm_path
            )));
        }

        let wasm = Wasm::file(&self.wasm_path);
        let manifest = Manifest::new([wasm]);

        Plugin::new(&manifest, self.host_functions, self.wasi_enabled).map_err(PluginError::Extism)
    }
}
