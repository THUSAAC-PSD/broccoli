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
