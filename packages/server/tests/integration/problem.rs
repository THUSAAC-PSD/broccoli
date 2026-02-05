use serde_json::json;

use crate::common::{TestApp, routes};

/// Build a ZIP archive in memory with given file entries.
fn build_zip(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut writer = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in files {
        writer.start_file(*name, options).expect("zip start_file");
        writer.write_all(content.as_bytes()).expect("zip write_all");
    }
    let cursor = writer.finish().expect("zip finish");
    cursor.into_inner()
}

/// Insert a submission row directly into the DB for a given problem.
async fn insert_submission_for_problem(app: &TestApp, problem_id: i32) {
    use sea_orm::{ActiveModelTrait, Set};
    use server::entity::submission;

    let files = serde_json::json!([{"filename": "main.cpp", "content": "int main() {}"}]);
    let sub = submission::ActiveModel {
        problem_id: Set(problem_id),
        user_id: Set(1),
        language: Set("cpp".into()),
        files: Set(files),
        status: Set(common::SubmissionStatus::Pending),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };
    sub.insert(&app.db).await.expect("insert submission");
}

/// Insert a contest and link it to a problem via contest_problem.
async fn insert_contest_association_for_problem(app: &TestApp, problem_id: i32) {
    use sea_orm::{ActiveModelTrait, Set};
    use server::entity::{contest, contest_problem};

    let now = chrono::Utc::now();
    let c = contest::ActiveModel {
        title: Set("Test Contest".into()),
        description: Set("A test contest".into()),
        start_time: Set(now),
        end_time: Set(now + chrono::Duration::hours(3)),
        is_public: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let contest_model = c.insert(&app.db).await.expect("insert contest");

    let cp = contest_problem::ActiveModel {
        contest_id: Set(contest_model.id),
        problem_id: Set(problem_id),
        label: Set("A".into()),
        position: Set(0),
    };
    cp.insert(&app.db).await.expect("insert contest_problem");
}

mod problem_creation {
    use super::*;

    #[tokio::test]
    async fn admin_can_create_a_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin1", "password123", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Two Sum",
                    "content": "Find two numbers that sum to target.",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["title"], "Two Sum");
        assert!(res.body["id"].is_number());
        assert!(res.body["created_at"].is_string());
        assert!(res.body["updated_at"].is_string());
    }

    #[tokio::test]
    async fn problem_setter_can_create_a_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("setter1", "password123", "problem_setter")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Array Max",
                    "content": "Find the maximum.",
                    "time_limit": 2000,
                    "memory_limit": 131072
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn contestant_cannot_create_a_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("contestant1", "password123", "contestant")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Nope",
                    "content": "Should fail.",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn cannot_create_a_problem_with_invalid_data() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin2", "password123", "admin")
            .await;

        // Empty title
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "   ",
                    "content": "Some content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");

        // Time limit out of range
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Valid",
                    "content": "Some content",
                    "time_limit": 0,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn create_problem_trims_title_whitespace() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin47", "password123", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "  Padded Title  ",
                    "content": "Some content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["title"], "Padded Title");
    }
}

mod problem_listing {
    use super::*;

    #[tokio::test]
    async fn list_returns_paginated_results() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin3", "password123", "admin")
            .await;

        for i in 0..3 {
            app.post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": format!("Problem {i}"),
                    "content": "Content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;
        }

        let res = app
            .get_with_token(&format!("{}?per_page=2", routes::PROBLEMS), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["data"].as_array().unwrap().len(), 2);
        assert_eq!(res.body["pagination"]["total"], 3);
        assert_eq!(res.body["pagination"]["total_pages"], 2);
    }

    #[tokio::test]
    async fn list_can_filter_problems_by_title_search() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin4", "password123", "admin")
            .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Binary Search",
                "content": "Implement binary search.",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Two Sum",
                "content": "Find pairs.",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        let res = app
            .get_with_token(&format!("{}?search=binary", routes::PROBLEMS), &token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["title"], "Binary Search");
    }

    #[tokio::test]
    async fn search_escapes_like_wildcard_characters() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin30", "password123", "admin")
            .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "100% Done",
                "content": "Content",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Totally Different",
                "content": "Content",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        let res = app
            .get_with_token(&format!("{}?search=100%25", routes::PROBLEMS), &token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["title"], "100% Done");
    }

    #[tokio::test]
    async fn list_rejects_invalid_sort_by() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin31", "password123", "admin")
            .await;

        let res = app
            .get_with_token(&format!("{}?sort_by=nonexistent", routes::PROBLEMS), &token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn list_can_sort_problems_by_title() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin5", "password123", "admin")
            .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Zebra",
                "content": "Z problem.",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        app.post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Apple",
                "content": "A problem.",
                "time_limit": 1000,
                "memory_limit": 262144
            }),
            &token,
        )
        .await;

        let res = app
            .get_with_token(
                &format!("{}?sort_by=title&sort_order=asc", routes::PROBLEMS),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().unwrap();
        assert_eq!(data[0]["title"], "Apple");
        assert_eq!(data[1]["title"], "Zebra");
    }

    #[tokio::test]
    async fn contestant_cannot_list_problems() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("contestant2", "password123", "contestant")
            .await;

        let res = app.get_with_token(routes::PROBLEMS, &token).await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod problem_detail {
    use super::*;

    #[tokio::test]
    async fn can_retrieve_a_problem_with_full_content() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin7", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;

        let res = app.get_with_token(&routes::problem(id), &token).await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["id"], id);
        assert!(
            res.body["content"].is_string(),
            "Detail should include content"
        );
    }

    #[tokio::test]
    async fn cannot_retrieve_a_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin8", "password123", "admin")
            .await;

        let res = app.get_with_token(&routes::problem(99999), &token).await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod problem_update {
    use super::*;

    #[tokio::test]
    async fn can_partially_update_a_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin9", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;

        let res = app
            .patch_with_token(
                &routes::problem(id),
                &json!({ "title": "Updated Title" }),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["title"], "Updated Title");
        // Other fields should remain unchanged
        assert_eq!(res.body["time_limit"], 1000);
    }

    #[tokio::test]
    async fn cannot_update_a_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin37", "password123", "admin")
            .await;

        let res = app
            .patch_with_token(
                &routes::problem(99999),
                &json!({ "title": "Ghost" }),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn contestant_cannot_update_a_problem() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin10", "password123", "admin")
            .await;
        let contestant_token = app
            .create_user_with_role("contestant3", "password123", "contestant")
            .await;

        let id = app.create_problem(&admin_token, "Test Problem").await;

        let res = app
            .patch_with_token(
                &routes::problem(id),
                &json!({ "title": "Hacked" }),
                &contestant_token,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn empty_patch_body_returns_unchanged_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin45", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;

        let original = app.get_with_token(&routes::problem(id), &token).await;
        assert_eq!(original.status, 200);

        let res = app
            .patch_with_token(&routes::problem(id), &json!({}), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["title"], original.body["title"]);
        assert_eq!(res.body["time_limit"], original.body["time_limit"]);
        assert_eq!(res.body["updated_at"], original.body["updated_at"]);
    }
}

mod problem_deletion {
    use super::*;

    #[tokio::test]
    async fn admin_can_delete_a_problem_and_its_test_cases() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin11", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;
        app.create_test_case(id, &token).await;

        let res = app.delete_with_token(&routes::problem(id), &token).await;

        assert_eq!(res.status, 204);

        // Confirm it's gone
        let get_res = app.get_with_token(&routes::problem(id), &token).await;
        assert_eq!(get_res.status, 404);
    }

    #[tokio::test]
    async fn problem_setter_cannot_delete_a_problem() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin12", "password123", "admin")
            .await;
        let setter_token = app
            .create_user_with_role("setter2", "password123", "problem_setter")
            .await;

        let id = app.create_problem(&admin_token, "Test Problem").await;

        let res = app
            .delete_with_token(&routes::problem(id), &setter_token)
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn cannot_delete_a_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin36", "password123", "admin")
            .await;

        let res = app.delete_with_token(&routes::problem(99999), &token).await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn cannot_delete_a_problem_that_has_submissions() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin13", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;
        insert_submission_for_problem(&app, id).await;

        let res = app.delete_with_token(&routes::problem(id), &token).await;

        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn cannot_delete_a_problem_associated_with_a_contest() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin46", "password123", "admin")
            .await;

        let id = app.create_problem(&token, "Test Problem").await;
        insert_contest_association_for_problem(&app, id).await;

        let res = app.delete_with_token(&routes::problem(id), &token).await;

        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "CONFLICT");
    }
}

mod test_case_creation {
    use super::*;

    #[tokio::test]
    async fn can_create_a_test_case_for_a_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin14", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "3\n1 2 3",
                    "expected_output": "6",
                    "score": 10,
                    "is_sample": true
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["score"], 10);
        assert_eq!(res.body["is_sample"], true);
        assert_eq!(res.body["problem_id"], pid);
    }

    #[tokio::test]
    async fn cannot_create_a_test_case_for_a_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin17", "password123", "admin")
            .await;

        let res = app
            .post_with_token(
                &routes::test_cases(99999),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 0,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn rejects_test_case_with_negative_score() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin40", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": -1,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_test_case_with_negative_position() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin42", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 5,
                    "is_sample": false,
                    "position": -1
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn position_is_auto_assigned_when_omitted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin24", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res1 = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 5,
                    "is_sample": true
                }),
                &token,
            )
            .await;
        let res2 = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "2",
                    "expected_output": "2",
                    "score": 5,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        let pos1 = res1.body["position"].as_i64().unwrap();
        let pos2 = res2.body["position"].as_i64().unwrap();
        assert!(
            pos2 > pos1,
            "Second test case should have a higher position"
        );
    }
}

mod test_case_listing {
    use super::*;

    #[tokio::test]
    async fn list_returns_previews_without_full_data() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin18", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        app.create_test_case(pid, &token).await;

        let res = app.get_with_token(&routes::test_cases(pid), &token).await;

        assert_eq!(res.status, 200);
        let items = res.body.as_array().unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert!(item.get("input_preview").is_some());
        assert!(item.get("output_preview").is_some());
        assert!(
            item.get("input").is_none(),
            "Full input should not be in list"
        );
        assert!(
            item.get("expected_output").is_none(),
            "Full output should not be in list"
        );
    }

    #[tokio::test]
    async fn list_test_cases_for_nonexistent_problem_returns_404() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin43", "password123", "admin")
            .await;

        let res = app.get_with_token(&routes::test_cases(99999), &token).await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn long_input_is_truncated_in_preview() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin19", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let long_input = "x".repeat(200);
        app.post_with_token(
            &routes::test_cases(pid),
            &json!({
                "input": long_input,
                "expected_output": "result",
                "score": 5,
                "is_sample": false
            }),
            &token,
        )
        .await;

        let res = app.get_with_token(&routes::test_cases(pid), &token).await;

        assert_eq!(res.status, 200);
        let preview = res.body[0]["input_preview"].as_str().unwrap();
        assert!(preview.ends_with("..."));
        // 100 data chars + "..." = 103 max
        assert!(preview.len() <= 103);
    }

    #[tokio::test]
    async fn unicode_input_is_truncated_at_character_boundary() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin51", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        // 200 multi-byte characters (each is 3 bytes in UTF-8)
        let unicode_input: String = std::iter::repeat('あ').take(200).collect();
        app.post_with_token(
            &routes::test_cases(pid),
            &json!({
                "input": unicode_input,
                "expected_output": "ok",
                "score": 5,
                "is_sample": false
            }),
            &token,
        )
        .await;

        let res = app.get_with_token(&routes::test_cases(pid), &token).await;
        assert_eq!(res.status, 200);

        let preview = res.body[0]["input_preview"].as_str().unwrap();
        assert!(preview.ends_with("..."));
        // Should be exactly 100 Unicode chars + "..."
        assert_eq!(preview.chars().count(), 103);
    }
}

mod test_case_detail {
    use super::*;

    #[tokio::test]
    async fn can_retrieve_full_test_case_data() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin20", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid, &token).await;

        let res = app
            .get_with_token(&routes::test_case(pid, tc_id), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["id"], tc_id);
        assert!(res.body["input"].is_string());
        assert!(res.body["expected_output"].is_string());
    }

    #[tokio::test]
    async fn cannot_access_a_test_case_via_the_wrong_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin21", "password123", "admin")
            .await;

        let pid1 = app.create_problem(&token, "Test Problem").await;
        let pid2 = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid1, &token).await;

        // Access test case of problem 1 via problem 2's URL
        let res = app
            .get_with_token(&routes::test_case(pid2, tc_id), &token)
            .await;

        assert_eq!(res.status, 404);
    }
}

mod test_case_update {
    use super::*;

    #[tokio::test]
    async fn can_partially_update_a_test_case() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin22", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid, &token).await;

        let res = app
            .patch_with_token(
                &routes::test_case(pid, tc_id),
                &json!({ "score": 20 }),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["score"], 20);
        // Other fields unchanged
        assert_eq!(res.body["is_sample"], true);
    }

    #[tokio::test]
    async fn cannot_update_a_nonexistent_test_case() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin38", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res = app
            .patch_with_token(
                &routes::test_case(pid, 99999),
                &json!({ "score": 50 }),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn can_set_description_to_null_via_patch() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin33", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        // Create test case with a description
        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 5,
                    "is_sample": false,
                    "description": "original desc"
                }),
                &token,
            )
            .await;
        assert_eq!(res.status, 201);
        let tc_id = res.id();
        assert_eq!(res.body["description"], "original desc");

        // Set description to null
        let res = app
            .patch_with_token(
                &routes::test_case(pid, tc_id),
                &json!({ "description": null }),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert!(res.body["description"].is_null());
    }
}

mod test_case_deletion {
    use super::*;

    #[tokio::test]
    async fn can_delete_a_test_case() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin23", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid, &token).await;

        let res = app
            .delete_with_token(&routes::test_case(pid, tc_id), &token)
            .await;

        assert_eq!(res.status, 204);

        // Confirm it's gone
        let get_res = app
            .get_with_token(&routes::test_case(pid, tc_id), &token)
            .await;
        assert_eq!(get_res.status, 404);
    }

    #[tokio::test]
    async fn delete_is_blocked_by_judge_results() {
        use common::SubmissionStatus;
        use sea_orm::{ActiveModelTrait, Set};
        use server::entity::{judge_result, submission, test_case_result};

        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin_del_blocked", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid, &token).await;

        let me = app.get_with_token(routes::ME, &token).await;
        let user_id = me.id();

        let now = chrono::Utc::now();

        let files = serde_json::json!([{"filename": "main.rs", "content": "fn main() {}"}]);
        let sub = submission::ActiveModel {
            files: Set(files),
            language: Set("rust".into()),
            status: Set(SubmissionStatus::Accepted),
            user_id: Set(user_id),
            problem_id: Set(pid),
            created_at: Set(now),
            ..Default::default()
        };
        let sub_model = sub
            .insert(&app.db)
            .await
            .expect("Failed to insert submission");

        let jr = judge_result::ActiveModel {
            verdict: Set(SubmissionStatus::Accepted),
            score: Set(100),
            time_used: Set(50),
            memory_used: Set(1024),
            submission_id: Set(sub_model.id),
            created_at: Set(now),
            ..Default::default()
        };
        let jr_model = jr
            .insert(&app.db)
            .await
            .expect("Failed to insert judge result");

        let tcr = test_case_result::ActiveModel {
            verdict: Set(SubmissionStatus::Accepted),
            score: Set(10),
            time_used: Set(50),
            memory_used: Set(1024),
            judge_result_id: Set(jr_model.id),
            test_case_id: Set(tc_id),
            created_at: Set(now),
            ..Default::default()
        };
        tcr.insert(&app.db)
            .await
            .expect("Failed to insert test case result");

        let res = app
            .delete_with_token(&routes::test_case(pid, tc_id), &token)
            .await;
        assert_eq!(res.status, 409);
        assert_eq!(res.body["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn cannot_delete_a_test_case_via_the_wrong_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin34", "password123", "admin")
            .await;

        let pid1 = app.create_problem(&token, "Test Problem").await;
        let pid2 = app.create_problem(&token, "Test Problem").await;
        let tc_id = app.create_test_case(pid1, &token).await;

        // Try to delete pid1's test case via pid2's URL
        let res = app
            .delete_with_token(&routes::test_case(pid2, tc_id), &token)
            .await;

        assert_eq!(res.status, 404);

        // Original test case should still exist
        let get_res = app
            .get_with_token(&routes::test_case(pid1, tc_id), &token)
            .await;
        assert_eq!(get_res.status, 200);
    }
}

mod test_case_reorder {
    use super::*;

    #[tokio::test]
    async fn can_reorder_test_cases() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin52", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc1 = app.create_test_case(pid, &token).await;
        let tc2 = app.create_test_case(pid, &token).await;
        let tc3 = app.create_test_case(pid, &token).await;

        // Reorder: tc3, tc1, tc2
        let body = json!({"test_case_ids": [tc3, tc1, tc2]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &token)
            .await;
        assert_eq!(res.status, 204);

        // Verify new positions
        let list = app.get_with_token(&routes::test_cases(pid), &token).await;
        assert_eq!(list.status, 200);
        let data = list.body.as_array().unwrap();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0]["id"], tc3);
        assert_eq!(data[0]["position"], 0);
        assert_eq!(data[1]["id"], tc1);
        assert_eq!(data[1]["position"], 1);
        assert_eq!(data[2]["id"], tc2);
        assert_eq!(data[2]["position"], 2);
    }

    #[tokio::test]
    async fn reorder_rejects_missing_test_case_ids() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin53", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc1 = app.create_test_case(pid, &token).await;
        let _tc2 = app.create_test_case(pid, &token).await;

        // Only include tc1, omit tc2
        let body = json!({"test_case_ids": [tc1]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &token)
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn reorder_rejects_extra_test_case_ids() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin54", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc1 = app.create_test_case(pid, &token).await;

        // Include tc1 + a non-existent test case ID
        let body = json!({"test_case_ids": [tc1, 99999]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &token)
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn reorder_rejects_duplicate_ids() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin55", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;
        let tc1 = app.create_test_case(pid, &token).await;

        let body = json!({"test_case_ids": [tc1, tc1]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &token)
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn reorder_rejects_empty_list() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin56", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let body = json!({"test_case_ids": []});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &token)
            .await;
        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn reorder_returns_not_found_for_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin57", "password123", "admin")
            .await;

        let body = json!({"test_case_ids": [1]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(99999), &body, &token)
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn contestant_cannot_reorder_test_cases() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin58", "password123", "admin")
            .await;
        let contestant_token = app
            .create_user_with_role("contestant58", "password123", "contestant")
            .await;

        let pid = app.create_problem(&admin_token, "Test Problem").await;
        let tc1 = app.create_test_case(pid, &admin_token).await;

        let body = json!({"test_case_ids": [tc1]});
        let res = app
            .put_with_token(&routes::test_cases_reorder(pid), &body, &contestant_token)
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod test_case_zip_upload {
    use super::*;

    #[tokio::test]
    async fn can_upload_test_cases_from_a_flat_zip() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin25", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let zip_data = build_zip(&[
            ("01.in", "1 2\n"),
            ("01.ans", "3\n"),
            ("02.in", "10 20\n"),
            ("02.ans", "30\n"),
        ]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 2);
        let tcs = res.body["test_cases"].as_array().unwrap();
        assert_eq!(tcs.len(), 2);
        // Flat files should default to is_sample = false
        assert_eq!(tcs[0]["is_sample"], false);
    }

    #[tokio::test]
    async fn can_upload_with_sample_and_main_directories() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin26", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let zip_data = build_zip(&[
            ("sample/01.in", "sample input\n"),
            ("sample/01.ans", "sample output\n"),
            ("main/01.in", "main input\n"),
            ("main/01.ans", "main output\n"),
        ]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 2);

        let tcs = res.body["test_cases"].as_array().unwrap();
        let sample = tcs.iter().find(|tc| tc["is_sample"] == true);
        let main_tc = tcs.iter().find(|tc| tc["is_sample"] == false);
        assert!(sample.is_some(), "Should have a sample test case");
        assert!(main_tc.is_some(), "Should have a main test case");
    }

    #[tokio::test]
    async fn upload_rejects_zip_with_unmatched_files() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin27", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let zip_data = build_zip(&[
            ("01.in", "input\n"),
            // Missing 01.ans
        ]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn can_upload_test_cases_with_dot_out_extension() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin35", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        // Use .out instead of .ans
        let zip_data = build_zip(&[("01.in", "input1\n"), ("01.out", "output1\n")]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 1);
    }

    #[tokio::test]
    async fn upload_rejects_non_zip_file() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin28", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "not-a-zip.txt",
                b"this is not a zip file".to_vec(),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cannot_upload_test_cases_to_a_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin29", "password123", "admin")
            .await;

        let zip_data = build_zip(&[("01.in", "input\n"), ("01.ans", "output\n")]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(99999),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn upload_rejects_zip_with_both_ans_and_out_for_same_stem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin50", "password123", "admin")
            .await;

        let pid = app.create_problem(&token, "Test Problem").await;

        let zip_data = build_zip(&[
            ("01.in", "input\n"),
            ("01.ans", "answer\n"),
            ("01.out", "output\n"),
        ]);

        let res = app
            .upload_with_token(
                &routes::test_cases_upload(pid),
                "tests.zip",
                zip_data,
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(res.body["message"].as_str().unwrap().contains("Duplicate"));
    }
}
