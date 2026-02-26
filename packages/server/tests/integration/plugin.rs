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
    async fn plugins_are_discovered_and_loaded_on_startup() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app.get_with_token(routes::ADMIN_LIST_PLUGINS, &token).await;
        assert_eq!(res.status, 200);

        let list = res.body.as_array().expect("Expected an array of plugins");
        assert_eq!(list.len(), 2, "Expected exactly two plugins in the list");

        let server_plugin = list
            .iter()
            .find(|p| p["id"] == "server-plugin")
            .expect("Expected to find server-plugin in the list");
        assert_eq!(server_plugin["status"], "Loaded");

        let web_plugin = list
            .iter()
            .find(|p| p["id"] == "web-plugin")
            .expect("Expected to find web-plugin in the list");
        assert_eq!(web_plugin["status"], "Loaded");
    }

    #[tokio::test]
    async fn unauthenticated_request_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::ADMIN_LIST_PLUGINS).await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");

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

    #[tokio::test]
    async fn admin_can_manage_plugin_lifecycle() {
        let app = TestApp::spawn().await;
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

    #[tokio::test]
    async fn enabling_loaded_plugin_returns_conflict() {
        let app = TestApp::spawn().await;
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

    #[tokio::test]
    async fn enabling_nonexistent_plugin_returns_not_found() {
        let app = TestApp::spawn().await;
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

    #[tokio::test]
    async fn path_and_query_params_are_passed_to_plugin() {
        let app = TestApp::spawn().await;

        // Route: GET /reflect/123?page=1&sort=desc
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

    #[tokio::test]
    async fn method_mismatch_returns_method_not_allowed() {
        let app = TestApp::spawn().await;

        // /sql/counter is POST only, try GET
        let res = app
            .get_without_token(&routes::plugin_proxy("server-plugin", "sql/counter"))
            .await;

        assert_eq!(res.status, 405);
        assert_eq!(res.body["code"], "METHOD_NOT_ALLOWED");
    }

    #[tokio::test]
    async fn non_existent_route_returns_not_found() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_proxy("server-plugin", "no-such-route"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");

        let res = app
            .get_without_token(&routes::plugin_proxy("no-such-plugin", "some-route"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod kv_store {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn plugin_can_use_kv_store_to_persist_data() {
        let app = TestApp::spawn().await;
        let url = routes::plugin_proxy("server-plugin", "kv/some-key");

        let res = app.get_without_token(&url).await;
        assert_eq!(res.status, 404);

        app.post_without_token(&url, &json!({"value": "42"})).await;

        let res = app.get_without_token(&url).await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["value"], "42");
    }
}

mod raw_sql {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn plugin_can_execute_raw_sql() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                &routes::plugin_proxy("server-plugin", "sql/counter"),
                &json!({}),
            )
            .await;
        assert_eq!(res.body["count"], 1);

        let res = app
            .post_without_token(
                &routes::plugin_proxy("server-plugin", "sql/counter"),
                &json!({}),
            )
            .await;
        assert_eq!(res.body["count"], 2);
    }
}

mod public_discovery {
    use super::*;

    #[tokio::test]
    async fn active_plugins_contain_only_loaded_web_plugins() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::ACTIVE_PLUGINS).await;
        assert_eq!(res.status, 200);

        let list = res
            .body
            .as_array()
            .expect("Expected an array of active plugins");
        assert_eq!(
            list.len(),
            1,
            "Expected exactly one active plugin in the list"
        );

        let web_plugin = list
            .iter()
            .find(|p| p["id"] == "web-plugin")
            .expect("Expected to find web-plugin in the list");
        assert_eq!(
            web_plugin["entry"],
            routes::plugin_asset("web-plugin", "index.js")
        );
    }
}

mod static_assets {
    use super::*;

    #[tokio::test]
    async fn web_plugin_assets_are_served() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_asset("web-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.headers["Content-Type"], "text/javascript");
    }

    #[tokio::test]
    async fn accessing_nonexistent_asset_returns_not_found() {
        let app = TestApp::spawn().await;

        // Nonexistent asset of loaded web plugin
        let res = app
            .get_without_token(&routes::plugin_asset("web-plugin", "nonexistent.js"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");

        // Plugin exists but has no web assets
        let res = app
            .get_without_token(&routes::plugin_asset("server-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");

        // Plugin not found
        let res = app
            .get_without_token(&routes::plugin_asset("no-such-plugin", "index.js"))
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn path_traversal_attempts_are_blocked() {
        let app = TestApp::spawn().await;

        let res = app
            .get_without_token(&routes::plugin_asset("web-plugin", "../secret.txt"))
            .await;
        assert_eq!(res.status, 404);
    }
}
