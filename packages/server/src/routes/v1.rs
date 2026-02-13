use utoipa_axum::{router::OpenApiRouter, routes};

use crate::config::AppConfig;
use crate::handlers;
use crate::state::AppState;

pub fn routes(config: &AppConfig) -> OpenApiRouter<AppState> {
    let submission_max_size = config.submission.max_size;

    OpenApiRouter::new()
        .nest("/auth", auth_routes())
        .nest("/admin", admin_routes())
        .nest("/plugins", plugin_routes())
        .nest("/problems", problem_routes(submission_max_size))
        .nest("/contests", contest_routes(submission_max_size))
        .nest("/submissions", submission_routes())
        .nest("/dlq", dlq_routes())
}

fn auth_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::auth::register))
        .routes(routes!(handlers::auth::login))
        .routes(routes!(handlers::auth::me))
}

fn admin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::admin::list_all_plugins))
        .routes(routes!(handlers::admin::get_plugin_details))
        .routes(routes!(handlers::admin::enable_plugin))
        .routes(routes!(handlers::admin::disable_plugin))
}

fn plugin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin::call_plugin_func))
        .routes(routes!(handlers::plugin::list_active_plugins))
}

fn problem_routes(submission_max_size: usize) -> OpenApiRouter<AppState> {
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
        .nest(
            "/{id}/submissions",
            problem_submission_routes(submission_max_size),
        )
}

fn problem_submission_routes(submission_max_size: usize) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::submission::create_submission))
        .layer(handlers::submission::submission_body_limit(
            submission_max_size,
        ))
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

fn contest_routes(submission_max_size: usize) -> OpenApiRouter<AppState> {
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
        .nest(
            "/{id}/problems",
            contest_problem_routes(submission_max_size),
        )
        .nest("/{id}/participants", contest_participant_routes())
        .nest("/{id}/submissions", contest_submission_routes())
        .routes(routes!(
            handlers::contest::register_for_contest,
            handlers::contest::unregister_from_contest,
        ))
}

fn contest_submission_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::submission::list_contest_submissions))
}

fn contest_problem_routes(submission_max_size: usize) -> OpenApiRouter<AppState> {
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
        .nest(
            "/{problem_id}/submissions",
            contest_problem_submission_routes(submission_max_size),
        )
}

fn contest_problem_submission_routes(submission_max_size: usize) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::submission::create_contest_submission))
        .layer(handlers::submission::submission_body_limit(
            submission_max_size,
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

fn submission_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::submission::list_submissions))
        .routes(routes!(handlers::submission::get_submission))
        .routes(routes!(handlers::submission::rejudge_submission))
}

fn dlq_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::dlq::list_dlq_messages))
        .routes(routes!(handlers::dlq::get_dlq_stats))
        .routes(routes!(
            handlers::dlq::get_dlq_message,
            handlers::dlq::delete_dlq_message,
        ))
        .routes(routes!(handlers::dlq::retry_dlq_message))
}
