use crate::common::TestApp;

#[tokio::test]
async fn version_endpoint_returns_server_version() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/api/v1/version").await;
    assert_eq!(resp.status, 200);
    let version = resp.body.get("version").and_then(|v| v.as_str()).unwrap();
    assert!(!version.is_empty(), "version should be non-empty");
    assert!(
        version.chars().next().unwrap().is_ascii_digit(),
        "version should start with a digit, got {version}"
    );
}
