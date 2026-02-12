use serde::Serialize;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ActivePluginResponse {
    /// Unique identifier for the plugin.
    #[schema(example = "plugin-123")]
    pub id: String,
    /// Plugin name.
    #[schema(example = "An Awesome Plugin")]
    pub name: String,
    /// Public URL to the plugin's frontend ESM entry point, empty if unavailable.
    #[schema(example = "/assets/plugin-123/index.js")]
    pub entry: String,
}
