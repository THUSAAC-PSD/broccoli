use crate::plugins::error::PluginError;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

/// The core, object-safe trait.
/// Can be used as `Arc<dyn PluginManager>`.
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

/// Extension trait to provide the nice generic API.
/// Automatically implemented for any T that implements PluginManager.
#[async_trait]
pub trait PluginManagerExt: PluginManager {
    async fn call<T, R>(&self, plugin_id: &str, func_name: &str, input: T) -> Result<R, PluginError>
    where
        T: Serialize + Send + Sync,
        R: DeserializeOwned + Send + Sync,
    {
        // 1. Serialize
        let input_bytes = serde_json::to_vec(&input)?;

        // 2. Call raw (Dynamic Dispatch)
        let output_bytes = self.call_raw(plugin_id, func_name, input_bytes).await?;

        // 3. Deserialize
        let result = serde_json::from_slice(&output_bytes)?;
        Ok(result)
    }
}

// Blanket implementation
impl<T: ?Sized + PluginManager> PluginManagerExt for T {}
