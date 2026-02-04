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
        .nest("/problems", problem_routes())
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

fn problem_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::problem::list_problems).post(handlers::problem::create_problem),
        )
        .route(
            "/{id}",
            get(handlers::problem::get_problem)
                .patch(handlers::problem::update_problem)
                .delete(handlers::problem::delete_problem),
        )
        .nest("/{id}/test-cases", test_case_routes())
}

fn test_case_routes() -> Router<AppState> {
    let crud = Router::new()
        .route(
            "/",
            get(handlers::problem::list_test_cases).post(handlers::problem::create_test_case),
        )
        .route(
            "/{tc_id}",
            get(handlers::problem::get_test_case)
                .patch(handlers::problem::update_test_case)
                .delete(handlers::problem::delete_test_case),
        )
        .layer(handlers::problem::test_case_body_limit());

    let upload = Router::new()
        .route("/upload", post(handlers::problem::upload_test_cases))
        .layer(handlers::problem::upload_body_limit());

    crud.merge(upload)
}
