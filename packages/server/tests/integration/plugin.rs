use serde_json::json;

use crate::common::{TestApp, routes};

/// Checks that the plugin with the given ID has the expected status.
async fn assert_plugin_status(app: &TestApp, token: &str, plugin_id: &str, expected_status: &str) {
    let res = app
        .get_with_token(&routes::admin_plugin_details(plugin_id), token)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(res.body["status"], expected_status);
}

mod plugin_management {
    use super::*;

    #[tokio::test]
    async fn unauthenticated_request_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::admin_plugin_details("server-plugin"))
            .await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");

        let res = app
            .post_without_token(&routes::admin_plugin_enable("server-plugin"), &json!({}))
            .await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");

        let res = app
            .post_without_token(&routes::admin_plugin_disable("server-plugin"), &json!({}))
            .await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_enable_a_valid_plugin() {
        let app = TestApp::spawn_with_plugins().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        // Check initial status
        assert_plugin_status(&app, &token, "server-plugin", "Loaded").await;

        // Disable the plugin first
        let res = app
            .post_with_token(
                &routes::admin_plugin_disable("server-plugin"),
                &json!({}),
                &token,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_plugin_status(&app, &token, "server-plugin", "Unloaded").await;

        // Enable the plugin again
        let res = app
            .post_with_token(
                &routes::admin_plugin_enable("server-plugin"),
                &json!({}),
                &token,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_plugin_status(&app, &token, "server-plugin", "Loaded").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enabling_same_plugin_twice_returns_conflict() {
        let app = TestApp::spawn_with_plugins().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(
                &routes::admin_plugin_enable("server-plugin"),
                &json!({}),
                &token,
            )
            .await;

        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "CONFLICT");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn enabling_nonexistent_plugin_returns_not_found() {
        let app = TestApp::spawn_with_plugins().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(
                &routes::admin_plugin_enable("no-such-plugin"),
                &json!({}),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod plugin_routing {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn public_route_returns_correct_response() {
        let app = TestApp::spawn_with_plugins().await;

        let res = app
            .get_without_token(&routes::plugin_proxy_with_query(
                "server-plugin",
                "reflect/123",
                "page=1&sort=desc",
            ))
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["params"]["id"], "123");
        assert_eq!(res.body["query"]["page"], "1");
        assert_eq!(res.body["query"]["sort"], "desc");
        assert_eq!(res.body["method"], "GET");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn valid_token_is_forwarded_to_unprotected_plugin_routes() {
        let app = TestApp::spawn_with_plugins().await;
        let token = app
            .create_user_with_role("plugin_user", "securepass", "contestant")
            .await;

        let me = app.get_with_token(routes::ME, &token).await;
        assert_eq!(me.status, 200);

        let res = app
            .get_with_token(
                &routes::plugin_proxy("server-plugin", "reflect/123"),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["auth_user_id"], me.body["id"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn protected_route_distinguishes_missing_and_invalid_tokens() {
        let app = TestApp::spawn_with_plugins().await;

        let missing = app
            .get_without_token(&routes::plugin_proxy("server-plugin", "protected/123"))
            .await;
        assert_eq!(missing.status, 401);
        assert_eq!(missing.body["code"], "TOKEN_MISSING");

        let invalid = app
            .get_with_token(
                &routes::plugin_proxy("server-plugin", "protected/123"),
                "bad.token",
            )
            .await;
        assert_eq!(invalid.status, 401);
        assert_eq!(invalid.body["code"], "TOKEN_INVALID");
    }

    #[tokio::test]
    async fn nonexistent_route_returns_not_found() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_proxy("server-plugin", "no-such-route"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn nonexistent_plugin_returns_not_found() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_proxy("no-such-plugin", "some-route"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sql_counter_increments_across_calls() {
        let app = TestApp::spawn_with_plugins().await;

        let res = app
            .post_without_token(
                &routes::plugin_proxy("server-plugin", "sql/counter"),
                &json!({}),
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["count"], 1);

        let res = app
            .post_without_token(
                &routes::plugin_proxy("server-plugin", "sql/counter"),
                &json!({}),
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["count"], 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn web_plugin_asset_is_served_with_correct_content_type() {
        let app = TestApp::spawn_with_plugins().await;

        let res = app
            .get_without_token(&routes::plugin_asset("web-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.headers["Content-Type"], "text/javascript");
    }

    #[tokio::test]
    async fn asset_request_for_plugin_without_web_assets_returns_not_found() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_asset("server-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn asset_request_for_nonexistent_plugin_returns_not_found() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_asset("no-such-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn path_traversal_in_asset_request_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_asset("web-plugin", "../secret.txt"))
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn calling_disabled_plugin_returns_not_found() {
        let app = TestApp::spawn_with_plugins().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(
                &routes::admin_plugin_disable("server-plugin"),
                &json!({}),
                &token,
            )
            .await;
        assert_eq!(res.status, 200);

        let res = app
            .get_without_token(&routes::plugin_proxy("server-plugin", "reflect/123"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}
