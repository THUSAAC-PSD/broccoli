pub mod config;
pub mod error;
pub mod models;

pub use models::{MqBuilder, MqConfig, MqQueue, init_mq};

pub type Mq = MqQueue;
