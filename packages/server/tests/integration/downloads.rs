use crate::common::TestApp;

#[tokio::test]
async fn downloads_serves_linux_x86_64_binary() {
    let app = TestApp::spawn().await;
    let resp = app
        .get_without_token("/downloads/stress-test/linux-x86_64")
        .await;
    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.headers.get("content-type").unwrap(),
        "application/octet-stream"
    );
    let cd = resp
        .headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("attachment"));
    assert!(cd.contains("broccoli-stress-test-linux-x86_64"));
    assert_eq!(resp.text.as_bytes(), b"FIXTURE-LINUX-X86");
}

#[tokio::test]
async fn downloads_serves_windows_binary_with_exe_filename() {
    let app = TestApp::spawn().await;
    let resp = app
        .get_without_token("/downloads/stress-test/windows-x86_64")
        .await;
    assert_eq!(resp.status, 200);
    let cd = resp
        .headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("broccoli-stress-test-windows-x86_64.exe"));
}

#[tokio::test]
async fn downloads_unknown_platform_returns_404() {
    let app = TestApp::spawn().await;
    let resp = app
        .get_without_token("/downloads/stress-test/freebsd-x86_64")
        .await;
    assert_eq!(resp.status, 404);
}

#[tokio::test]
async fn downloads_serves_sha256_file() {
    let app = TestApp::spawn().await;
    let resp = app
        .get_without_token("/downloads/stress-test/linux-x86_64.sha256")
        .await;
    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.headers.get("content-type").unwrap(),
        "text/plain; charset=utf-8"
    );
    assert!(resp.text.contains("broccoli-stress-test-linux-x86_64"));
    assert_eq!(resp.text.split_whitespace().next().unwrap().len(), 64);
}

#[tokio::test]
async fn manifest_endpoint_returns_json_with_all_platforms() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/manifest.json").await;
    assert_eq!(resp.status, 200);
    let platforms = resp
        .body
        .get("platforms")
        .and_then(|p| p.as_object())
        .unwrap();
    for key in [
        "linux-x86_64",
        "linux-aarch64",
        "windows-x86_64",
        "macos-universal",
    ] {
        assert!(
            platforms.get(key).is_some(),
            "manifest missing platform {key}"
        );
    }
    assert!(resp.body.get("version").is_some());
}

#[tokio::test]
async fn manifest_rewrites_urls_to_relative_server_paths() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/manifest.json").await;
    let url = resp
        .body
        .pointer("/platforms/linux-x86_64/url")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(url, "/downloads/stress-test/linux-x86_64", "got {url}");
    assert!(
        !url.contains("github.com"),
        "manifest leaked github URL: {url}"
    );
}
