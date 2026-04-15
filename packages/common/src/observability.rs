use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::ObservabilityConfig;

pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("OpenTelemetry shutdown error: {e}");
            }
        }
    }
}

pub fn init_tracing(config: &ObservabilityConfig) -> TelemetryGuard {
    let is_json = config.log_format.eq_ignore_ascii_case("json");

    let filter_directive = std::env::var("RUST_LOG").unwrap_or_else(|_| config.log_filter.clone());

    let provider = if !config.otlp.endpoint.is_empty() {
        match build_otel_provider(config) {
            Ok(prov) => Some(prov),
            Err(e) => {
                eprintln!("Failed to initialize OpenTelemetry, continuing without: {e}");
                None
            }
        }
    } else {
        None
    };

    match (is_json, &provider) {
        (true, Some(p)) => {
            let otel = tracing_opentelemetry::layer().with_tracer(p.tracer("broccoli"));
            tracing_subscriber::registry()
                .with(EnvFilter::new(&filter_directive))
                .with(tracing_subscriber::fmt::layer().json().with_target(false))
                .with(otel)
                .init();
        }
        (true, None) => {
            tracing_subscriber::registry()
                .with(EnvFilter::new(&filter_directive))
                .with(tracing_subscriber::fmt::layer().json().with_target(false))
                .init();
        }
        (false, Some(p)) => {
            let otel = tracing_opentelemetry::layer().with_tracer(p.tracer("broccoli"));
            tracing_subscriber::registry()
                .with(EnvFilter::new(&filter_directive))
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .with(otel)
                .init();
        }
        (false, None) => {
            tracing_subscriber::registry()
                .with(EnvFilter::new(&filter_directive))
                .with(tracing_subscriber::fmt::layer().with_target(false))
                .init();
        }
    }

    TelemetryGuard { provider }
}

fn build_otel_provider(
    config: &ObservabilityConfig,
) -> Result<SdkTracerProvider, Box<dyn std::error::Error + Send + Sync>> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp.endpoint)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(config.otlp.service_name.clone())
                .build(),
        )
        .build();

    Ok(provider)
}
