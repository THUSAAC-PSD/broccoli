use std::fmt::Display;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,

    /// Configuration for the Server environment
    pub server: Option<ServerConfig>,

    /// Configuration for the Worker environment
    pub worker: Option<WorkerConfig>,

    /// Configuration for the Web (Frontend) environment
    pub web: Option<WebConfig>,
}

impl PluginManifest {
    pub fn is_hollow(&self) -> bool {
        self.server.is_none() && self.worker.is_none() && self.web.is_none()
    }
}

impl Display for PluginManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (v{})", self.name, self.version)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Path to the Wasm file relative to the plugin root
    pub entry: String,

    /// List of permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorkerConfig {
    /// Path to the Wasm file relative to the plugin root
    pub entry: String,

    /// List of permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebConfig {
    /// The root directory for the web assets, e.g., "dist" or "public".
    pub root: String,

    /// Path to the JS entry file relative to the web root, e.g., "index.js".
    pub entry: String,
    // TODO: styles
}
