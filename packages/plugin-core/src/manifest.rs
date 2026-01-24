use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,

    /// Configuration for the Server environment
    pub server: Option<ServerConfig>,

    /// Configuration for the Judger environment
    pub judger: Option<JudgerConfig>,

    /// Configuration for the Web (Frontend) environment
    pub web: Option<WebConfig>,
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
pub struct JudgerConfig {
    /// Path to the Wasm file relative to the plugin root
    pub entry: String,

    /// List of permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebConfig {
    /// Path to the JS entry point
    pub entry: String,
}
