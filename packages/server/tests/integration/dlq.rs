use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::json;

use server::entity::dead_letter_message;

use crate::common::{TestApp, routes};

async fn create_dlq_entry(
    app: &TestApp,
    submission_id: i32,
    message_type: &str,
    resolved: bool,
) -> i32 {
    let now = Utc::now();
    let model = dead_letter_message::ActiveModel {
        message_id: Set(format!("test-job-{}", submission_id)),
        message_type: Set(message_type.to_string()),
        submission_id: Set(Some(submission_id)),
        payload: Set(json!({
            "submission_id": submission_id,
            "job_id": format!("test-job-{}", submission_id),
        })),
        error_code: Set("MAX_RETRIES_EXCEEDED".to_string()),
        error_message: Set("Test error message".to_string()),
        retry_count: Set(3),
        retry_history: Set(json!([
            {"attempt": 1, "error": "Error 1", "timestamp": now.to_rfc3339()},
            {"attempt": 2, "error": "Error 2", "timestamp": now.to_rfc3339()},
            {"attempt": 3, "error": "Error 3", "timestamp": now.to_rfc3339()},
        ])),
        first_failed_at: Set(now),
        created_at: Set(now),
        resolved: Set(resolved),
        resolved_at: Set(if resolved { Some(now) } else { None }),
        ..Default::default()
    };

    let result = model
        .insert(&app.db)
        .await
        .expect("Failed to create DLQ entry");
    result.id
}

mod dlq_listing {
    use super::*;

    #[tokio::test]
    async fn admin_can_list_dlq_messages() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "password123", "admin")
            .await;

        create_dlq_entry(&app, 1, "judge_job", false).await;
        create_dlq_entry(&app, 2, "judge_result", false).await;
        create_dlq_entry(&app, 3, "judge_job", true).await;

        let res = app.get_with_token(routes::DLQ, &admin_token).await;
        assert_eq!(res.status, 200);

        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 3);
        assert!(res.body["pagination"]["total"].as_u64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn can_filter_by_message_type() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin2", "password123", "admin")
            .await;

        create_dlq_entry(&app, 10, "judge_job", false).await;
        create_dlq_entry(&app, 11, "judge_result", false).await;

        let res = app
            .get_with_token(
                &format!("{}?message_type=judge_job", routes::DLQ),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 200);

        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["message_type"], "judge_job");
    }

    #[tokio::test]
    async fn can_filter_by_resolved_status() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin3", "password123", "admin")
            .await;

        create_dlq_entry(&app, 20, "judge_job", false).await;
        create_dlq_entry(&app, 21, "judge_job", true).await;

        // Filter unresolved
        let res = app
            .get_with_token(&format!("{}?resolved=false", routes::DLQ), &admin_token)
            .await;
        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["resolved"], false);

        // Filter resolved
        let res = app
            .get_with_token(&format!("{}?resolved=true", routes::DLQ), &admin_token)
            .await;
        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["resolved"], true);
    }

    #[tokio::test]
    async fn returns_pagination_info() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin4", "password123", "admin")
            .await;

        for i in 0..25 {
            create_dlq_entry(&app, 100 + i, "judge_job", false).await;
        }

        let res = app
            .get_with_token(&format!("{}?per_page=10&page=1", routes::DLQ), &admin_token)
            .await;
        assert_eq!(res.status, 200);

        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(res.body["pagination"]["page"], 1);
        assert_eq!(res.body["pagination"]["per_page"], 10);
        assert!(res.body["pagination"]["total"].as_u64().unwrap() >= 25);
        assert!(res.body["pagination"]["total_pages"].as_u64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn unauthenticated_user_cannot_list_dlq() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::DLQ).await;
        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn contestant_cannot_list_dlq() {
        let app = TestApp::spawn().await;
        let contestant_token = app
            .create_authenticated_user("contestant1", "password123")
            .await;

        let res = app.get_with_token(routes::DLQ, &contestant_token).await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn rejects_invalid_message_type_parameter() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_dir", "password123", "admin")
            .await;

        let res = app
            .get_with_token(
                &format!("{}?message_type=invalid", routes::DLQ),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(
            res.body["message"]
                .as_str()
                .unwrap()
                .contains("Invalid message_type")
        );
    }
}

mod dlq_stats {
    use super::*;

    #[tokio::test]
    async fn admin_can_get_dlq_stats() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin5", "password123", "admin")
            .await;

        create_dlq_entry(&app, 200, "judge_job", false).await;
        create_dlq_entry(&app, 201, "judge_result", false).await;
        create_dlq_entry(&app, 202, "judge_job", true).await;

        let res = app.get_with_token(routes::DLQ_STATS, &admin_token).await;
        assert_eq!(res.status, 200);

        assert!(res.body["total_unresolved"].as_u64().unwrap() >= 2);
        assert!(res.body["total_resolved"].as_u64().unwrap() >= 1);
        assert!(res.body["unresolved_by_message_type"].is_object());
    }

    #[tokio::test]
    async fn unauthenticated_user_cannot_get_stats() {
        let app = TestApp::spawn().await;

        let res = app.get_without_token(routes::DLQ_STATS).await;
        assert_eq!(res.status, 401);
    }

    #[tokio::test]
    async fn contestant_cannot_get_stats() {
        let app = TestApp::spawn().await;
        let contestant_token = app
            .create_authenticated_user("contestant2", "password123")
            .await;

        let res = app
            .get_with_token(routes::DLQ_STATS, &contestant_token)
            .await;
        assert_eq!(res.status, 403);
    }
}

mod dlq_detail {
    use super::*;

    #[tokio::test]
    async fn admin_can_get_dlq_message_detail() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin6", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 300, "judge_job", false).await;

        let res = app
            .get_with_token(&routes::dlq_message(dlq_id), &admin_token)
            .await;
        assert_eq!(res.status, 200);

        assert_eq!(res.body["id"], dlq_id);
        assert_eq!(res.body["submission_id"], 300);
        assert_eq!(res.body["message_type"], "judge_job");
        assert!(res.body["payload"].is_object());
        assert!(res.body["retry_history"].is_array());
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_message() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin7", "password123", "admin")
            .await;

        let res = app
            .get_with_token(&routes::dlq_message(99999), &admin_token)
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn contestant_cannot_get_dlq_detail() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin8", "password123", "admin")
            .await;
        let contestant_token = app
            .create_authenticated_user("contestant3", "password123")
            .await;

        let dlq_id = create_dlq_entry(&app, 301, "judge_job", false).await;

        // Admin can access
        let res = app
            .get_with_token(&routes::dlq_message(dlq_id), &admin_token)
            .await;
        assert_eq!(res.status, 200);

        // Contestant cannot
        let res = app
            .get_with_token(&routes::dlq_message(dlq_id), &contestant_token)
            .await;
        assert_eq!(res.status, 403);
    }
}

mod dlq_resolve {
    use super::*;

    #[tokio::test]
    async fn admin_can_resolve_dlq_message() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin9", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 400, "judge_job", false).await;

        let res = app
            .delete_with_token(&routes::dlq_message(dlq_id), &admin_token)
            .await;
        assert_eq!(res.status, 204);

        // Verify it's now resolved
        let msg = dead_letter_message::Entity::find_by_id(dlq_id)
            .one(&app.db)
            .await
            .expect("DB query failed")
            .expect("Message should exist");
        assert!(msg.resolved);
        assert!(msg.resolved_at.is_some());
    }

    #[tokio::test]
    async fn resolving_already_resolved_message_is_idempotent() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin10", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 401, "judge_job", true).await;

        let res = app
            .delete_with_token(&routes::dlq_message(dlq_id), &admin_token)
            .await;
        assert_eq!(res.status, 204);
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_message() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin11", "password123", "admin")
            .await;

        let res = app
            .delete_with_token(&routes::dlq_message(99998), &admin_token)
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn contestant_cannot_resolve_dlq_message() {
        let app = TestApp::spawn().await;
        let contestant_token = app
            .create_authenticated_user("contestant4", "password123")
            .await;

        let dlq_id = create_dlq_entry(&app, 402, "judge_job", false).await;

        let res = app
            .delete_with_token(&routes::dlq_message(dlq_id), &contestant_token)
            .await;
        assert_eq!(res.status, 403);
    }
}

mod dlq_retry {
    use super::*;

    #[tokio::test]
    async fn cannot_retry_already_resolved_message() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin12", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 500, "judge_job", true).await;

        // Retry handler takes no body; empty JSON is the convention for bodyless POST via post_with_token
        let res = app
            .post_with_token(&routes::dlq_retry(dlq_id), &json!({}), &admin_token)
            .await;
        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn cannot_retry_judge_result_messages() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin13", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 501, "judge_result", false).await;

        let res = app
            .post_with_token(&routes::dlq_retry(dlq_id), &json!({}), &admin_token)
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(
            res.body["message"]
                .as_str()
                .unwrap()
                .contains("judge_result")
        );
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_message() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin14", "password123", "admin")
            .await;

        let res = app
            .post_with_token(&routes::dlq_retry(99997), &json!({}), &admin_token)
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn contestant_cannot_retry_dlq_message() {
        let app = TestApp::spawn().await;
        let contestant_token = app
            .create_authenticated_user("contestant5", "password123")
            .await;

        let dlq_id = create_dlq_entry(&app, 502, "judge_job", false).await;

        let res = app
            .post_with_token(&routes::dlq_retry(dlq_id), &json!({}), &contestant_token)
            .await;
        assert_eq!(res.status, 403);
    }

    #[tokio::test]
    async fn retry_returns_error_when_mq_not_available() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin15", "password123", "admin")
            .await;

        let dlq_id = create_dlq_entry(&app, 503, "judge_job", false).await;

        let res = app
            .post_with_token(&routes::dlq_retry(dlq_id), &json!({}), &admin_token)
            .await;
        assert_eq!(res.status, 500);
        assert_eq!(res.body["code"], "INTERNAL_ERROR");
    }
}
