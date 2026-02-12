pub mod config;
pub mod consumers;
pub mod database;
pub mod dlq;
pub mod entity;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod host_funcs;
pub mod manager;
pub mod models;
pub mod routes;
pub mod seed;
pub mod state;
pub mod utils;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_scalar::{Scalar, Servable as ScalarServable};
use utoipa_swagger_ui::SwaggerUi;

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

/// Build the application router.
pub fn build_router(state: AppState) -> axum::Router {
    let (router, api) = routes::api_routes(&state.config, ApiDoc::openapi());

    axum::Router::new()
        .nest("/api", router)
        .route(
            "/assets/{plugin_id}/{*file_path}",
            axum::routing::get(handlers::assets::serve_plugin_asset),
        )
        .with_state(state)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api.clone()))
        .merge(Scalar::with_url("/scalar", api))
}
