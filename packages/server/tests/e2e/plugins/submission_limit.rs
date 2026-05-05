use chrono::Utc;
use common::{SubmissionStatus, Verdict};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use server::entity::{submission, user};

use crate::common::E2eTestApp;

async fn seed_counted_submission(
    app: &E2eTestApp,
    username: &str,
    problem_id: i32,
    contest_id: i32,
) -> i32 {
    let user_model = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(&app.db)
        .await
        .expect("query user")
        .expect("user should exist");
    let now = Utc::now();
    submission::ActiveModel {
        files: Set(json!([{ "filename": "main.cpp", "content": "int main() { return 0; }" }])),
        language: Set("cpp".into()),
        user_id: Set(user_model.id),
        problem_id: Set(problem_id),
        contest_id: Set(Some(contest_id)),
        contest_type: Set("icpc".into()),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(Verdict::Accepted)),
        score: Set(Some(0.0)),
        judge_epoch: Set(1),
        created_at: Set(now),
        judged_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert counted submission")
    .id
}

#[tokio::test(flavor = "multi_thread")]
async fn limit_rejects_after_max_submissions() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin1", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user1", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 1").await;

    let contest_id = app
        .create_typed_contest(&admin, "Limit Contest 1", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let config_path = format!(
        "/api/v1/contests/{contest_id}/problems/{problem_id}/config/submission-limit/limits"
    );
    app.put_with_token(
        &config_path,
        &json!({ "config": { "max_submissions": 2 }, "enabled": true }),
        &admin,
    )
    .await;

    seed_counted_submission(&app, "sl_user1", problem_id, contest_id).await;
    seed_counted_submission(&app, "sl_user1", problem_id, contest_id).await;

    let sub3_path = format!("/api/v1/contests/{contest_id}/problems/{problem_id}/submissions");
    let sub3_res = app
        .post_with_token(
            &sub3_path,
            &json!({
                "files": [{"filename": "main.cpp", "content": "int main() { return 2; }"}],
                "language": "cpp",
            }),
            &contestant,
        )
        .await;
    assert_eq!(
        sub3_res.status, 429,
        "Third submission should be rejected: {}",
        sub3_res.text
    );
    assert_eq!(
        sub3_res.body["code"].as_str().unwrap_or(""),
        "SUBMISSION_LIMIT_EXCEEDED",
        "Error code should be SUBMISSION_LIMIT_EXCEEDED"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn limit_zero_means_unlimited() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin2", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user2", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 2").await;

    let contest_id = app
        .create_typed_contest(&admin, "Limit Contest 2", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let config_path = format!(
        "/api/v1/contests/{contest_id}/problems/{problem_id}/config/submission-limit/limits"
    );
    app.put_with_token(
        &config_path,
        &json!({ "config": { "max_submissions": 0 }, "enabled": true }),
        &admin,
    )
    .await;

    seed_counted_submission(&app, "sl_user2", problem_id, contest_id).await;
    seed_counted_submission(&app, "sl_user2", problem_id, contest_id).await;

    let status_path = format!(
        "/api/v1/p/submission-limit/api/plugins/submission-limit/contests/{contest_id}/problems/{problem_id}/status"
    );
    let res = app.get_with_token(&status_path, &contestant).await;
    assert_eq!(
        res.status, 200,
        "Limit status endpoint failed: {}",
        res.text
    );
    assert_eq!(res.body["unlimited"].as_bool(), Some(true));
    assert_eq!(res.body["remaining"], serde_json::Value::Null);
    assert_eq!(res.body["submissions_made"].as_u64(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn limit_status_endpoint_shows_remaining() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin3", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user3", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 3").await;

    let contest_id = app
        .create_typed_contest(&admin, "Limit Contest 3", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let config_path = format!(
        "/api/v1/contests/{contest_id}/problems/{problem_id}/config/submission-limit/limits"
    );
    app.put_with_token(
        &config_path,
        &json!({ "config": { "max_submissions": 5 }, "enabled": true }),
        &admin,
    )
    .await;

    seed_counted_submission(&app, "sl_user3", problem_id, contest_id).await;

    let status_path = format!(
        "/api/v1/p/submission-limit/api/plugins/submission-limit/contests/{contest_id}/problems/{problem_id}/status"
    );
    let res = app.get_with_token(&status_path, &contestant).await;
    assert_eq!(
        res.status, 200,
        "Limit status endpoint failed: {}",
        res.text
    );
    assert_eq!(res.body["enabled"].as_bool(), Some(true));
    assert_eq!(res.body["max_submissions"].as_u64(), Some(5));
    assert_eq!(res.body["submissions_made"].as_u64(), Some(1));
    assert_eq!(res.body["remaining"].as_u64(), Some(4));
    assert_eq!(res.body["unlimited"].as_bool(), Some(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn different_problems_have_independent_limits() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin4", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user4", "password").await;

    let prob_a = app.create_problem(&admin, "Limit ProbA 4").await;
    let prob_b = app.create_problem(&admin, "Limit ProbB 4").await;

    let contest_id = app
        .create_typed_contest(&admin, "Limit Contest 4", "icpc", true, true)
        .await;
    app.add_problem_to_contest_with_label(contest_id, prob_a, "A", &admin)
        .await;
    app.add_problem_to_contest_with_label(contest_id, prob_b, "B", &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let config_a =
        format!("/api/v1/contests/{contest_id}/problems/{prob_a}/config/submission-limit/limits");
    app.put_with_token(
        &config_a,
        &json!({ "config": { "max_submissions": 1 }, "enabled": true }),
        &admin,
    )
    .await;

    seed_counted_submission(&app, "sl_user4", prob_a, contest_id).await;

    let sub_a2_path = format!("/api/v1/contests/{contest_id}/problems/{prob_a}/submissions");
    let sub_a2 = app
        .post_with_token(
            &sub_a2_path,
            &json!({
                "files": [{"filename": "main.cpp", "content": "int main() { return 1; }"}],
                "language": "cpp",
            }),
            &contestant,
        )
        .await;
    assert_eq!(
        sub_a2.status, 429,
        "Problem A second submission should be rejected: {}",
        sub_a2.text
    );

    let status_b = format!(
        "/api/v1/p/submission-limit/api/plugins/submission-limit/contests/{contest_id}/problems/{prob_b}/status"
    );
    let res = app.get_with_token(&status_b, &contestant).await;
    assert_eq!(res.status, 200, "Problem B status failed: {}", res.text);
    assert_eq!(res.body["submissions_made"].as_u64(), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn different_users_have_independent_limits() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin5", "password", "admin")
        .await;
    let user_a = app.create_authenticated_user("sl_userA5", "password").await;
    let user_b = app.create_authenticated_user("sl_userB5", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 5").await;

    let contest_id = app
        .create_typed_contest(&admin, "Limit Contest 5", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &user_a).await;
    app.register_for_contest(contest_id, &user_b).await;

    let config_path = format!(
        "/api/v1/contests/{contest_id}/problems/{problem_id}/config/submission-limit/limits"
    );
    app.put_with_token(
        &config_path,
        &json!({ "config": { "max_submissions": 1 }, "enabled": true }),
        &admin,
    )
    .await;

    seed_counted_submission(&app, "sl_userA5", problem_id, contest_id).await;

    let status_path = format!(
        "/api/v1/p/submission-limit/api/plugins/submission-limit/contests/{contest_id}/problems/{problem_id}/status"
    );
    let res = app.get_with_token(&status_path, &user_b).await;
    assert_eq!(res.status, 200, "User B status failed: {}", res.text);
    assert_eq!(res.body["submissions_made"].as_u64(), Some(0));
    assert_eq!(res.body["remaining"].as_u64(), Some(1));
}
