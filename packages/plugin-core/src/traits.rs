use crate::error::PluginError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

#[async_trait]
pub trait PluginManager: Send + Sync {
    fn load_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
    fn has_plugin(&self, plugin_id: &str) -> bool;

    /// Low-level execution using raw bytes.
    async fn call_raw(
        &self,
        plugin_id: &str,
        func_name: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>, PluginError>;
}

/// Extension trait for typed calls.
/// Automatically implemented for any T that implements PluginManager.
#[async_trait]
pub trait PluginManagerExt: PluginManager {
    async fn call<T, R>(&self, plugin_id: &str, func_name: &str, input: T) -> Result<R, PluginError>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned + Send + Sync,
    {
        let input_bytes = serde_json::to_vec(&input)?;

        let output_bytes = self.call_raw(plugin_id, func_name, input_bytes).await?;

        let result = serde_json::from_slice(&output_bytes)?;
        Ok(result)
    }
}

// Blanket implementation
impl<T: ?Sized + PluginManager> PluginManagerExt for T {}
