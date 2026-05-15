use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};

#[derive(Clone)]
pub struct Metrics {
    pub http_request_duration: Histogram<f64>,
    pub http_requests_total: Counter<u64>,
    pub http_requests_in_flight: UpDownCounter<i64>,
    pub http_request_body_size: Histogram<f64>,
    pub http_response_body_size: Histogram<f64>,

    pub task_process_duration: Histogram<f64>,
    pub task_queue_wait_duration: Histogram<f64>,
    pub task_in_flight: UpDownCounter<i64>,
    pub worker_active_tasks: UpDownCounter<i64>,
    pub tasks_received_total: Counter<u64>,
    pub tasks_completed_total: Counter<u64>,
    pub step_duration: Histogram<f64>,
    pub step_results_total: Counter<u64>,
    pub sandbox_executions_total: Counter<u64>,
    pub sandbox_time_used: Histogram<f64>,
    pub sandbox_wall_time_used: Histogram<f64>,
    pub sandbox_memory_used: Histogram<f64>,
    pub task_retries_total: Counter<u64>,
    pub task_dedup_claims_total: Counter<u64>,
    pub dlq_messages_total: Counter<u64>,

    pub plugin_call_duration: Histogram<f64>,
    pub plugin_instance_acquire_duration: Histogram<f64>,
    pub plugin_instance_acquire_failures: Counter<u64>,
    pub plugin_pool_contention_total: Counter<u64>,
    pub plugin_evaluator_semaphore_wait_duration: Histogram<f64>,
    pub plugin_call_failures: Counter<u64>,

    pub host_fn_duration: Histogram<f64>,
    pub host_fn_calls_total: Counter<u64>,
    pub host_fn_block_in_place_total: Counter<u64>,

    pub batch_started_total: Counter<u64>,
    pub batch_cancelled_total: Counter<u64>,
    pub batch_reaped_total: Counter<u64>,
    pub batch_results_total: Counter<u64>,
    pub batch_wait_duration: Histogram<f64>,
    pub batch_active: UpDownCounter<i64>,
    pub batch_pending_items: UpDownCounter<i64>,
    pub batch_evaluator_fanout_wait_duration: Histogram<f64>,
    pub batch_evaluator_fanout_saturated_total: Counter<u64>,

    pub submission_dispatch_failure_total: Counter<u64>,

    pub operation_result_messages_total: Counter<u64>,
    pub operation_result_consume_duration: Histogram<f64>,

    pub mq_publish_duration: Histogram<f64>,
    pub mq_publish_messages_total: Counter<u64>,
    pub mq_consume_duration: Histogram<f64>,
    pub mq_consume_messages_total: Counter<u64>,
    pub mq_message_age: Histogram<f64>,

    pub blob_store_operation_duration: Histogram<f64>,
    pub blob_store_operations_total: Counter<u64>,
    pub blob_store_bytes_total: Counter<u64>,
    pub blob_store_errors_total: Counter<u64>,
    pub blob_store_remote_hits_total: Counter<u64>,

    pub blob_cache_hits_total: Counter<u64>,
    pub blob_cache_misses_total: Counter<u64>,
    pub blob_cache_size_bytes: UpDownCounter<i64>,
    pub blob_cache_evictions_total: Counter<u64>,

    pub operation_file_materialization_duration: Histogram<f64>,
    pub operation_file_materialization_bytes: Counter<u64>,

    pub task_cache_operation_duration: Histogram<f64>,
    pub task_cache_operations_total: Counter<u64>,

    pub mq_queue_depth: UpDownCounter<i64>,
}

const HTTP_BUCKETS_SECONDS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

const JUDGE_BUCKETS_SECONDS: &[f64] =
    &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0];

const PLUGIN_BUCKETS_SECONDS: &[f64] = &[0.0001, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0];

const FANOUT_WAIT_BUCKETS_MS: &[f64] = &[
    1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1_000.0, 5_000.0, 10_000.0,
];

const MEMORY_BUCKETS_KIB: &[f64] = &[
    1_024.0,
    4_096.0,
    16_384.0,
    65_536.0,
    262_144.0,
    1_048_576.0,
    4_194_304.0,
    16_777_216.0,
];

const SIZE_BUCKETS_BYTES: &[f64] = &[
    128.0,
    512.0,
    1_024.0,
    4_096.0,
    16_384.0,
    65_536.0,
    262_144.0,
    1_048_576.0,
    4_194_304.0,
    16_777_216.0,
    67_108_864.0,
    268_435_456.0,
];

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
                .u64_counter("http.server.request")
                .with_description("Total number of HTTP server requests")
                .build(),
            http_requests_in_flight: meter
                .i64_up_down_counter("http.server.active_requests")
                .with_description("Number of HTTP requests currently in flight")
                .build(),
            http_request_body_size: meter
                .f64_histogram("http.server.request.body.size")
                .with_unit("By")
                .with_description("HTTP request body size from Content-Length when available")
                .with_boundaries(SIZE_BUCKETS_BYTES.to_vec())
                .build(),
            http_response_body_size: meter
                .f64_histogram("http.server.response.body.size")
                .with_unit("By")
                .with_description("HTTP response body size from Content-Length when available")
                .with_boundaries(SIZE_BUCKETS_BYTES.to_vec())
                .build(),

            task_process_duration: meter
                .f64_histogram("broccoli.task.process.duration")
                .with_unit("s")
                .with_description("Duration of task processing in the worker pipeline")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            task_queue_wait_duration: meter
                .f64_histogram("broccoli.task.queue_wait.duration")
                .with_unit("s")
                .with_description("Time between task enqueue and worker receive")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            task_in_flight: meter
                .i64_up_down_counter("broccoli.task.in_flight")
                .with_description("Number of worker tasks currently in flight")
                .build(),
            worker_active_tasks: meter
                .i64_up_down_counter("broccoli.worker.active_tasks")
                .with_description("Number of tasks currently active on each worker")
                .build(),
            tasks_received_total: meter
                .u64_counter("broccoli.task.received")
                .with_description("Total number of worker tasks received")
                .build(),
            tasks_completed_total: meter
                .u64_counter("broccoli.task.completed")
                .with_description("Total number of worker tasks completed")
                .build(),
            step_duration: meter
                .f64_histogram("broccoli.step.duration")
                .with_unit("s")
                .with_description("Duration of individual pipeline steps in seconds")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            step_results_total: meter
                .u64_counter("broccoli.step.results")
                .with_description("Total number of operation step results")
                .build(),
            sandbox_executions_total: meter
                .u64_counter("broccoli.sandbox.executions")
                .with_description("Total number of sandbox executions")
                .build(),
            sandbox_time_used: meter
                .f64_histogram("broccoli.sandbox.time_used")
                .with_unit("s")
                .with_description("Sandbox CPU time reported by the sandbox backend")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            sandbox_wall_time_used: meter
                .f64_histogram("broccoli.sandbox.wall_time_used")
                .with_unit("s")
                .with_description("Sandbox wall time reported by the sandbox backend")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            sandbox_memory_used: meter
                .f64_histogram("broccoli.sandbox.memory_used")
                .with_unit("KiBy")
                .with_description("Sandbox memory usage reported by the sandbox backend")
                .with_boundaries(MEMORY_BUCKETS_KIB.to_vec())
                .build(),
            task_retries_total: meter
                .u64_counter("broccoli.task.retries")
                .with_description("Total number of task retries")
                .build(),
            task_dedup_claims_total: meter
                .u64_counter("broccoli.task.dedup.claims")
                .with_description("Total number of worker task dedup claim outcomes")
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
            plugin_instance_acquire_duration: meter
                .f64_histogram("broccoli.plugin.instance.acquire.duration")
                .with_unit("s")
                .with_description("Duration spent acquiring a WASM plugin runtime instance")
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),
            plugin_instance_acquire_failures: meter
                .u64_counter("broccoli.plugin.instance.acquire.failures")
                .with_description(
                    "Total number of failed WASM plugin runtime instance acquisitions",
                )
                .build(),
            plugin_pool_contention_total: meter
                .u64_counter("broccoli.plugin.pool.contention")
                .with_description(
                    "Total number of slow WASM plugin runtime instance acquisitions by latency bucket",
                )
                .build(),
            plugin_evaluator_semaphore_wait_duration: meter
                .f64_histogram("broccoli.plugin.evaluator.semaphore.wait.duration")
                .with_unit("s")
                .with_description(
                    "Duration spent waiting for the server-wide evaluator semaphore before \
                     invoking an evaluator plugin",
                )
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),
            plugin_call_failures: meter
                .u64_counter("broccoli.plugin.call.failures")
                .with_description("Total number of failed WASM plugin calls")
                .build(),

            host_fn_duration: meter
                .f64_histogram("broccoli.host_fn.duration")
                .with_unit("s")
                .with_description(
                    "Wall-clock duration of a single host function invocation, including inner async work",
                )
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),
            host_fn_calls_total: meter
                .u64_counter("broccoli.host_fn.calls")
                .with_description("Total number of host function invocations by outcome")
                .build(),
            host_fn_block_in_place_total: meter
                .u64_counter("broccoli.host_fn.block_in_place")
                .with_description(
                    "Regression sentinel: increments when a host_fn enters block_in_place. \
                     Should always be zero in shipping code (see UP#14g).",
                )
                .build(),

            batch_started_total: meter
                .u64_counter("broccoli.batch.started")
                .with_description("Total number of plugin-dispatched batches started")
                .build(),
            batch_cancelled_total: meter
                .u64_counter("broccoli.batch.cancelled")
                .with_description("Total number of plugin-dispatched batches cancelled")
                .build(),
            batch_reaped_total: meter
                .u64_counter("broccoli.batch.reaped")
                .with_description("Total number of stale plugin-dispatched batches reaped")
                .build(),
            batch_results_total: meter
                .u64_counter("broccoli.batch.results")
                .with_description("Total number of plugin-dispatched batch results observed")
                .build(),
            batch_wait_duration: meter
                .f64_histogram("broccoli.batch.wait.duration")
                .with_unit("s")
                .with_description(
                    "Duration spent waiting for the next plugin-dispatched batch result",
                )
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            batch_active: meter
                .i64_up_down_counter("broccoli.batch.active")
                .with_description("Number of active plugin-dispatched batches")
                .build(),
            batch_pending_items: meter
                .i64_up_down_counter("broccoli.batch.items.pending")
                .with_description("Number of pending items inside plugin-dispatched batches")
                .build(),
            batch_evaluator_fanout_wait_duration: meter
                .f64_histogram("broccoli.batch_evaluator.fanout.wait.duration")
                .with_unit("ms")
                .with_description(
                    "Duration spent waiting for a server-wide evaluator fan-out permit before \
                     spawning a per-test-case dispatch task (UP#14b)",
                )
                .with_boundaries(FANOUT_WAIT_BUCKETS_MS.to_vec())
                .build(),
            batch_evaluator_fanout_saturated_total: meter
                .u64_counter("broccoli.batch_evaluator.fanout.saturated")
                .with_description(
                    "Total number of evaluator fan-out acquires that had to block on a permit \
                     (vs. acquiring immediately). UP#14b backpressure signal.",
                )
                .build(),

            submission_dispatch_failure_total: meter
                .u64_counter("broccoli.submission_dispatch.failures")
                .with_description(
                    "Total submissions that hit a dispatch failure. Labels: error_code \
                     (cause), recovered (true if SystemError was persisted to DB).",
                )
                .build(),

            operation_result_messages_total: meter
                .u64_counter("broccoli.operation_result.messages")
                .with_description(
                    "Total number of operation result messages consumed by the server",
                )
                .build(),
            operation_result_consume_duration: meter
                .f64_histogram("broccoli.operation_result.consume.duration")
                .with_unit("s")
                .with_description("Duration spent handling operation result messages")
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),

            mq_publish_duration: meter
                .f64_histogram("broccoli.mq.publish.duration")
                .with_unit("s")
                .with_description("Duration of message publish operations")
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),
            mq_publish_messages_total: meter
                .u64_counter("broccoli.mq.publish.messages")
                .with_description("Total number of message publish operations")
                .build(),
            mq_consume_duration: meter
                .f64_histogram("broccoli.mq.consume.duration")
                .with_unit("s")
                .with_description("Duration of message consume handlers")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            mq_consume_messages_total: meter
                .u64_counter("broccoli.mq.consume.messages")
                .with_description("Total number of consumed message handler outcomes")
                .build(),
            mq_message_age: meter
                .f64_histogram("broccoli.mq.message.age")
                .with_unit("s")
                .with_description("Approximate message age when consumed")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),

            blob_store_operation_duration: meter
                .f64_histogram("broccoli.blob_store.operation.duration")
                .with_unit("s")
                .with_description("Duration of blob store operations")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            blob_store_operations_total: meter
                .u64_counter("broccoli.blob_store.operations")
                .with_description("Total number of blob store operations")
                .build(),
            blob_store_bytes_total: meter
                .u64_counter("broccoli.blob_store.bytes")
                .with_unit("By")
                .with_description("Total number of bytes read or written via blob store operations")
                .build(),
            blob_store_errors_total: meter
                .u64_counter("broccoli.blob_store.errors")
                .with_description("Total number of failed blob store operations")
                .build(),
            blob_store_remote_hits_total: meter
                .u64_counter("broccoli.blob_store.remote_hits")
                .with_description(
                    "Total number of uploads short-circuited because the blob already existed remotely",
                )
                .build(),

            blob_cache_hits_total: meter
                .u64_counter("broccoli.blob_cache.hits")
                .with_description("Total number of local blob cache hits (served from disk)")
                .build(),
            blob_cache_misses_total: meter
                .u64_counter("broccoli.blob_cache.misses")
                .with_description(
                    "Total number of local blob cache misses (streamed from blob store)",
                )
                .build(),
            blob_cache_size_bytes: meter
                .i64_up_down_counter("broccoli.blob_cache.size")
                .with_unit("By")
                .with_description("Current size of the local blob cache on disk")
                .build(),
            blob_cache_evictions_total: meter
                .u64_counter("broccoli.blob_cache.evictions")
                .with_description("Total number of LRU evictions from the local blob cache")
                .build(),

            operation_file_materialization_duration: meter
                .f64_histogram("broccoli.operation.file_materialization.duration")
                .with_unit("s")
                .with_description("Duration spent materializing operation files into sandboxes")
                .with_boundaries(JUDGE_BUCKETS_SECONDS.to_vec())
                .build(),
            operation_file_materialization_bytes: meter
                .u64_counter("broccoli.operation.file_materialization.bytes")
                .with_unit("By")
                .with_description("Total bytes materialized into operation sandbox files")
                .build(),

            task_cache_operation_duration: meter
                .f64_histogram("broccoli.task_cache.operation.duration")
                .with_unit("s")
                .with_description("Duration of worker task cache operations")
                .with_boundaries(PLUGIN_BUCKETS_SECONDS.to_vec())
                .build(),
            task_cache_operations_total: meter
                .u64_counter("broccoli.task_cache.operations")
                .with_description("Total number of worker task cache operations")
                .build(),

            mq_queue_depth: meter
                .i64_up_down_counter("broccoli.mq.queue.depth")
                .with_description("Current approximate depth of the message queue")
                .build(),
        }
    }
}
