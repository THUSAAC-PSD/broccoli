use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use extism::Plugin;
use serde::Serialize;

use crate::error::{AssetError, PluginError};
use crate::manifest::PluginManifest;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PluginStatus {
    /// Found on disk, parsed successfully, but not running.
    Discovered,
    /// Fully loaded and running in the runtime.
    Loaded,
    /// Encountered an error during discovery or activation.
    Failed(String),
}

/// Represents an entry in the plugin registry.
pub struct PluginEntry {
    pub id: String,
    pub root_dir: PathBuf,
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub runtime: Option<Mutex<Plugin>>,
}

pub type PluginRegistry = Arc<RwLock<HashMap<String, PluginEntry>>>;

/// Represents the public information about a plugin, suitable for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub status: PluginStatus,
}

impl From<&PluginEntry> for PluginInfo {
    fn from(entry: &PluginEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.manifest.name.clone(),
            version: entry.manifest.version.clone(),
            description: entry.manifest.description.clone(),
            status: entry.status.clone(),
        }
    }
}

impl PluginEntry {
    pub fn new(id: String, root_dir: PathBuf, manifest: PluginManifest) -> Self {
        Self {
            id,
            root_dir,
            manifest,
            status: PluginStatus::Discovered,
            runtime: None,
        }
    }

    /// Loads a plugin from a directory by parsing the plugin.toml.
    pub fn from_dir(plugin_dir: &Path) -> Result<Self, PluginError> {
        if !plugin_dir.exists() || !plugin_dir.is_dir() {
            return Err(PluginError::LoadFailed(format!(
                "Plugin directory '{}' does not exist or is not a directory",
                plugin_dir.display()
            )));
        }

        let id = plugin_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| PluginError::LoadFailed("Invalid plugin directory name".into()))?
            .to_string();

        let toml_path = plugin_dir.join("plugin.toml");
        let toml_content = std::fs::read_to_string(&toml_path)
            .map_err(|e| PluginError::LoadFailed(format!("Failed to read manifest: {}", e)))?;

        let manifest: PluginManifest = toml::from_str(&toml_content)
            .map_err(|e| PluginError::LoadFailed(format!("Invalid manifest syntax: {}", e)))?;

        Ok(Self {
            id,
            root_dir: plugin_dir.to_path_buf(),
            manifest,
            status: PluginStatus::Discovered,
            runtime: None,
        })
    }

    /// Resolves a web asset path based on the plugin's manifest configuration.
    pub fn resolve_web_asset(&self, relative_path: &str) -> Result<PathBuf, AssetError> {
        let web_config = self.manifest.web.as_ref().ok_or(AssetError::NoWebConfig)?;

        let web_root = self.root_dir.join(&web_config.root);
        let asset_path = web_root.join(relative_path);

        // Prevent path traversal attacks
        let canonical_web_root = web_root.canonicalize().map_err(AssetError::Io)?;
        let canonical_asset_path = asset_path
            .canonicalize()
            .map_err(|_| AssetError::NotFound)?;

        if !canonical_asset_path.starts_with(&canonical_web_root) {
            return Err(AssetError::PathTraversal);
        }

        if !canonical_asset_path.exists() || !canonical_asset_path.is_file() {
            return Err(AssetError::NotFound);
        }

        Ok(canonical_asset_path)
    }
}
