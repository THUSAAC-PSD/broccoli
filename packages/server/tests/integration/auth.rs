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

mod token_refresh {
    use super::*;

    #[tokio::test]
    async fn authenticated_user_can_refresh_access_token() {
        let app = TestApp::spawn().await;
        let body = json!({"username": "alice", "password": "securepass"});
        app.post_without_token(routes::REGISTER, &body).await;

        // Login sets the HttpOnly refresh cookie in the TestApp client's cookie jar
        let login_res = app.post_without_token(routes::LOGIN, &body).await;
        let old_token = login_res.body["token"].as_str().unwrap().to_string();

        // Wait a bit to ensure timestamps would differ (optional)
        // Refresh uses the cookie automatically
        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;

        assert_eq!(refresh_res.status, 200);
        assert!(refresh_res.body["token"].is_string());
        // New access token should be different from the old one (due to exp/issued time)
        assert_ne!(refresh_res.body["token"].as_str().unwrap(), old_token);
    }

    #[tokio::test]
    async fn cannot_refresh_without_cookie() {
        let app = TestApp::spawn().await;

        // No login, so no cookie in the jar
        let res = app.post_without_token(routes::REFRESH, &json!({})).await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
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

        // Logout revokes the token in DB and clears the client's cookie
        let logout_res = app.post_without_token(routes::LOGOUT, &json!({})).await;
        assert_eq!(logout_res.status, 204);

        // Attempt refresh after logout
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

mod list_users {
    use super::*;

    #[tokio::test]
    async fn admin_can_list_all_users_with_full_fields() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_user", "securepass", "admin")
            .await;
        let _alice_token = app.create_authenticated_user("alice", "securepass").await;

        let res = app.get_with_token(routes::USERS, &admin_token).await;

        assert_eq!(res.status, 200, "list users failed: {}", res.text);
        let users = res
            .body
            .as_array()
            .expect("users response should be an array");
        assert!(users.len() >= 2);

        let admin = users
            .iter()
            .find(|u| u["username"] == "admin_user")
            .expect("admin_user should exist");
        assert_eq!(admin["role"], "admin");
        assert!(admin["id"].is_number());
        assert!(admin["created_at"].is_string());
        let admin_password = admin["password"]
            .as_str()
            .expect("password should be string");
        assert_ne!(admin_password, "securepass");
        assert!(admin_password.len() > 20);

        let alice = users
            .iter()
            .find(|u| u["username"] == "alice")
            .expect("alice should exist");
        assert_eq!(alice["role"], "contestant");
    }

    #[tokio::test]
    async fn non_admin_cannot_list_all_users() {
        let app = TestApp::spawn().await;
        let token = app.create_authenticated_user("alice", "securepass").await;

        let res = app.get_with_token(routes::USERS, &token).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn list_users_requires_token() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::USERS).await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }
}

mod user_deletion {
    use super::*;

    fn user_detail_path(user_id: i32) -> String {
        format!("{}/{}", routes::USERS, user_id)
    }

    #[tokio::test]
    async fn admin_can_soft_delete_user_and_hide_from_list() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_delete_1", "securepass", "admin")
            .await;
        let victim_token = app
            .create_authenticated_user("victim_delete_1", "securepass")
            .await;
        let victim_id = app.get_with_token(routes::ME, &victim_token).await.id();

        let delete_res = app
            .delete_with_token(&user_detail_path(victim_id), &admin_token)
            .await;
        assert_eq!(delete_res.status, 204, "delete failed: {}", delete_res.text);

        let list_res = app.get_with_token(routes::USERS, &admin_token).await;
        assert_eq!(list_res.status, 200, "list failed: {}", list_res.text);
        let users = list_res
            .body
            .as_array()
            .expect("users response should be an array");
        assert!(
            users.iter().all(|u| u["id"] != json!(victim_id)),
            "soft-deleted user should not be returned by /users"
        );
    }

    #[tokio::test]
    async fn soft_deleted_user_cannot_login_again() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_delete_2", "securepass", "admin")
            .await;
        let username = "victim_delete_2";
        let password = "securepass";
        let victim_token = app.create_authenticated_user(username, password).await;
        let victim_id = app.get_with_token(routes::ME, &victim_token).await.id();

        let delete_res = app
            .delete_with_token(&user_detail_path(victim_id), &admin_token)
            .await;
        assert_eq!(delete_res.status, 204, "delete failed: {}", delete_res.text);

        let login_res = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": username, "password": password}),
            )
            .await;
        assert_eq!(login_res.status, 401);
        assert_eq!(login_res.body["code"], "INVALID_CREDENTIALS");
    }

    #[tokio::test]
    async fn soft_deleted_user_cannot_refresh_token() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_delete_3", "securepass", "admin")
            .await;
        let username = "victim_delete_3";
        let password = "securepass";
        let victim_token = app.create_authenticated_user(username, password).await;
        let victim_id = app.get_with_token(routes::ME, &victim_token).await.id();

        let delete_res = app
            .delete_with_token(&user_detail_path(victim_id), &admin_token)
            .await;
        assert_eq!(delete_res.status, 204, "delete failed: {}", delete_res.text);

        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(refresh_res.status, 403);
        assert_eq!(refresh_res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn can_register_same_username_after_soft_delete() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_delete_3", "securepass", "admin")
            .await;
        let username = "recyclable_user";
        let password = "securepass";

        let reg1 = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": username, "password": password}),
            )
            .await;
        assert_eq!(
            reg1.status, 201,
            "initial registration failed: {}",
            reg1.text
        );

        let login1 = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": username, "password": password}),
            )
            .await;
        assert_eq!(login1.status, 200, "initial login failed: {}", login1.text);
        let first_user_id = login1.body["id"]
            .as_i64()
            .expect("login response should contain user id") as i32;

        let delete_res = app
            .delete_with_token(&user_detail_path(first_user_id), &admin_token)
            .await;
        assert_eq!(delete_res.status, 204, "delete failed: {}", delete_res.text);

        let reg2 = app
            .post_without_token(
                routes::REGISTER,
                &json!({"username": username, "password": password}),
            )
            .await;
        assert_eq!(
            reg2.status, 201,
            "re-registration should be allowed after soft delete: {}",
            reg2.text
        );
        assert_ne!(reg2.body["id"], json!(first_user_id));
    }
}
