use serde_json::json;

use crate::common::E2eTestApp;

mod dlq_listing {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_list_dlq_messages_empty() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("dlq_ls1", "password123", "admin")
            .await;

        let res = app.get_with_token("/api/v1/dlq", &admin).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_list_dlq() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("dlq_ls2", "password123", "contestant")
            .await;

        let res = app.get_with_token("/api/v1/dlq", &user).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unauthenticated_user_cannot_list_dlq() {
        let app = E2eTestApp::spawn_without_plugins().await;

        let res = app.get_without_token("/api/v1/dlq").await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }
}

mod dlq_stats {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_get_dlq_stats() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("dlq_st1", "password123", "admin")
            .await;

        let res = app.get_with_token("/api/v1/dlq/stats", &admin).await;

        assert_eq!(res.status, 200);
        assert!(res.body["total_unresolved"].is_number());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_get_dlq_stats() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("dlq_st2", "password123", "contestant")
            .await;

        let res = app.get_with_token("/api/v1/dlq/stats", &user).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod dlq_message_operations {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn get_nonexistent_dlq_message_returns_404() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("dlq_op1", "password123", "admin")
            .await;

        let res = app.get_with_token("/api/v1/dlq/99999", &admin).await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_retry_dlq() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("dlq_op2", "password123", "contestant")
            .await;

        let res = app
            .post_with_token(
                "/api/v1/dlq/bulk-retry",
                &json!({"message_ids": [1]}),
                &user,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_delete_dlq() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let user = app
            .create_user_with_role("dlq_op3", "password123", "contestant")
            .await;

        let res = app
            .delete_with_body_and_token("/api/v1/dlq/bulk", &json!({"message_ids": [1]}), &user)
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}
