use serde_json::json;

use crate::common::E2eTestApp;

#[tokio::test(flavor = "multi_thread")]
async fn limit_rejects_after_max_submissions() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin1", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user1", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 1").await;
    app.create_test_case(problem_id, &admin).await;

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

    let _sub1 = app
        .create_contest_submission(
            contest_id,
            problem_id,
            &contestant,
            "cpp",
            "int main() { return 0; }",
        )
        .await;
    let _sub2 = app
        .create_contest_submission(
            contest_id,
            problem_id,
            &contestant,
            "cpp",
            "int main() { return 1; }",
        )
        .await;

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
    app.create_test_case(problem_id, &admin).await;

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

    for i in 0..3 {
        let _sub = app
            .create_contest_submission(
                contest_id,
                problem_id,
                &contestant,
                "cpp",
                &format!("int main() {{ return {}; }}", i),
            )
            .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn limit_status_endpoint_shows_remaining() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("sl_admin3", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("sl_user3", "password").await;

    let problem_id = app.create_problem(&admin, "Limit Problem 3").await;
    app.create_test_case(problem_id, &admin).await;

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

    app.create_contest_submission(
        contest_id,
        problem_id,
        &contestant,
        "cpp",
        "int main() { return 0; }",
    )
    .await;

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
    app.create_test_case(prob_a, &admin).await;
    let prob_b = app.create_problem(&admin, "Limit ProbB 4").await;
    app.create_test_case(prob_b, &admin).await;

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

    app.create_contest_submission(
        contest_id,
        prob_a,
        &contestant,
        "cpp",
        "int main() { return 0; }",
    )
    .await;

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

    let _sub_b = app
        .create_contest_submission(
            contest_id,
            prob_b,
            &contestant,
            "cpp",
            "int main() { return 0; }",
        )
        .await;
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
    app.create_test_case(problem_id, &admin).await;

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

    app.create_contest_submission(
        contest_id,
        problem_id,
        &user_a,
        "cpp",
        "int main() { return 0; }",
    )
    .await;

    let _sub_b = app
        .create_contest_submission(
            contest_id,
            problem_id,
            &user_b,
            "cpp",
            "int main() { return 0; }",
        )
        .await;
}
