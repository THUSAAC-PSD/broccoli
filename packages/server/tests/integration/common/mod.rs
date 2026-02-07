use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use plugin_core::config::PluginConfig;
use reqwest::Client;
use sea_orm::{
    ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, Set, Statement,
};
use serde_json::Value;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

use server::config::{
    AppConfig, AuthConfig, CorsConfig, DatabaseConfig, MqAppConfig, ServerConfig, SubmissionConfig,
};
use server::entity::user;
use server::manager::ServerManager;
use server::state::AppState;

/// PostgreSQL container shared across all tests in this binary.
static SHARED_PG: OnceCell<(ContainerAsync<Postgres>, u16)> = OnceCell::const_new();

/// Monotonic counter for unique database names.
static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Container ID for atexit cleanup.
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

extern "C" fn cleanup_container() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", "-v", id])
            .output();
    }
}

/// Start (or reuse) the shared PostgreSQL container, create and initialize a
/// template database, and return the host port.
async fn shared_pg_port() -> u16 {
    let (_, port) = SHARED_PG
        .get_or_init(|| async {
            let container = Postgres::default()
                .start()
                .await
                .expect("Failed to start PostgreSQL container");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("Failed to get PostgreSQL port");

            let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            let admin_db = Database::connect(ConnectOptions::new(&admin_url))
                .await
                .expect("Failed to connect to admin database for template setup");
            admin_db
                .execute_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "CREATE DATABASE \"template_test\"".to_string(),
                ))
                .await
                .expect("Failed to create template database");
            drop(admin_db);

            let _ = CONTAINER_ID.set(container.id().to_string());

            // The `watchdog` feature handles signal-based
            // cleanup (Ctrl+C), but normal process exit doesn't trigger `Drop` on statics.
            unsafe { libc::atexit(cleanup_container) };

            let template_url =
                format!("postgres://postgres:postgres@127.0.0.1:{port}/template_test");
            let template_db = server::database::init_db(&template_url)
                .await
                .expect("Failed to initialize template database");
            server::seed::seed_role_permissions(&template_db)
                .await
                .expect("Failed to seed template database");
            server::seed::ensure_indexes(&template_db)
                .await
                .expect("Failed to create indexes");
            drop(template_db);

            (container, port)
        })
        .await;
    *port
}

pub mod routes {
    pub const REGISTER: &str = "/api/v1/auth/register";
    pub const LOGIN: &str = "/api/v1/auth/login";
    pub const ME: &str = "/api/v1/auth/me";
    pub const PROBLEMS: &str = "/api/v1/problems";

    pub fn plugin_load(id: &str) -> String {
        format!("/api/v1/plugins/{id}/load")
    }

    pub fn plugin_call(id: &str, func: &str) -> String {
        format!("/api/v1/plugins/{id}/call/{func}")
    }

    pub fn problem(id: i32) -> String {
        format!("/api/v1/problems/{id}")
    }

    pub fn test_cases(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/test-cases")
    }

    pub fn test_case(problem_id: i32, tc_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/test-cases/{tc_id}")
    }

    pub fn test_cases_upload(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/test-cases/upload")
    }

    pub const CONTESTS: &str = "/api/v1/contests";

    pub fn contest(id: i32) -> String {
        format!("/api/v1/contests/{id}")
    }

    pub fn contest_problems(id: i32) -> String {
        format!("/api/v1/contests/{id}/problems")
    }

    pub fn contest_problem(id: i32, problem_id: i32) -> String {
        format!("/api/v1/contests/{id}/problems/{problem_id}")
    }

    pub fn contest_participants(id: i32) -> String {
        format!("/api/v1/contests/{id}/participants")
    }

    pub fn contest_participant(id: i32, user_id: i32) -> String {
        format!("/api/v1/contests/{id}/participants/{user_id}")
    }

    pub fn contest_problems_reorder(id: i32) -> String {
        format!("/api/v1/contests/{id}/problems/reorder")
    }

    pub fn contest_register(id: i32) -> String {
        format!("/api/v1/contests/{id}/register")
    }

    pub fn test_cases_reorder(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/test-cases/reorder")
    }

    pub const SUBMISSIONS: &str = "/api/v1/submissions";

    pub fn submission(id: i32) -> String {
        format!("/api/v1/submissions/{id}")
    }

    pub fn submission_rejudge(id: i32) -> String {
        format!("/api/v1/submissions/{id}/rejudge")
    }

    pub fn problem_submissions(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/submissions")
    }

    pub fn contest_submissions(contest_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/submissions")
    }

    pub fn contest_problem_submissions(contest_id: i32, problem_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/problems/{problem_id}/submissions")
    }

    pub const DLQ: &str = "/api/v1/dlq";
    pub const DLQ_STATS: &str = "/api/v1/dlq/stats";

    pub fn dlq_message(id: i32) -> String {
        format!("/api/v1/dlq/{id}")
    }

    pub fn dlq_retry(id: i32) -> String {
        format!("/api/v1/dlq/{id}/retry")
    }
}

/// A running test server.
pub struct TestApp {
    pub addr: SocketAddr,
    pub client: Client,
    pub db: DatabaseConnection,
}

/// Path to the test fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Parsed HTTP response for test assertions.
pub struct TestResponse {
    pub status: u16,
    /// Raw response body as text.
    pub text: String,
    /// Parsed JSON body, or `Null` if the response is not valid JSON.
    pub body: Value,
}

impl TestApp {
    pub async fn spawn() -> Self {
        let port = shared_pg_port().await;
        let db_name = format!("test_{}", DB_COUNTER.fetch_add(1, Ordering::Relaxed));

        let admin_opts = ConnectOptions::new(format!(
            "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
        ));
        let admin_db = Database::connect(admin_opts)
            .await
            .expect("Failed to connect to admin database");
        admin_db
            .execute_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("CREATE DATABASE \"{db_name}\" TEMPLATE template_test"),
            ))
            .await
            .expect("Failed to create test database from template");
        drop(admin_db);

        let db_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/{db_name}");
        let mut opts = ConnectOptions::new(&db_url);
        opts.max_connections(5).min_connections(1);
        let db = Database::connect(opts)
            .await
            .expect("Failed to connect to test database");

        let app_config = AppConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                cors: CorsConfig {
                    allow_origins: vec![],
                    max_age: 3600,
                },
            },
            database: DatabaseConfig {
                url: db_url.clone(),
            },
            auth: AuthConfig {
                jwt_secret: "test-secret-for-integration-tests".to_string(),
            },
            plugin: PluginConfig {
                plugins_dir: fixtures_dir(),
                ..Default::default()
            },
            submission: SubmissionConfig::default(),
            mq: MqAppConfig {
                enabled: false,
                ..Default::default()
            },
        };

        let state = AppState {
            plugins: Arc::new(ServerManager::new(app_config.plugin.clone(), db.clone())),
            db: db.clone(),
            config: app_config,
            mq: None,
        };

        let app = server::build_router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            addr,
            client: Client::new(),
            db,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub async fn post_with_token(&self, path: &str, body: &Value, token: &str) -> TestResponse {
        let res = self
            .client
            .post(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .expect("Failed to send POST request");

        TestResponse::from_response(res).await
    }

    pub async fn post_without_token(&self, path: &str, body: &Value) -> TestResponse {
        let res = self
            .client
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .expect("Failed to send POST request");

        TestResponse::from_response(res).await
    }

    pub async fn get_with_token(&self, path: &str, token: &str) -> TestResponse {
        let res = self
            .client
            .get(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("Failed to send GET request");

        TestResponse::from_response(res).await
    }

    pub async fn get_without_token(&self, path: &str) -> TestResponse {
        let res = self
            .client
            .get(self.url(path))
            .send()
            .await
            .expect("Failed to send GET request");

        TestResponse::from_response(res).await
    }

    pub async fn patch_with_token(&self, path: &str, body: &Value, token: &str) -> TestResponse {
        let res = self
            .client
            .patch(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .expect("Failed to send PATCH request");

        TestResponse::from_response(res).await
    }

    pub async fn put_with_token(&self, path: &str, body: &Value, token: &str) -> TestResponse {
        let res = self
            .client
            .put(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .expect("Failed to send PUT request");

        TestResponse::from_response(res).await
    }

    pub async fn delete_with_token(&self, path: &str, token: &str) -> TestResponse {
        let res = self
            .client
            .delete(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("Failed to send DELETE request");

        TestResponse::from_response(res).await
    }

    pub async fn upload_with_token(
        &self,
        path: &str,
        file_name: &str,
        file_bytes: Vec<u8>,
        token: &str,
    ) -> TestResponse {
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string())
            .mime_str("application/zip")
            .expect("Failed to set MIME type");
        let form = reqwest::multipart::Form::new().part("file", part);

        let res = self
            .client
            .post(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .multipart(form)
            .send()
            .await
            .expect("Failed to send multipart upload request");

        TestResponse::from_response(res).await
    }

    /// Register a user and log in, returning the auth token.
    pub async fn create_authenticated_user(&self, username: &str, password: &str) -> String {
        let body = serde_json::json!({
            "username": username,
            "password": password,
        });

        let reg = self.post_without_token(routes::REGISTER, &body).await;
        assert_eq!(reg.status, 201, "Registration failed: {}", reg.text);

        let res = self.post_without_token(routes::LOGIN, &body).await;
        assert_eq!(res.status, 200, "Login failed: {}", res.text);

        res.body["token"]
            .as_str()
            .expect("Login response should contain a token")
            .to_string()
    }

    /// Create a problem via the API and return its `id`.
    pub async fn create_problem(&self, token: &str, title: &str) -> i32 {
        let res = self
            .post_with_token(
                routes::PROBLEMS,
                &serde_json::json!({
                    "title": title,
                    "content": "## Description\nSolve this.",
                    "time_limit": 1000,
                    "memory_limit": 262144,
                }),
                token,
            )
            .await;
        assert_eq!(res.status, 201, "create_problem failed: {}", res.text);
        res.id()
    }

    /// Create a test case for a problem via the API and return its `id`.
    pub async fn create_test_case(&self, problem_id: i32, token: &str) -> i32 {
        let res = self
            .post_with_token(
                &routes::test_cases(problem_id),
                &serde_json::json!({
                    "input": "5\n1 2 3 4 5",
                    "expected_output": "15",
                    "score": 10,
                    "is_sample": true,
                }),
                token,
            )
            .await;
        assert_eq!(res.status, 201, "create_test_case failed: {}", res.text);
        res.id()
    }

    /// Create a contest via the API and return its `id`.
    pub async fn create_contest(
        &self,
        token: &str,
        title: &str,
        is_public: bool,
        submissions_visible: bool,
    ) -> i32 {
        let res = self
            .post_with_token(
                routes::CONTESTS,
                &serde_json::json!({
                    "title": title,
                    "description": "Contest description",
                    "start_time": "2020-01-01T00:00:00Z",
                    "end_time": "2099-01-02T00:00:00Z",
                    "is_public": is_public,
                    "submissions_visible": submissions_visible,
                }),
                token,
            )
            .await;
        assert_eq!(res.status, 201, "create_contest failed: {}", res.text);
        res.id()
    }

    /// Create a submission via the API and return its `id`.
    pub async fn create_submission(
        &self,
        problem_id: i32,
        token: &str,
        language: &str,
        code: &str,
    ) -> i32 {
        let res = self
            .post_with_token(
                &routes::problem_submissions(problem_id),
                &serde_json::json!({
                    "files": [{"filename": "main.cpp", "content": code}],
                    "language": language,
                }),
                token,
            )
            .await;
        assert_eq!(res.status, 201, "create_submission failed: {}", res.text);
        res.id()
    }

    /// Add a problem to a contest via the API.
    pub async fn add_problem_to_contest(&self, contest_id: i32, problem_id: i32, token: &str) {
        let res = self
            .post_with_token(
                &routes::contest_problems(contest_id),
                &serde_json::json!({
                    "problem_id": problem_id,
                    "label": "A",
                }),
                token,
            )
            .await;
        assert_eq!(
            res.status, 201,
            "add_problem_to_contest failed: {}",
            res.text
        );
    }

    /// Register a user as a participant in a contest.
    pub async fn register_for_contest(&self, contest_id: i32, token: &str) {
        let res = self
            .post_with_token(
                &routes::contest_register(contest_id),
                &serde_json::json!({}),
                token,
            )
            .await;
        assert_eq!(res.status, 201, "register_for_contest failed: {}", res.text);
    }

    /// Register a user with a specific role, then log in and return the auth token.
    pub async fn create_user_with_role(
        &self,
        username: &str,
        password: &str,
        role: &str,
    ) -> String {
        let body = serde_json::json!({
            "username": username,
            "password": password,
        });

        let reg = self.post_without_token(routes::REGISTER, &body).await;
        assert_eq!(reg.status, 201, "Registration failed: {}", reg.text);

        let db_user = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(&self.db)
            .await
            .expect("DB query failed")
            .expect("User not found after registration");

        let mut active: user::ActiveModel = db_user.into();
        active.role = Set(role.to_string());
        user::Entity::update(active)
            .exec(&self.db)
            .await
            .expect("Failed to update user role");

        let res = self.post_without_token(routes::LOGIN, &body).await;
        assert_eq!(res.status, 200, "Login failed: {}", res.text);

        res.body["token"]
            .as_str()
            .expect("Login response should contain a token")
            .to_string()
    }
}

impl TestResponse {
    pub async fn from_response(res: reqwest::Response) -> Self {
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Self { status, text, body }
    }

    pub fn id(&self) -> i32 {
        self.body["id"]
            .as_i64()
            .expect("response body should contain 'id'") as i32
    }
}
