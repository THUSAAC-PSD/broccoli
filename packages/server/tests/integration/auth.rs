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

        assert_eq!(res.body["roles"], json!(["contestant"]));
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

mod token_refresh {
    use super::*;

    #[tokio::test]
    async fn authenticated_user_can_refresh_access_token() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});
        app.post_without_token(routes::REGISTER, &body).await;

        let login_res = app.post_without_token(routes::LOGIN, &body).await;
        let old_token = login_res.body["token"].as_str().unwrap().to_string();

        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;

        assert_eq!(refresh_res.status, 200);
        assert!(refresh_res.body["token"].is_string());
        assert_ne!(refresh_res.body["token"].as_str().unwrap(), old_token);
    }

    #[tokio::test]
    async fn cannot_refresh_without_cookie() {
        let app = TestApp::spawn().await;

        let res = app.post_without_token(routes::REFRESH, &json!({})).await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn refresh_rotates_the_refresh_cookie() {
        fn extract_refresh_cookie(resp: &TestResponse) -> String {
            for hv in resp.headers.get_all("set-cookie").iter() {
                let s = hv.to_str().expect("set-cookie utf-8");
                if let Some(rest) = s.strip_prefix("broccoli_refresh=") {
                    let val = rest.split(';').next().unwrap_or("").to_string();
                    if !val.is_empty() {
                        return val;
                    }
                }
            }
            panic!("no broccoli_refresh Set-Cookie on response");
        }

        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});
        app.post_without_token(routes::REGISTER, &body).await;

        let login_res = app.post_without_token(routes::LOGIN, &body).await;
        let cookie_a = extract_refresh_cookie(&login_res);

        // First refresh via the cookie-store-aware client: success, sets cookie B.
        let r1 = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(r1.status, 200);
        let cookie_b = extract_refresh_cookie(&r1);
        assert_ne!(
            cookie_a, cookie_b,
            "refresh must rotate the cookie (selector + validator)"
        );

        let raw_client = reqwest::Client::builder()
            .no_proxy()
            .cookie_store(false)
            .build()
            .unwrap();
        let replay_a = raw_client
            .post(format!("http://{}{}", app.addr, routes::REFRESH))
            .header("cookie", format!("broccoli_refresh={}", cookie_a))
            .json(&json!({}))
            .send()
            .await
            .expect("send refresh replay A");
        assert_eq!(replay_a.status(), 401);
        let replay_a_body: serde_json::Value = replay_a.json().await.unwrap();
        assert_eq!(replay_a_body["code"], "TOKEN_INVALID");

        // Second legit refresh with cookie B (via the cookie-store client): rotates to cookie C.
        let r2 = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(r2.status, 200);
        let cookie_c = extract_refresh_cookie(&r2);
        assert_ne!(cookie_b, cookie_c, "second refresh must also rotate");

        // Replay cookie B: 401 TOKEN_INVALID.
        let replay_b = raw_client
            .post(format!("http://{}{}", app.addr, routes::REFRESH))
            .header("cookie", format!("broccoli_refresh={}", cookie_b))
            .json(&json!({}))
            .send()
            .await
            .expect("send refresh replay B");
        assert_eq!(replay_b.status(), 401);
        let replay_b_body: serde_json::Value = replay_b.json().await.unwrap();
        assert_eq!(replay_b_body["code"], "TOKEN_INVALID");
    }
}

mod logout {
    use super::*;

    #[tokio::test]
    async fn authenticated_user_can_logout_and_revoke_refresh_token() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});
        app.post_without_token(routes::REGISTER, &body).await;
        app.post_without_token(routes::LOGIN, &body).await;

        let logout_res = app.post_without_token(routes::LOGOUT, &json!({})).await;
        assert_eq!(logout_res.status, 204);

        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(refresh_res.status, 401);
        assert_eq!(refresh_res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn logout_is_idempotent_and_handles_missing_token_gracefully() {
        let app = TestApp::spawn().await;

        let res = app.post_without_token(routes::LOGOUT, &json!({})).await;

        assert_eq!(res.status, 204);
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
        assert_eq!(res.body["roles"], json!(["contestant"]));
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
