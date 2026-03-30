use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use plugin_core::config::PluginConfig;
use reqwest::Client;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection,
    DbBackend, EntityTrait, QueryFilter, Set, Statement, TransactionTrait,
};
use serde_json::Value;
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::{Mutex, OnceCell, RwLock};

use common::language::LanguageDefinition;
use common::storage::config::create_blob_store;
use server::config::{
    AppConfig, AuthConfig, BlobStoreConfig, CorsConfig, DatabaseConfig, MqAppConfig, ServerConfig,
    SubmissionConfig,
};
use server::entity::{user, user_role};
use server::manager::ServerManager;
use server::registry::{
    CheckerFormatRegistry, ContestTypeRegistry, EvaluateBatches, EvaluatorRegistry,
    OperationBatches, OperationWaiters,
};
use server::state::AppState;
use server::utils::plugin::sync_plugins;

/// PostgreSQL container shared across all tests in this binary.
static SHARED_PG: OnceCell<(ContainerAsync<Postgres>, u16)> = OnceCell::const_new();

/// Monotonic counter for unique database names.
static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Serializes CREATE DATABASE operations to prevent connection exhaustion.
static CREATE_DB_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Container ID for atexit cleanup.
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

/// Shared admin connection pool for CREATE DATABASE operations.
extern "C" fn cleanup_container() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", "-v", id])
            .output();
    }
}

fn test_languages() -> HashMap<String, LanguageDefinition> {
    HashMap::from([
        (
            "cpp".to_string(),
            LanguageDefinition {
                compile_cmd: Some(vec![
                    "g++".to_string(),
                    "{source}".to_string(),
                    "-O2".to_string(),
                    "-std=c++17".to_string(),
                    "-o".to_string(),
                    "{binary}".to_string(),
                ]),
                run_cmd: vec!["./{binary}".to_string()],
                source_filename: "solution.cpp".to_string(),
                binary_name: "solution".to_string(),
                version_cmd: None,
                basename_fallback: "solution".to_string(),
            },
        ),
        (
            "c".to_string(),
            LanguageDefinition {
                compile_cmd: Some(vec![
                    "gcc".to_string(),
                    "{source}".to_string(),
                    "-O2".to_string(),
                    "-std=c11".to_string(),
                    "-o".to_string(),
                    "{binary}".to_string(),
                ]),
                run_cmd: vec!["./{binary}".to_string()],
                source_filename: "solution.c".to_string(),
                binary_name: "solution".to_string(),
                version_cmd: None,
                basename_fallback: "solution".to_string(),
            },
        ),
        (
            "java".to_string(),
            LanguageDefinition {
                compile_cmd: Some(vec!["javac".to_string(), "{source}".to_string()]),
                run_cmd: vec!["java".to_string(), "{basename}".to_string()],
                source_filename: "{basename}.java".to_string(),
                binary_name: "{basename}.class".to_string(),
                version_cmd: None,
                basename_fallback: "Main".to_string(),
            },
        ),
        (
            "python3".to_string(),
            LanguageDefinition {
                compile_cmd: None,
                run_cmd: vec!["python3".to_string(), "{source}".to_string()],
                source_filename: "solution.py".to_string(),
                binary_name: "solution.py".to_string(),
                version_cmd: None,
                basename_fallback: "solution".to_string(),
            },
        ),
        (
            "javascript".to_string(),
            LanguageDefinition {
                compile_cmd: None,
                run_cmd: vec!["node".to_string(), "{source}".to_string()],
                source_filename: "solution.js".to_string(),
                binary_name: "solution.js".to_string(),
                version_cmd: None,
                basename_fallback: "solution".to_string(),
            },
        ),
    ])
}

/// Start (or reuse) the shared PostgreSQL container, create and initialize a
/// template database, and return the host port.
async fn shared_pg_port() -> u16 {
    let (_, port) = SHARED_PG
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("17-alpine")
                .with_cmd(["postgres", "-c", "max_connections=500"])
                .start()
                .await
                .expect("Failed to start PostgreSQL container");
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("Failed to get PostgreSQL port");

            let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
            let mut template_created = false;
            for attempt in 0..15u32 {
                if attempt > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        500 * u64::from(attempt),
                    ))
                    .await;
                }
                let Ok(admin_db) = Database::connect(ConnectOptions::new(&admin_url)).await else {
                    continue;
                };
                if admin_db
                    .execute_raw(Statement::from_string(
                        DbBackend::Postgres,
                        "CREATE DATABASE \"template_test\"".to_string(),
                    ))
                    .await
                    .is_ok()
                {
                    drop(admin_db);
                    template_created = true;
                    break;
                }
                drop(admin_db);
            }
            assert!(
                template_created,
                "Failed to create template_test database after 15 attempts"
            );

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
    pub const REFRESH: &str = "/api/v1/auth/refresh";
    pub const LOGOUT: &str = "/api/v1/auth/logout";
    pub const ME: &str = "/api/v1/auth/me";

    pub const USERS: &str = "/api/v1/users";

    pub fn user(id: i32) -> String {
        format!("/api/v1/users/{id}")
    }

    pub fn user_roles(id: i32) -> String {
        format!("/api/v1/users/{id}/roles")
    }

    pub fn user_role(id: i32, role_name: &str) -> String {
        format!("/api/v1/users/{id}/roles/{role_name}")
    }

    pub const ROLES: &str = "/api/v1/roles";

    pub fn role_permissions(role_name: &str) -> String {
        format!("/api/v1/roles/{role_name}/permissions")
    }

    pub fn role_permission(role_name: &str, permission_name: &str) -> String {
        format!("/api/v1/roles/{role_name}/permissions/{permission_name}")
    }

    pub fn admin_plugin_details(id: &str) -> String {
        format!("/api/v1/admin/plugins/{id}")
    }

    pub fn admin_plugin_enable(id: &str) -> String {
        format!("/api/v1/admin/plugins/{id}/enable")
    }

    pub fn admin_plugin_disable(id: &str) -> String {
        format!("/api/v1/admin/plugins/{id}/disable")
    }

    pub fn plugin_proxy(id: &str, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("/api/v1/p/{id}/{path}")
    }

    pub fn plugin_proxy_with_query(id: &str, path: &str, query: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("/api/v1/p/{id}/{path}/?{query}")
    }

    pub fn plugin_asset(id: &str, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("/assets/{id}/{path}")
    }

    pub const PROBLEMS: &str = "/api/v1/problems";

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

    pub fn contest_my_info(id: i32) -> String {
        format!("/api/v1/contests/{id}/me")
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

    pub fn problem_code_runs(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/code-runs")
    }

    pub fn contest_problem_code_runs(contest_id: i32, problem_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/problems/{problem_id}/code-runs")
    }

    pub fn code_run(id: i32) -> String {
        format!("/api/v1/code-runs/{id}")
    }

    pub fn contest_clarifications(contest_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/clarifications")
    }

    pub fn contest_clarification_reply(contest_id: i32, clar_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/clarifications/{clar_id}/reply")
    }

    pub fn contest_clarification_toggle(contest_id: i32, clar_id: i32, reply_id: i32) -> String {
        format!(
            "/api/v1/contests/{contest_id}/clarifications/{clar_id}/replies/{reply_id}/toggle-public"
        )
    }

    pub fn contest_clarification_resolve(contest_id: i32, clar_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/clarifications/{clar_id}/resolve")
    }

    pub const DLQ: &str = "/api/v1/dlq";
    pub const DLQ_STATS: &str = "/api/v1/dlq/stats";

    pub fn dlq_message(id: i32) -> String {
        format!("/api/v1/dlq/{id}")
    }

    pub fn dlq_retry(id: i32) -> String {
        format!("/api/v1/dlq/{id}/retry")
    }

    pub fn test_cases_bulk(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/test-cases/bulk")
    }

    pub fn contest_problems_bulk(contest_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/problems/bulk")
    }

    pub fn contest_participants_bulk(contest_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/participants/bulk")
    }

    pub const DLQ_BULK_RETRY: &str = "/api/v1/dlq/bulk-retry";
    pub const DLQ_BULK: &str = "/api/v1/dlq/bulk";
    pub const SUBMISSIONS_BULK_REJUDGE: &str = "/api/v1/submissions/bulk-rejudge";

    pub fn attachments(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/attachments")
    }

    pub fn attachment(problem_id: i32, ref_id: &str) -> String {
        format!("/api/v1/problems/{problem_id}/attachments/{ref_id}")
    }

    pub fn problem_config(problem_id: i32) -> String {
        format!("/api/v1/problems/{problem_id}/config")
    }

    pub fn problem_config_ns(problem_id: i32, plugin_id: &str, namespace: &str) -> String {
        format!("/api/v1/problems/{problem_id}/config/{plugin_id}/{namespace}")
    }

    pub fn contest_config(contest_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/config")
    }

    pub fn contest_config_ns(contest_id: i32, plugin_id: &str, namespace: &str) -> String {
        format!("/api/v1/contests/{contest_id}/config/{plugin_id}/{namespace}")
    }

    pub fn plugin_global_config(plugin_id: &str) -> String {
        format!("/api/v1/admin/plugins/{plugin_id}/config")
    }

    pub fn plugin_global_config_ns(plugin_id: &str, namespace: &str) -> String {
        format!("/api/v1/admin/plugins/{plugin_id}/config/{namespace}")
    }

    pub fn contest_problem_config(contest_id: i32, problem_id: i32) -> String {
        format!("/api/v1/contests/{contest_id}/problems/{problem_id}/config")
    }

    pub fn contest_problem_config_ns(
        contest_id: i32,
        problem_id: i32,
        plugin_id: &str,
        namespace: &str,
    ) -> String {
        format!(
            "/api/v1/contests/{contest_id}/problems/{problem_id}/config/{plugin_id}/{namespace}"
        )
    }
}

/// A running test server.
pub struct TestApp {
    pub addr: SocketAddr,
    pub client: Client,
    pub db: DatabaseConnection,
    /// Handle to the spawned axum server task. Aborted on Drop to free connections.
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        // Abort the server task immediately (non-blocking).
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
}

/// Path to the test fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Parsed HTTP response for test assertions.
pub struct TestResponse {
    pub status: u16,
    /// Headers from the response.
    pub headers: reqwest::header::HeaderMap,
    /// Raw response body as text.
    pub text: String,
    /// Parsed JSON body, or `Null` if the response is not valid JSON.
    pub body: Value,
}

impl TestApp {
    /// Spawn a test server WITHOUT loading plugins (fast path for most tests).
    pub async fn spawn() -> Self {
        Self::spawn_internal(false).await
    }

    /// Spawn a test server WITH plugins loaded (slow, only for plugin-specific tests).
    pub async fn spawn_with_plugins() -> Self {
        Self::spawn_internal(true).await
    }

    async fn spawn_internal(load_plugins: bool) -> Self {
        let port = shared_pg_port().await;
        let db_name = format!("test_{}", DB_COUNTER.fetch_add(1, Ordering::Relaxed));

        let _lock = CREATE_DB_LOCK.get_or_init(|| Mutex::new(())).lock().await;

        let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let mut admin_opts = ConnectOptions::new(&admin_url);
        admin_opts
            .max_connections(1) // Only need 1 connection for CREATE DATABASE
            .min_connections(0); // Don't maintain idle connections
        let admin_conn = Database::connect(admin_opts)
            .await
            .expect("Failed to connect to admin database");

        admin_conn
            .execute_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("CREATE DATABASE \"{db_name}\" TEMPLATE template_test"),
            ))
            .await
            .expect("Failed to create test database from template");

        admin_conn.close().await.ok();

        drop(_lock);

        let db_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/{db_name}");
        let mut opts = ConnectOptions::new(&db_url);
        opts.max_connections(2)
            .min_connections(0)
            .idle_timeout(std::time::Duration::from_secs(2));
        let db = Database::connect(opts)
            .await
            .expect("Failed to connect to test database");

        let blob_store = create_blob_store(&BlobStoreConfig::default(), db.clone())
            .await
            .expect("Failed to initialize blob store");

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
            storage: BlobStoreConfig::default(),
            mq: MqAppConfig {
                enabled: false,
                ..Default::default()
            },
            languages: test_languages(),
            batch_max_age_secs: 600,
        };

        let contest_type_registry: ContestTypeRegistry = Arc::new(RwLock::new(HashMap::new()));
        let evaluator_registry: EvaluatorRegistry = Arc::new(RwLock::new(HashMap::new()));
        let checker_format_registry: CheckerFormatRegistry = Arc::new(RwLock::new(HashMap::new()));
        let operation_batches: OperationBatches = Arc::new(dashmap::DashMap::new());
        let operation_waiters: OperationWaiters = Arc::new(dashmap::DashMap::new());
        let evaluate_batches: EvaluateBatches = Arc::new(dashmap::DashMap::new());

        let plugins = ServerManager::new(
            app_config.plugin.clone(),
            db.clone(),
            None, // mq
            operation_batches.clone(),
            operation_waiters.clone(),
            contest_type_registry.clone(),
            evaluator_registry.clone(),
            checker_format_registry.clone(),
            evaluate_batches.clone(),
            app_config.clone(),
            blob_store.clone(),
        )
        .expect("Failed to initialize plugin manager");

        if !load_plugins {
            // Register built-in defaults so create_problem validation passes
            // without needing actual plugins loaded.
            evaluator_registry.write().await.insert(
                "standard".into(),
                server::registry::PluginHandler {
                    plugin_id: "__test__".into(),
                    function_name: "noop".into(),
                },
            );
            checker_format_registry.write().await.insert(
                "exact".into(),
                server::registry::PluginHandler {
                    plugin_id: "__test__".into(),
                    function_name: "noop".into(),
                },
            );
            contest_type_registry.write().await.insert(
                "standard".into(),
                server::registry::ContestTypeHandlers {
                    plugin_id: "__test__".into(),
                    submission_fn: "noop".into(),
                    code_run_fn: "noop".into(),
                },
            );
        }

        let state = AppState {
            plugins,
            db: db.clone(),
            config: app_config,
            mq: None,
            blob_store,
            registries: server::state::RegistryState {
                contest_type_registry,
                evaluator_registry,
                checker_format_registry,
                operation_batches,
                operation_waiters,
                evaluate_batches,
                hook_registry: server::hooks::new_shared_registry(),
            },
            device_codes: std::sync::Arc::new(dashmap::DashMap::new()),
        };
        if load_plugins {
            sync_plugins(&state).await.expect("Failed to sync plugins");
        }

        let app = server::build_router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            addr,
            // Integration tests talk to a local ephemeral axum server.
            // Disable proxy resolution to avoid corporate/system proxies
            // turning localhost calls into spurious 502 responses.
            client: Client::builder()
                .no_proxy()
                .cookie_store(true)
                .build()
                .expect("Failed to build reqwest client"),
            db,
            server_handle: Some(server_handle),
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

    pub async fn delete_with_body_and_token(
        &self,
        path: &str,
        body: &Value,
        token: &str,
    ) -> TestResponse {
        let res = self
            .client
            .delete(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .expect("Failed to send DELETE request with body");

        TestResponse::from_response(res).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upload_with_token(
        &self,
        path: &str,
        file_name: &str,
        file_bytes: Vec<u8>,
        input_format: Option<&str>,
        output_format: Option<&str>,
        strategy: Option<&str>,
        token: &str,
    ) -> TestResponse {
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string())
            .mime_str("application/zip")
            .expect("Failed to set MIME type");
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(input_format) = input_format {
            form = form.text("input_format", input_format.to_string());
        }
        if let Some(output_format) = output_format {
            form = form.text("output_format", output_format.to_string());
        }
        if let Some(s) = strategy {
            form = form.text("strategy", s.to_string());
        }

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
                    "problem_type": "standard",
                    "checker_format": "exact",
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
                    "activate_time": "2020-01-01T00:00:00Z",
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
        let filename = match language {
            "cpp" => "main.cpp",
            "c" => "main.c",
            "java" => "Main.java",
            "python3" => "solution.py",
            "javascript" => "solution.js",
            _ => "main.txt",
        };
        let res = self
            .post_with_token(
                &routes::problem_submissions(problem_id),
                &serde_json::json!({
                    "files": [{"filename": filename, "content": code}],
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

    /// Upload an attachment with optional virtual path.
    pub async fn upload_attachment(
        &self,
        problem_id: i32,
        file_name: &str,
        file_bytes: Vec<u8>,
        path: Option<&str>,
        token: &str,
    ) -> TestResponse {
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.to_string())
            .mime_str("application/octet-stream")
            .expect("Failed to set MIME type");
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(p) = path {
            form = form.text("path", p.to_string());
        }

        let res = self
            .client
            .post(self.url(&routes::attachments(problem_id)))
            .header("Authorization", format!("Bearer {token}"))
            .multipart(form)
            .send()
            .await
            .expect("Failed to send attachment upload request");

        TestResponse::from_response(res).await
    }

    /// Download raw bytes from a URL path.
    pub async fn download_raw(&self, path: &str, token: &str) -> reqwest::Response {
        self.client
            .get(self.url(path))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("Failed to send download request")
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

        let txn = self.db.begin().await.expect("Failed to begin transaction");

        // Clear any existing roles
        user_role::Entity::delete_many()
            .filter(user_role::Column::UserId.eq(db_user.id))
            .exec(&txn)
            .await
            .expect("Failed to clear existing user roles");

        user_role::ActiveModel {
            user_id: Set(db_user.id),
            role: Set(role.to_string()),
        }
        .insert(&txn)
        .await
        .expect("Failed to update user role");

        txn.commit().await.expect("Failed to commit transaction");

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
        let headers = res.headers().clone();
        let text = res.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Self {
            status,
            headers,
            text,
            body,
        }
    }

    pub fn id(&self) -> i32 {
        self.body["id"]
            .as_i64()
            .expect("response body should contain 'id'") as i32
    }
}
