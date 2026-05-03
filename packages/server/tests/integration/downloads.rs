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
