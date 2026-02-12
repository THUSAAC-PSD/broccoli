use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AssetError, PluginError};
use crate::manifest::PluginManifest;

/// Represents a loaded plugin bundle on disk (parsed manifest + root directory).
/// This structure holds the static data before it is loaded into the runtime.
#[derive(Debug)]
pub struct PluginBundle {
    pub manifest: PluginManifest,
    pub root_dir: PathBuf,
}

impl PluginBundle {
    /// Loads a plugin bundle from a directory by parsing the plugin.toml.
    pub fn load_from_dir(plugin_dir: &Path) -> Result<Self, PluginError> {
        if !plugin_dir.exists() || !plugin_dir.is_dir() {
            return Err(PluginError::NotFound(plugin_dir.display().to_string()));
        }

        let toml_path = plugin_dir.join("plugin.toml");
        let toml_content = fs::read_to_string(&toml_path)
            .map_err(|e| PluginError::LoadFailed(format!("Failed to read plugin.toml: {}", e)))?;

        let manifest: PluginManifest = toml::from_str(&toml_content)
            .map_err(|e| PluginError::LoadFailed(format!("Invalid plugin.toml syntax: {}", e)))?;

        Ok(Self {
            manifest,
            root_dir: plugin_dir.to_path_buf(),
        })
    }

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
