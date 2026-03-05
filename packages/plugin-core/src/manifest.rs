use std::collections::HashMap;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
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

    /// Translations for i18n, where the key is the locale (e.g., "en-US") and
    /// the value is a path to a translation file in TOML format.
    #[serde(default)]
    pub translations: HashMap<String, String>,
}

impl PluginManifest {
    pub fn has_server(&self) -> bool {
        self.server.is_some()
    }

    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }

    pub fn has_web(&self) -> bool {
        self.web.is_some()
    }

    pub fn is_hollow(&self) -> bool {
        !self.has_server() && !self.has_worker() && !self.has_web()
    }
}

impl Display for PluginManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (v{})", self.name, self.version)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    /// Path to the Wasm file relative to the plugin root
    pub entry: String,

    /// List of permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<String>,

    /// List of HTTP routes exposed by the plugin
    #[serde(default)]
    pub routes: Vec<ServerRouteConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerRouteConfig {
    /// The HTTP method for the route, e.g., "GET", "POST", etc.
    pub method: String,

    /// The URL path for the route, e.g., "/problems/{id}/export".
    pub path: String,

    /// The handler function for this route.
    pub handler: String,

    /// The permission required to access this route, e.g., "problems:export".
    /// If not specified, the route is public.
    #[serde(default)]
    pub permission: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkerConfig {
    /// Path to the Wasm file relative to the plugin root
    pub entry: String,

    /// List of permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebConfig {
    /// The root directory for the web assets, e.g., "dist" or "public".
    pub root: String,

    /// Path to the JS entry file relative to the web root, e.g., "index.js".
    pub entry: String,

    /// Components exposed by the plugin, where the key is the component name
    /// and the value is the name as exported by the JS entry file.
    #[serde(default)]
    pub components: ComponentMap,

    /// Slots for UI extension.
    #[serde(default)]
    pub slots: Vec<WebSlotConfig>,

    /// Routes for client-side navigation.
    #[serde(default)]
    pub routes: Vec<WebRouteConfig>,
}

// pub type ComponentMap = HashMap<String, String>;

#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema, Default)]
#[schema(example = json!({
    "MyComponent": "MyComponent",
    "MyPage": "MyPage"
}))]
pub struct ComponentMap(HashMap<String, String>);

#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum WebSlotPosition {
    Append,
    Prepend,
    Replace,
    Before,
    After,
    Wrap,
}

#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema)]
pub struct WebSlotConfig {
    /// Name of the slot to render into, e.g., "sidebar.footer".
    pub name: String,

    /// Positioning strategy for the component in the slot.
    pub position: WebSlotPosition,

    /// Name of the component to render in this slot, which must match a key in
    /// the `components` map.
    pub component: String,

    /// Priority for ordering when multiple plugins target the same slot.
    pub priority: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone, utoipa::ToSchema)]
pub struct WebRouteConfig {
    /// Path for client-side navigation, e.g., "/problems/{id}/export".
    pub path: String,

    /// Component to render for this route, which must match a key in the
    /// `components` map.
    pub component: String,

    /// Meta information for this route, which can be used for things like page
    /// titles or icons in the frontend.
    pub meta: Option<HashMap<String, String>>,
}
