use serde_json::json;

use crate::common::E2eTestApp;

#[tokio::test(flavor = "multi_thread")]
async fn ioi_contest_type_registered() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("ioi_admin1", "password", "admin")
        .await;
    let contest_id = app
        .create_typed_contest(&admin, "IOI Contest 1", "ioi", true, true)
        .await;
    assert!(contest_id > 0, "Should successfully create an IOI contest");
}

#[tokio::test(flavor = "multi_thread")]
async fn ioi_scoreboard_empty_before_submissions() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("ioi_admin2", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("ioi_user2", "password").await;

    let problem_id = app.create_problem(&admin, "IOI Problem 2").await;
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_typed_contest(&admin, "IOI Contest 2", "ioi", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let scoreboard_path = format!("/api/v1/p/ioi/api/plugins/ioi/contests/{contest_id}/scoreboard");
    let res = app.get_with_token(&scoreboard_path, &contestant).await;
    assert_eq!(res.status, 200, "Scoreboard request failed: {}", res.text);
    assert!(
        res.body["rankings"].is_array(),
        "Scoreboard rows should be an array, got: {}",
        res.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ioi_scoreboard_reflects_judged_submission() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("ioi_admin3", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("ioi_user3", "password").await;

    let problem_id = app.create_problem(&admin, "IOI Problem 3").await;
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_typed_contest(&admin, "IOI Contest 3", "ioi", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let sub_id = app
        .create_contest_submission(
            contest_id,
            problem_id,
            &contestant,
            "cpp",
            "int main() { return 0; }",
        )
        .await;
    app.wait_for_submission_terminal(sub_id, &contestant, 60)
        .await;

    let scoreboard_path = format!("/api/v1/p/ioi/api/plugins/ioi/contests/{contest_id}/scoreboard");
    let res = app.get_with_token(&scoreboard_path, &contestant).await;
    assert_eq!(res.status, 200, "Scoreboard request failed: {}", res.text);

    let rows = &res.body["rankings"];
    assert!(rows.is_array(), "rows should be an array");
    let rankings_arr = rows.as_array().unwrap();
    assert!(
        !rankings_arr.is_empty(),
        "Scoreboard should have at least one entry after a judged submission"
    );
    assert!(
        rankings_arr[0]["username"].is_string(),
        "Row should have a username"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ioi_token_endpoint_exists() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("ioi_admin4", "password", "admin")
        .await;
    let contestant = app.create_authenticated_user("ioi_user4", "password").await;

    let problem_id = app.create_problem(&admin, "IOI Problem 4").await;
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_typed_contest(&admin, "IOI Contest 4", "ioi", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &contestant).await;

    let sub_id = app
        .create_contest_submission(
            contest_id,
            problem_id,
            &contestant,
            "cpp",
            "int main() { return 0; }",
        )
        .await;
    app.wait_for_submission_terminal(sub_id, &contestant, 60)
        .await;

    let token_path =
        format!("/api/v1/p/ioi/api/plugins/ioi/contests/{contest_id}/submissions/{sub_id}/token");
    let res = app
        .post_with_token(&token_path, &json!({}), &contestant)
        .await;
    assert!(
        res.status < 500,
        "Token endpoint should not return 5xx: status={}, body={}",
        res.status,
        res.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ioi_contest_config_scoring_mode() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("ioi_admin5", "password", "admin")
        .await;

    let contest_id = app
        .create_typed_contest(&admin, "IOI Contest 5", "ioi", true, true)
        .await;

    let config_path = format!("/api/v1/contests/{contest_id}/config/ioi/contest");
    let put_res = app
        .put_with_token(
            &config_path,
            &json!({
                "config": {
                    "scoring_mode": "sum_best_subtask",
                    "feedback_level": "subtask_scores"
                },
                "enabled": true
            }),
            &admin,
        )
        .await;
    assert_eq!(
        put_res.status, 200,
        "Failed to set IOI contest config: {}",
        put_res.text
    );

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(
        get_res.status, 200,
        "Failed to get IOI contest config: {}",
        get_res.text
    );
    assert_eq!(
        get_res.body["config"]["scoring_mode"].as_str(),
        Some("sum_best_subtask")
    );
    assert_eq!(
        get_res.body["config"]["feedback_level"].as_str(),
        Some("subtask_scores")
    );
}
