// TODO: This is a temporary implementation. Now we only re-export broccoli_queue's config types.
// In the future, we may want to define our own config types to decouple from broccoli_queue.

pub type PublishConfig = broccoli_queue::queue::PublishOptions;
pub type ConsumeConfig = broccoli_queue::queue::ConsumeOptions;
pub type RetryStrategy = broccoli_queue::queue::RetryStrategy;
