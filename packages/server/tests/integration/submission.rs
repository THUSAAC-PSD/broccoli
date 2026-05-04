use crate::common::{TestApp, routes};
use serde_json::json;

fn valid_submission_body(language: &str) -> serde_json::Value {
    json!({
        "files": [{"filename": "main.cpp", "content": "#include <iostream>\nint main() {}"}],
        "language": language,
    })
}

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

        for i in 0..10 {
            let res = app
                .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
                .await;
            assert_eq!(res.status, 201, "Submission {} failed", i + 1);
        }

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

        for _ in 0..10 {
            app.post_with_token(
                &routes::problem_submissions(problem_id),
                &body,
                &user1_token,
            )
            .await;
        }

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

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
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

        app.create_submission(problem_id, &user1_token, "cpp", "int main() {}")
            .await;

        let res = app.get_with_token(routes::SUBMISSIONS, &user2_token).await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
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
        app.create_submission(problem_id, &admin_token, "python3", "print('hi')")
            .await;

        let url = format!("{}?language=cpp", routes::SUBMISSIONS);
        let res = app.get_with_token(&url, &admin_token).await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["language"], "cpp");
    }

    #[tokio::test]
    async fn rejects_unsupported_language() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Test Problem").await;
        let user_token = app.create_authenticated_user("user1", "pass1234").await;

        let body = json!({
            "files": [{"filename": "main.rb", "content": "puts 'hi'"}],
            "language": "ruby",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert_eq!(res.body["message"], "Unsupported language: ruby");
    }

    #[tokio::test]
    async fn rejects_submission_format_filename_mismatch() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Multi-file Problem",
                    "content": "## Description\nSolve this.",
                    "time_limit": 1000,
                    "memory_limit": 262144,
                    "problem_type": "standard",
                    "checker_format": "exact",
                    "submission_format": {
                        "cpp": ["main.cpp", "grader.cpp"]
                    },
                }),
                &admin_token,
            )
            .await;
        assert_eq!(
            problem_res.status, 201,
            "create_problem failed: {}",
            problem_res.text
        );
        let problem_id = problem_res.id();

        let user_token = app.create_authenticated_user("user1", "pass1234").await;
        let body = json!({
            "files": [
                {"filename": "main.cpp", "content": "int main() {}"},
            ],
            "language": "cpp",
        });
        let res = app
            .post_with_token(&routes::problem_submissions(problem_id), &body, &user_token)
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
        assert_eq!(
            res.body["message"],
            "Files for language 'cpp' must exactly match: grader.cpp, main.cpp",
        );
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
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use server::entity::{role_permission, submission};

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

    #[tokio::test]
    async fn rejudge_requires_system_admin_to_clear_worker_pin() {
        let app = TestApp::spawn().await;
        role_permission::ActiveModel {
            role: Set("problem_setter".to_string()),
            permission: Set("submission:rejudge".to_string()),
        }
        .insert(&app.db)
        .await
        .expect("grant rejudge permission");

        let admin_token = app
            .create_user_with_role("admin_clear_pin_single", "pass1234", "admin")
            .await;
        let rejudge_token = app
            .create_user_with_role("setter_clear_pin_single", "pass1234", "problem_setter")
            .await;
        let problem_id = app
            .create_problem(&admin_token, "Pinned Rejudge Problem")
            .await;
        let submission_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::TargetWorkerId,
                sea_orm::sea_query::Expr::value(Some("worker-a".to_string())),
            )
            .filter(submission::Column::Id.eq(submission_id))
            .exec(&app.db)
            .await
            .expect("pin submission");

        let res = app
            .post_with_token(
                &routes::submission_rejudge(submission_id),
                &json!({"target_worker_id": ""}),
                &rejudge_token,
            )
            .await;

        assert_eq!(res.status, 403, "unexpected body: {}", res.body);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");

        let submission = submission::Entity::find_by_id(submission_id)
            .one(&app.db)
            .await
            .expect("load submission")
            .expect("submission exists");
        assert_eq!(submission.target_worker_id.as_deref(), Some("worker-a"));
    }
}

mod judgement_history {
    use super::*;
    use chrono::Utc;
    use common::{SubmissionStatus, Verdict};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use server::entity::{submission, submission_judgement, test_case, test_case_result, user};

    async fn seed_history(app: &TestApp, username: &str, problem_id: i32) -> (i32, i32, i32) {
        let now = Utc::now();
        let user_model = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(&app.db)
            .await
            .expect("query user")
            .expect("user should exist");

        let submission = submission::ActiveModel {
            files: Set(json!([{ "filename": "main.cpp", "content": "int main() {}" }])),
            language: Set("cpp".into()),
            user_id: Set(user_model.id),
            problem_id: Set(problem_id),
            contest_id: Set(None),
            contest_type: Set("standard".into()),
            status: Set(SubmissionStatus::Judged),
            verdict: Set(Some(Verdict::Accepted)),
            score: Set(Some(100.0)),
            time_used: Set(Some(7)),
            memory_used: Set(Some(128)),
            compile_output: Set(Some("ok".to_string())),
            judge_epoch: Set(11),
            created_at: Set(now),
            judged_at: Set(Some(now)),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .expect("insert submission");
        let submission_id = submission.id;

        let old = submission_judgement::ActiveModel {
            submission_id: Set(submission_id),
            version: Set(10),
            is_current: Set(false),
            is_finalized: Set(true),
            triggered_by_user_id: Set(None),
            status: Set(SubmissionStatus::Judged),
            verdict: Set(Some(Verdict::WrongAnswer)),
            score: Set(Some(40.0)),
            judge_epoch: Set(10),
            created_at: Set(now),
            finalized_at: Set(Some(now)),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .expect("insert old judgement");

        let current = submission_judgement::ActiveModel {
            submission_id: Set(submission_id),
            version: Set(11),
            is_current: Set(true),
            is_finalized: Set(true),
            triggered_by_user_id: Set(None),
            status: Set(SubmissionStatus::Judged),
            verdict: Set(Some(Verdict::Accepted)),
            score: Set(Some(100.0)),
            time_used: Set(Some(7)),
            memory_used: Set(Some(128)),
            compile_output: Set(Some("ok".to_string())),
            judge_epoch: Set(11),
            created_at: Set(now),
            finalized_at: Set(Some(now)),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .expect("insert current judgement");

        let test_case = test_case::ActiveModel {
            problem_id: Set(problem_id),
            input: Set("1 2\n".to_string()),
            expected_output: Set("3\n".to_string()),
            score: Set(100),
            label: Set("sum".to_string()),
            is_sample: Set(false),
            position: Set(1),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .expect("insert test case");

        for (judgement_id, verdict, score) in [
            (old.id, Verdict::WrongAnswer, 40.0),
            (current.id, Verdict::Accepted, 100.0),
        ] {
            test_case_result::ActiveModel {
                submission_id: Set(submission_id),
                judgement_id: Set(Some(judgement_id)),
                test_case_id: Set(Some(test_case.id)),
                run_index: Set(None),
                verdict: Set(verdict),
                score: Set(score),
                created_at: Set(now),
                ..Default::default()
            }
            .insert(&app.db)
            .await
            .expect("insert test case result");
        }

        (submission_id, old.id, current.id)
    }

    #[tokio::test]
    async fn get_submission_only_returns_current_judgement_results() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "History Problem").await;
        let (submission_id, _old_judgement_id, current_judgement_id) =
            seed_history(&app, "admin_jhist1", problem_id).await;

        let res = app
            .get_with_token(&routes::submission(submission_id), &admin_token)
            .await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
        let results = res.body["result"]["test_case_results"]
            .as_array()
            .expect("test_case_results should be an array");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["score"], 100.0);

        let db_rows = test_case_result::Entity::find()
            .filter(test_case_result::Column::JudgementId.eq(Some(current_judgement_id)))
            .all(&app.db)
            .await
            .expect("query current rows");
        assert_eq!(db_rows.len(), 1);
    }

    #[tokio::test]
    async fn list_judgements_returns_each_version_with_its_results() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist2", "pass1234", "admin")
            .await;
        let problem_id = app
            .create_problem(&admin_token, "History List Problem")
            .await;
        let (submission_id, _, _) = seed_history(&app, "admin_jhist2", problem_id).await;

        let res = app
            .get_with_token(&routes::submission_judgements(submission_id), &admin_token)
            .await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
        let versions = res.body.as_array().expect("response should be an array");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0]["version"], 10);
        assert_eq!(versions[0]["test_case_results"][0]["score"], 40.0);
        assert_eq!(versions[0]["test_case_results"][0]["input"], "1 2\n");
        assert_eq!(
            versions[0]["test_case_results"][0]["expected_output"],
            "3\n"
        );
        assert_eq!(versions[1]["version"], 11);
        assert_eq!(versions[1]["is_current"], true);
        assert_eq!(versions[1]["test_case_results"][0]["score"], 100.0);
    }

    #[tokio::test]
    async fn deferred_rejudge_preserves_current_cache_and_creates_pending_non_current_version() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist3", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Deferred Problem").await;
        let (submission_id, _old_judgement_id, current_judgement_id) =
            seed_history(&app, "admin_jhist3", problem_id).await;

        let res = app
            .post_with_token(
                &routes::submission_rejudge(submission_id),
                &json!({"apply_immediately": false}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
        assert_eq!(res.body["status"], "Judged");
        assert_eq!(res.body["result"]["verdict"], "Accepted");
        assert_eq!(res.body["judge_epoch"], 11);

        let current = submission_judgement::Entity::find_by_id(current_judgement_id)
            .one(&app.db)
            .await
            .expect("load current judgement")
            .expect("current judgement exists");
        assert!(current.is_current);

        let pending = submission_judgement::Entity::find()
            .filter(submission_judgement::Column::SubmissionId.eq(submission_id))
            .filter(submission_judgement::Column::Version.eq(12))
            .one(&app.db)
            .await
            .expect("load pending judgement")
            .expect("pending judgement exists");
        assert!(!pending.is_current);
        assert_eq!(pending.judge_epoch, 12);
    }

    #[tokio::test]
    async fn apply_finalized_judgement_makes_it_current_and_updates_submission_cache() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist4", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Apply Problem").await;
        let (submission_id, old_judgement_id, _current_judgement_id) =
            seed_history(&app, "admin_jhist4", problem_id).await;

        let res = app
            .post_with_token(
                &routes::submission_judgement_apply(submission_id, old_judgement_id),
                &json!({}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
        assert_eq!(res.body["result"]["verdict"], "WrongAnswer");
        assert_eq!(res.body["result"]["score"], 40.0);

        let applied = submission_judgement::Entity::find_by_id(old_judgement_id)
            .one(&app.db)
            .await
            .expect("load applied judgement")
            .expect("applied judgement exists");
        assert!(applied.is_current);
    }

    #[tokio::test]
    async fn discard_non_current_judgement_deletes_only_that_version_and_results() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist5", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Discard Problem").await;
        let (submission_id, old_judgement_id, current_judgement_id) =
            seed_history(&app, "admin_jhist5", problem_id).await;

        let res = app
            .post_with_token(
                &routes::submission_judgement_discard(submission_id, old_judgement_id),
                &json!({}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 204, "unexpected body: {}", res.body);
        let old = submission_judgement::Entity::find_by_id(old_judgement_id)
            .one(&app.db)
            .await
            .expect("query discarded judgement");
        assert!(old.is_none());

        let current_rows = test_case_result::Entity::find()
            .filter(test_case_result::Column::JudgementId.eq(Some(current_judgement_id)))
            .all(&app.db)
            .await
            .expect("query current rows");
        assert_eq!(current_rows.len(), 1);
    }

    #[tokio::test]
    async fn discard_rejects_pending_non_current_judgement() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_jhist6", "pass1234", "admin")
            .await;
        let problem_id = app
            .create_problem(&admin_token, "Pending Discard Problem")
            .await;
        let (submission_id, _, _) = seed_history(&app, "admin_jhist6", problem_id).await;

        let pending = submission_judgement::ActiveModel {
            submission_id: Set(submission_id),
            version: Set(12),
            is_current: Set(false),
            is_finalized: Set(false),
            triggered_by_user_id: Set(None),
            status: Set(SubmissionStatus::Running),
            judge_epoch: Set(12),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .expect("insert pending judgement");

        let res = app
            .post_with_token(
                &routes::submission_judgement_discard(submission_id, pending.id),
                &json!({}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 409, "unexpected body: {}", res.body);
        let still_exists = submission_judgement::Entity::find_by_id(pending.id)
            .one(&app.db)
            .await
            .expect("query pending judgement")
            .is_some();
        assert!(still_exists);
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
    async fn non_participant_cannot_submit_to_public_contest() {
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

        let body = valid_submission_body("cpp");
        let res = app
            .post_with_token(
                &routes::contest_problem_submissions(contest_id, problem_id),
                &body,
                &user_token,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn non_participant_cannot_submit_to_private_contest() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;
        let contest_id = app
            .create_contest(&admin_token, "Private Contest", false, false)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user_token = app.create_authenticated_user("user1", "pass1234").await;

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
    async fn cannot_submit_to_problem_not_in_contest() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Not In Contest").await;
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, false)
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
    async fn cannot_submit_before_contest_activates() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;

        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Future Contest",
                    "description": "Hasn't activated yet",
                    "activate_time": "2099-01-01T00:00:00Z",
                    "start_time": "2099-01-02T00:00:00Z",
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
    async fn cannot_submit_after_contest_deactivates() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;

        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Past Contest",
                    "description": "Already deactivated",
                    "activate_time": "2020-01-01T00:00:00Z",
                    "start_time": "2020-01-02T00:00:00Z",
                    "end_time": "2020-12-31T00:00:00Z",
                    "deactivate_time": "2020-12-31T00:00:00Z",
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
    async fn cannot_submit_before_contest_starts() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Contest Problem").await;

        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Future Contest",
                    "description": "Hasn't started yet",
                    "activate_time": "2020-01-01T00:00:00Z",
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

        let res = app
            .post_with_token(
                routes::CONTESTS,
                &json!({
                    "title": "Past Contest",
                    "description": "Already ended",
                    "activate_time": "2020-01-01T00:00:00Z",
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

        let user_token = app.create_authenticated_user("user1", "pass1234").await;

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
        assert!(res.body["message"].as_str().unwrap().contains("ended"));
    }
}

mod bulk_rejudge {
    use super::*;

    #[tokio::test]
    async fn admin_can_bulk_rejudge_by_submission_ids() {
        use common::{SubmissionStatus, Verdict};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        use server::entity::submission;

        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_brj1", "pass1234", "admin")
            .await;
        let problem_id = app.create_problem(&admin_token, "Rejudge Problem").await;

        let sub1 = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;
        let sub2 = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() { return 0; }")
            .await;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Judged),
            )
            .col_expr(
                submission::Column::Verdict,
                sea_orm::sea_query::Expr::value(Some(Verdict::WrongAnswer)),
            )
            .filter(submission::Column::Id.is_in(vec![sub1, sub2]))
            .exec(&app.db)
            .await
            .expect("update submission status");

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": [sub1, sub2]}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["queued"], 2);

        let s1 = app
            .get_with_token(&routes::submission(sub1), &admin_token)
            .await;
        assert_ne!(
            s1.body["verdict"], "WrongAnswer",
            "verdict should have been cleared"
        );

        let s2 = app
            .get_with_token(&routes::submission(sub2), &admin_token)
            .await;
        assert_ne!(
            s2.body["verdict"], "WrongAnswer",
            "verdict should have been cleared"
        );
    }

    #[tokio::test]
    async fn returns_validation_error_with_empty_submission_ids() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_brj2", "pass1234", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": []}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn returns_validation_error_for_invalid_submission_id() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_brj3", "pass1234", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": [0]}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn bulk_rejudge_allows_non_terminal_submissions() {
        use common::{SubmissionStatus, Verdict};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        use server::entity::submission;

        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_brj_custom", "pass1234", "admin")
            .await;
        let problem_id = app
            .create_problem(&admin_token, "Custom Verdict Problem")
            .await;

        let terminal_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() { return 0; }")
            .await;
        let pending_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() { return 1; }")
            .await;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Judged),
            )
            .col_expr(
                submission::Column::Verdict,
                sea_orm::sea_query::Expr::value(Some(Verdict::Other(
                    "PartiallyAccepted".to_string(),
                ))),
            )
            .filter(submission::Column::Id.eq(terminal_id))
            .exec(&app.db)
            .await
            .expect("update terminal submission");

        submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending),
            )
            .col_expr(
                submission::Column::Verdict,
                sea_orm::sea_query::Expr::value(Option::<Verdict>::None),
            )
            .filter(submission::Column::Id.eq(pending_id))
            .exec(&app.db)
            .await
            .expect("update pending submission");

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": [terminal_id, pending_id]}),
                &admin_token,
            )
            .await;

        assert_eq!(res.status, 200, "unexpected body: {}", res.body);
        assert_eq!(res.body["queued"], 2);

        let terminal = submission::Entity::find_by_id(terminal_id)
            .one(&app.db)
            .await
            .expect("load terminal submission")
            .expect("terminal submission should exist");
        assert_ne!(
            terminal.verdict,
            Some(Verdict::Other("PartiallyAccepted".to_string())),
            "terminal submission verdict should have been cleared after rejudge dispatch"
        );

        let pending = submission::Entity::find_by_id(pending_id)
            .one(&app.db)
            .await
            .expect("load pending submission")
            .expect("pending submission should exist");
        assert_eq!(pending.status, SubmissionStatus::Pending);
        assert_eq!(pending.verdict, None);
        assert!(
            pending.judge_epoch > 0,
            "pending submission should have been re-dispatched"
        );
    }

    #[tokio::test]
    async fn contestant_cannot_bulk_rejudge() {
        let app = TestApp::spawn().await;
        let admin_token = app
            .create_user_with_role("admin_brj4", "pass1234", "admin")
            .await;
        let contestant_token = app
            .create_authenticated_user("contestant_brj4", "pass1234")
            .await;

        let problem_id = app.create_problem(&admin_token, "Rejudge Problem").await;
        let submission_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": [submission_id]}),
                &contestant_token,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn bulk_rejudge_requires_system_admin_to_clear_worker_pin() {
        use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
        use server::entity::{role_permission, submission};

        let app = TestApp::spawn().await;
        role_permission::ActiveModel {
            role: Set("problem_setter".to_string()),
            permission: Set("submission:rejudge".to_string()),
        }
        .insert(&app.db)
        .await
        .expect("grant rejudge permission");

        let admin_token = app
            .create_user_with_role("admin_clear_pin_bulk", "pass1234", "admin")
            .await;
        let rejudge_token = app
            .create_user_with_role("setter_clear_pin_bulk", "pass1234", "problem_setter")
            .await;
        let problem_id = app
            .create_problem(&admin_token, "Pinned Bulk Rejudge Problem")
            .await;
        let submission_id = app
            .create_submission(problem_id, &admin_token, "cpp", "int main() {}")
            .await;

        submission::Entity::update_many()
            .col_expr(
                submission::Column::TargetWorkerId,
                sea_orm::sea_query::Expr::value(Some("worker-a".to_string())),
            )
            .filter(submission::Column::Id.eq(submission_id))
            .exec(&app.db)
            .await
            .expect("pin submission");

        let res = app
            .post_with_token(
                routes::SUBMISSIONS_BULK_REJUDGE,
                &json!({"submission_ids": [submission_id], "target_worker_id": ""}),
                &rejudge_token,
            )
            .await;

        assert_eq!(res.status, 403, "unexpected body: {}", res.body);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");

        let submission = submission::Entity::find_by_id(submission_id)
            .one(&app.db)
            .await
            .expect("load submission")
            .expect("submission exists");
        assert_eq!(submission.target_worker_id.as_deref(), Some("worker-a"));
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

        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user_token,
        )
        .await;

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

        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user1_token,
        )
        .await;

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
        let contest_id = app
            .create_contest(&admin_token, "Test Contest", true, true)
            .await;
        app.add_problem_to_contest(contest_id, problem_id, &admin_token)
            .await;

        let user1_token = app.create_authenticated_user("user1", "pass1234").await;
        let user2_token = app.create_authenticated_user("user2", "pass1234").await;
        app.register_for_contest(contest_id, &user1_token).await;
        app.register_for_contest(contest_id, &user2_token).await;

        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user1_token,
        )
        .await;

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

        let body = valid_submission_body("cpp");
        app.post_with_token(
            &routes::contest_problem_submissions(contest_id, problem_id),
            &body,
            &user_token,
        )
        .await;

        let res = app
            .get_with_token(&routes::contest_submissions(contest_id), &admin_token)
            .await;

        assert_eq!(res.status, 200);
        let data = res.body["data"].as_array().expect("data should be array");
        assert_eq!(data.len(), 1);
    }
}
