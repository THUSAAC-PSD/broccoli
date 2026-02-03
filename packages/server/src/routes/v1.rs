use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth_routes())
        .nest("/plugins", plugin_routes())
}

fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::auth::register))
        .route("/login", post(handlers::auth::login))
        .route("/me", get(handlers::auth::me))
}

fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/load", post(handlers::plugin::load_plugin))
        .route(
            "/{id}/call/{func}",
            post(handlers::plugin::call_plugin_func),
        )
}
