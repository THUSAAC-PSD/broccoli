use serde_json::json;

use crate::common::{TestApp, TestResponse, routes};

mod registration {
    use super::*;

    #[tokio::test]
    async fn new_user_can_register_with_valid_credentials() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "alice", "password": "securepass"}),
            )
            .await;

        assert_eq!(res.status, 201);
        assert!(res.body["id"].is_number());
        assert_eq!(res.body["username"], "alice");
    }

    #[tokio::test]
    async fn cannot_register_with_an_already_taken_username() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});

        let first = app.post_without_token(routes::REGISTER, &body).await;
        assert_eq!(
            first.status, 201,
            "First registration failed: {}",
            first.text
        );

        let res = app.post_without_token(routes::REGISTER, &body).await;

        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "USERNAME_TAKEN");
    }

    #[tokio::test]
    async fn cannot_register_with_a_password_that_is_too_short() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "alice", "password": "short"}),
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cannot_register_with_a_password_that_is_too_long() {
        let app = TestApp::spawn().await;
        let long_password = "a".repeat(129);

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "alice", "password": long_password}),
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cannot_register_with_an_invalid_username() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "no spaces!", "password": "securepass"}),
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cannot_register_with_an_empty_username() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "   ", "password": "securepass"}),
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cannot_register_with_a_username_that_is_too_long() {
        let app = TestApp::spawn().await;
        let long_name = "a".repeat(33);

        let res = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": long_name, "password": "securepass"}),
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }
}

mod login {
    use super::*;

    #[tokio::test]
    async fn registered_user_can_login_and_receives_token() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});

        let reg = app.post_without_token(routes::REGISTER, &body).await;
        assert_eq!(reg.status, 201, "Registration failed: {}", reg.text);
        let res = app.post_without_token(routes::LOGIN, &body).await;

        assert_eq!(res.status, 200);
        assert!(res.body["token"].is_string());
        assert_eq!(res.body["username"], "alice");
    }

    #[tokio::test]
    async fn new_user_receives_contestant_role_with_submit_permission() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});

        let reg = app.post_without_token(routes::REGISTER, &body).await;
        assert_eq!(reg.status, 201, "Registration failed: {}", reg.text);
        let res = app.post_without_token(routes::LOGIN, &body).await;

        assert_eq!(res.body["role"], "contestant");
        let permissions = res.body["permissions"]
            .as_array()
            .expect("permissions should be an array");
        assert!(permissions.contains(&json!("submission:submit")));
    }

    #[tokio::test]
    async fn cannot_login_with_wrong_password() {
        let app = TestApp::spawn().await;

        let reg = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": "alice", "password": "securepass"}),
            )
            .await;
        assert_eq!(reg.status, 201, "Registration failed: {}", reg.text);

        let res = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": "alice", "password": "wrongpass"}),
            )
            .await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "INVALID_CREDENTIALS");
    }

    #[tokio::test]
    async fn cannot_login_with_nonexistent_username() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": "nobody", "password": "securepass"}),
            )
            .await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "INVALID_CREDENTIALS");
    }
}

mod request_validation {
    use super::*;

    #[tokio::test]
    async fn malformed_json_body_returns_validation_error() {
        let app = TestApp::spawn().await;

        let res = app
            .client
            .post(format!("http://{}{}", app.addr, routes::REGISTER))
            .header("Content-Type", "application/json")
            .body("not valid json")
            .send()
            .await
            .expect("Failed to send request");

        let res = TestResponse::from_response(res).await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn missing_required_fields_returns_validation_error() {
        let app = TestApp::spawn().await;

        let res = app
            .post_without_token(routes::REGISTER, &json!({"username": "alice"}))
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }
}

mod authenticated_access {
    use super::*;

    #[tokio::test]
    async fn authenticated_user_can_retrieve_their_profile() {
        let app = TestApp::spawn().await;
        let token = app.create_authenticated_user("alice", "securepass").await;

        let res = app.get_with_token(routes::ME, &token).await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["username"], "alice");
        assert!(res.body["id"].is_number());
        assert_eq!(res.body["role"], "contestant");
        assert!(res.body["permissions"].is_array());
    }

    #[tokio::test]
    async fn request_without_token_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::ME).await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn request_with_malformed_token_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app.get_with_token(routes::ME, "not-a-valid-jwt").await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_INVALID");
    }

    #[tokio::test]
    async fn request_with_non_bearer_auth_scheme_is_rejected() {
        let app = TestApp::spawn().await;

        let res = app
            .client
            .get(format!("http://{}{}", app.addr, routes::ME))
            .header("Authorization", "Basic abc123")
            .send()
            .await
            .expect("Failed to send request");

        let res = TestResponse::from_response(res).await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_INVALID");
    }
}
