use chrono::{Duration, TimeZone, Utc};
use common::{SubmissionStatus, Verdict};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use server::entity::{plugin_storage, submission, submission_judgement, user};

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

    submission_judgement::ActiveModel {
        submission_id: Set(submission.id),
        version: Set(1),
        is_current: Set(true),
        is_finalized: Set(true),
        triggered_by_user_id: Set(None),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(Verdict::Accepted)),
        score: Set(Some(1.0)),
        judge_epoch: Set(1),
        created_at: Set(now),
        finalized_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert ICPC judgement");

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

async fn user_id_by_username(app: &E2eTestApp, username: &str) -> i32 {
    user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(&app.db)
        .await
        .expect("query contestant")
        .expect("contestant should exist")
        .id
}

async fn seed_current_icpc_submission(
    app: &E2eTestApp,
    user_id: i32,
    problem_id: i32,
    contest_id: i32,
    verdict: Verdict,
    created_at: chrono::DateTime<Utc>,
    version: i32,
) -> i32 {
    let accepted = verdict == Verdict::Accepted;
    let submission = submission::ActiveModel {
        files: Set(json!([{ "filename": "main.cpp", "content": "int main() { return 0; }" }])),
        language: Set("cpp".into()),
        user_id: Set(user_id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        contest_type: Set("icpc".into()),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(verdict.clone())),
        score: Set(Some(if accepted { 1.0 } else { 0.0 })),
        judge_epoch: Set(version),
        created_at: Set(created_at),
        judged_at: Set(Some(Utc::now())),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert ICPC submission");

    submission_judgement::ActiveModel {
        submission_id: Set(submission.id),
        version: Set(version),
        is_current: Set(true),
        is_finalized: Set(true),
        triggered_by_user_id: Set(None),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(verdict)),
        score: Set(Some(if accepted { 1.0 } else { 0.0 })),
        judge_epoch: Set(version),
        created_at: Set(created_at),
        finalized_at: Set(Some(Utc::now())),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert ICPC judgement");

    submission.id
}

async fn seed_icpc_standings_state(
    app: &E2eTestApp,
    contest_id: i32,
    user_id: i32,
    problem_id: i32,
    attempts: i32,
    solved: bool,
    solve_time_ms: Option<i64>,
) {
    plugin_storage::ActiveModel {
        plugin_id: Set("icpc".into()),
        collection: Set("default".into()),
        key: Set(format!("standings:{contest_id}:{user_id}:{problem_id}")),
        data: Set(json!(
            serde_json::to_string(&json!({
                "attempts": attempts,
                "solved": solved,
                "solve_time_ms": solve_time_ms
            }))
            .unwrap()
        )),
        created_at: Set(Utc::now()),
    }
    .insert(&app.db)
    .await
    .expect("insert stale ICPC standings state");
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
async fn icpc_standings_uses_current_judgements_instead_of_stale_storage() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin_rejudge1", "password", "admin")
        .await;
    let contestant = app
        .create_authenticated_user("icpc_user_rejudge1", "password")
        .await;

    let problem_id = app.create_problem(&admin, "ICPC Rejudge Problem 1").await;
    let contest_id = app
        .create_typed_contest(&admin, "ICPC Rejudge Contest 1", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let user_id = user_id_by_username(&app, "icpc_user_rejudge1").await;
    let submitted_at = Utc.with_ymd_and_hms(2020, 1, 1, 0, 10, 0).unwrap();
    seed_current_icpc_submission(
        &app,
        user_id,
        problem_id,
        contest_id,
        Verdict::WrongAnswer,
        submitted_at,
        2,
    )
    .await;
    seed_icpc_standings_state(&app, contest_id, user_id, problem_id, 0, true, Some(60_000)).await;

    let standings_path = format!("/api/v1/p/icpc/api/plugins/icpc/contests/{contest_id}/standings");
    let res = app.get_with_token(&standings_path, &admin).await;
    assert_eq!(res.status, 200, "Standings request failed: {}", res.text);

    let row = res.body["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["username"].as_str() == Some("icpc_user_rejudge1"))
        })
        .expect("contestant row should exist");
    assert_eq!(row["solved"].as_i64(), Some(0), "{}", res.text);
    assert_eq!(
        row["problems"]["A"]["solved"].as_bool(),
        Some(false),
        "{}",
        res.text
    );
    assert_eq!(
        row["problems"]["A"]["attempts"].as_i64(),
        Some(1),
        "{}",
        res.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn icpc_standings_uses_submission_time_for_accept_penalty() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("icpc_admin_rejudge2", "password", "admin")
        .await;
    let contestant = app
        .create_authenticated_user("icpc_user_rejudge2", "password")
        .await;

    let problem_id = app.create_problem(&admin, "ICPC Rejudge Problem 2").await;
    let contest_id = app
        .create_typed_contest(&admin, "ICPC Rejudge Contest 2", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let user_id = user_id_by_username(&app, "icpc_user_rejudge2").await;
    let contest_start = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    seed_current_icpc_submission(
        &app,
        user_id,
        problem_id,
        contest_id,
        Verdict::WrongAnswer,
        contest_start + Duration::minutes(3),
        1,
    )
    .await;
    seed_current_icpc_submission(
        &app,
        user_id,
        problem_id,
        contest_id,
        Verdict::Accepted,
        contest_start + Duration::minutes(10),
        1,
    )
    .await;
    seed_icpc_standings_state(&app, contest_id, user_id, problem_id, 0, true, Some(60_000)).await;

    let standings_path = format!("/api/v1/p/icpc/api/plugins/icpc/contests/{contest_id}/standings");
    let res = app.get_with_token(&standings_path, &admin).await;
    assert_eq!(res.status, 200, "Standings request failed: {}", res.text);

    let row = res.body["rows"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["username"].as_str() == Some("icpc_user_rejudge2"))
        })
        .expect("contestant row should exist");
    assert_eq!(row["solved"].as_i64(), Some(1), "{}", res.text);
    assert_eq!(row["penalty"].as_i64(), Some(30), "{}", res.text);
    assert_eq!(
        row["problems"]["A"]["time"].as_i64(),
        Some(10),
        "{}",
        res.text
    );
    assert_eq!(
        row["problems"]["A"]["attempts"].as_i64(),
        Some(1),
        "{}",
        res.text
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
