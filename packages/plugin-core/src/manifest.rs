use std::collections::HashMap;
use std::fmt::Display;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

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

    /// Config schemas declared by this plugin. Keys are namespace names.
    #[serde(default)]
    pub config: HashMap<String, ConfigNamespace>,
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

    /// Resolve external schema file references in config namespaces.
    /// When a namespace declares `schema = "path/to/file.toml"`, the file is
    /// read and its contents replace the inline `properties`.
    pub fn resolve_schema_includes(&mut self, root_dir: &Path) -> Result<(), PluginError> {
        for (ns_name, ns) in &mut self.config {
            if let Some(ref schema_path) = ns.schema {
                let full_path = root_dir.join(schema_path);
                let content = std::fs::read_to_string(&full_path).map_err(|e| {
                    PluginError::LoadFailed(format!(
                        "Failed to read schema file '{}' for config namespace '{}': {}",
                        full_path.display(),
                        ns_name,
                        e
                    ))
                })?;
                let properties: HashMap<String, SchemaProperty> = toml::from_str(&content)
                    .map_err(|e| {
                        PluginError::LoadFailed(format!(
                            "Invalid schema file '{}' for config namespace '{}': {}",
                            full_path.display(),
                            ns_name,
                            e
                        ))
                    })?;
                ns.properties = properties;
            }
        }
        Ok(())
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

/// A config namespace declaration with typed schema properties.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigNamespace {
    pub description: Option<String>,
    /// Scopes where this config applies: "plugin", "problem", "contest", "contest_problem"
    #[serde(default)]
    pub scopes: Vec<String>,
    /// External schema file path (relative to plugin root). Overrides inline properties.
    pub schema: Option<String>,
    /// Inline schema properties.
    #[serde(default)]
    pub properties: HashMap<String, SchemaProperty>,
}

impl ConfigNamespace {
    /// Convert this namespace to a JSON Schema object.
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), "object".into());
        if let Some(ref desc) = self.description {
            schema.insert("description".into(), desc.clone().into());
        }
        if !self.properties.is_empty() {
            let mut props = serde_json::Map::new();
            for (name, prop) in &self.properties {
                props.insert(name.clone(), prop.to_json_schema());
            }
            schema.insert("properties".into(), props.into());
        }
        schema.into()
    }
}

/// A property definition that converts to JSON Schema.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SchemaProperty {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub default: Option<serde_json::Value>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub pattern: Option<String>,
    pub format: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<serde_json::Value>>,
    pub items: Option<Box<SchemaProperty>>,
    #[serde(default)]
    pub properties: HashMap<String, SchemaProperty>,
    pub required: Option<Vec<String>>,
    pub additional_properties: Option<bool>,
}

impl SchemaProperty {
    /// Convert this property to a JSON Schema value.
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), self.schema_type.clone().into());

        if let Some(ref v) = self.title {
            schema.insert("title".into(), v.clone().into());
        }
        if let Some(ref v) = self.description {
            schema.insert("description".into(), v.clone().into());
        }
        if let Some(ref v) = self.default {
            schema.insert("default".into(), v.clone());
        }
        if let Some(v) = self.min {
            schema.insert("minimum".into(), v.into());
        }
        if let Some(v) = self.max {
            schema.insert("maximum".into(), v.into());
        }
        if let Some(v) = self.min_length {
            schema.insert("minLength".into(), v.into());
        }
        if let Some(v) = self.max_length {
            schema.insert("maxLength".into(), v.into());
        }
        if let Some(ref v) = self.pattern {
            schema.insert("pattern".into(), v.clone().into());
        }
        if let Some(ref v) = self.format {
            schema.insert("format".into(), v.clone().into());
        }
        if let Some(ref v) = self.enum_values {
            schema.insert("enum".into(), v.clone().into());
        }
        if let Some(ref v) = self.items {
            schema.insert("items".into(), v.to_json_schema());
        }
        if !self.properties.is_empty() {
            let mut props = serde_json::Map::new();
            for (name, prop) in &self.properties {
                props.insert(name.clone(), prop.to_json_schema());
            }
            schema.insert("properties".into(), props.into());
        }
        if let Some(ref v) = self.required {
            schema.insert("required".into(), v.clone().into());
        }
        if let Some(v) = self.additional_properties {
            schema.insert("additionalProperties".into(), v.into());
        }

        schema.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_manifest_with_inline_config_schema() {
        let toml_str = r#"
            name = "test-plugin"
            version = "1.0.0"

            [config.my-ns]
            description = "Test config"
            scopes = ["plugin", "problem"]

            [config.my-ns.properties.timeout]
            type = "number"
            title = "Timeout"
            default = 30.0
            min = 0.0
            max = 300.0
        "#;

        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.config.len(), 1);

        let ns = &manifest.config["my-ns"];
        assert_eq!(ns.description.as_deref(), Some("Test config"));
        assert_eq!(ns.scopes, vec!["plugin", "problem"]);
        assert_eq!(ns.properties.len(), 1);

        let prop = &ns.properties["timeout"];
        assert_eq!(prop.schema_type, "number");
        assert_eq!(prop.title.as_deref(), Some("Timeout"));
        assert_eq!(prop.default, Some(json!(30.0)));
        assert_eq!(prop.min, Some(0.0));
        assert_eq!(prop.max, Some(300.0));
    }

    #[test]
    fn parse_manifest_without_config() {
        let toml_str = r#"
            name = "test-plugin"
            version = "1.0.0"
        "#;

        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.config.is_empty());
    }

    #[test]
    fn schema_property_to_json_schema_basic() {
        let prop = SchemaProperty {
            schema_type: "string".into(),
            title: Some("Name".into()),
            description: Some("A name field".into()),
            default: Some(json!("hello")),
            min: None,
            max: None,
            min_length: Some(1),
            max_length: Some(100),
            pattern: Some("^[a-z]+$".into()),
            format: None,
            enum_values: None,
            items: None,
            properties: HashMap::new(),
            required: None,
            additional_properties: None,
        };

        let schema = prop.to_json_schema();
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["title"], "Name");
        assert_eq!(schema["description"], "A name field");
        assert_eq!(schema["default"], "hello");
        assert_eq!(schema["minLength"], 1);
        assert_eq!(schema["maxLength"], 100);
        assert_eq!(schema["pattern"], "^[a-z]+$");
    }

    #[test]
    fn schema_property_to_json_schema_nested_object() {
        let toml_str = r#"
            name = "test-plugin"
            version = "1.0.0"

            [config.settings]
            description = "Settings"
            scopes = ["plugin"]

            [config.settings.properties.compiler]
            type = "object"
            title = "Compiler"
            required = ["path"]

            [config.settings.properties.compiler.properties.path]
            type = "string"
            default = "/usr/bin/gcc"

            [config.settings.properties.compiler.properties.flags]
            type = "array"
            default = ["-O2"]
            items = { type = "string" }
        "#;

        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        let ns = &manifest.config["settings"];
        let schema = ns.to_json_schema();

        assert_eq!(schema["type"], "object");
        let compiler = &schema["properties"]["compiler"];
        assert_eq!(compiler["type"], "object");
        assert_eq!(compiler["required"], json!(["path"]));

        let path_prop = &compiler["properties"]["path"];
        assert_eq!(path_prop["type"], "string");
        assert_eq!(path_prop["default"], "/usr/bin/gcc");

        let flags_prop = &compiler["properties"]["flags"];
        assert_eq!(flags_prop["type"], "array");
        assert_eq!(flags_prop["default"], json!(["-O2"]));
        assert_eq!(flags_prop["items"]["type"], "string");
    }

    #[test]
    fn schema_property_to_json_schema_enum_values() {
        let prop = SchemaProperty {
            schema_type: "string".into(),
            title: Some("Mode".into()),
            description: None,
            default: Some(json!("fast")),
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            enum_values: Some(vec![json!("fast"), json!("slow"), json!("balanced")]),
            items: None,
            properties: HashMap::new(),
            required: None,
            additional_properties: None,
        };

        let schema = prop.to_json_schema();
        assert_eq!(schema["enum"], json!(["fast", "slow", "balanced"]));
    }

    #[test]
    fn schema_property_minimal_produces_minimal_schema() {
        let toml_str = r#"
            type = "string"
        "#;

        let prop: SchemaProperty = toml::from_str(toml_str).unwrap();
        let schema = prop.to_json_schema();

        // Should only have "type", nothing else
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["type"], "string");
    }

    #[test]
    fn resolve_schema_includes_loads_external_file() {
        let dir = tempfile::tempdir().unwrap();
        let schema_dir = dir.path().join("config");
        std::fs::create_dir_all(&schema_dir).unwrap();

        // Write external schema file
        std::fs::write(
            schema_dir.join("settings.schema.toml"),
            r#"
                [timeout]
                type = "number"
                default = 30.0

                [name]
                type = "string"
                default = "test"
            "#,
        )
        .unwrap();

        let mut manifest = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: None,
            server: None,
            worker: None,
            web: None,
            translations: HashMap::new(),
            config: HashMap::from([(
                "settings".into(),
                ConfigNamespace {
                    description: Some("External".into()),
                    scopes: vec!["plugin".into()],
                    schema: Some("config/settings.schema.toml".into()),
                    properties: HashMap::new(),
                },
            )]),
        };

        manifest.resolve_schema_includes(dir.path()).unwrap();

        let ns = &manifest.config["settings"];
        assert_eq!(ns.properties.len(), 2);
        assert_eq!(ns.properties["timeout"].schema_type, "number");
        assert_eq!(ns.properties["name"].schema_type, "string");
    }

    #[test]
    fn standard_checkers_manifest_parses() {
        let toml_content = include_str!("../../../plugins/standard-checkers/plugin.toml");
        let manifest: PluginManifest = toml::from_str(toml_content).unwrap();

        assert_eq!(manifest.name, "standard-checkers");

        // Verify config:read permission
        let server = manifest.server.unwrap();
        assert!(server.permissions.contains(&"config:read".to_string()));

        // Verify config schema
        let testlib = &manifest.config["testlib"];
        assert_eq!(testlib.scopes, vec!["plugin"]);
        assert!(testlib.properties.contains_key("compile_time_limit_s"));
        assert!(testlib.properties.contains_key("cpp"));

        // Verify JSON Schema conversion
        let schema = testlib.to_json_schema();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["properties"]["compile_time_limit_s"]["minimum"]
                .as_f64()
                .is_some()
        );
    }
}
