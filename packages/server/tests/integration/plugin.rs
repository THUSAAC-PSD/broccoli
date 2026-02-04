use serde_json::json;

use crate::common::{TestApp, routes};

mod load_plugin {
    use super::*;

    #[tokio::test]
    async fn unauthenticated_request_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(&routes::plugin_load("echo-plugin"), &json!({}))
            .await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn contestant_without_plugin_load_permission_is_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("alice", "securepass", "contestant")
            .await;

        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn admin_can_load_a_valid_plugin() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;

        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn loading_same_plugin_twice_succeeds() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;
        assert_eq!(res.status, 200);

        // Loading again should not fail
        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;
        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn loading_nonexistent_plugin_returns_not_found() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        let res = app
            .post_with_token(&routes::plugin_load("no-such-plugin"), &json!({}), &token)
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod call_plugin {
    use super::*;

    #[tokio::test]
    async fn unauthenticated_request_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(&routes::plugin_call("echo-plugin", "echo"), &json!({}))
            .await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn authenticated_user_can_call_loaded_plugin_function() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        // Load the plugin first
        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;
        assert_eq!(res.status, 200);

        // Call the echo function
        let res = app
            .post_with_token(
                &routes::plugin_call("echo-plugin", "echo"),
                &json!("hello world"),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body, json!("hello world"));
    }

    #[tokio::test]
    async fn contestant_can_call_loaded_plugin_function() {
        let app = TestApp::spawn().await;

        // Admin loads the plugin
        let admin_token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;
        let res = app
            .post_with_token(
                &routes::plugin_load("echo-plugin"),
                &json!({}),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 200);

        // Contestant calls the loaded plugin
        let contestant_token = app
            .create_user_with_role("contestant_user", "securepass", "contestant")
            .await;
        let res = app
            .post_with_token(
                &routes::plugin_call("echo-plugin", "echo"),
                &json!({"data": [1, 2, 3]}),
                &contestant_token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body, json!({"data": [1, 2, 3]}));
    }

    #[tokio::test]
    async fn calling_function_on_unloaded_plugin_returns_not_found() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("user1", "securepass", "contestant")
            .await;

        let res = app
            .post_with_token(
                &routes::plugin_call("echo-plugin", "echo"),
                &json!("test"),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn calling_nonexistent_function_returns_error() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;

        // Load the plugin first
        let res = app
            .post_with_token(&routes::plugin_load("echo-plugin"), &json!({}), &token)
            .await;
        assert_eq!(res.status, 200);

        // Call a function that doesn't exist
        let res = app
            .post_with_token(
                &routes::plugin_call("echo-plugin", "no_such_func"),
                &json!("test"),
                &token,
            )
            .await;

        assert_eq!(res.status, 500);
        assert_eq!(res.body["code"], "INTERNAL_ERROR");
    }
}
