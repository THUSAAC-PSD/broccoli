use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};

#[derive(Clone)]
pub struct Metrics {
    pub http_request_duration: Histogram<f64>,
    pub http_requests_total: Counter<u64>,
    pub http_requests_in_flight: UpDownCounter<i64>,

    pub task_process_duration: Histogram<f64>,
    pub task_retries_total: Counter<u64>,
    pub dlq_messages_total: Counter<u64>,

    pub plugin_call_duration: Histogram<f64>,

    pub mq_queue_depth: UpDownCounter<i64>,
}

impl Metrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            http_request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_description("Duration of HTTP server requests")
                .build(),
            http_requests_total: meter
                .u64_counter("http.server.request.total")
                .with_description("Total number of HTTP server requests")
                .build(),
            http_requests_in_flight: meter
                .i64_up_down_counter("http.server.active_requests")
                .with_description("Number of HTTP requests currently in flight")
                .build(),

            task_process_duration: meter
                .f64_histogram("broccoli.task.process.duration")
                .with_unit("s")
                .with_description("Duration of task processing in the worker pipeline")
                .build(),
            task_retries_total: meter
                .u64_counter("broccoli.task.retries")
                .with_description("Total number of task retries")
                .build(),
            dlq_messages_total: meter
                .u64_counter("broccoli.dlq.messages")
                .with_description("Total number of messages sent to the dead letter queue")
                .build(),

            plugin_call_duration: meter
                .f64_histogram("broccoli.plugin.call.duration")
                .with_unit("s")
                .with_description("Duration of WASM plugin calls")
                .build(),

            mq_queue_depth: meter
                .i64_up_down_counter("broccoli.mq.queue.depth")
                .with_description("Current approximate depth of the message queue")
                .build(),
        }
    }
}
