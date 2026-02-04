use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};

use crate::handlers;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth_routes())
        .nest("/plugins", plugin_routes())
        .nest("/problems", problem_routes())
        .nest("/contests", contest_routes())
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
        .route("/reorder", put(handlers::problem::reorder_test_cases))
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

fn contest_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::contest::list_contests).post(handlers::contest::create_contest),
        )
        .route(
            "/{id}",
            get(handlers::contest::get_contest)
                .patch(handlers::contest::update_contest)
                .delete(handlers::contest::delete_contest),
        )
        .nest("/{id}/problems", contest_problem_routes())
        .nest("/{id}/participants", contest_participant_routes())
        .route(
            "/{id}/register",
            post(handlers::contest::register_for_contest)
                .delete(handlers::contest::unregister_from_contest),
        )
}

fn contest_problem_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::contest::list_contest_problems)
                .post(handlers::contest::add_contest_problem),
        )
        .route("/reorder", put(handlers::contest::reorder_contest_problems))
        .route(
            "/{problem_id}",
            patch(handlers::contest::update_contest_problem)
                .delete(handlers::contest::remove_contest_problem),
        )
}

fn contest_participant_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::contest::list_participants).post(handlers::contest::add_participant),
        )
        .route("/{user_id}", delete(handlers::contest::remove_participant))
}
