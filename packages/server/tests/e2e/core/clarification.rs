use serde_json::json;

use crate::common::E2eTestApp;

mod clarification_creation {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn participant_can_create_a_question() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_cr1", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_cr2", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Clar Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "Is the input guaranteed to be sorted?",
                    "clarification_type": "question",
                }),
                &user,
            )
            .await;

        assert_eq!(res.status, 201);
        assert!(res.body["id"].is_number());
        assert_eq!(res.body["content"], "Is the input guaranteed to be sorted?");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_participant_cannot_create_clarification_in_private_contest() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_cr3", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_cr4", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Private Clar Contest", false, false)
            .await;

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "Hello?",
                    "clarification_type": "question",
                }),
                &user,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_content_returns_validation_error() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_cr5", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_cr6", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Clar Val Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "   ",
                    "clarification_type": "question",
                }),
                &user,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }
}

mod clarification_reply {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_reply_to_clarification() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_rp1", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_rp2", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Reply Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        let clar = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "What is the time limit?",
                    "clarification_type": "question",
                }),
                &user,
            )
            .await;
        assert_eq!(clar.status, 201);
        let clar_id = clar.id();

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications/{clar_id}/reply"),
                &json!({
                    "content": "The time limit is 2 seconds.",
                    "is_public": true,
                }),
                &admin,
            )
            .await;

        assert_eq!(res.status, 200);
        let replies = res.body["replies"].as_array().unwrap();
        assert!(!replies.is_empty());
        assert_eq!(
            replies.last().unwrap()["content"],
            "The time limit is 2 seconds."
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn participant_cannot_reply_to_others_clarification() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_rp3", "password123", "admin")
            .await;
        let user1 = app
            .create_authenticated_user("clar_rp4", "password123")
            .await;
        let user2 = app
            .create_authenticated_user("clar_rp5", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Reply Perm Contest", true, false)
            .await;
        app.register_for_contest(cid, &user1).await;
        app.register_for_contest(cid, &user2).await;

        let clar = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "My question",
                    "clarification_type": "question",
                }),
                &user1,
            )
            .await;
        let clar_id = clar.id();

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications/{clar_id}/reply"),
                &json!({
                    "content": "I should not be able to reply",
                    "is_public": false,
                }),
                &user2,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod clarification_listing {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn participant_can_list_clarifications() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_ls1", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_ls2", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "List Clar Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        app.post_with_token(
            &format!("/api/v1/contests/{cid}/clarifications"),
            &json!({
                "content": "Question one",
                "clarification_type": "question",
            }),
            &user,
        )
        .await;
        app.post_with_token(
            &format!("/api/v1/contests/{cid}/clarifications"),
            &json!({
                "content": "Question two",
                "clarification_type": "question",
            }),
            &user,
        )
        .await;

        let res = app
            .get_with_token(&format!("/api/v1/contests/{cid}/clarifications"), &user)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert!(data.len() >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_list_all_clarifications() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_ls3", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_ls4", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Admin Clar Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        app.post_with_token(
            &format!("/api/v1/contests/{cid}/clarifications"),
            &json!({
                "content": "User question",
                "clarification_type": "question",
            }),
            &user,
        )
        .await;

        let res = app
            .get_with_token(&format!("/api/v1/contests/{cid}/clarifications"), &admin)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert!(!data.is_empty());
    }
}

mod clarification_resolve {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_resolve_clarification() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("clar_res1", "password123", "admin")
            .await;
        let user = app
            .create_authenticated_user("clar_res2", "password123")
            .await;

        let cid = app
            .create_contest(&admin, "Resolve Contest", true, false)
            .await;
        app.register_for_contest(cid, &user).await;

        let clar = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications"),
                &json!({
                    "content": "Resolve me",
                    "clarification_type": "question",
                }),
                &user,
            )
            .await;
        let clar_id = clar.id();

        let res = app
            .post_with_token(
                &format!("/api/v1/contests/{cid}/clarifications/{clar_id}/resolve"),
                &json!({"resolved": true}),
                &admin,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["resolved"], true);
    }
}
