use chrono::Utc;
use common::{SubmissionStatus, Verdict};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use server::entity::{submission, user};

use crate::common::E2eTestApp;

fn skip_with_mock_sandbox() -> bool {
    if std::env::var("E2E_SERVER_URL").is_ok() {
        return false;
    }
    if std::env::var("E2E_SANDBOX_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("mock")) {
        eprintln!("skip submission sandbox test under mock sandbox");
        return true;
    }
    false
}

async fn seed_submission(
    app: &E2eTestApp,
    username: &str,
    problem_id: i32,
    contest_id: Option<i32>,
    contest_type: &str,
) -> i32 {
    let user_model = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(&app.db)
        .await
        .expect("query submitter")
        .expect("submitter should exist");
    let now = Utc::now();
    submission::ActiveModel {
        files: Set(json!([{ "filename": "main.cpp", "content": CPP_SUM }])),
        language: Set("cpp".into()),
        user_id: Set(user_model.id),
        problem_id: Set(problem_id),
        contest_id: Set(contest_id),
        contest_type: Set(contest_type.into()),
        status: Set(SubmissionStatus::Judged),
        verdict: Set(Some(Verdict::Accepted)),
        score: Set(Some(1.0)),
        judge_epoch: Set(1),
        created_at: Set(now),
        judged_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert submission")
    .id
}

#[tokio::test(flavor = "multi_thread")]
async fn submission_reaches_terminal_state() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("sub_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Terminal Test").await;
    app.create_test_case(problem_id, &admin).await;

    let sub_id = app
        .create_submission(problem_id, &admin, "cpp", CPP_SUM)
        .await;

    let res = app.wait_for_submission_terminal(sub_id, &admin, 60).await;
    let status = res.body["status"].as_str().unwrap();
    assert!(
        matches!(status, "Judged" | "CompilationError" | "SystemError"),
        "Expected terminal status, got: {status}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn submission_has_test_case_results_when_judged() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("sub_admin2", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "TCR Test").await;
    app.create_test_case(problem_id, &admin).await;

    let sub_id = app
        .create_submission(problem_id, &admin, "cpp", CPP_SUM)
        .await;

    let res = app.wait_for_submission_terminal(sub_id, &admin, 60).await;
    let status = res.body["status"].as_str().unwrap();

    if status == "Judged" || status == "CompilationError" {
        let result = &res.body["result"];
        assert!(
            !result.is_null(),
            "Submission in terminal state should have a result object"
        );
        if status == "Judged" {
            let tcrs = result["test_case_results"].as_array().unwrap();
            assert!(
                !tcrs.is_empty(),
                "Judged submission should have at least one test case result"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn submission_status_transitions_observed() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("sub_admin3", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Status Transition").await;
    app.create_test_case(problem_id, &admin).await;

    let sub_res = app
        .post_with_token(
            &format!("/api/v1/problems/{problem_id}/submissions"),
            &json!({
                "files": [{"filename": "main.cpp", "content": CPP_SUM}],
                "language": "cpp",
            }),
            &admin,
        )
        .await;
    assert_eq!(sub_res.status, 201);
    let sub_id = sub_res.id();

    let immediate = app
        .get_with_token(&format!("/api/v1/submissions/{sub_id}"), &admin)
        .await;
    assert_eq!(immediate.status, 200);
    let initial_status = immediate.body["status"].as_str().unwrap();
    assert!(
        [
            "Pending",
            "Compiling",
            "Running",
            "Judging",
            "Judged",
            "CompilationError",
            "SystemError"
        ]
        .contains(&initial_status),
        "Unexpected status: {initial_status}"
    );

    app.wait_for_submission_terminal(sub_id, &admin, 60).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn contest_submission_reaches_terminal_state() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("csub_admin1", "pass1234", "admin")
        .await;
    let user = app
        .create_authenticated_user("csub_user1", "pass1234")
        .await;

    let problem_id = app.create_problem(&admin, "Contest Terminal").await;
    app.create_test_case(problem_id, &admin).await;

    let contest_id = app
        .create_contest(&admin, "Contest Sub Test", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &user).await;

    let sub_id = app
        .create_contest_submission(contest_id, problem_id, &user, "cpp", CPP_SUM)
        .await;

    let res = app.wait_for_submission_terminal(sub_id, &user, 60).await;
    let status = res.body["status"].as_str().unwrap();
    assert!(
        matches!(status, "Judged" | "CompilationError" | "SystemError"),
        "Expected terminal status, got: {status}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn contest_submission_has_contest_id_set() {
    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("csub_admin2", "pass1234", "admin")
        .await;
    let user = app
        .create_authenticated_user("csub_user2", "pass1234")
        .await;

    let problem_id = app.create_problem(&admin, "Contest ID Check").await;

    let contest_id = app
        .create_contest(&admin, "Contest ID Test", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &user).await;

    let sub_id = seed_submission(&app, "csub_user2", problem_id, Some(contest_id), "icpc").await;

    let res = app
        .get_with_token(&format!("/api/v1/submissions/{sub_id}"), &admin)
        .await;
    assert_eq!(res.status, 200);
    assert_eq!(
        res.body["contest_id"].as_i64().unwrap(),
        contest_id as i64,
        "contest_id should match the contest the submission was created in"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn contest_submission_visible_in_contest_submissions_list() {
    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("csub_admin3", "pass1234", "admin")
        .await;
    let user = app
        .create_authenticated_user("csub_user3", "pass1234")
        .await;

    let problem_id = app.create_problem(&admin, "Contest List").await;

    let contest_id = app
        .create_contest(&admin, "Contest List Test", true, true)
        .await;
    app.add_problem_to_contest(contest_id, problem_id, &admin)
        .await;
    app.register_for_contest(contest_id, &user).await;

    let sub_id = seed_submission(&app, "csub_user3", problem_id, Some(contest_id), "icpc").await;

    let list = app
        .get_with_token(
            &format!("/api/v1/contests/{contest_id}/submissions"),
            &admin,
        )
        .await;
    assert_eq!(list.status, 200);

    let data = list.body["data"].as_array().unwrap();
    assert!(
        data.iter()
            .any(|s| s["id"].as_i64().unwrap() == sub_id as i64),
        "Submission {sub_id} should appear in contest submissions list"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rejudge_resets_to_pending_then_reaches_terminal() {
    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("rejudge_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Rejudge Test").await;

    let sub_id = app
        .create_submission(problem_id, &admin, "cpp", CPP_SUM)
        .await;
    let first = app.wait_for_submission_terminal(sub_id, &admin, 60).await;
    let original_epoch = first.body["judge_epoch"].as_i64().unwrap();

    let rejudge_res = app
        .post_with_token(
            &format!("/api/v1/submissions/{sub_id}/rejudge"),
            &json!({}),
            &admin,
        )
        .await;
    assert_eq!(
        rejudge_res.status, 200,
        "Rejudge should succeed: {}",
        rejudge_res.text
    );
    let rejudge_epoch = rejudge_res.body["judge_epoch"].as_i64().unwrap();
    assert!(
        rejudge_epoch > original_epoch,
        "judge_epoch should increment on rejudge (was {original_epoch}, got {rejudge_epoch})"
    );

    let second = app.wait_for_submission_terminal(sub_id, &admin, 60).await;
    let final_epoch = second.body["judge_epoch"].as_i64().unwrap();
    assert_eq!(
        final_epoch, rejudge_epoch,
        "judge_epoch should remain at the rejudge value"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bulk_rejudge_resets_multiple_submissions() {
    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("bulk_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Bulk Rejudge").await;

    let mut sub_ids = Vec::new();
    for _ in 0..3 {
        let id = app
            .create_submission(problem_id, &admin, "cpp", CPP_SUM)
            .await;
        sub_ids.push(id);
    }
    for &id in &sub_ids {
        app.wait_for_submission_terminal(id, &admin, 60).await;
    }

    let mut original_epochs = Vec::new();
    for &id in &sub_ids {
        let res = app
            .get_with_token(&format!("/api/v1/submissions/{id}"), &admin)
            .await;
        original_epochs.push(res.body["judge_epoch"].as_i64().unwrap());
    }

    let bulk_res = app
        .post_with_token(
            "/api/v1/submissions/bulk-rejudge",
            &json!({ "submission_ids": sub_ids }),
            &admin,
        )
        .await;
    assert_eq!(
        bulk_res.status, 200,
        "Bulk rejudge failed: {}",
        bulk_res.text
    );
    let queued = bulk_res.body["queued"].as_i64().unwrap();
    assert_eq!(
        queued, 3,
        "All 3 submissions should be queued for rejudging"
    );

    for &id in &sub_ids {
        app.wait_for_submission_terminal(id, &admin, 60).await;
    }

    for (i, &id) in sub_ids.iter().enumerate() {
        let res = app
            .get_with_token(&format!("/api/v1/submissions/{id}"), &admin)
            .await;
        let new_epoch = res.body["judge_epoch"].as_i64().unwrap();
        assert!(
            new_epoch > original_epochs[i],
            "Submission {id}: judge_epoch should increase after bulk rejudge \
             (was {}, got {new_epoch})",
            original_epochs[i]
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multiple_test_cases_all_get_results() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("multi_tc_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Multi TC").await;
    let tc1 = app
        .create_test_case_with(problem_id, "1\n", "1\n", 10, true, &admin)
        .await;
    let tc2 = app
        .create_test_case_with(problem_id, "2\n", "2\n", 10, false, &admin)
        .await;
    let tc3 = app
        .create_test_case_with(problem_id, "3\n", "3\n", 10, false, &admin)
        .await;

    let sub_id = app
        .create_submission(problem_id, &admin, "cpp", CPP_SUM)
        .await;
    let res = app.wait_for_submission_terminal(sub_id, &admin, 60).await;

    if res.body["status"].as_str().unwrap() == "Judged" {
        let tcrs = res.body["result"]["test_case_results"].as_array().unwrap();
        assert_eq!(
            tcrs.len(),
            3,
            "Should have exactly 3 test case results, got {}",
            tcrs.len()
        );

        let tcr_tc_ids: Vec<i64> = tcrs
            .iter()
            .filter_map(|r| r["test_case_id"].as_i64())
            .collect();
        for expected_id in [tc1, tc2, tc3] {
            assert!(
                tcr_tc_ids.contains(&(expected_id as i64)),
                "Missing TCR for test_case_id {expected_id}; found: {tcr_tc_ids:?}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn submission_judges_blob_backed_testcase_input_and_expected_output() {
    if skip_with_mock_sandbox() {
        return;
    }

    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("blob_tc_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Blob Backed Judging").await;
    let large_body = "a".repeat(1_048_576 + 64);
    app.create_test_case_with(problem_id, &large_body, &large_body, 10, false, &admin)
        .await;

    let echo_source = r#"
#include <iostream>
int main() {
    std::cout << std::cin.rdbuf();
    return 0;
}
"#;
    let sub_id = app
        .create_submission(problem_id, &admin, "cpp", echo_source)
        .await;
    let res = app.wait_for_submission_terminal(sub_id, &admin, 120).await;

    assert_eq!(
        res.body["status"].as_str(),
        Some("Judged"),
        "blob-backed testcase submission should judge successfully: {}",
        res.text
    );
    assert_eq!(res.body["verdict"].as_str(), Some("Accepted"));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_submissions_all_complete() {
    let app = E2eTestApp::spawn().await;
    let admin = app
        .create_user_with_role("conc_admin1", "pass1234", "admin")
        .await;

    let problem_id = app.create_problem(&admin, "Concurrent").await;

    let mut sub_ids = Vec::new();
    for _ in 0..5 {
        let id = app
            .create_submission(problem_id, &admin, "cpp", CPP_SUM)
            .await;
        sub_ids.push(id);
    }

    for &id in &sub_ids {
        let res = app.wait_for_submission_terminal(id, &admin, 60).await;
        let status = res.body["status"].as_str().unwrap();
        assert!(
            matches!(status, "Judged" | "CompilationError" | "SystemError"),
            "Submission {id} did not reach terminal state, got: {status}"
        );
    }
}

use super::CPP_SUM;
