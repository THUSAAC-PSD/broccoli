pub mod config;
pub mod consumers;
pub mod database;
pub mod dlq;
pub mod entity;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod hooks;
pub mod host_funcs;
pub mod manager;
pub mod middleware;
pub mod models;
pub mod registry;
pub mod routes;
pub mod seed;
pub mod state;
pub mod utils;

use axum::extract::{MatchedPath, State};
use axum::response::IntoResponse;
use opentelemetry::KeyValue;
use tower_http::trace::TraceLayer;
use tracing::{info, info_span};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Broccoli Online Judge API",
        version = "1.0.0",
        description = "API for the Broccoli online judge system"
    ),
    servers(
        (url = "/api/v1", description = "Version 1 API server")
    ),
    tags(
        (name = "Auth", description = "Authentication and user management"),
        (name = "Problems", description = "Problem CRUD operations"),
        (name = "Test Cases", description = "Test case management for problems"),
        (name = "Contests", description = "Contest CRUD operations"),
        (name = "Contest Problems", description = "Managing problems within contests"),
        (name = "Contest Participants", description = "Managing contest participants"),
        (name = "Plugins", description = "WASM plugin management"),
        (name = "Dead Letter Queue", description = "Failed message management and retry"),
    ),
    modifiers(&SecurityAddon),
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            "jwt",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    use prometheus::Encoder;

    let encoder = prometheus::TextEncoder::new();
    let families = state.prometheus_registry.gather();
    let mut buf = Vec::new();
    encoder.encode(&families, &mut buf).unwrap();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        buf,
    )
}

pub fn build_router(state: AppState) -> axum::Router {
    let metrics = state.metrics.clone();

    let (router, api) = routes::api_routes(&state.config, ApiDoc::openapi());

    let router = router.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::idempotency_middleware,
    ));

    let metrics_on_request = metrics.clone();
    let metrics_on_failure = metrics.clone();
    let metrics_on_response = metrics;

    axum::Router::new()
        .nest("/api", router)
        .route("/metrics", axum::routing::get(metrics_handler))
        .route(
            "/assets/{plugin_id}/{*file_path}",
            axum::routing::get(handlers::assets::serve_plugin_asset),
        )
        .with_state(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api.clone()))
        .merge(Scalar::with_url("/scalar", api))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let path = request
                        .extensions()
                        .get::<MatchedPath>()
                        .map(|mp| mp.as_str().to_owned())
                        .unwrap_or_else(|| request.uri().path().to_owned());

                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_owned())
                        .unwrap_or_else(|| Uuid::now_v7().to_string());

                    info_span!(
                        "http_request",
                        method = %request.method(),
                        path,
                        request_id,
                    )
                })
                .on_request(
                    move |_request: &axum::http::Request<_>, _span: &tracing::Span| {
                        metrics_on_request.http_requests_in_flight.add(1, &[]);
                    },
                )
                .on_response(
                    move |response: &axum::http::Response<_>,
                          latency: std::time::Duration,
                          _span: &tracing::Span| {
                        let attrs = [KeyValue::new(
                            "http.response.status_code",
                            response.status().as_u16() as i64,
                        )];

                        metrics_on_response.http_requests_in_flight.add(-1, &[]);
                        metrics_on_response.http_requests_total.add(1, &attrs);
                        metrics_on_response
                            .http_request_duration
                            .record(latency.as_secs_f64(), &attrs);

                        info!(
                            status = response.status().as_u16(),
                            latency_ms = latency.as_millis() as u64,
                            "response",
                        );
                    },
                )
                .on_failure(
                    move |_error: tower_http::classify::ServerErrorsFailureClass,
                          _latency: std::time::Duration,
                          _span: &tracing::Span| {
                        metrics_on_failure.http_requests_in_flight.add(-1, &[]);
                    },
                ),
        )
}
