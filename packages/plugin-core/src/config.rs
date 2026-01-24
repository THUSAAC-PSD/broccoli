use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub plugins_dir: PathBuf,
    pub enable_wasi: bool,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            plugins_dir: PathBuf::from("./plugins"),
            enable_wasi: true,
        }
    }
}

impl PluginConfig {
    pub fn check_plugins_dir(&self) -> bool {
        self.plugins_dir.exists() && self.plugins_dir.is_dir()
    }
}
