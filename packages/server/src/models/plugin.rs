use plugin_core::registry::{PluginInfo, PluginStatus};
use serde::Serialize;

/// Response model for listing active web plugins.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ActivePluginResponse {
    /// Unique identifier for the plugin.
    #[schema(example = "plugin-123")]
    pub id: String,
    /// Plugin name.
    #[schema(example = "An Awesome Plugin")]
    pub name: String,
    /// Public URL to the plugin's frontend ESM entry point.
    #[schema(example = "/assets/plugin-123/index.js")]
    pub entry: String,
}

/// Detailed information about a plugin.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PluginDetailResponse {
    /// Unique identifier for the plugin.
    #[schema(example = "plugin-123")]
    pub id: String,
    /// Plugin status.
    #[schema(example = "Loaded")]
    pub status: PluginStatusResponse,

    /// Plugin name.
    #[schema(example = "An Awesome Plugin")]
    pub name: String,
    /// Plugin version.
    #[schema(example = "1.0.0")]
    pub version: String,
    /// Plugin description.
    #[schema(example = "This plugin does awesome things!")]
    pub description: Option<String>,

    /// Indicates if the plugin has a server component.
    #[schema(example = true)]
    pub has_server: bool,
    /// Indicates if the plugin has a worker component.
    #[schema(example = false)]
    pub has_worker: bool,
    /// Indicates if the plugin has a web (frontend) component.
    #[schema(example = true)]
    pub has_web: bool,
}

/// Plugin status for API responses, abstracting away error details.
#[derive(Serialize, utoipa::ToSchema)]
pub enum PluginStatusResponse {
    /// The plugin is discovered but not loaded.
    Discovered,
    /// The plugin is fully loaded and running.
    Loaded,
    /// The plugin failed to load or encountered an error.
    Failed,
}

impl From<PluginInfo> for ActivePluginResponse {
    fn from(info: PluginInfo) -> Self {
        let web_manifest = info.manifest.web.as_ref().expect(
            "PluginInfo should only be converted to ActivePluginResponse if it has a web component",
        );

        Self {
            id: info.id.clone(),
            name: info.manifest.name,
            entry: format!("/assets/{}/{}", info.id, web_manifest.entry),
        }
    }
}

impl From<PluginStatus> for PluginStatusResponse {
    fn from(status: PluginStatus) -> Self {
        match status {
            PluginStatus::Discovered => Self::Discovered,
            PluginStatus::Loaded => Self::Loaded,
            PluginStatus::Failed(_) => Self::Failed,
        }
    }
}

impl From<PluginInfo> for PluginDetailResponse {
    fn from(info: PluginInfo) -> Self {
        Self {
            id: info.id,
            status: info.status.into(),
            name: info.manifest.name,
            version: info.manifest.version,
            description: info.manifest.description,
            has_server: info.manifest.server.is_some(),
            has_worker: info.manifest.worker.is_some(),
            has_web: info.manifest.web.is_some(),
        }
    }
}
