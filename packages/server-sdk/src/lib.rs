pub mod error;
pub mod types;

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
    pub use crate::error::SdkError;
    pub use crate::types::*;

    #[cfg(feature = "guest")]
    pub use crate::db::Params;
    #[cfg(feature = "guest")]
    pub use crate::evaluator;
    #[cfg(feature = "guest")]
    pub use crate::sdk::*;
}
