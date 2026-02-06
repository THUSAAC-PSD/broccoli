use broccoli_queue::queue::BroccoliQueueBuilder;
pub use broccoli_queue::{
    brokers::broker::BrokerMessage,
    error::BroccoliError,
    queue::{BroccoliQueue, ConsumeOptions},
};

use crate::error::MqError;

// TODO: replace with our own Mq trait
pub type MqQueue = BroccoliQueue;
pub type MqBuilder = BroccoliQueueBuilder;

pub struct MqConfig {
    pub url: String,
    pub pool_size: u8,
}

pub async fn init_mq(config: MqConfig) -> Result<MqQueue, MqError> {
    BroccoliQueue::builder(&config.url)
        .pool_connections(config.pool_size)
        .build()
        .await
        .map_err(MqError::from)
}
