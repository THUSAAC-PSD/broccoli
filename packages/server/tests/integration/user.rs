use serde_json::json;

use crate::common::{TestApp, routes};

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
        assert!(admin["roles"].as_array().unwrap().contains(&json!("admin")));
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
        assert_eq!(alice["roles"], json!(["contestant"]));
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
