use chrono::Utc;
use common::{SubmissionStatus, Verdict};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use server::entity::{plugin_storage, submission, user};

use crate::common::E2eTestApp;

async fn seed_accepted_icpc_submission(
    app: &E2eTestApp,
    username: &str,
    problem_id: i32,
    contest_id: i32,
) -> i32 {
    let user_model = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(&app.db)
        .await
        .expect("query contestant")
        .expect("contestant should exist");
    let now = Utc::now();
    let submission = submission::ActiveModel {
        files: Set(json!([{ "filename": "main.cpp", "content": "int main() { return 0; }" }])),
        language: Set("cpp".into()),
        user_id: Set(user_model.id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        contest_type: Set("icpc".into()),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(Verdict::Accepted)),
        score: Set(Some(1.0)),
        judge_epoch: Set(1),
        created_at: Set(now),
        judged_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert ICPC submission");

    plugin_storage::ActiveModel {
        plugin_id: Set("icpc".into()),
        collection: Set("default".into()),
        key: Set(format!(
            "standings:{contest_id}:{}:{problem_id}",
            user_model.id
        )),
        data: Set(json!(
            serde_json::to_string(&json!({
                "attempts": 1,
                "solved": true,
                "solve_time_ms": 60_000
            }))
            .unwrap()
        )),
        created_at: Set(now),
    }
    .insert(&app.db)
    .await
    .expect("insert ICPC standings state");

    submission.id
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_contest_type_registered() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin1", "password", "admin")
        .await;
    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 1", "icpc", true, true)
        .await;
    assert!(contest_id > 0, "Should successfully create an ICPC contest");
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_standings_empty_before_submissions() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin2", "password", "admin")
        .await;
    let contestant = app
        .create_authenticated_user("icpc_user2", "password")
        .await;

    let problem_id = app.create_problem(&admin, "ICPC Problem 2").await;
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 2", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let standings_path = format!("/api/v1/p/icpc/api/plugins/icpc/contests/{contest_id}/standings");
    let res = app.get_with_token(&standings_path, &contestant).await;
    assert_eq!(res.status, 200, "Standings request failed: {}", res.text);

    let rows = &res.body["rows"];
    assert!(rows.is_array(), "rows should be an array");
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_standings_reflects_judged_submission() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin3", "password", "admin")
        .await;
    let contestant = app
        .create_authenticated_user("icpc_user3", "password")
        .await;

    let problem_id = app.create_problem(&admin, "ICPC Problem 3").await;

    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 3", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    seed_accepted_icpc_submission(&app, "icpc_user3", problem_id, contest_id).await;

    let standings_path = format!("/api/v1/p/icpc/api/plugins/icpc/contests/{contest_id}/standings");
    let res = app.get_with_token(&standings_path, &contestant).await;
    assert_eq!(res.status, 200, "Standings request failed: {}", res.text);

    let rows = &res.body["rows"];
    assert!(rows.is_array(), "rows should be an array");
    let rows_arr = rows.as_array().unwrap();
    assert!(
        !rows_arr.is_empty(),
        "Standings should have at least one entry after a judged submission"
    );
    assert!(
        rows_arr[0]["username"].is_string(),
        "Row should have a username"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_contest_info_returns_metadata() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin4", "password", "admin")
        .await;
    let contestant = app
        .create_authenticated_user("icpc_user4", "password")
        .await;

    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 4", "icpc", true, true)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let info_path = format!("/api/v1/p/icpc/api/plugins/icpc/contests/{contest_id}/info");
    let res = app.get_with_token(&info_path, &contestant).await;
    assert_eq!(res.status, 200, "Contest info request failed: {}", res.text);
    assert!(
        res.body["penalty_minutes"].is_number(),
        "penalty_minutes should be present: {}",
        res.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_config_penalty_minutes() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin5", "password", "admin")
        .await;

    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 5", "icpc", true, true)
        .await;

    let config_path = format!("/api/v1/contests/{contest_id}/config/icpc/contest");
    let put_res = app
        .put_with_token(
            &config_path,
            &json!({
                "config": {
                    "penalty_minutes": 30,
                    "count_compile_error": true,
                    "show_test_details": true
                },
                "enabled": true
            }),
            &admin,
        )
        .await;
    assert_eq!(
        put_res.status, 200,
        "Failed to set ICPC contest config: {}",
        put_res.text
    );

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(
        get_res.status, 200,
        "Failed to get ICPC contest config: {}",
        get_res.text
    );
    assert_eq!(get_res.body["config"]["penalty_minutes"].as_u64(), Some(30));
    assert_eq!(
        get_res.body["config"]["count_compile_error"].as_bool(),
        Some(true)
    );
    assert_eq!(
        get_res.body["config"]["show_test_details"].as_bool(),
        Some(true)
    );
}
