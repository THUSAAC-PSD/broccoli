use crate::common::TestApp;

mod healthz {
    use super::*;

    #[tokio::test]
    async fn returns_200_when_db_and_mq_are_up() {
        let app = TestApp::spawn().await;
        let resp = app.get_without_token("/healthz").await;

        assert_eq!(resp.status, 200, "body: {}", resp.text);

        assert_eq!(
            resp.body.get("status").and_then(|v| v.as_str()),
            Some("ok"),
            "body: {}",
            resp.text
        );
        assert_eq!(
            resp.body.get("db").and_then(|v| v.as_str()),
            Some("ok"),
            "body: {}",
            resp.text
        );
        // The TestApp spawns with `mq.enabled = false`, so the MQ field
        // reports `disabled` rather than `ok`. The aggregate `status` is
        // still `ok` because disabled components are not failures.
        assert_eq!(
            resp.body.get("mq").and_then(|v| v.as_str()),
            Some("disabled"),
            "body: {}",
            resp.text
        );

        let version = resp.body.get("version").and_then(|v| v.as_str()).unwrap();
        assert!(!version.is_empty(), "version should be non-empty");
        assert!(
            resp.body.get("git_sha").and_then(|v| v.as_str()).is_some(),
            "git_sha should be present"
        );
    }
}

mod api_health {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_same_body_shape_as_healthz() {
        let app = TestApp::spawn().await;
        let resp = app.get_without_token("/api/v1/health").await;

        assert_eq!(resp.status, 200, "body: {}", resp.text);
        assert_eq!(resp.body.get("status").and_then(|v| v.as_str()), Some("ok"));
        assert_eq!(resp.body.get("db").and_then(|v| v.as_str()), Some("ok"));
        assert_eq!(
            resp.body.get("mq").and_then(|v| v.as_str()),
            Some("disabled")
        );
        assert!(resp.body.get("version").is_some());
        assert!(resp.body.get("git_sha").is_some());
    }
}
