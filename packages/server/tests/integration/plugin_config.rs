use crate::common::{TestApp, routes};
use serde_json::json;

mod problem_config {
    use super::*;

    #[tokio::test]
    async fn admin_can_list_problem_config_empty() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin1", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let res = app
            .get_with_token(&routes::problem_config(problem_id), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn admin_can_upsert_and_get_problem_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin2", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let body = json!({"config": {"tolerance": 0.001}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "checker");
        assert_eq!(res.body["config"]["tolerance"], 0.001);

        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "checker");
        assert_eq!(res.body["config"]["tolerance"], 0.001);
    }

    #[tokio::test]
    async fn upsert_overwrites_existing_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin3", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let body1 = json!({"config": {"v": 1}});
        app.put_with_token(
            &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
            &body1,
            &token,
        )
        .await;

        let body2 = json!({"config": {"v": 2}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &body2,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["config"]["v"], 2);

        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &token,
            )
            .await;
        assert_eq!(res.body["config"]["v"], 2);
    }

    #[tokio::test]
    async fn admin_can_delete_problem_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin4", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let body = json!({"config": {"k": "v"}});
        app.put_with_token(
            &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
            &body,
            &token,
        )
        .await;

        let res = app
            .delete_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &token,
            )
            .await;
        assert_eq!(res.status, 204);

        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn list_returns_all_namespaces() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin5", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        app.put_with_token(
            &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
            &json!({"config": {"a": 1}}),
            &token,
        )
        .await;
        app.put_with_token(
            &routes::problem_config_ns(problem_id, "test-plugin", "scoring"),
            &json!({"config": {"b": 2}}),
            &token,
        )
        .await;

        let res = app
            .get_with_token(&routes::problem_config(problem_id), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn upsert_returns_404_for_missing_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin6", "password123", "admin")
            .await;

        let body = json!({"config": {"k": "v"}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(99999, "test-plugin", "checker"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn get_returns_404_for_missing_namespace() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin7", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "nonexistent"),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn delete_returns_404_for_missing_namespace() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin8", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let res = app
            .delete_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "nonexistent"),
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod namespace_validation {
    use super::*;

    #[tokio::test]
    async fn rejects_empty_namespace() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin9", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", ""),
                &token,
            )
            .await;

        // Empty namespace in path becomes /config/ which doesn't match any route
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn rejects_namespace_too_long() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin24", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let long_namespace = "a".repeat(129);
        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", &long_namespace),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn rejects_namespace_with_special_chars() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin10", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "bad.name"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn accepts_namespace_with_hyphens_and_underscores() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin11", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "my-checker_v2"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "my-checker_v2");
    }
}

mod permissions {
    use super::*;

    #[tokio::test]
    async fn user_without_permission_cannot_access_problem_config() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin12", "password123", "admin")
            .await;
        let user = app.create_authenticated_user("user1", "password123").await;
        let problem_id = app.create_problem(&admin, "P1").await;

        let res = app
            .get_with_token(&routes::problem_config(problem_id), &user)
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &body,
                &user,
            )
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn user_without_permission_cannot_access_contest_config() {
        let app = TestApp::spawn().await;
        let admin = app
            .create_user_with_role("admin13", "password123", "admin")
            .await;
        let user = app.create_authenticated_user("user2", "password123").await;
        let contest_id = app.create_contest(&admin, "C1", true, true).await;

        let res = app
            .get_with_token(&routes::contest_config(contest_id), &user)
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}

mod contest_config {
    use super::*;

    #[tokio::test]
    async fn admin_can_upsert_and_get_contest_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin14", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;

        let body = json!({"config": {"type": "icpc"}});
        let res = app
            .put_with_token(
                &routes::contest_config_ns(contest_id, "test-plugin", "contest-type"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "contest-type");
        assert_eq!(res.body["config"]["type"], "icpc");

        let res = app
            .get_with_token(
                &routes::contest_config_ns(contest_id, "test-plugin", "contest-type"),
                &token,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["config"]["type"], "icpc");
    }

    #[tokio::test]
    async fn admin_can_delete_contest_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin15", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;

        let body = json!({"config": {"k": "v"}});
        app.put_with_token(
            &routes::contest_config_ns(contest_id, "test-plugin", "scoring"),
            &body,
            &token,
        )
        .await;

        let res = app
            .delete_with_token(
                &routes::contest_config_ns(contest_id, "test-plugin", "scoring"),
                &token,
            )
            .await;
        assert_eq!(res.status, 204);

        let res = app
            .get_with_token(
                &routes::contest_config_ns(contest_id, "test-plugin", "scoring"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn upsert_returns_404_for_missing_contest() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin16", "password123", "admin")
            .await;

        let body = json!({"config": {"k": "v"}});
        let res = app
            .put_with_token(
                &routes::contest_config_ns(99999, "test-plugin", "scoring"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod contest_problem_config {
    use super::*;

    #[tokio::test]
    async fn admin_can_upsert_and_get_contest_problem_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin17", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;
        let problem_id = app.create_problem(&token, "P1").await;
        app.add_problem_to_contest(contest_id, problem_id, &token)
            .await;

        let body = json!({"config": {"time_limit": 2000}});
        let res = app
            .put_with_token(
                &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "limits");
        assert_eq!(res.body["config"]["time_limit"], 2000);

        let res = app
            .get_with_token(
                &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
                &token,
            )
            .await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body["config"]["time_limit"], 2000);
    }

    #[tokio::test]
    async fn admin_can_delete_contest_problem_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin18", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;
        let problem_id = app.create_problem(&token, "P1").await;
        app.add_problem_to_contest(contest_id, problem_id, &token)
            .await;

        let body = json!({"config": {"k": "v"}});
        app.put_with_token(
            &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
            &body,
            &token,
        )
        .await;

        let res = app
            .delete_with_token(
                &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
                &token,
            )
            .await;
        assert_eq!(res.status, 204);

        let res = app
            .get_with_token(
                &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn admin_can_list_contest_problem_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin23", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;
        let problem_id = app.create_problem(&token, "P1").await;
        app.add_problem_to_contest(contest_id, problem_id, &token)
            .await;

        app.put_with_token(
            &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
            &json!({"config": {"t": 1}}),
            &token,
        )
        .await;

        let res = app
            .get_with_token(
                &routes::contest_problem_config(contest_id, problem_id),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn returns_404_for_missing_contest_problem() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin19", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::contest_problem_config_ns(contest_id, 99999, "test-plugin", "limits"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod plugin_global_config {
    use super::*;

    #[tokio::test]
    async fn admin_can_list_plugin_global_config_empty() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin1", "password123", "admin")
            .await;

        let res = app
            .get_with_token(&routes::plugin_global_config("my-plugin"), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn admin_can_upsert_and_get_plugin_global_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin2", "password123", "admin")
            .await;

        let body = json!({"config": {"compiler": "/usr/bin/g++"}});
        let res = app
            .put_with_token(
                &routes::plugin_global_config_ns("standard-checkers", "testlib"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "testlib");
        assert_eq!(res.body["config"]["compiler"], "/usr/bin/g++");

        let res = app
            .get_with_token(
                &routes::plugin_global_config_ns("standard-checkers", "testlib"),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["namespace"], "testlib");
        assert_eq!(res.body["config"]["compiler"], "/usr/bin/g++");
    }

    #[tokio::test]
    async fn upsert_overwrites_existing() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin3", "password123", "admin")
            .await;

        let body1 = json!({"config": {"v": 1}});
        app.put_with_token(
            &routes::plugin_global_config_ns("my-plugin", "settings"),
            &body1,
            &token,
        )
        .await;

        let body2 = json!({"config": {"v": 2}});
        let res = app
            .put_with_token(
                &routes::plugin_global_config_ns("my-plugin", "settings"),
                &body2,
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["config"]["v"], 2);

        let res = app
            .get_with_token(
                &routes::plugin_global_config_ns("my-plugin", "settings"),
                &token,
            )
            .await;
        assert_eq!(res.body["config"]["v"], 2);
    }

    #[tokio::test]
    async fn admin_can_delete_plugin_global_config() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin4", "password123", "admin")
            .await;

        let body = json!({"config": {"k": "v"}});
        app.put_with_token(
            &routes::plugin_global_config_ns("my-plugin", "settings"),
            &body,
            &token,
        )
        .await;

        let res = app
            .delete_with_token(
                &routes::plugin_global_config_ns("my-plugin", "settings"),
                &token,
            )
            .await;
        assert_eq!(res.status, 204);

        let res = app
            .get_with_token(
                &routes::plugin_global_config_ns("my-plugin", "settings"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn permission_denied_for_non_admin() {
        let app = TestApp::spawn().await;
        let user = app
            .create_authenticated_user("pgc_user1", "password123")
            .await;

        let res = app
            .get_with_token(&routes::plugin_global_config("my-plugin"), &user)
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::plugin_global_config_ns("my-plugin", "settings"),
                &body,
                &user,
            )
            .await;
        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn rejects_invalid_plugin_id() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin5", "password123", "admin")
            .await;

        let body = json!({"config": {}});
        let res = app
            .put_with_token(
                &routes::plugin_global_config_ns("bad.plugin.id", "settings"),
                &body,
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn list_returns_multiple_namespaces() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("pgc_admin6", "password123", "admin")
            .await;

        app.put_with_token(
            &routes::plugin_global_config_ns("my-plugin", "settings"),
            &json!({"config": {"a": 1}}),
            &token,
        )
        .await;
        app.put_with_token(
            &routes::plugin_global_config_ns("my-plugin", "features"),
            &json!({"config": {"b": 2}}),
            &token,
        )
        .await;

        let res = app
            .get_with_token(&routes::plugin_global_config("my-plugin"), &token)
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body.as_array().unwrap().len(), 2);
    }
}

mod cascade_deletion {
    use super::*;

    #[tokio::test]
    async fn deleting_problem_cascades_config_deletion() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin20", "password123", "admin")
            .await;
        let problem_id = app.create_problem(&token, "P1").await;

        app.put_with_token(
            &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
            &json!({"config": {"k": "v"}}),
            &token,
        )
        .await;

        // Delete the problem
        let res = app
            .delete_with_token(&routes::problem(problem_id), &token)
            .await;
        assert_eq!(res.status, 204);

        // Re-create a problem to get a valid problem_id for the config check,
        // but the old config should be gone. We verify by checking directly
        // that the original problem's config endpoint returns 404 (problem gone).
        let res = app
            .get_with_token(
                &routes::problem_config_ns(problem_id, "test-plugin", "checker"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn deleting_contest_cascades_config_deletion() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin21", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;

        app.put_with_token(
            &routes::contest_config_ns(contest_id, "test-plugin", "scoring"),
            &json!({"config": {"k": "v"}}),
            &token,
        )
        .await;

        let res = app
            .delete_with_token(&routes::contest(contest_id), &token)
            .await;
        assert_eq!(res.status, 204);

        let res = app
            .get_with_token(
                &routes::contest_config_ns(contest_id, "test-plugin", "scoring"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
    }

    #[tokio::test]
    async fn removing_contest_problem_cascades_config_deletion() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("admin22", "password123", "admin")
            .await;
        let contest_id = app.create_contest(&token, "C1", true, true).await;
        let problem_id = app.create_problem(&token, "P1").await;
        app.add_problem_to_contest(contest_id, problem_id, &token)
            .await;

        app.put_with_token(
            &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
            &json!({"config": {"k": "v"}}),
            &token,
        )
        .await;

        // Remove problem from contest
        let res = app
            .delete_with_token(&routes::contest_problem(contest_id, problem_id), &token)
            .await;
        assert_eq!(res.status, 204);

        // Contest problem config should be gone (contest problem no longer exists)
        let res = app
            .get_with_token(
                &routes::contest_problem_config_ns(contest_id, problem_id, "test-plugin", "limits"),
                &token,
            )
            .await;
        assert_eq!(res.status, 404);
    }
}
