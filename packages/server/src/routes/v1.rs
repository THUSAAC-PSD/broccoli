use utoipa_axum::{router::OpenApiRouter, routes};

use crate::handlers;
use crate::state::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .nest("/auth", auth_routes())
        .nest("/plugins", plugin_routes())
        .nest("/problems", problem_routes())
        .nest("/contests", contest_routes())
}

fn auth_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::auth::register))
        .routes(routes!(handlers::auth::login))
        .routes(routes!(handlers::auth::me))
}

fn plugin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin::load_plugin))
        .routes(routes!(handlers::plugin::call_plugin_func))
}

fn problem_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::problem::list_problems,
            handlers::problem::create_problem,
        ))
        .routes(routes!(
            handlers::problem::get_problem,
            handlers::problem::update_problem,
            handlers::problem::delete_problem,
        ))
        .nest("/{id}/test-cases", test_case_routes())
}

fn test_case_routes() -> OpenApiRouter<AppState> {
    let crud = OpenApiRouter::new()
        .routes(routes!(
            handlers::problem::list_test_cases,
            handlers::problem::create_test_case,
        ))
        .routes(routes!(handlers::problem::reorder_test_cases))
        .routes(routes!(
            handlers::problem::get_test_case,
            handlers::problem::update_test_case,
            handlers::problem::delete_test_case,
        ))
        .layer(handlers::problem::test_case_body_limit());

    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::problem::upload_test_cases))
        .layer(handlers::problem::upload_body_limit());

    crud.merge(upload)
}

fn contest_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::contest::list_contests,
            handlers::contest::create_contest,
        ))
        .routes(routes!(
            handlers::contest::get_contest,
            handlers::contest::update_contest,
            handlers::contest::delete_contest,
        ))
        .nest("/{id}/problems", contest_problem_routes())
        .nest("/{id}/participants", contest_participant_routes())
        .routes(routes!(
            handlers::contest::register_for_contest,
            handlers::contest::unregister_from_contest,
        ))
}

fn contest_problem_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::contest::list_contest_problems,
            handlers::contest::add_contest_problem,
        ))
        .routes(routes!(handlers::contest::reorder_contest_problems))
        .routes(routes!(
            handlers::contest::update_contest_problem,
            handlers::contest::remove_contest_problem,
        ))
}

fn contest_participant_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::contest::list_participants,
            handlers::contest::add_participant,
        ))
        .routes(routes!(handlers::contest::remove_participant))
}
