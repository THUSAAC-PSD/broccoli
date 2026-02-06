pub mod config;
pub mod error;
pub mod models;

pub use config::ConsumeConfig;
pub use models::{BroccoliError, BrokerMessage, MqBuilder, MqConfig, MqQueue, init_mq};

pub type Mq = MqQueue;
