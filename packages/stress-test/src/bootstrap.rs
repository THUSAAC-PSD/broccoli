use std::collections::HashMap;

use tracing::info;

use crate::client::Client;
use crate::dto::{
    AddContestProblemRequest, CreateContestRequest, CreateProblemRequest, CreateTestCaseRequest,
    RegistriesResponse,
};
use crate::error::{StressError, StressResult};
use crate::scenarios::Scenario;

#[derive(Debug, Clone)]
pub struct BootstrapState {
    pub contest_type: String,
    pub problem_type: String,
    pub contest_id: i32,
    pub problem_ids_by_scenario: HashMap<&'static str, i32>,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapConfig {
    pub contest_type: Option<String>,
    pub problem_type: Option<String>,
}

pub async fn bootstrap(
    client: &Client,
    scenarios: &[Scenario],
    config: &BootstrapConfig,
) -> StressResult<BootstrapState> {
    let registries = client.list_registries().await?;
    let contest_type = resolve_contest_type(&registries, config.contest_type.as_deref())?;
    let problem_type = resolve_problem_type(&registries, config.problem_type.as_deref())?;

    info!(
        contest_type = %contest_type,
        problem_type = %problem_type,
        "stress-test bootstrap resolved registry types",
    );

    let contest = client
        .create_contest(&build_create_contest_request(&contest_type))
        .await?;
    info!(contest_id = contest.id, "bootstrap created scratch contest",);

    let mut problem_ids_by_scenario = HashMap::with_capacity(scenarios.len());

    for (idx, scenario) in scenarios.iter().enumerate() {
        let req = build_create_problem_request(scenario, &contest_type, &problem_type);
        let problem = client.create_problem(&req).await?;

        let tc_req = CreateTestCaseRequest {
            input: scenario.test_input.to_string(),
            expected_output: scenario.test_expected_output.to_string(),
            score: 100,
            is_sample: false,
            label: None,
        };
        client.create_test_case(problem.id, &tc_req).await?;

        let attach_req = AddContestProblemRequest {
            problem_id: problem.id,
            label: contest_label(idx),
            position: None,
        };
        client
            .add_problem_to_contest(contest.id, &attach_req)
            .await?;

        info!(
            scenario_id = scenario.id,
            problem_id = problem.id,
            "bootstrap created problem + test case",
        );
        problem_ids_by_scenario.insert(scenario.id, problem.id);
    }

    Ok(BootstrapState {
        contest_type,
        problem_type,
        contest_id: contest.id,
        problem_ids_by_scenario,
    })
}

fn build_create_contest_request(contest_type: &str) -> CreateContestRequest {
    let now = chrono::Utc::now();
    let suffix = now.timestamp();
    CreateContestRequest {
        title: format!("stress-test-{suffix}"),
        description: "Auto-created by broccoli-stress-test. \
                      Will be deleted after the run unless --keep-fixtures is set."
            .into(),
        start_time: now - chrono::Duration::seconds(60),
        end_time: now + chrono::Duration::hours(24),
        is_public: false,
        contest_type: Some(contest_type.to_string()),
    }
}

fn contest_label(index: usize) -> String {
    const ALPHA: usize = (b'Z' - b'A' + 1) as usize;
    if index < ALPHA {
        ((b'A' + index as u8) as char).to_string()
    } else {
        format!("X{index}")
    }
}

fn resolve_contest_type(
    registries: &RegistriesResponse,
    override_value: Option<&str>,
) -> StressResult<String> {
    resolve_type(
        "contest_type",
        &registries.contest_types,
        override_value,
        empty_contest_types_hint,
    )
}

fn resolve_problem_type(
    registries: &RegistriesResponse,
    override_value: Option<&str>,
) -> StressResult<String> {
    resolve_type(
        "problem_type",
        &registries.problem_types,
        override_value,
        empty_problem_types_hint,
    )
}

fn resolve_type(
    label: &str,
    available: &[String],
    override_value: Option<&str>,
    empty_hint: fn() -> &'static str,
) -> StressResult<String> {
    if let Some(requested) = override_value {
        if available.iter().any(|t| t == requested) {
            return Ok(requested.to_string());
        }
        return Err(StressError::Other(anyhow::anyhow!(
            "{label} override `{requested}` is not registered on the server. \
             Available {label}s: [{}]. {}",
            available.join(", "),
            empty_hint(),
        )));
    }

    if let Some(first) = available.first() {
        return Ok(first.clone());
    }

    Err(StressError::Other(anyhow::anyhow!(
        "no {label}s registered on the server. {}",
        empty_hint(),
    )))
}

fn empty_contest_types_hint() -> &'static str {
    "Load a contest plugin (e.g. `icpc` or `ioi`) plus `batch-evaluator` so \
     the server's plugin registries are populated, then re-run the stress test."
}

fn empty_problem_types_hint() -> &'static str {
    "Load `batch-evaluator` (and a contest plugin such as `icpc`) so the \
     server's plugin registries are populated, then re-run the stress test."
}

fn build_create_problem_request(
    scenario: &Scenario,
    contest_type: &str,
    problem_type: &str,
) -> CreateProblemRequest {
    let mut submission_format = HashMap::new();
    submission_format.insert(
        scenario.language.to_string(),
        scenario
            .files
            .iter()
            .map(|(name, _)| name.to_string())
            .collect(),
    );

    CreateProblemRequest {
        title: format!("stress-test:{}", scenario.id),
        content: format!(
            "Auto-created by broccoli-stress-test for scenario `{}`. \n\
             This problem will be deleted after the run unless --keep-fixtures is set.",
            scenario.id,
        ),
        time_limit: scenario.time_limit_ms,
        memory_limit: scenario.memory_limit_kb,
        problem_type: problem_type.to_string(),
        checker_format: scenario.checker_format.to_string(),
        default_contest_type: contest_type.to_string(),
        show_test_details: None,
        submission_format: Some(submission_format),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{AuthCreds, Client};
    use crate::scenarios::SCENARIOS;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    fn login_body(token: &str) -> Value {
        json!({
            "token": token,
            "id": 1,
            "username": "admin",
            "roles": ["admin"],
            "permissions": []
        })
    }

    async fn build_client(server: &MockServer) -> Client {
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(login_body("t")))
            .mount(server)
            .await;

        Client::new(
            server.uri(),
            AuthCreds::UsernamePassword {
                username: "admin".into(),
                password: "secret".into(),
            },
        )
        .await
        .expect("client builds")
    }

    fn problem_response_json(id: i32, problem_type: &str, contest_type: &str) -> Value {
        json!({
            "id": id,
            "title": "stress-test:x",
            "content": "...",
            "time_limit": 1000,
            "memory_limit": 65536,
            "problem_type": problem_type,
            "checker_source": null,
            "checker_format": "exact",
            "default_contest_type": contest_type,
            "show_test_details": false,
            "submission_format": null,
            "samples": [],
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:00Z"
        })
    }

    fn test_case_response_json(id: i32, problem_id: i32) -> Value {
        json!({
            "id": id,
            "input": "1\n",
            "expected_output": "1\n",
            "score": 100,
            "description": null,
            "label": "01",
            "is_sample": false,
            "position": 0,
            "problem_id": problem_id,
            "created_at": "2026-05-01T00:00:00Z"
        })
    }

    #[derive(Default)]
    struct CapturingMatcher {
        captured: std::sync::Mutex<Vec<Value>>,
    }

    impl CapturingMatcher {
        fn snapshot(&self) -> Vec<Value> {
            self.captured.lock().unwrap().clone()
        }
    }

    impl wiremock::Match for &CapturingMatcher {
        fn matches(&self, req: &Request) -> bool {
            if let Ok(parsed) = serde_json::from_slice::<Value>(&req.body) {
                self.captured.lock().unwrap().push(parsed);
            }
            true
        }
    }

    async fn mount_problem_creation_with_capture(
        server: &MockServer,
        capture: &'static CapturingMatcher,
    ) {
        Mock::given(method("POST"))
            .and(path("/api/v1/problems"))
            .and(capture)
            .respond_with(|req: &Request| {
                let body_len = req.body.len() as i32;
                let id = 1000 + body_len;
                let parsed: Value = serde_json::from_slice(&req.body).unwrap();
                let problem_type = parsed
                    .get("problem_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("batch")
                    .to_string();
                let contest_type = parsed
                    .get("default_contest_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("icpc")
                    .to_string();
                ResponseTemplate::new(201).set_body_json(problem_response_json(
                    id,
                    &problem_type,
                    &contest_type,
                ))
            })
            .expect(9)
            .mount(server)
            .await;
    }

    async fn mount_test_case_creation(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path_regex(r"^/api/v1/problems/\d+/test-cases$"))
            .respond_with(ResponseTemplate::new(201).set_body_json(test_case_response_json(1, 1)))
            .expect(9)
            .mount(server)
            .await;
    }

    async fn mount_contest_creation(server: &MockServer, contest_id: i32, contest_type: &str) {
        Mock::given(method("POST"))
            .and(path("/api/v1/contests"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": contest_id,
                "title": "stress-test-scratch",
                "contest_type": contest_type,
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    async fn mount_contest_problem_attach(server: &MockServer, contest_id: i32) {
        Mock::given(method("POST"))
            .and(path(format!("/api/v1/contests/{contest_id}/problems")))
            .respond_with(|req: &Request| {
                let parsed: Value = serde_json::from_slice(&req.body).unwrap();
                let pid = parsed
                    .get("problem_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                let label = parsed
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("A")
                    .to_string();
                ResponseTemplate::new(201).set_body_json(json!({
                    "contest_id": 9001,
                    "problem_id": pid,
                    "label": label,
                    "position": 0,
                    "problem_title": "stress-test:x",
                }))
            })
            .expect(9)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn happy_path_creates_one_problem_and_one_test_case_per_scenario() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/plugins/registries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "problem_types": ["batch"],
                "checker_formats": ["exact", "tokens-case-insensitive"],
                "contest_types": ["icpc"],
                "languages": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        mount_contest_creation(&server, 9001, "icpc").await;
        mount_contest_problem_attach(&server, 9001).await;
        let capture: &'static CapturingMatcher = Box::leak(Box::default());
        mount_problem_creation_with_capture(&server, capture).await;
        mount_test_case_creation(&server).await;

        let state = bootstrap(&client, SCENARIOS, &BootstrapConfig::default())
            .await
            .expect("bootstrap succeeds");

        assert_eq!(state.contest_type, "icpc");
        assert_eq!(state.problem_type, "batch");
        assert_eq!(state.contest_id, 9001);
        assert_eq!(state.problem_ids_by_scenario.len(), 9);
        for s in SCENARIOS {
            assert!(
                state.problem_ids_by_scenario.contains_key(s.id),
                "missing problem id for scenario `{}`",
                s.id
            );
        }

        let captured = capture.snapshot();
        assert_eq!(captured.len(), 9);

        for body in &captured {
            let title = body.get("title").and_then(|v| v.as_str()).unwrap();
            let scenario = SCENARIOS
                .iter()
                .find(|s| title == format!("stress-test:{}", s.id))
                .expect("captured request matches a known scenario");

            let fmt = body
                .get("submission_format")
                .expect("submission_format must be present");
            assert!(!fmt.is_null(), "submission_format must not be null");
            let lang_files = fmt
                .get(scenario.language)
                .unwrap_or_else(|| panic!("missing language `{}` for {}", scenario.language, title))
                .as_array()
                .expect("language entry must be array");
            let want_filenames: Vec<&str> = scenario.files.iter().map(|(n, _)| *n).collect();
            let got_filenames: Vec<&str> = lang_files.iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(
                got_filenames, want_filenames,
                "filenames for scenario {}",
                scenario.id
            );

            assert_eq!(
                body.get("default_contest_type").unwrap().as_str().unwrap(),
                "icpc"
            );
            assert_eq!(body.get("problem_type").unwrap().as_str().unwrap(), "batch");
            assert_eq!(
                body.get("checker_format").unwrap().as_str().unwrap(),
                scenario.checker_format
            );
        }
    }

    #[tokio::test]
    async fn cli_override_wins_over_first_entry_when_valid() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/plugins/registries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "problem_types": ["batch", "interactive"],
                "checker_formats": ["exact"],
                "contest_types": ["icpc", "ioi"],
                "languages": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        mount_contest_creation(&server, 9002, "ioi").await;
        mount_contest_problem_attach(&server, 9002).await;
        let capture: &'static CapturingMatcher = Box::leak(Box::default());
        mount_problem_creation_with_capture(&server, capture).await;
        mount_test_case_creation(&server).await;

        let state = bootstrap(
            &client,
            SCENARIOS,
            &BootstrapConfig {
                contest_type: Some("ioi".into()),
                problem_type: None,
            },
        )
        .await
        .expect("bootstrap succeeds with valid override");

        assert_eq!(state.contest_type, "ioi");
        assert_eq!(state.problem_type, "batch");
        assert_eq!(state.contest_id, 9002);

        for body in capture.snapshot() {
            assert_eq!(
                body.get("default_contest_type").unwrap().as_str().unwrap(),
                "ioi"
            );
        }
    }

    #[tokio::test]
    async fn override_not_in_registries_fails_with_helpful_message() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/plugins/registries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "problem_types": ["batch"],
                "checker_formats": ["exact"],
                "contest_types": ["icpc"],
                "languages": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = bootstrap(
            &client,
            SCENARIOS,
            &BootstrapConfig {
                contest_type: Some("nonsense".into()),
                problem_type: None,
            },
        )
        .await
        .expect_err("must fail on unknown override");

        let msg = format!("{}", err);
        assert!(
            msg.contains("nonsense"),
            "error must name the bad override; got: {msg}"
        );
        assert!(
            msg.contains("icpc"),
            "error must list available types; got: {msg}"
        );
    }

    #[test]
    fn contest_label_uses_uppercase_letters_and_falls_back_for_overflow() {
        assert_eq!(contest_label(0), "A");
        assert_eq!(contest_label(1), "B");
        assert_eq!(contest_label(8), "I");
        assert_eq!(contest_label(25), "Z");
        assert_eq!(contest_label(26), "X26");
        assert_eq!(contest_label(99), "X99");
    }

    #[tokio::test]
    async fn empty_registries_fails_fast_with_canonical_fix_hint() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/plugins/registries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "problem_types": [],
                "checker_formats": [],
                "contest_types": [],
                "languages": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = bootstrap(&client, SCENARIOS, &BootstrapConfig::default())
            .await
            .expect_err("must fail on empty registries");

        let msg = format!("{}", err);
        assert!(
            msg.contains("batch-evaluator"),
            "error message must name `batch-evaluator` as the canonical fix; got: {msg}"
        );
        assert!(
            msg.contains("contest_type") || msg.contains("contest"),
            "error message must explain which type is missing; got: {msg}"
        );
    }
}
