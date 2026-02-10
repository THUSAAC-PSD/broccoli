mod v1;

use utoipa::openapi::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::config::AppConfig;
use crate::state::AppState;

pub fn api_routes(config: &AppConfig, base_openapi: OpenApi) -> (axum::Router<AppState>, OpenApi) {
    let (router, api) = OpenApiRouter::with_openapi(base_openapi)
        .merge(v1::routes(config))
        .split_for_parts();
    (axum::Router::new().nest("/v1", router), api)
}
