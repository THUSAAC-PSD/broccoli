use crate::common::{TestApp, routes};
use serde_json::json;

fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut writer = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in files {
        writer.start_file(*name, options).expect("zip start_file");
        writer.write_all(*content).expect("zip write_all");
    }
    let cursor = writer.finish().expect("zip finish");
    cursor.into_inner()
}

mod authoring_content_variations {
    use super::*;

    #[tokio::test]
    async fn empty_title_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin1", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "",
                    "content": "Some content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn whitespace_only_title_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin2", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "   \t\n  ",
                    "content": "Valid content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn title_at_character_limit_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin3", "password12345", "admin")
            .await;

        let title = "a".repeat(256);
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": &title,
                    "content": "Valid content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn title_over_character_limit_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin4", "password12345", "admin")
            .await;

        let title = "a".repeat(257);
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": &title,
                    "content": "Valid content",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn content_with_markdown_features_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin5", "password12345", "admin")
            .await;

        let content = r#"# Problem

## Description
Find two numbers that sum to target.

## Example
```cpp
#include <iostream>
int main() { return 0; }
```

```python
def main(): pass
```

## Table

| Input | Output |
|-------|--------|
| 4 | 2 |

## Math
Inline: $x^2 + y^2 = z^2$

Block:
$$
\frac{1}{2}
$$

## Lists
- Item 1
  - Nested
- Item 2

## Images
![alt text](https://example.com/image.png)
"#;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Markdown Test",
                    "content": content,
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn content_1mb_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin6", "password12345", "admin")
            .await;

        let content = "a".repeat(1_000_000);
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "1MB Content",
                    "content": &content,
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn content_over_1mb_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin7", "password12345", "admin")
            .await;

        let content = "a".repeat(1_000_001);
        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Over 1MB",
                    "content": &content,
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn cjk_content_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin8", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "两数之和",
                    "content": "给定一个整数数组和一个目标值，找出数组中和为目标值的两个数。\n日本語のテキスト。\n한국어 텍스트입니다.",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["title"], "两数之和");
    }

    #[tokio::test]
    async fn emoji_in_title_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin9", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "🎯 Target Sum 🎯",
                    "content": "Find pairs summing to target 🎨",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn rtl_text_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin10", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "בעיה",
                    "content": "This is an Arabic problem: مرحبا بك في المسابقة",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn xss_in_markdown_stored_safely() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin11", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "XSS Test",
                    "content": "<script>alert('xss')</script>\n<img onerror='alert(1)'>",
                    "time_limit": 1000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert!(res.body["content"].as_str().unwrap().contains("<script>"));
    }
}

mod authoring_field_edge_cases {
    use super::*;

    #[tokio::test]
    async fn time_limit_1ms_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin12", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Fast",
                    "content": "Content",
                    "time_limit": 1,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn time_limit_0_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin13", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Zero Time",
                    "content": "Content",
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
    async fn time_limit_30000ms_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin14", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Slow",
                    "content": "Content",
                    "time_limit": 30000,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn time_limit_30001ms_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin15", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Too Slow",
                    "content": "Content",
                    "time_limit": 30001,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn time_limit_negative_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin16", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Negative",
                    "content": "Content",
                    "time_limit": -1,
                    "memory_limit": 262144
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn memory_limit_1kb_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin17", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Tiny Memory",
                    "content": "Content",
                    "time_limit": 1000,
                    "memory_limit": 1
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn memory_limit_max_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin18", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Max Memory",
                    "content": "Content",
                    "time_limit": 1000,
                    "memory_limit": 1048576
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn memory_limit_over_max_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin19", "password12345", "admin")
            .await;

        let res = app
            .post_with_token(
                routes::PROBLEMS,
                &json!({
                    "title": "Over Max",
                    "content": "Content",
                    "time_limit": 1000,
                    "memory_limit": 1048577
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn patch_empty_does_not_modify_updated_at() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin20", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Original").await;

        let res1 = app.get_with_token(&routes::problem(pid), &token).await;
        let original_updated_at = res1.body["updated_at"].as_str().unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let res2 = app
            .patch_with_token(&routes::problem(pid), &json!({}), &token)
            .await;

        assert_eq!(res2.status, 200);
        assert_eq!(res2.body["updated_at"], original_updated_at);
    }

    #[tokio::test]
    async fn patch_null_description_clears_it() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin21", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let tc_res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "description": "original"
                }),
                &token,
            )
            .await;
        assert_eq!(tc_res.status, 201);
        let tc_id = tc_res.body["id"].as_i64().unwrap() as i32;

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

    #[tokio::test]
    async fn patch_omitted_description_leaves_unchanged() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin22", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let tc_res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "description": "original"
                }),
                &token,
            )
            .await;
        assert_eq!(tc_res.status, 201);
        let tc_id = tc_res.body["id"].as_i64().unwrap() as i32;

        let res = app
            .patch_with_token(
                &routes::test_case(pid, tc_id),
                &json!({ "score": 20 }),
                &token,
            )
            .await;

        assert_eq!(res.status, 200);
        assert_eq!(res.body["description"], "original");
    }
}

mod authoring_test_case_variations {
    use super::*;

    #[tokio::test]
    async fn create_test_case_with_empty_input() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin23", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "",
                    "expected_output": "output",
                    "score": 10,
                    "is_sample": true
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn create_test_case_with_empty_expected_output() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin24", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "input",
                    "expected_output": "",
                    "score": 10,
                    "is_sample": true
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn create_test_case_both_empty() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin25", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "",
                    "expected_output": "",
                    "score": 10,
                    "is_sample": true
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn test_case_score_0_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin26", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 0,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn test_case_score_max_accepted() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin27", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10000,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn test_case_score_over_max_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin28", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10001,
                    "is_sample": false
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn test_case_position_auto_assignment() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin29", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let mut positions = Vec::new();
        for i in 0..5 {
            let res = app
                .post_with_token(
                    &routes::test_cases(pid),
                    &json!({
                        "input": format!("input{}", i),
                        "expected_output": "output",
                        "score": 10,
                        "is_sample": false
                    }),
                    &token,
                )
                .await;

            assert_eq!(res.status, 201);
            positions.push(res.body["position"].as_i64().unwrap());
        }

        assert_eq!(positions, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_case_with_label() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin30", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": true,
                    "label": "sample_01"
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["label"], "sample_01");
    }

    #[tokio::test]
    async fn test_case_label_max_length() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin31", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;
        let label = "a".repeat(64);

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "label": label
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn test_case_label_over_max_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin32", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;
        let label = "a".repeat(65);

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "label": label
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn test_case_empty_label_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin33", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "label": ""
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn test_case_whitespace_label_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin34", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "label": "   "
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn test_case_description_max_length() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin35", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;
        let desc = "a".repeat(256);

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "description": desc
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 201);
    }

    #[tokio::test]
    async fn test_case_description_over_max_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin36", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "Test").await;
        let desc = "a".repeat(257);

        let res = app
            .post_with_token(
                &routes::test_cases(pid),
                &json!({
                    "input": "1",
                    "expected_output": "1",
                    "score": 10,
                    "is_sample": false,
                    "description": desc
                }),
                &token,
            )
            .await;

        assert_eq!(res.status, 400);
        assert_eq!(res.body["code"], "VALIDATION_ERROR");
    }
}

mod authoring_zip_upload {
    use super::*;

    #[tokio::test]
    async fn zip_single_pair_creates_test_case() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin37", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[("1.in", b"input"), ("1.ans", b"output")]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 1);
    }

    #[tokio::test]
    async fn zip_multiple_pairs() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin38", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[
            ("1.in", b"in1"),
            ("1.ans", b"out1"),
            ("2.in", b"in2"),
            ("2.ans", b"out2"),
            ("3.in", b"in3"),
            ("3.ans", b"out3"),
        ]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 3);
    }

    #[tokio::test]
    async fn zip_with_sample_directory() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin39", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[
            ("sample/1.in", b"sample_in"),
            ("sample/1.ans", b"sample_out"),
            ("2.in", b"main_in"),
            ("2.ans", b"main_out"),
        ]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 2);
        assert_eq!(res.body["test_cases"][0]["is_sample"], true);
        assert_eq!(res.body["test_cases"][1]["is_sample"], false);
    }

    #[tokio::test]
    async fn zip_path_traversal_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin40", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[("../escape.in", b"bad"), ("../escape.ans", b"bad")]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn zip_without_wildcard_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin41", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[("1.in", b"input"), ("1.ans", b"output")]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "input",
                "output",
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn zip_unmatched_input_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin42", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[("1.in", b"input"), ("2.ans", b"output")]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 400);
        assert!(
            res.body["message"]
                .as_str()
                .unwrap()
                .contains("Input files without matching output")
        );
    }

    #[tokio::test]
    async fn zip_not_a_zip_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin43", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let client = reqwest::Client::new();
        let form = reqwest::multipart::Form::new()
            .text("input_format", "*.in")
            .text("output_format", "*.ans")
            .text("strategy", "abort")
            .part(
                "file",
                reqwest::multipart::Part::bytes(b"not a zip".to_vec()).file_name("test.zip"),
            );

        let url = format!("http://{}{}", app.addr, &routes::test_cases_upload(pid));
        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .multipart(form)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400);
    }

    #[tokio::test]
    async fn zip_empty_rejected() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin44", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.ans",
            )
            .await;

        assert_eq!(res.status, 400);
    }

    #[tokio::test]
    async fn zip_with_out_extension() {
        let app = TestApp::spawn().await;
        let token = app
            .create_user_with_role("qa_admin45", "password12345", "admin")
            .await;

        let pid = app.create_problem(&token, "ZIP Test").await;

        let zip = build_zip(&[("1.in", b"input"), ("1.out", b"output")]);

        let res = app
            .upload_test_cases(
                &routes::test_cases_upload(pid),
                &token,
                zip,
                "*.in",
                "*.out",
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["created"], 1);
    }
}
