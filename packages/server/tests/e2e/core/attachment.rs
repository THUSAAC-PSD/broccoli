use crate::common::E2eTestApp;

mod attachment_upload {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_upload_attachment() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_up1", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "Attach Problem").await;

        let res = app
            .upload_attachment(pid, "readme.txt", b"Hello World".to_vec(), None, &admin)
            .await;

        assert_eq!(res.status, 201);
        assert!(res.body["id"].is_string());
        assert_eq!(res.body["filename"], "readme.txt");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_upload_attachment_with_custom_path() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_up2", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "Attach Path Problem").await;

        let res = app
            .upload_attachment(
                pid,
                "data.txt",
                b"Some data".to_vec(),
                Some("subfolder"),
                &admin,
            )
            .await;

        assert_eq!(res.status, 201);
        assert_eq!(res.body["filename"], "data.txt");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_upload_attachment() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_up3", "password123", "admin")
            .await;
        let user = app
            .create_user_with_role("att_up4", "password123", "contestant")
            .await;

        let pid = app.create_problem(&admin, "No Upload Problem").await;

        let res = app
            .upload_attachment(pid, "bad.txt", b"nope".to_vec(), None, &user)
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn upload_to_nonexistent_problem_returns_404() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_up5", "password123", "admin")
            .await;

        let res = app
            .upload_attachment(99999, "file.txt", b"data".to_vec(), None, &admin)
            .await;

        assert_eq!(res.status, 404);
        assert_eq!(res.body["code"], "NOT_FOUND");
    }
}

mod attachment_listing {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_list_attachments() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_ls1", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "List Attach Problem").await;
        app.upload_attachment(pid, "file1.txt", b"one".to_vec(), None, &admin)
            .await;
        app.upload_attachment(pid, "file2.txt", b"two".to_vec(), None, &admin)
            .await;

        let res = app
            .get_with_token(&format!("/api/v1/problems/{pid}/attachments"), &admin)
            .await;

        assert_eq!(res.status, 200);
        let items = res.body["attachments"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_problem_returns_empty_attachment_list() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_ls2", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "Empty Attach Problem").await;

        let res = app
            .get_with_token(&format!("/api/v1/problems/{pid}/attachments"), &admin)
            .await;

        assert_eq!(res.status, 200);
        let items = res.body["attachments"].as_array().unwrap();
        assert_eq!(items.len(), 0);
    }
}

mod attachment_download {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_download_attachment() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_dl1", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "Download Problem").await;
        let upload = app
            .upload_attachment(pid, "hello.txt", b"Hello Download".to_vec(), None, &admin)
            .await;
        assert_eq!(upload.status, 201);
        let att_id = upload.body["id"].as_str().unwrap();

        let response = app
            .download_raw(
                &format!("/api/v1/problems/{pid}/attachments/{att_id}"),
                &admin,
            )
            .await;

        assert_eq!(response.status().as_u16(), 200);
        let body_bytes = response.bytes().await.unwrap();
        assert_eq!(&body_bytes[..], b"Hello Download");
    }
}

mod attachment_deletion {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn admin_can_delete_attachment() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_del1", "password123", "admin")
            .await;

        let pid = app.create_problem(&admin, "Delete Attach Problem").await;
        let upload = app
            .upload_attachment(pid, "del.txt", b"delete me".to_vec(), None, &admin)
            .await;
        assert_eq!(upload.status, 201);
        let att_id = upload.body["id"].as_str().unwrap();

        let del = app
            .delete_with_token(
                &format!("/api/v1/problems/{pid}/attachments/{att_id}"),
                &admin,
            )
            .await;
        assert_eq!(del.status, 204);

        let list = app
            .get_with_token(&format!("/api/v1/problems/{pid}/attachments"), &admin)
            .await;
        let items = list.body["attachments"].as_array().unwrap();
        assert_eq!(items.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_admin_cannot_delete_attachment() {
        let app = E2eTestApp::spawn_without_plugins().await;
        let admin = app
            .create_user_with_role("att_del2", "password123", "admin")
            .await;
        let user = app
            .create_user_with_role("att_del3", "password123", "contestant")
            .await;

        let pid = app.create_problem(&admin, "No Del Attach Prob").await;
        let upload = app
            .upload_attachment(pid, "keep.txt", b"keep me".to_vec(), None, &admin)
            .await;
        let att_id = upload.body["id"].as_str().unwrap();

        let res = app
            .delete_with_token(
                &format!("/api/v1/problems/{pid}/attachments/{att_id}"),
                &user,
            )
            .await;

        assert_eq!(res.status, 403);
        assert_eq!(res.body["code"], "PERMISSION_DENIED");
    }
}
