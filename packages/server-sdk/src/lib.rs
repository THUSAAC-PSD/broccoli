pub mod error;
pub mod traits;
pub mod types;

#[cfg(feature = "guest")]
pub mod db;
#[cfg(feature = "guest")]
pub mod evaluator;
#[cfg(feature = "guest")]
pub mod host;

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
mod wasm_host;
#[cfg(all(feature = "guest", target_arch = "wasm32"))]
pub use wasm_host::WasmHost;

#[cfg(all(feature = "guest", not(target_arch = "wasm32")))]
pub mod testing;

pub mod prelude {
    pub use crate::error::SdkError;
    pub use crate::traits::PluginHost;
    pub use crate::types::*;

    #[cfg(feature = "guest")]
    pub use crate::db;
    #[cfg(feature = "guest")]
    pub use crate::evaluator;
    #[cfg(feature = "guest")]
    pub use crate::host;

    #[cfg(all(feature = "guest", target_arch = "wasm32"))]
    pub use crate::WasmHost;

    #[cfg(all(feature = "guest", not(target_arch = "wasm32")))]
    pub use crate::testing::MockHost;
}
