use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub backend: PluginBackend,
    // TODO: pub frontend: Option<PluginFrontend>
}

#[derive(Debug, Deserialize)]
pub struct PluginBackend {
    /// Relative path to the .wasm file (e.g., "./main.wasm")
    pub wasm: String,
}
