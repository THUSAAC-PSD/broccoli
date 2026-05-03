use crate::common::TestApp;

#[tokio::test]
async fn slim_server_returns_404_for_downloads() {
    let app = TestApp::spawn().await;
    let resp = app
        .get_without_token("/downloads/stress-test/linux-x86_64")
        .await;
    assert_eq!(resp.status, 404);
}
