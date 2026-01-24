use crate::error::PluginError;
use extism::{Manifest, Plugin, Wasm};
use std::path::PathBuf;

/// Builder for creating Extism Plugin instances.
pub struct PluginBuilder {
    wasm_path: PathBuf,
    wasi_enabled: bool,
}

impl PluginBuilder {
    /// Creates a new builder for the specified Wasm file.
    pub fn new(wasm_path: PathBuf) -> Self {
        Self {
            wasm_path,
            wasi_enabled: false,
        }
    }

    /// Enables or disables WASI support.
    pub fn with_wasi(mut self, enable: bool) -> Self {
        self.wasi_enabled = enable;
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

        Plugin::new(&manifest, [], self.wasi_enabled).map_err(PluginError::Extism)
    }
}
