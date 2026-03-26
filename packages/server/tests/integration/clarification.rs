use crate::common::{TestApp, routes};
use serde_json::json;

mod clarification_creation {
    use super::*;

    #[tokio::test]
    async fn contestant_can_create_a_question() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let user = app
            .create_user_with_role("user1", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        app.register_for_contest(cid, &user).await;

        let body = json!({
            "content": "Is N <= 1000?",
            "clarification_type": "question",
            "is_public": false
        });

        let res = app
            .post_with_token(&routes::contest_clarifications(cid), &body, &user)
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["content"], "Is N <= 1000?");
        assert_eq!(res.body["author_name"], "user1");
        assert_eq!(res.body["is_public"], false);
    }

    #[tokio::test]
    async fn contestant_cannot_create_announcement_or_dm() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let user = app
            .create_user_with_role("user1", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        // Try announcement
        let body_ann = json!({
            "content": "Hack the server",
            "clarification_type": "announcement",
        });
        let res1 = app
            .post_with_token(&routes::contest_clarifications(cid), &body_ann, &user)
            .await;
        assert_eq!(res1.status, 403);

        // Try DM
        let body_dm = json!({
            "content": "Psst",
            "clarification_type": "direct_message",
            "recipient_id": 1
        });
        let res2 = app
            .post_with_token(&routes::contest_clarifications(cid), &body_dm, &user)
            .await;
        assert_eq!(res2.status, 403);
    }

    #[tokio::test]
    async fn admin_can_create_announcement() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        let body = json!({
            "content": "Contest extended by 10 mins",
            "clarification_type": "announcement"
        });

        let res = app
            .post_with_token(&routes::contest_clarifications(cid), &body, &admin)
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["is_public"], true); // Forced to true by handler
    }
}

mod clarification_visibility {
    use super::*;

    #[tokio::test]
    async fn enforces_visibility_rules_for_questions_and_dms() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let u1 = app
            .create_user_with_role("u1", "pass1234", "contestant")
            .await;
        let u2 = app
            .create_user_with_role("u2", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        app.register_for_contest(cid, &u1).await;
        app.register_for_contest(cid, &u2).await;
        let u2_id = app.get_with_token(routes::ME, &u2).await.id();

        // U1 asks a private question
        app.post_with_token(
            &routes::contest_clarifications(cid),
            &json!({
                "content": "U1 private question",
                "clarification_type": "question"
            }),
            &u1,
        )
        .await;

        // Admin creates a DM for U2
        app.post_with_token(
            &routes::contest_clarifications(cid),
            &json!({
                "content": "DM to U2",
                "clarification_type": "direct_message",
                "recipient_id": u2_id
            }),
            &admin,
        )
        .await;

        // Admin makes a public announcement
        app.post_with_token(
            &routes::contest_clarifications(cid),
            &json!({
                "content": "Public Announcement",
                "clarification_type": "announcement"
            }),
            &admin,
        )
        .await;

        // Admin sees all 3
        let res_admin = app
            .get_with_token(&routes::contest_clarifications(cid), &admin)
            .await;
        assert_eq!(res_admin.body["data"].as_array().unwrap().len(), 3);

        // U1 sees: own question + announcement
        let res_u1 = app
            .get_with_token(&routes::contest_clarifications(cid), &u1)
            .await;
        let data_u1 = res_u1.body["data"].as_array().unwrap();
        assert_eq!(data_u1.len(), 2);
        assert!(
            data_u1
                .iter()
                .any(|c| c["content"] == "U1 private question")
        );
        assert!(
            data_u1
                .iter()
                .any(|c| c["content"] == "Public Announcement")
        );
        assert!(!data_u1.iter().any(|c| c["content"] == "DM to U2")); // Cannot see DM to U2

        // U2 sees: DM + announcement
        let res_u2 = app
            .get_with_token(&routes::contest_clarifications(cid), &u2)
            .await;
        let data_u2 = res_u2.body["data"].as_array().unwrap();
        assert_eq!(data_u2.len(), 2);
        assert!(data_u2.iter().any(|c| c["content"] == "DM to U2"));
        assert!(
            !data_u2
                .iter()
                .any(|c| c["content"] == "U1 private question")
        ); // Cannot see U1's Q
    }

    #[tokio::test]
    async fn public_reply_makes_thread_visible_with_redacted_question() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let u1 = app
            .create_user_with_role("u1", "pass1234", "contestant")
            .await;
        let u2 = app
            .create_user_with_role("u2", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        // U1 asks question
        let q_res = app
            .post_with_token(
                &routes::contest_clarifications(cid),
                &json!({
                    "content": "Secret cheat code?",
                    "clarification_type": "question"
                }),
                &u1,
            )
            .await;
        let clar_id = q_res.id();

        // Admin replies (initially private)
        let rep_res = app
            .post_with_token(
                &routes::contest_clarification_reply(cid, clar_id),
                &json!({
                    "content": "No.",
                    "is_public": false
                }),
                &admin,
            )
            .await;
        assert_eq!(rep_res.status, 200);

        let reply_id = rep_res.body["replies"][0]["id"].as_i64().unwrap() as i32;

        // U2 shouldn't see it yet
        let res_u2_before = app
            .get_with_token(&routes::contest_clarifications(cid), &u2)
            .await;
        assert_eq!(res_u2_before.body["data"].as_array().unwrap().len(), 0);

        // Admin toggles reply to public, BUT does NOT include question (include_question=false default)
        app.post_with_token(
            &routes::contest_clarification_toggle(cid, clar_id, reply_id),
            &json!({}),
            &admin,
        )
        .await;

        // U2 should now see the thread, but question is redacted
        let res_u2_after = app
            .get_with_token(&routes::contest_clarifications(cid), &u2)
            .await;
        let data = res_u2_after.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);

        let thread = &data[0];
        assert_eq!(thread["content"], "", "Question content should be redacted");
        assert_eq!(
            thread["author_name"], "Anonymous",
            "Author should be hidden"
        );
        assert_eq!(
            thread["replies"][0]["content"], "No.",
            "Reply content visible"
        );
    }
}

mod clarification_actions {
    use super::*;

    #[tokio::test]
    async fn author_can_resolve_and_reopen_thread() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let u1 = app
            .create_user_with_role("u1", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        let q_res = app
            .post_with_token(
                &routes::contest_clarifications(cid),
                &json!({
                    "content": "Help?",
                    "clarification_type": "question"
                }),
                &u1,
            )
            .await;
        let clar_id = q_res.id();

        // Resolve
        let res = app
            .post_with_token(
                &routes::contest_clarification_resolve(cid, clar_id),
                &json!({"resolved": true}),
                &u1,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["resolved"], true);
        assert_eq!(res.body["resolved_by_name"], "u1");

        // Reopen
        let res = app
            .post_with_token(
                &routes::contest_clarification_resolve(cid, clar_id),
                &json!({"resolved": false}),
                &u1,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["resolved"], false);
        assert_eq!(res.body["resolved_by_name"], json!(null));
    }

    #[tokio::test]
    async fn non_author_cannot_reply_or_resolve() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let u1 = app
            .create_user_with_role("u1", "pass1234", "contestant")
            .await;
        let u2 = app
            .create_user_with_role("u2", "pass1234", "contestant")
            .await;
        let cid = app.create_contest(&admin, "C1", true, false).await;

        let q_res = app
            .post_with_token(
                &routes::contest_clarifications(cid),
                &json!({
                    "content": "Help?",
                    "clarification_type": "question"
                }),
                &u1,
            )
            .await;
        let clar_id = q_res.id();

        // U2 tries to reply
        let rep_res = app
            .post_with_token(
                &routes::contest_clarification_reply(cid, clar_id),
                &json!({
                    "content": "I know!",
                    "is_public": false
                }),
                &u2,
            )
            .await;
        assert_eq!(rep_res.status, 403);

        // U2 tries to resolve
        let res_res = app
            .post_with_token(
                &routes::contest_clarification_resolve(cid, clar_id),
                &json!({"resolved": true}),
                &u2,
            )
            .await;
        assert_eq!(res_res.status, 403);
    }
}
