use plugin_core::manifest::{ComponentMap, WebRouteConfig, WebSlotConfig};
use plugin_core::registry::{PluginInfo, PluginStatus};
use serde::Serialize;

#[derive(Serialize, utoipa::ToSchema)]
pub struct LanguageRegistryItem {
    /// Language identifier used in submission payloads.
    #[schema(example = "cpp")]
    pub id: String,
    /// Human-friendly display name.
    #[schema(example = "C++")]
    pub name: String,
    /// Default source filename derived from the language config.
    #[schema(example = "solution.cpp")]
    pub default_filename: String,
}

/// Available registry values for problem types, checker formats, and contest types.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RegistriesResponse {
    /// Available problem types (e.g. "batch", "interactive").
    #[schema(example = json!(["batch", "interactive"]))]
    pub problem_types: Vec<String>,

    /// Available checker formats (e.g. "exact", "tokens").
    #[schema(example = json!(["exact", "tokens"]))]
    pub checker_formats: Vec<String>,

    /// Available contest types (e.g. "icpc", "ioi").
    #[schema(example = json!(["icpc", "ioi"]))]
    pub contest_types: Vec<String>,

    /// Configured submission languages from the server config.
    pub languages: Vec<LanguageRegistryItem>,
}

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

    /// CSS files to load alongside the JS entry point.
    #[schema(example = json!(["style.css"]))]
    pub css: Vec<String>,
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

    /// Config schemas declared by this plugin.
    pub config_schemas: Vec<ConfigSchemaResponse>,
}

/// Config schema for a single namespace declared by a plugin.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ConfigSchemaResponse {
    /// Namespace name, e.g. "testlib".
    #[schema(example = "testlib")]
    pub namespace: String,
    /// Human-readable description of what this config controls.
    #[schema(example = "Testlib checker compiler settings")]
    pub description: Option<String>,
    /// Scopes where this config applies.
    pub scopes: Vec<String>,
    /// JSON Schema generated from the plugin's TOML schema definition.
    pub json_schema: serde_json::Value,
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

        let entry = {
            let base = format!("/assets/{}/{}", info.id, web_manifest.entry);
            let entry_path = info
                .root_dir
                .join(&web_manifest.root)
                .join(&web_manifest.entry);
            let mtime = std::fs::metadata(&entry_path)
                .and_then(|m| m.modified())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
                .unwrap_or(0);
            format!("{}?t={}", base, mtime)
        };

        let css: Vec<String> = web_manifest
            .css
            .iter()
            .map(|css_file| {
                let base = format!("/assets/{}/{}", info.id, css_file);
                let css_path = info.root_dir.join(&web_manifest.root).join(css_file);
                let mtime = std::fs::metadata(&css_path)
                    .and_then(|m| m.modified())
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    })
                    .unwrap_or(0);
                format!("{}?t={}", base, mtime)
            })
            .collect();

        Self {
            id: info.id.clone(),
            name: info.manifest.name,
            entry,
            components: web_manifest.components.clone(),
            slots: web_manifest.slots.clone(),
            routes: web_manifest.routes.clone(),
            css,
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

/// Response for the reload-all endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReloadAllResponse {
    /// Plugin IDs that were successfully reloaded.
    pub reloaded: Vec<String>,
    /// Plugin IDs that were newly discovered and loaded.
    pub new: Vec<String>,
    /// Plugins that failed to reload.
    pub failed: Vec<ReloadFailure>,
}

/// A plugin that failed to reload.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReloadFailure {
    /// The plugin ID.
    pub id: String,
    /// The error message.
    pub error: String,
}

impl From<PluginInfo> for PluginDetailResponse {
    fn from(info: PluginInfo) -> Self {
        let has_server = info.manifest.has_server();
        let has_worker = info.manifest.has_worker();
        let has_web = info.manifest.has_web();

        let mut config_schemas: Vec<_> = info
            .manifest
            .config
            .iter()
            .map(|(ns, entry)| ConfigSchemaResponse {
                namespace: ns.clone(),
                description: entry.description.clone(),
                scopes: entry.scopes.clone(),
                json_schema: entry.to_json_schema(),
            })
            .collect();
        config_schemas.sort_by(|a, b| a.namespace.cmp(&b.namespace));

        Self {
            id: info.id,
            status: info.status.into(),
            name: info.manifest.name,
            version: info.manifest.version,
            description: info.manifest.description,
            has_server,
            has_worker,
            has_web,
            config_schemas,
        }
    }
}
