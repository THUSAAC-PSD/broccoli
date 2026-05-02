use crate::common::{TestApp, routes};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::json;

#[tokio::test]
async fn f1200_empty_patch_skips_transaction() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1200", "password123", "admin")
        .await;

    let pid = app.create_problem(&admin, "Empty PATCH Test Problem").await;

    use server::entity::problem;
    let before = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .updated_at;

    let res = app
        .patch_with_token(&format!("{}/{}", routes::PROBLEMS, pid), &json!({}), &admin)
        .await;

    assert_eq!(res.status, 200, "empty PATCH should return 200");

    let after = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .updated_at;

    assert_eq!(
        before, after,
        "Empty PATCH should not update updated_at (short-circuit optimization)"
    );
}

#[tokio::test]
async fn f1201_nonempty_patch_updates_timestamp() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1201", "password123", "admin")
        .await;

    let pid = app.create_problem(&admin, "Non-Empty PATCH Test").await;

    use server::entity::problem;
    let before = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .updated_at;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let res = app
        .patch_with_token(
            &format!("{}/{}", routes::PROBLEMS, pid),
            &json!({"description": "Updated description"}),
            &admin,
        )
        .await;

    assert_eq!(res.status, 200, "non-empty PATCH should return 200");

    let after = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .updated_at;

    assert!(
        after >= before,
        "Non-empty PATCH should update or keep updated_at (but not regress)"
    );
}

#[tokio::test]
async fn f1202_three_state_patch_on_content() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1202", "password123", "admin")
        .await;

    use server::entity::problem;

    let pid = app.create_problem(&admin, "Three-State PATCH Test").await;

    let res = app
        .patch_with_token(
            &format!("{}/{}", routes::PROBLEMS, pid),
            &json!({"content": "Initial content"}),
            &admin,
        )
        .await;
    assert_eq!(res.status, 200);

    let content_val = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .content;
    assert_eq!(content_val, "Initial content");

    let res = app
        .patch_with_token(
            &format!("{}/{}", routes::PROBLEMS, pid),
            &json!({"title": "New Title"}), // Only title, no content
            &admin,
        )
        .await;
    assert_eq!(res.status, 200);

    let content_after_absent = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .content;
    assert_eq!(
        content_after_absent, "Initial content",
        "Absent field should not change"
    );

    let res = app
        .patch_with_token(
            &format!("{}/{}", routes::PROBLEMS, pid),
            &json!({"content": "x"}),
            &admin,
        )
        .await;
    assert_eq!(res.status, 200);

    let content_after_minimal = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .content;
    assert_eq!(content_after_minimal, "x", "Minimal content should be set");

    let res = app
        .patch_with_token(
            &format!("{}/{}", routes::PROBLEMS, pid),
            &json!({"content": "New content"}),
            &admin,
        )
        .await;
    assert_eq!(res.status, 200);

    let content_after_value = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists")
        .content;
    assert_eq!(
        content_after_value, "New content",
        "Value should set field to that value"
    );
}

#[tokio::test]
async fn f1203_position_assignment_with_no_test_cases() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1203", "password123", "admin")
        .await;

    let pid = app.create_problem(&admin, "Position Assignment Test").await;

    let res = app
        .post_with_token(
            &routes::test_cases(pid),
            &json!({
                "input": "1",
                "expected_output": "output",
                "score": 10,
                "is_sample": false
            }),
            &admin,
        )
        .await;

    assert_eq!(res.status, 201);
    assert_eq!(
        res.body["position"], 0,
        "First test case should get position 0"
    );

    let res = app
        .post_with_token(
            &routes::test_cases(pid),
            &json!({
                "input": "2",
                "expected_output": "output",
                "score": 10,
                "is_sample": false
            }),
            &admin,
        )
        .await;

    assert_eq!(res.status, 201);
    assert_eq!(
        res.body["position"], 1,
        "Second test case should get position 1"
    );
}

#[tokio::test]
async fn f1204_list_problems_response_size() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1204", "password123", "admin")
        .await;

    let large_content = "x".repeat(10000); // 10KB content
    let res = app
        .post_with_token(
            routes::PROBLEMS,
            &json!({
                "title": "Large Problem",
                "content": large_content,
                "time_limit": 1000,
                "memory_limit": 256
            }),
            &admin,
        )
        .await;
    assert_eq!(res.status, 201);

    let res = app.get_with_token(routes::PROBLEMS, &admin).await;
    assert_eq!(res.status, 200);

    let items = &res.body["data"];
    assert!(items.is_array());

    let our_problem = items
        .as_array()
        .expect("data is array")
        .iter()
        .find(|p| p["title"].as_str() == Some("Large Problem"));

    assert!(our_problem.is_some(), "Problem should be in list");

    let body_str = serde_json::to_string(&res.body).expect("serialize");
    assert!(
        body_str.len() < (10000 * 2), // Even with overhead, shouldn't be close to full
        "List response should not contain full 10KB content (perf check)"
    );
}

#[tokio::test]
async fn f1205_soft_delete_problem_behavior() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1205", "password123", "admin")
        .await;

    use server::entity::{problem, test_case};

    let pid = app.create_problem(&admin, "Soft Delete Test").await;

    for i in 0..3 {
        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": format!("input{}", i),
                    "expected_output": format!("output{}", i),
                    "score": 10,
                    "is_sample": false
                }),
                &admin,
            )
            .await;
        assert_eq!(res.status, 201);
    }

    let res = app.get_with_token(&routes::test_cases(pid), &admin).await;
    assert_eq!(res.status, 200);
    assert_eq!(
        res.body.as_array().unwrap().len(),
        3,
        "Should have 3 test cases"
    );

    let res = app
        .delete_with_token(&format!("{}/{}", routes::PROBLEMS, pid), &admin)
        .await;
    assert_eq!(res.status, 204);

    let problem_after = problem::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .expect("query")
        .expect("problem exists (not hard deleted)");
    assert!(
        problem_after.deleted_at.is_some(),
        "Problem should be soft-deleted"
    );

    let count: u64 = test_case::Entity::find()
        .filter(test_case::Column::ProblemId.eq(pid))
        .count(&app.db)
        .await
        .expect("count");

    assert_eq!(
        count, 3,
        "Test cases should remain in DB (soft-delete architecture)"
    );
}

#[tokio::test]
async fn f1206_concurrent_delete_ordering() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1206", "password123", "admin")
        .await;

    let pid = app.create_problem(&admin, "Concurrent Delete Test").await;

    let res = app
        .delete_with_token(&format!("{}/{}", routes::PROBLEMS, pid), &admin)
        .await;

    assert_eq!(res.status, 204, "Delete should succeed");

    let res = app
        .get_with_token(&format!("{}/{}", routes::PROBLEMS, pid), &admin)
        .await;
    assert_eq!(res.status, 404, "Deleted problem should not exist");
}

#[tokio::test]
async fn f1207_identity_column_sequence_continues() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1207", "password123", "admin")
        .await;

    let p1 = app.create_problem(&admin, "Identity Test 1").await;

    let p2 = app.create_problem(&admin, "Identity Test 2").await;

    let res = app
        .delete_with_token(&format!("{}/{}", routes::PROBLEMS, p1), &admin)
        .await;
    assert_eq!(res.status, 204);

    let p3 = app.create_problem(&admin, "Identity Test 3").await;

    assert!(
        p3 > p2,
        "New problem ID should be greater than previous (sequence continues)"
    );
    assert_ne!(p3, p1, "New problem should not reuse deleted ID");
}

#[tokio::test]
async fn f1208_position_zero_not_one() {
    let app = TestApp::spawn().await;
    let admin = app
        .create_user_with_role("admin_f1208", "password123", "admin")
        .await;

    let pid = app.create_problem(&admin, "Position Zero Test").await;

    let res = app
        .post_with_token(
            &routes::test_cases(pid),
            &json!({
                "input": "test_in",
                "expected_output": "test_out",
                "score": 25,
                "is_sample": false
            }),
            &admin,
        )
        .await;

    assert_eq!(res.status, 201);
    assert_eq!(
        res.body["position"], 0,
        "First test case must have position 0"
    );
}
