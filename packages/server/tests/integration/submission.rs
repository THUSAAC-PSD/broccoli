use crate::common::{TestApp, routes};
use serde_json::json;

/// Create a minimal valid submission payload.
fn valid_submission_body(language: &str) -> serde_json::Value {
    json!({
        "files": [{"filename": "main.cpp", "content": "#include <iostream>\nint main() {}"}],
        "language": language,
    })
}

/// Create a multi-file submission payload.
fn multi_file_submission_body() -> serde_json::Value {
    json!({
        "files": [
            {"filename": "Main.java", "content": "public class Main {}"},
            {"filename": "Helper.java", "content": "public class Helper {}"},
        ],
        "language": "java",
    })
}

mod submission_creation {
    use super::*;

    #[tokio::test]
    async fn user_can_create_a_submission() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["language"], "cpp");
        assert_eq!(res.body["status"], "Pending");
        assert!(res.body["id"].as_i64().is_some());
    }

    #[tokio::test]
    async fn submission_includes_user_info() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["username"], "user1");
        assert_eq!(res.body["problem_title"], "Test Problem");
    }

    #[tokio::test]
    async fn requires_authentication() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_without_token(&routes::problem_submissions(problem_id), &body)
            .await;

        assert_eq!(res.status, 401);
        assert_eq!(res.body["code"], "TOKEN_MISSING");
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_problem() {
        let app = TestApp::spawn().await;
        let user_token = app.create_authenticated_user("user1", "pass1234").await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(&routes::problem_submissions(99999), &body, &user_token)
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn supports_multi_file_submissions() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Java Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = multi_file_submission_body();
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 201);
        let files = res.body["files"].as_array().expect("files should be array");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["filename"], "Main.java");
        assert_eq!(files[1]["filename"], "Helper.java");
    }
}

mod submission_validation {
    use super::*;

    #[tokio::test]
    async fn rejects_empty_files_array() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_empty_filename() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "   ", "content": "code"}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_empty_content() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "main.cpp", "content": ""}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_path_traversal_in_filename() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "../../../etc/passwd", "content": "malicious"}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_forward_slash_in_filename() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "src/main.cpp", "content": "code"}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_backslash_in_filename() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "src\\main.cpp", "content": "code"}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_duplicate_filenames() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [
                {"filename": "main.cpp", "content": "first"},
                {"filename": "main.cpp", "content": "duplicate"},
            ],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(
            res.body["message"]
                .as_str()
                .unwrap()
                .contains("Duplicate filename")
        );
    }

    #[tokio::test]
    async fn rejects_empty_language() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": "main.cpp", "content": "code"}],
            "language": "   ",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_hidden_files() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [{"filename": ".hidden", "content": "code"}],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(res.body["message"].as_str().unwrap().contains("hidden"));
    }
}

mod rate_limiting {
    use super::*;

    #[tokio::test]
    async fn returns_429_when_rate_limit_exceeded() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = valid_submission_body("cpp");

        // Create submissions up to the limit (default is 10)
        for i in 0..10 {
            let res = app
                .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
                .await;
            assert_eq!(res.status, 201, "Submission {} failed", i + 1);
        }

        // The 11th should be rate limited
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 429);
        assert_eq!(res.body["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn rate_limit_is_per_user() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;
        let body = valid_submission_body("cpp");

        // User1 creates 10 submissions
        for _ in 0..10 {
            app.post_with_token(
                &routes::problem_submissions(problem_id),
                &body,
                &user1_token,
            )
            .await;
        }

        // User2 should still be able to submit
        let res = app
            .post_with_token(
                &routes::problem_submissions(problem_id),
                &body,
                &user2_token,
            )
            .await;

        assert_eq!(res.status, 201);
    }
}

mod submission_listing {
    use super::*;

    #[tokio::test]
    async fn user_sees_own_submissions() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.create_submission(problem_id, &user_token, "cpp", "int main() {}")
            .await;

        let res = app.get_with_token(routes::SUBMISSIONS, &user_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["username"], "user1");
    }

    #[tokio::test]
    async fn user_does_not_see_other_users_submissions() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;

        // User1 creates a submission
        app.create_submission(problem_id, &user1_token, "cpp", "int main() {}")
            .await;

        // User2 lists submissions, should not see user1's submission
        let res = app.get_with_token(routes::SUBMISSIONS, &user2_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 0);
    }

    #[tokio::test]
    async fn admin_sees_all_submissions() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;

        app.create_submission(problem_id, &user1_token, "cpp", "int main() {}")
            .await;
        app.create_submission(problem_id, &user2_token, "java", "class Main {}")
            .await;

        let res = app.get_with_token(routes::SUBMISSIONS, &admin_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 2);
    }

    #[tokio::test]
    async fn can_filter_by_problem_id() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem1_id = app.create_problem(&admin_token, "Problem 1").await;
        let problem2_id = app.create_problem(&admin_token, "Problem 2").await;

        app.create_submission(problem1_id, &admin_token, "cpp", "int main() {}")
            .await;
        app.create_submission(problem2_id, &admin_token, "cpp", "int main() {}")
            .await;

        let url = format!("{}?problem_id={}", routes::SUBMISSIONS, problem1_id);
        let res = app.get_with_token(&url, &admin_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["problem_id"], problem1_id);
    }

    #[tokio::test]
    async fn can_filter_by_language() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        app.create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;
        app.create_submission(problem_id, &admin_token, "python", "print('hi')")
            .await;

        let url = format!("{}?language=cpp", routes::SUBMISSIONS);
        let res = app.get_with_token(&url, &admin_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["language"], "cpp");
    }

    #[tokio::test]
    async fn returns_pagination_info() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        for _ in 0..5 {
            app.create_submission(problem_id, &admin_token, "cpp", "int main() {}")
                .await;
        }

        let url = format!("{}?per_page=2", routes::SUBMISSIONS);
        let res = app.get_with_token(&url, &admin_token).await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["pagination"]["per_page"], 2);
        assert_eq!(res.body["pagination"]["total"], 5);
        assert_eq!(res.body["pagination"]["total_pages"], 3);
    }
}

mod submission_detail {
    use super::*;

    #[tokio::test]
    async fn owner_can_view_submission_detail() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let submission_id = app
            .create_submission(problem_id, &user_token, "cpp", "int main() {}")
            .await;

        let res = app
            .get_with_token(&routes::submission(submission_id), &user_token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["id"], submission_id);
        assert!(res.body["files"].as_array().is_some());
    }

    #[tokio::test]
    async fn non_owner_cannot_view_submission() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;
        let submission_id = app
            .create_submission(problem_id, &user1_token, "cpp", "int main() {}")
            .await;

        let res = app
            .get_with_token(&routes::submission(submission_id), &user2_token)
            .await;

        // Returns 404 to prevent enumeration of valid submission IDs
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn admin_can_view_any_submission() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let submission_id = app
            .create_submission(problem_id, &user_token, "cpp", "int main() {}")
            .await;

        let res = app
            .get_with_token(&routes::submission(submission_id), &admin_token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["id"], submission_id);
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_submission() {
        let app = TestApp::spawn().await;
        let user_token = app.create_authenticated_user("user1", "pass1234").await;

        let res = app
            .get_with_token(&routes::submission(99999), &user_token)
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod rejudge {
    use super::*;

    #[tokio::test]
    async fn admin_can_rejudge_submission() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;
        let submission_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;

        let res = app
            .post_with_token(
                &routes::submission_rejudge(submission_id),
                &json!({}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["status"], "Pending");
    }

    #[tokio::test]
    async fn contestant_cannot_rejudge() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let submission_id = app
            .create_submission(problem_id, &user_token, "cpp", "int main() {}")
            .await;

        let res = app
            .post_with_token(
                &routes::submission_rejudge(submission_id),
                &json!({}),
                &user_token,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod contest_submissions {
    use super::*;

    #[tokio::test]
    async fn participant_can_submit_to_contest_problem() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.register_for_contest(contest_id, &user_token).await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &user_token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["contest_id"], contest_id);
    }

    #[tokio::test]
    async fn non_participant_cannot_submit_to_contest() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        // Not registering for contest

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &user_token,
            )
            .await;

        // Returns 404 to prevent enumeration of valid contest/problem IDs
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn cannot_submit_to_problem_not_in_contest() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Not In Contest").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        // Not adding problem to contest

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.register_for_contest(contest_id, &user_token).await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &user_token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn admin_can_submit_without_registration() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn cannot_submit_before_contest_starts() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;

        // Create a contest that starts in the future
        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Future Contest",
                    "description": "Hasn't started yet",
                    "start_time": "2099-01-01T00:00:00Z",
                    "end_time": "2099-12-31T00:00:00Z",
                    "is_public": true,
                    "submissions_visible": false,
                }),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 201);
        let contest_id = res.id();

        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.register_for_contest(contest_id, &user_token).await;

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &user_token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(
            res.body["message"]
                .as_str()
                .unwrap()
                .contains("not started")
        );
    }

    #[tokio::test]
    async fn cannot_submit_after_contest_ends() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;

        // Create a contest that has already ended
        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Past Contest",
                    "description": "Already ended",
                    "start_time": "2020-01-01T00:00:00Z",
                    "end_time": "2020-01-02T00:00:00Z",
                    "is_public": true,
                    "submissions_visible": false,
                }),
                &admin_token,
            )
            .await;
        assert_eq!(res.status, 201);
        let contest_id = res.id();

        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        // Admin (who has contest:manage) can submit without registration
        // but should still be blocked by timing constraint
        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert!(res.body["message"].as_str().unwrap().contains("ended"));
    }
}

mod contest_submission_visibility {
    use super::*;

    #[tokio::test]
    async fn participant_sees_own_submissions_when_visibility_off() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.register_for_contest(contest_id, &user_token).await;

        // Create a contest submission
        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user_token,
        )
        .await;

        // List contest submissions
        let res = app
            .get_with_token(&routes::contest_submissions(contest_id), &user_token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
    }

    #[tokio::test]
    async fn participant_cannot_see_others_when_visibility_off() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;
        app.register_for_contest(contest_id, &user1_token).await;
        app.register_for_contest(contest_id, &user2_token).await;

        // User1 submits
        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user1_token,
        )
        .await;

        // User2 tries to list, should not see user1's submission
        let res = app
            .get_with_token(&routes::contest_submissions(contest_id), &user2_token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 0);
    }

    #[tokio::test]
    async fn participant_sees_all_when_visibility_on() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        // Create contest with submissions_visible = true
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, true)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;
        app.register_for_contest(contest_id, &user1_token).await;
        app.register_for_contest(contest_id, &user2_token).await;

        // User1 submits
        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user1_token,
        )
        .await;

        // User2 can see user1's submission when visibility is on
        let res = app
            .get_with_token(&routes::contest_submissions(contest_id), &user2_token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
    }

    #[tokio::test]
    async fn admin_sees_all_regardless_of_visibility() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        app.register_for_contest(contest_id, &user_token).await;

        // User submits
        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user_token,
        )
        .await;

        // Admin can see all submissions regardless of visibility setting
        let res = app
            .get_with_token(&routes::contest_submissions(contest_id), &admin_token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
    }
}
