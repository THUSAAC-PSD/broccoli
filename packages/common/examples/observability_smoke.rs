
use std::time::Duration;

use common::config::{ObservabilityConfig, OtlpConfig};
use common::observability::{init_metrics, init_tracing};
use opentelemetry::KeyValue;
use tracing::{info, info_span};

#[tokio::main]
async fn main() {
    let endpoint = std::env::var("BROCCOLI__OBSERVABILITY__OTLP__ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let config = ObservabilityConfig {
        log_format: "pretty".into(),
        log_filter: "info".into(),
        otlp: OtlpConfig {
            endpoint: endpoint.clone(),
            service_name: "broccoli-smoke-test".into(),
        },
    };

    let _guard = init_tracing(&config);
    let (metrics, registry) = init_metrics("broccoli-smoke-test");

    info!(endpoint = %endpoint, "smoke test starting");

    for i in 0..5 {
        let outer = info_span!("smoke_request", iteration = i);
        let _e = outer.enter();

        info!("handling request");

        {
            let inner = info_span!("db_query", table = "problems");
            let _ie = inner.enter();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        metrics
            .http_requests_total
            .add(1, &[KeyValue::new("http.response.status_code", 200_i64)]);
        metrics.http_request_duration.record(
            0.050,
            &[KeyValue::new("http.response.status_code", 200_i64)],
        );
    }

    for verdict in ["accepted", "wrong_answer", "accepted"] {
        metrics.task_process_duration.record(
            0.250,
            &[KeyValue::new("task_type", "operation".to_string())],
        );
        info!(%verdict, "task completed");
    }

    for outcome in ["success", "success", "cache_hit", "failure"] {
        metrics
            .step_duration
            .record(0.800, &[KeyValue::new("outcome", outcome)]);
    }

    use prometheus::Encoder;
    let mut buf = Vec::new();
    prometheus::TextEncoder::new()
        .encode(&registry.gather(), &mut buf)
        .unwrap();
    let text = String::from_utf8(buf).unwrap();
    let lines = text.lines().filter(|l| !l.starts_with('#')).count();
    println!("\n=== Prometheus /metrics ({} data lines) ===", lines);
    println!("{text}");

    println!("\n=== Next steps ===");
    println!("1. Jaeger UI: http://localhost:16686 (service: broccoli-smoke-test)");
    println!("2. Prometheus targets: http://localhost:9090/targets");
    println!("3. Grafana: http://localhost:3001 (admin/admin)");

    info!("waiting for batch exporter to flush");
    tokio::time::sleep(Duration::from_secs(6)).await;
    info!("smoke test done");
}
