use serde_json::json;

use crate::common::E2eTestApp;

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
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_typed_contest(&admin, "ICPC Contest 3", "icpc", true, true)
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
