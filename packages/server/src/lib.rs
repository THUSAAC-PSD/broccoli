mod client_ip;
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
pub mod serve;
pub mod state;
pub mod upload_limits;
pub mod utils;

use std::path::{Path, PathBuf};

use axum::extract::{MatchedPath, State};
use axum::http::{HeaderValue, Request, Response, StatusCode, header};
use axum::response::IntoResponse;
use opentelemetry::KeyValue;
use tower::Service;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};
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
        (name = "Admin", description = "Administrative endpoints"),
        (name = "Additional Files", description = "Judge-private supplemental files attached to a problem"),
        (name = "Checker Source", description = "Custom checker source files"),
        (name = "Clarifications", description = "Contest Q&A messages between contestants and admins"),
        (name = "Code Runs", description = "Custom test runs (test-your-code feature)"),
        (name = "Config", description = "Server-side configuration"),
        (name = "I18n", description = "Localization resource bundles"),
        (name = "Meta", description = "Server metadata (version, build info)"),
        (name = "Plugin Config", description = "Per-plugin configuration values keyed by scope"),
        (name = "Problem Attachments", description = "Contestant-visible files attached to a problem"),
        (name = "Roles", description = "Role and permission management"),
        (name = "Submissions", description = "Submitted solutions to problems"),
        (name = "Telemetry", description = "Public client error and web-vitals reporting"),
        (name = "Users", description = "User account management"),
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

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

fn normalize_javascript_content_type<B>(mut response: Response<B>) -> Response<B> {
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value == "text/javascript")
    {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript"),
        );
    }

    response
}

fn frontend_assets_service(
    frontend_dist: impl AsRef<Path>,
) -> impl Clone
+ Send
+ Sync
+ 'static
+ Service<
    Request<axum::body::Body>,
    Response: IntoResponse,
    Error = std::convert::Infallible,
    Future: Send + 'static,
> {
    tower::ServiceBuilder::new()
        .map_response(normalize_javascript_content_type)
        .service(ServeDir::new(frontend_dist.as_ref().join("assets")))
}

fn spa_fallback_service(frontend_dist: impl Into<PathBuf>) -> ServeDir<ServeFile> {
    let frontend_dist = frontend_dist.into();
    ServeDir::new(&frontend_dist).fallback(ServeFile::new(frontend_dist.join("index.html")))
}

pub fn build_router(state: AppState) -> axum::Router {
    let metrics = state.metrics.clone();
    let trusted_proxies =
        client_ip::parse_trusted_proxy_networks(&state.config.server.trusted_proxies);

    let (router, api) = routes::api_routes(&state.config, ApiDoc::openapi());
    let frontend_dist = state.config.server.frontend_dist.clone();

    let router = router.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::idempotency_middleware,
    ));
    let router = router.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        std::time::Duration::from_secs(60),
    ));

    let metrics_on_request = metrics.clone();
    let metrics_on_response = metrics;

    #[cfg(feature = "bundled-stress-test")]
    let state_for_downloads = state.clone();

    let assets_router = axum::Router::new()
        .route(
            "/{plugin_id}/{*file_path}",
            axum::routing::get(handlers::assets::serve_plugin_asset),
        )
        .fallback_service(frontend_assets_service(&frontend_dist));

    let app = axum::Router::new()
        .nest("/api", router.fallback(api_not_found))
        .route("/metrics", axum::routing::get(metrics_handler))
        .route("/healthz", axum::routing::get(handlers::health::healthz))
        .nest("/assets", assets_router)
        .fallback_service(spa_fallback_service(frontend_dist))
        .with_state(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api.clone()))
        .merge(Scalar::with_url("/scalar", api));

    #[cfg(feature = "bundled-stress-test")]
    let app = app.merge(routes::downloads::router().with_state(state_for_downloads));

    app.layer(axum::middleware::from_fn_with_state(
        trusted_proxies,
        client_ip::client_ip_source_middleware,
    ))
    .layer(CompressionLayer::new())
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

                let span = info_span!(
                    "http_request",
                    method = %request.method(),
                    path,
                    request_id,
                );

                if request.headers().contains_key("traceparent") {
                    use opentelemetry_http::HeaderExtractor;
                    use tracing_opentelemetry::OpenTelemetrySpanExt;

                    let parent_cx = opentelemetry::global::get_text_map_propagator(|prop| {
                        prop.extract(&HeaderExtractor(request.headers()))
                    });
                    let _ = span.set_parent(parent_cx);
                }

                span
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
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use axum::routing::get;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn spa_fallback_serves_index_for_root_and_client_routes() {
        let dist = tempfile::tempdir().unwrap();
        std::fs::write(dist.path().join("index.html"), "<html>app</html>").unwrap();
        let service = spa_fallback_service(dist.path().to_path_buf());

        for (method, path) in [(Method::GET, "/"), (Method::HEAD, "/contests/42")] {
            let response = service
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_TYPE],
                "text/html"
            );
        }
    }

    #[tokio::test]
    async fn frontend_assets_service_serves_assets_without_spa_fallback() {
        let dist = tempfile::tempdir().unwrap();
        let assets = dist.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("main.js"), "console.log('ok');").unwrap();
        let service = frontend_assets_service(dist.path());

        for method in [Method::GET, Method::HEAD] {
            let response = service
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/main.js")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let response = response.into_response();

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_TYPE],
                "application/javascript"
            );
        }

        let response = service
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/typo.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let response = response.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn frontend_asset_mount_keeps_plugin_asset_route_precedence() {
        let dist = tempfile::tempdir().unwrap();
        let assets = dist.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(dist.path().join("index.html"), "<html>app</html>").unwrap();
        std::fs::write(dist.path().join("robots.txt"), "User-agent: *").unwrap();
        std::fs::write(assets.join("main.js"), "console.log('ok');").unwrap();

        let assets_router = axum::Router::new()
            .route(
                "/{plugin_id}/{*file_path}",
                get(|| async { "plugin asset" }),
            )
            .fallback_service(frontend_assets_service(dist.path()));
        let api_router = axum::Router::new()
            .route(
                "/v1/health",
                get(|| async { Json(json!({ "status": "ok" })) }),
            )
            .fallback(api_not_found);

        let app = axum::Router::new()
            .nest("/api", api_router)
            .nest("/assets", assets_router)
            .fallback_service(spa_fallback_service(dist.path().to_path_buf()));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/assets/main.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "application/javascript"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/assets/typo.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/assets/plugin/foo.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/contests/42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "text/html"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/robots.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "text/plain"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "application/json"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
