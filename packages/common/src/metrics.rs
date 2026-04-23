use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};

#[derive(Clone)]
pub struct Metrics {
    pub http_request_duration: Histogram<f64>,
    pub http_requests_total: Counter<u64>,
    pub http_requests_in_flight: UpDownCounter<i64>,

    pub task_process_duration: Histogram<f64>,
    pub step_duration: Histogram<f64>,
    pub task_retries_total: Counter<u64>,
    pub dlq_messages_total: Counter<u64>,

    pub plugin_call_duration: Histogram<f64>,

    pub mq_queue_depth: UpDownCounter<i64>,
}

const HTTP_BUCKETS_SECONDS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

const JUDGE_BUCKETS_SECONDS: &[f64] =
    &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0];

const PLUGIN_BUCKETS_SECONDS: &[f64] = &[0.0001, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0];

impl Metrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            http_request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_description("Duration of HTTP server requests")
                .with_boundaries(HTTP_BUCKETS_SECONDS.to_vec())
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
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            step_duration: meter
                .f64_histogram("broccoli.step.duration")
                .with_unit("s")
                .with_description("Duration of individual pipeline steps in seconds")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
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
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),

            mq_queue_depth: meter
                .i64_up_down_counter("broccoli.mq.queue.depth")
                .with_description("Current approximate depth of the message queue")
                .build(),
        }
    }
}
