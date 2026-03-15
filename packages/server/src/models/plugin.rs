use plugin_core::manifest::{ComponentMap, ServerRouteConfig, WebRouteConfig, WebSlotConfig};
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

    /// Components exposed by the plugin, where the key is the component name
    /// and the value is the name as exported by the JS entry file.
    pub components: ComponentMap,

    /// Slots for UI extension.
    #[schema(example = json!([
        {
            "name": "sidebar.footer",
            "position": "append",
            "component": "MyComponent",
            "priority": 10
        }
    ]))]
    pub slots: Vec<WebSlotConfig>,

    /// Routes for client-side navigation.
    #[schema(example = json!([
        {
            "path": "/problems/{id}/export",
            "component": "MyPage"
        }
    ]))]
    pub routes: Vec<WebRouteConfig>,
}

/// Detailed information about a plugin (used in list endpoint).
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

/// Full detailed information about a plugin including manifest data.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PluginFullDetailResponse {
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

    /// Server component details (if present).
    pub server: Option<ServerDetailResponse>,
    /// Worker component details (if present).
    pub worker: Option<WorkerDetailResponse>,
    /// Web component details (if present).
    pub web: Option<WebDetailResponse>,
    /// Available translation locales.
    pub translations: Vec<String>,
}

/// Server component details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ServerDetailResponse {
    /// Permissions requested by the server component.
    pub permissions: Vec<String>,
    /// HTTP routes exposed by the server component.
    pub routes: Vec<ServerRouteConfig>,
}

/// Worker component details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct WorkerDetailResponse {
    /// Permissions requested by the worker component.
    pub permissions: Vec<String>,
}

/// Web (frontend) component details.
#[derive(Serialize, utoipa::ToSchema)]
pub struct WebDetailResponse {
    /// Components exposed by the plugin.
    pub components: ComponentMap,
    /// Slots for UI extension.
    pub slots: Vec<WebSlotConfig>,
    /// Routes for client-side navigation.
    pub routes: Vec<WebRouteConfig>,
}

/// Plugin status for API responses, abstracting away error details.
#[derive(Serialize, utoipa::ToSchema)]
pub enum PluginStatusResponse {
    /// The plugin is discovered but not loaded.
    Unloaded,
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
            components: web_manifest.components.clone(),
            slots: web_manifest.slots.clone(),
            routes: web_manifest.routes.clone(),
        }
    }
}

impl From<PluginStatus> for PluginStatusResponse {
    fn from(status: PluginStatus) -> Self {
        match status {
            PluginStatus::Unloaded => Self::Unloaded,
            PluginStatus::Loaded => Self::Loaded,
            PluginStatus::Failed(_) => Self::Failed,
        }
    }
}

impl From<PluginInfo> for PluginDetailResponse {
    fn from(info: PluginInfo) -> Self {
        let has_server = info.manifest.has_server();
        let has_worker = info.manifest.has_worker();
        let has_web = info.manifest.has_web();

        Self {
            id: info.id,
            status: info.status.into(),
            name: info.manifest.name,
            version: info.manifest.version,
            description: info.manifest.description,
            has_server,
            has_worker,
            has_web,
        }
    }
}

impl From<PluginInfo> for PluginFullDetailResponse {
    fn from(info: PluginInfo) -> Self {
        let has_server = info.manifest.has_server();
        let has_worker = info.manifest.has_worker();
        let has_web = info.manifest.has_web();

        let server = info.manifest.server.as_ref().map(|s| ServerDetailResponse {
            permissions: s.permissions.clone(),
            routes: s.routes.clone(),
        });

        let worker = info.manifest.worker.as_ref().map(|w| WorkerDetailResponse {
            permissions: w.permissions.clone(),
        });

        let web = info.manifest.web.as_ref().map(|w| WebDetailResponse {
            components: w.components.clone(),
            slots: w.slots.clone(),
            routes: w.routes.clone(),
        });

        let translations: Vec<String> = info.manifest.translations.keys().cloned().collect();

        Self {
            id: info.id,
            status: info.status.into(),
            name: info.manifest.name,
            version: info.manifest.version,
            description: info.manifest.description,
            has_server,
            has_worker,
            has_web,
            server,
            worker,
            web,
            translations,
        }
    }
}
