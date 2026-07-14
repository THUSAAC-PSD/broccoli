pub mod api;
pub mod contest;

// The wire/domain types and SdkError live in the dependency-light `broccoli-types`
// crate at the bottom of the graph. Re-export them here so the published guest SDK
// surface (`broccoli_server_sdk::types::*`, `::error`, and the prelude) is unchanged
// for every plugin and downstream crate.
pub use broccoli_types::{error, types};

#[cfg(feature = "guest")]
pub mod db;
#[cfg(feature = "guest")]
pub mod evaluator;
#[cfg(feature = "guest")]
pub(crate) mod host;
#[cfg(feature = "guest")]
mod sdk;

#[cfg(feature = "guest")]
pub use sdk::*;

pub mod prelude {
    pub use crate::api::ApiError;
    pub use crate::error::SdkError;
    pub use crate::types::*;

    #[cfg(target_arch = "wasm32")]
    pub use crate::api::run_api_handler;
    #[cfg(feature = "guest")]
    pub use crate::contest;
    #[cfg(feature = "guest")]
    pub use crate::db::Params;
    #[cfg(feature = "guest")]
    pub use crate::evaluator;
    #[cfg(feature = "guest")]
    pub use crate::sdk::*;
}
