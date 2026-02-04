pub mod config;
pub mod database;
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

use crate::state::AppState;

/// Build the application router.
pub fn build_router(state: AppState) -> axum::Router {
    axum::Router::new()
        .nest("/api", routes::api_routes())
        .with_state(state)
}
