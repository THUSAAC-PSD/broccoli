use serde_json::json;

use crate::common::E2eTestApp;

mod user_listing {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_list_users() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_ls1", "password123", "admin")
            .await;

        app.create_authenticated_user("usr_ls2", "password123")
            .await;

        let res = app.get_with_token("/api/v1/users", &admin).await;

        assert_eq!(res.status, 200);
        let users = res.body.as_array().unwrap();
        assert!(users.len() >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_list_users() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("usr_ls3", "password123", "contestant")
            .await;

        let res = app.get_with_token("/api/v1/users", &user).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unauthenticated_user_cannot_list_users() {
        let app = E2eTestApp::spawn_without_plugins().await;

        let res = app.get_without_token("/api/v1/users").await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }
}

mod user_details {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_get_user_by_id() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_det1", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("usr_det2", "password123")
            .await;
        let user_id = app.get_with_token("/api/v1/auth/me", &user).await.id();

        let res = app
            .get_with_token(&format!("/api/v1/users/{user_id}"), &admin)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["id"], user_id);
        assert_eq!(res.body["username"], "usr_det2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_get_user_details() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("usr_det3", "password123", "contestant")
            .await;

        let res = app.get_with_token("/api/v1/users/1", &user).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nonexistent_user_returns_404() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_det4", "password123", "admin")
            .await;

        let res = app.get_with_token("/api/v1/users/99999", &admin).await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod role_assignment {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_assign_role_to_user() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_role1", "password123", "admin")
            .await;
        let user_token = app
            .create_authenticated_user("usr_role2", "password123")
            .await;
        let user_id = app
            .get_with_token("/api/v1/auth/me", &user_token)
            .await
            .id();

        let res = app
            .post_with_token(
                &format!("/api/v1/users/{user_id}/roles"),
                &json!({"role": "problem_setter"}),
                &admin,
            )
            .await;

        assert_eq!(res.status, 201);

        let user_res = app
            .get_with_token(&format!("/api/v1/users/{user_id}"), &admin)
            .await;
        assert_eq!(user_res.status, 200);
        let roles = user_res.body["roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(roles.contains(&"problem_setter"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_assign_roles() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("usr_role3", "password123", "contestant")
            .await;

        let res = app
            .post_with_token("/api/v1/users/1/roles", &json!({"role": "admin"}), &user)
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn assigning_invalid_role_returns_validation_error() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_role4", "password123", "admin")
            .await;
        let user_token = app
            .create_authenticated_user("usr_role5", "password123")
            .await;
        let user_id = app
            .get_with_token("/api/v1/auth/me", &user_token)
            .await
            .id();

        let res = app
            .post_with_token(
                &format!("/api/v1/users/{user_id}/roles"),
                &json!({"role": "nonexistent_role"}),
                &admin,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_revoke_role_from_user() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("usr_role6", "password123", "admin")
            .await;
        let user_token = app
            .create_authenticated_user("usr_role7", "password123")
            .await;
        let user_id = app
            .get_with_token("/api/v1/auth/me", &user_token)
            .await
            .id();

        let res = app
            .delete_with_token(&format!("/api/v1/users/{user_id}/roles/contestant"), &admin)
            .await;

        assert_eq!(res.status, 204);
    }
}
