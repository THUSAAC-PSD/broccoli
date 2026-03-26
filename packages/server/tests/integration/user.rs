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
            .delete_with_token(&routes::user(victim_id), &admin_token)
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
            .delete_with_token(&routes::user(victim_id), &admin_token)
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
            .delete_with_token(&routes::user(victim_id), &admin_token)
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
            .delete_with_token(&routes::user(first_user_id), &admin_token)
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

mod user_modification {
    use super::*;

    #[tokio::test]
    async fn admin_can_update_user_password_and_login_works() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_mod", "securepass", "admin")
            .await;
        let victim_token = app.create_authenticated_user("victim", "old_pass").await;
        let victim_id = app.get_with_token(routes::ME, &victim_token).await.id();

        // Admin updates the password
        let res = app
            .patch_with_token(
                &routes::user(victim_id),
                &json!({"password": "new_secure_pass"}),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 200);

        // Refresh token should be revoked
        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(refresh_res.status, 401);
        assert_eq!(refresh_res.body["code"], "TOKEN_INVALID");

        // Old password should fail
        let login_old = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": "victim", "password": "old_pass"}),
            )
            .await;
        assert_eq!(login_old.status, 401);

        // New password should succeed
        let login_new = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": "victim", "password": "new_secure_pass"}),
            )
            .await;
        assert_eq!(login_new.status, 200);
    }

    #[tokio::test]
    async fn user_cannot_modify_themselves_without_permission() {
        let app = TestApp::spawn().await;
        let token = app
            .create_authenticated_user("self_hacker", "securepass")
            .await;
        let id = app.get_with_token(routes::ME, &token).await.id();

        let res = app
            .patch_with_token(&routes::user(id), &json!({"username": "new_name"}), &token)
            .await;

        // Fails because the user doesn't have 'user:manage' even for their own ID
        assert_eq!(res.status, 403);
    }

    #[tokio::test]
    async fn admin_can_assign_and_unassign_roles() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_role_mgr", "securepass", "admin")
            .await;
        let user_token = app
            .create_authenticated_user("role_victim", "securepass")
            .await;
        let user_id = app.get_with_token(routes::ME, &user_token).await.id();

        // Assign 'problem_setter' role
        let assign_res = app
            .post_with_token(
                &routes::user_roles(user_id),
                &json!({"role": "problem_setter"}),
                &admin_token,
            )
            .await;
        assert_eq!(assign_res.status, 201);

        // Refresh token should be revoked
        let refresh_res = app.post_without_token(routes::REFRESH, &json!({})).await;
        assert_eq!(refresh_res.status, 401);
        assert_eq!(refresh_res.body["code"], "TOKEN_INVALID");

        // Re-login to get updated roles
        let login_res = app
            .post_without_token(
                routes::LOGIN,
                &json!({"username": "role_victim", "password": "securepass"}),
            )
            .await;
        assert_eq!(login_res.status, 200);
        let roles = login_res.body["roles"].as_array().unwrap();
        assert!(roles.contains(&json!("problem_setter")));

        // Unassign the role
        let unassign_res = app
            .delete_with_token(&routes::user_role(user_id, "problem_setter"), &admin_token)
            .await;
        assert_eq!(unassign_res.status, 204);

        // Verify it is gone
        let me_res_after = app.get_with_token(routes::ME, &user_token).await;
        assert!(
            !me_res_after.body["roles"]
                .as_array()
                .unwrap()
                .contains(&json!("problem_setter"))
        );
    }
}

mod role_management {
    use super::*;

    #[tokio::test]
    async fn admin_can_list_all_roles() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("super_admin", "securepass", "admin")
            .await;

        let res = app.get_with_token(routes::ROLES, &admin_token).await;
        assert_eq!(res.status, 200);
        let roles = res.body.as_array().unwrap();
        assert!(roles.contains(&json!("admin")));
        assert!(roles.contains(&json!("contestant")));
    }

    #[tokio::test]
    async fn admin_can_manage_role_permissions() {
        let app = TestApp::spawn().await;
        // Assume 'admin' role has 'role:manage' permission
        let admin_token = app
            .create_user_with_role("super_admin", "securepass", "admin")
            .await;
        let test_role = "contestant";

        // Grant a new permission
        let grant_res = app
            .post_with_token(
                &routes::role_permissions(test_role),
                &json!({"permission": "experimental:feature"}),
                &admin_token,
            )
            .await;
        assert_eq!(grant_res.status, 201);

        // List permissions and verify
        let list_res = app
            .get_with_token(&routes::role_permissions(test_role), &admin_token)
            .await;
        let perms = list_res.body.as_array().unwrap();
        assert!(perms.contains(&json!("experimental:feature")));

        // Revoke the permission
        let revoke_res = app
            .delete_with_token(
                &routes::role_permission(test_role, "experimental:feature"),
                &admin_token,
            )
            .await;
        assert_eq!(revoke_res.status, 204);

        // Verify it's gone
        let list_res_final = app
            .get_with_token(&routes::role_permissions(test_role), &admin_token)
            .await;
        assert!(
            !list_res_final
                .body
                .as_array()
                .unwrap()
                .contains(&json!("experimental:feature"))
        );
    }

    #[tokio::test]
    async fn regular_user_cannot_access_role_permissions() {
        let app = TestApp::spawn().await;
        let token = app.create_authenticated_user("peasant", "securepass").await;

        let res = app
            .get_with_token(&routes::role_permissions("admin"), &token)
            .await;
        assert_eq!(res.status, 403);
    }
}
