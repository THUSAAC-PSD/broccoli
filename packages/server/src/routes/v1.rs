use utoipa_axum::{router::OpenApiRouter, routes};

use crate::config::AppConfig;
use crate::handlers;
use crate::state::AppState;

pub fn routes(config: &AppConfig) -> OpenApiRouter<AppState> {
    let submission_max_size = config.submission.max_size;

    OpenApiRouter::new()
        .nest("/auth", auth_routes())
        .nest("/users", user_routes())
        .nest("/admin", admin_routes())
        .nest("/plugins", plugin_routes())
        .nest("/p", proxy_routes())
        .nest("/i18n", i18n_routes())
        .nest("/config/upload", config_upload_routes())
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
        .routes(routes!(handlers::auth::request_device_code))
        .routes(routes!(handlers::auth::authorize_device))
        .routes(routes!(handlers::auth::poll_device_token))
}

fn user_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::user::list_users))
        .routes(routes!(handlers::user::delete_user))
}

fn admin_routes() -> OpenApiRouter<AppState> {
    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::admin::upload_plugin))
        .layer(handlers::admin::upload_body_limit());

    OpenApiRouter::new()
        .routes(routes!(handlers::admin::list_all_plugins))
        .routes(routes!(handlers::admin::get_plugin_details))
        .routes(routes!(handlers::admin::enable_plugin))
        .routes(routes!(handlers::admin::disable_plugin))
        .routes(routes!(handlers::admin::reload_plugin))
        .routes(routes!(handlers::admin::reload_all_plugins))
        .merge(upload)
        .nest("/plugins/{id}/config", plugin_global_config_routes())
}

fn plugin_global_config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin_config::list_plugin_global_config))
        .routes(routes!(
            handlers::plugin_config::get_plugin_global_config,
            handlers::plugin_config::upsert_plugin_global_config,
            handlers::plugin_config::delete_plugin_global_config,
        ))
}

fn config_upload_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::config_upload::upload_config_blob))
        .layer(handlers::config_upload::config_upload_body_limit())
}

fn plugin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin::list_registries))
        .routes(routes!(handlers::plugin::list_active_plugins))
}

fn proxy_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(
        handlers::proxy::get_plugin_request,
        handlers::proxy::post_plugin_request,
        handlers::proxy::put_plugin_request,
        handlers::proxy::delete_plugin_request,
        handlers::proxy::patch_plugin_request,
        handlers::proxy::options_plugin_request,
        handlers::proxy::head_plugin_request,
        handlers::proxy::trace_plugin_request,
    ))
}

fn i18n_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::i18n::get_locales))
        .routes(routes!(handlers::i18n::get_translations))
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
        .nest("/{id}/attachments", attachment_routes())
        .nest("/{id}/additional-files", additional_file_routes())
        .nest("/{id}/config", problem_config_routes())
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
        .routes(routes!(handlers::problem::bulk_delete_test_cases))
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

fn attachment_routes() -> OpenApiRouter<AppState> {
    let read_delete = OpenApiRouter::new()
        .routes(routes!(handlers::attachment::list_attachments))
        .routes(routes!(
            handlers::attachment::download_attachment,
            handlers::attachment::delete_attachment,
        ));

    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::attachment::upload_attachment))
        .layer(handlers::attachment::attachment_upload_body_limit());

    read_delete.merge(upload)
}

fn additional_file_routes() -> OpenApiRouter<AppState> {
    let read_delete = OpenApiRouter::new()
        .routes(routes!(handlers::additional_file::list_additional_files))
        .routes(routes!(
            handlers::additional_file::download_additional_file,
            handlers::additional_file::delete_additional_file,
        ));

    let upload = OpenApiRouter::new()
        .routes(routes!(handlers::additional_file::upload_additional_file))
        .layer(handlers::additional_file::additional_file_upload_body_limit());

    read_delete.merge(upload)
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
        .routes(routes!(handlers::contest::get_contest_my_info))
        .nest(
            "/{id}/problems",
            contest_problem_routes(submission_max_size),
        )
        .nest("/{id}/participants", contest_participant_routes())
        .nest("/{id}/submissions", contest_submission_routes())
        .nest("/{id}/config", contest_config_routes())
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
        .routes(routes!(handlers::contest::bulk_delete_contest_problems))
        .routes(routes!(
            handlers::contest::update_contest_problem,
            handlers::contest::remove_contest_problem,
        ))
        .nest("/{problem_id}/config", contest_problem_config_routes())
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
        .routes(routes!(handlers::contest::bulk_add_participants))
        .routes(routes!(handlers::contest::remove_participant))
}

fn submission_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::submission::list_submissions))
        .routes(routes!(handlers::submission::bulk_rejudge_submissions))
        .routes(routes!(handlers::submission::get_submission))
        .routes(routes!(handlers::submission::rejudge_submission))
}

fn problem_config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin_config::list_problem_config))
        .routes(routes!(
            handlers::plugin_config::get_problem_config,
            handlers::plugin_config::upsert_problem_config,
            handlers::plugin_config::delete_problem_config,
        ))
}

fn contest_config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::plugin_config::list_contest_config))
        .routes(routes!(
            handlers::plugin_config::get_contest_config,
            handlers::plugin_config::upsert_contest_config,
            handlers::plugin_config::delete_contest_config,
        ))
}

fn contest_problem_config_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            handlers::plugin_config::list_contest_problem_config
        ))
        .routes(routes!(
            handlers::plugin_config::get_contest_problem_config,
            handlers::plugin_config::upsert_contest_problem_config,
            handlers::plugin_config::delete_contest_problem_config,
        ))
}

fn dlq_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::dlq::list_dlq_messages))
        .routes(routes!(handlers::dlq::get_dlq_stats))
        .routes(routes!(handlers::dlq::bulk_retry_dlq))
        .routes(routes!(handlers::dlq::bulk_delete_dlq))
        .routes(routes!(
            handlers::dlq::get_dlq_message,
            handlers::dlq::delete_dlq_message,
        ))
        .routes(routes!(handlers::dlq::retry_dlq_message))
}
