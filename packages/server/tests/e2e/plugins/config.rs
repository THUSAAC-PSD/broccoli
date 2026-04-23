use serde_json::json;

use crate::common::E2eTestApp;

#[tokio::test(flavor = "multi_thread")]
async fn set_and_get_problem_scope_config() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin1", "password", "admin")
        .await;
    let problem_id = app.create_problem(&admin, "Config Problem 1").await;

    let config_path = format!("/api/v1/problems/{problem_id}/config/cooldown/cooldown");
    let put_res = app
        .put_with_token(
            &config_path,
            &json!({ "config": { "cooldown_seconds": 30 }, "enabled": true }),
            &admin,
        )
        .await;
    assert_eq!(put_res.status, 200, "PUT config failed: {}", put_res.text);
    assert_eq!(put_res.body["plugin_id"].as_str(), Some("cooldown"));
    assert_eq!(put_res.body["namespace"].as_str(), Some("cooldown"));
    assert_eq!(
        put_res.body["config"]["cooldown_seconds"].as_u64(),
        Some(30)
    );
    assert_eq!(put_res.body["enabled"].as_bool(), Some(true));

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(get_res.status, 200, "GET config failed: {}", get_res.text);
    assert_eq!(
        get_res.body["config"]["cooldown_seconds"].as_u64(),
        Some(30)
    );
    assert_eq!(get_res.body["enabled"].as_bool(), Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn set_and_get_contest_scope_config() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin2", "password", "admin")
        .await;
    let contest_id = app
        .create_typed_contest(&admin, "Config Contest 2", "icpc", true, true)
        .await;

    let config_path = format!("/api/v1/contests/{contest_id}/config/icpc/contest");
    let put_res = app
        .put_with_token(
            &config_path,
            &json!({
                "config": { "penalty_minutes": 25 },
                "enabled": true
            }),
            &admin,
        )
        .await;
    assert_eq!(put_res.status, 200, "PUT config failed: {}", put_res.text);
    assert_eq!(put_res.body["config"]["penalty_minutes"].as_u64(), Some(25));

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(get_res.status, 200, "GET config failed: {}", get_res.text);
    assert_eq!(get_res.body["config"]["penalty_minutes"].as_u64(), Some(25));
}

#[tokio::test(flavor = "multi_thread")]
async fn set_and_get_contest_problem_scope_config() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin3", "password", "admin")
        .await;
    let problem_id = app.create_problem(&admin, "Config Problem 3").await;
    let contest_id = app
        .create_typed_contest(&admin, "Config Contest 3", "icpc", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;

    let config_path =
        format!("/api/v1/contests/{contest_id}/problems/{problem_id}/config/cooldown/cooldown");
    let put_res = app
        .put_with_token(
            &config_path,
            &json!({ "config": { "cooldown_seconds": 120 }, "enabled": true }),
            &admin,
        )
        .await;
    assert_eq!(put_res.status, 200, "PUT config failed: {}", put_res.text);
    assert_eq!(
        put_res.body["config"]["cooldown_seconds"].as_u64(),
        Some(120)
    );

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(get_res.status, 200, "GET config failed: {}", get_res.text);
    assert_eq!(
        get_res.body["config"]["cooldown_seconds"].as_u64(),
        Some(120)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_config_removes_it() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin4", "password", "admin")
        .await;
    let problem_id = app.create_problem(&admin, "Config Problem 4").await;

    let config_path = format!("/api/v1/problems/{problem_id}/config/cooldown/cooldown");

    let put_res = app
        .put_with_token(
            &config_path,
            &json!({ "config": { "cooldown_seconds": 10 }, "enabled": true }),
            &admin,
        )
        .await;
    assert_eq!(put_res.status, 200, "PUT config failed: {}", put_res.text);

    let del_res = app.delete_with_token(&config_path, &admin).await;
    assert_eq!(
        del_res.status, 204,
        "DELETE config failed: {}",
        del_res.text
    );

    let get_res = app.get_with_token(&config_path, &admin).await;
    assert_eq!(
        get_res.status, 404,
        "GET after DELETE should be 404: {}",
        get_res.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn list_config_shows_all_namespaces() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin5", "password", "admin")
        .await;
    let problem_id = app.create_problem(&admin, "Config Problem 5").await;

    let cd_path = format!("/api/v1/problems/{problem_id}/config/cooldown/cooldown");
    app.put_with_token(
        &cd_path,
        &json!({ "config": { "cooldown_seconds": 45 }, "enabled": true }),
        &admin,
    )
    .await;

    let sl_path = format!("/api/v1/problems/{problem_id}/config/submission-limit/limits");
    app.put_with_token(
        &sl_path,
        &json!({ "config": { "max_submissions": 10 }, "enabled": true }),
        &admin,
    )
    .await;

    let list_path = format!("/api/v1/problems/{problem_id}/config");
    let list_res = app.get_with_token(&list_path, &admin).await;
    assert_eq!(
        list_res.status, 200,
        "List config failed: {}",
        list_res.text
    );

    let configs = list_res
        .body
        .as_array()
        .expect("Response should be an array");
    let plugin_ids: Vec<&str> = configs
        .iter()
        .filter_map(|c| c["plugin_id"].as_str())
        .collect();
    assert!(
        plugin_ids.contains(&"cooldown"),
        "Should list cooldown config, got: {:?}",
        plugin_ids
    );
    assert!(
        plugin_ids.contains(&"submission-limit"),
        "Should list submission-limit config, got: {:?}",
        plugin_ids
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn config_for_nonexistent_problem_returns_404() {
    let app = E2eTestApp::spawn().await;

    let admin = app
        .create_user_with_role("cfg_admin6", "password", "admin")
        .await;

    let config_path = "/api/v1/problems/99999/config/cooldown/cooldown";
    let res = app.get_with_token(config_path, &admin).await;
    assert_eq!(
        res.status, 404,
        "Should return 404 for nonexistent problem: {}",
        res.text
    );
}
