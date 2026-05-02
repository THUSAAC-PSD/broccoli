use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use hdrhistogram::Histogram;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep};
use tracing::warn;

use crate::bootstrap::BootstrapState;
use crate::client::Client;
use crate::dto::{
    CreateSubmissionRequest, SubmissionFileDto, SubmissionResponse, SubmissionStatus, Verdict,
};
use crate::error::{StressError, StressResult};
use crate::events::{ActualTerminal, Event, ExpectedTerminal, Phase};
use crate::scenarios::Scenario;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

const HISTOGRAM_LOW: u64 = 1;
const HISTOGRAM_HIGH: u64 = 600_000;
const HISTOGRAM_SIGFIG: u8 = 3;

#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub total: u64,
    pub rate: u32,
    pub concurrency: u32,
    pub per_job_timeout: Duration,
    pub p95_budget_ms: u64,
    pub seed: u64,
}

#[derive(Debug)]
pub struct LoadOutcome {
    pub completed: u64,
    pub passed: u64,
    pub histogram: Histogram<u64>,
    pub drain_time: Duration,
    pub errors: Vec<(u64, String)>,
    pub passed_budget: bool,
    pub passed_overall: bool,
}

impl LoadOutcome {
    fn empty() -> Self {
        Self {
            completed: 0,
            passed: 0,
            histogram: Histogram::<u64>::new_with_bounds(
                HISTOGRAM_LOW,
                HISTOGRAM_HIGH,
                HISTOGRAM_SIGFIG,
            )
            .expect("static histogram bounds are valid"),
            drain_time: Duration::ZERO,
            errors: Vec::new(),
            passed_budget: true,
            passed_overall: true,
        }
    }
}

fn build_mix<'a>(scenarios: &'a [Scenario], total: u64, seed: u64) -> Vec<&'a Scenario> {
    use rand::Rng;

    let by_id = |id: &str| scenarios.iter().find(|s| s.id == id);

    let ac_ids = ["ab-cpp-ac", "ab-py-ac", "ab-cpp-igncase", "ab-cpp-multi"];
    let ac_scenarios: Vec<&Scenario> = ac_ids.iter().filter_map(|id| by_id(id)).collect();

    struct Bucket<'a> {
        weight: u32,
        items: Vec<&'a Scenario>,
    }

    let mut buckets: Vec<Bucket<'a>> = Vec::new();
    if !ac_scenarios.is_empty() {
        buckets.push(Bucket {
            weight: 700,
            items: ac_scenarios,
        });
    }
    if let Some(s) = by_id("ab-cpp-wa") {
        buckets.push(Bucket {
            weight: 100,
            items: vec![s],
        });
    }
    if let Some(s) = by_id("ab-cpp-tle") {
        buckets.push(Bucket {
            weight: 100,
            items: vec![s],
        });
    }
    if let Some(s) = by_id("ab-cpp-re") {
        buckets.push(Bucket {
            weight: 50,
            items: vec![s],
        });
    }
    if let Some(s) = by_id("ab-cpp-mle") {
        buckets.push(Bucket {
            weight: 50,
            items: vec![s],
        });
    }

    if buckets.is_empty() {
        return Vec::new();
    }

    let total_weight: u32 = buckets.iter().map(|b| b.weight).sum();

    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = Vec::with_capacity(total as usize);
    for _ in 0..total {
        let pick = rng.random_range(0..total_weight);
        let mut acc = 0u32;
        let bucket = buckets
            .iter()
            .find(|b| {
                acc += b.weight;
                pick < acc
            })
            .expect("weights sum to total_weight, pick is within bounds");
        let item_idx = rng.random_range(0..bucket.items.len());
        out.push(bucket.items[item_idx]);
    }
    out
}

fn build_submission_request(scenario: &Scenario, contest_type: &str) -> CreateSubmissionRequest {
    let files = scenario
        .files
        .iter()
        .map(|(filename, content)| SubmissionFileDto {
            filename: (*filename).to_string(),
            content: (*content).to_string(),
        })
        .collect();

    CreateSubmissionRequest {
        files,
        language: scenario.language.to_string(),
        contest_type: Some(contest_type.to_string()),
    }
}

fn scenario_passes(
    scenario: &Scenario,
    actual_status: SubmissionStatus,
    actual_verdict: Option<&Verdict>,
) -> bool {
    if actual_status == SubmissionStatus::SystemError {
        return false;
    }
    if actual_status != scenario.expected_status {
        return false;
    }
    if actual_status == SubmissionStatus::Judged {
        return actual_verdict == scenario.expected_verdict.as_ref();
    }
    true
}

async fn poll_until_terminal(
    client: &Client,
    submission_id: i32,
    timeout: Duration,
) -> StressResult<SubmissionResponse> {
    let deadline = Instant::now() + timeout;
    loop {
        let resp = client.get_submission(submission_id).await?;
        if resp.status.is_terminal() {
            return Ok(resp);
        }
        if Instant::now() >= deadline {
            return Err(StressError::Other(anyhow!(
                "submission {} did not reach a terminal status within {:?} (last status = {:?})",
                submission_id,
                timeout,
                resp.status,
            )));
        }
        sleep(POLL_INTERVAL).await;
    }
}

pub async fn run(
    client: &Client,
    state: &BootstrapState,
    scenarios: &[Scenario],
    config: &LoadConfig,
    tx: &mpsc::UnboundedSender<Event>,
) -> LoadOutcome {
    let _ = tx.send(Event::PhaseStarted { phase: Phase::Load });

    if config.total == 0 || config.rate == 0 || config.concurrency == 0 {
        let outcome = LoadOutcome::empty();
        let _ = tx.send(Event::PhaseFinished {
            phase: Phase::Load,
            ok: outcome.passed_overall,
        });
        return outcome;
    }

    let mix = build_mix(scenarios, config.total, config.seed);
    if mix.is_empty() {
        let mut outcome = LoadOutcome::empty();
        outcome.passed_overall = false;
        outcome
            .errors
            .push((0, "no eligible scenarios in load mix".to_string()));
        let _ = tx.send(Event::Error {
            phase: Some(Phase::Load),
            message: "no eligible scenarios in load mix".to_string(),
        });
        let _ = tx.send(Event::PhaseFinished {
            phase: Phase::Load,
            ok: false,
        });
        return outcome;
    }

    let histogram = Arc::new(Mutex::new(
        Histogram::<u64>::new_with_bounds(HISTOGRAM_LOW, HISTOGRAM_HIGH, HISTOGRAM_SIGFIG)
            .expect("static histogram bounds are valid"),
    ));
    let errors: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let completed = Arc::new(Mutex::new(0u64));
    let passed = Arc::new(Mutex::new(0u64));
    let last_completion = Arc::new(Mutex::new(Instant::now()));

    let semaphore = Arc::new(Semaphore::new(config.concurrency as usize));

    let tick = Duration::from_micros(1_000_000 / config.rate.max(1) as u64);
    let mut ticker = interval(tick.max(Duration::from_micros(1)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut join_set: JoinSet<()> = JoinSet::new();
    let mut last_post: Option<Instant> = None;

    for (idx, scenario) in mix.into_iter().enumerate() {
        ticker.tick().await;

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore is never closed");

        let sequence = idx as u64;
        let scenario_id = scenario.id.to_string();
        let _ = tx.send(Event::LoadSubmitted {
            sequence,
            scenario: scenario_id.clone(),
        });

        let problem_id = match state.problem_ids_by_scenario.get(scenario.id) {
            Some(id) => *id,
            None => {
                let msg = format!(
                    "scenario `{}` has no bootstrapped problem id; bug in caller",
                    scenario.id
                );
                let _ = tx.send(Event::Error {
                    phase: Some(Phase::Load),
                    message: msg.clone(),
                });
                errors.lock().await.push((sequence, msg));
                drop(permit);
                continue;
            }
        };

        let req = build_submission_request(scenario, &state.contest_type);
        let contest_id = state.contest_id;
        let client = client.clone();
        let tx = tx.clone();
        let histogram = histogram.clone();
        let errors = errors.clone();
        let completed = completed.clone();
        let passed = passed.clone();
        let last_completion = last_completion.clone();
        let scenario_clone = scenario.clone();
        let per_job_timeout = config.per_job_timeout;
        let expected_status = scenario.expected_status;
        let expected_verdict = scenario.expected_verdict.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let started = Instant::now();

            let post_result = client
                .create_contest_submission(contest_id, problem_id, &req)
                .await;
            let submitted = match post_result {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!(
                        "submission #{}: scenario `{}`: submit failed: {}",
                        sequence, scenario_clone.id, e,
                    );
                    let _ = tx.send(Event::Error {
                        phase: Some(Phase::Load),
                        message: msg.clone(),
                    });
                    errors.lock().await.push((sequence, msg));
                    *last_completion.lock().await = Instant::now();
                    let _ = tx.send(Event::LoadCompleted {
                        sequence,
                        ok: false,
                        latency_ms: started.elapsed().as_millis() as u64,
                        expected: ExpectedTerminal {
                            status: expected_status,
                            verdict: expected_verdict.clone(),
                        },
                        actual: ActualTerminal {
                            status: SubmissionStatus::Pending,
                            verdict: None,
                        },
                    });
                    return;
                }
            };

            let polled = poll_until_terminal(&client, submitted.id, per_job_timeout).await;
            match polled {
                Ok(final_resp) => {
                    let actual_status = final_resp.status;
                    let actual_verdict = final_resp
                        .result
                        .as_ref()
                        .and_then(|r| r.verdict.as_ref())
                        .cloned();
                    let ok = scenario_passes(
                        &scenario_clone,
                        actual_status,
                        actual_verdict.as_ref(),
                    );
                    let latency_ms = started.elapsed().as_millis() as u64;

                    *completed.lock().await += 1;

                    if ok {
                        *passed.lock().await += 1;
                        let mut h = histogram.lock().await;
                        let v = latency_ms.clamp(HISTOGRAM_LOW, HISTOGRAM_HIGH);
                        let _ = h.record(v);
                    } else {
                        let msg = format!(
                            "submission #{}: scenario `{}`: expected ({:?}, {:?}), actual ({:?}, {:?})",
                            sequence,
                            scenario_clone.id,
                            scenario_clone.expected_status,
                            scenario_clone.expected_verdict,
                            actual_status,
                            actual_verdict,
                        );
                        warn!(
                            sequence,
                            scenario_id = scenario_clone.id,
                            "load submission verdict mismatch",
                        );
                        errors.lock().await.push((sequence, msg));
                    }

                    *last_completion.lock().await = Instant::now();
                    let _ = tx.send(Event::LoadCompleted {
                        sequence,
                        ok,
                        latency_ms,
                        expected: ExpectedTerminal {
                            status: scenario_clone.expected_status,
                            verdict: scenario_clone.expected_verdict.clone(),
                        },
                        actual: ActualTerminal {
                            status: actual_status,
                            verdict: actual_verdict,
                        },
                    });
                }
                Err(e) => {
                    let msg = format!(
                        "submission #{}: scenario `{}`: poll failed: {}",
                        sequence, scenario_clone.id, e,
                    );
                    let _ = tx.send(Event::Error {
                        phase: Some(Phase::Load),
                        message: msg.clone(),
                    });
                    errors.lock().await.push((sequence, msg));
                    *last_completion.lock().await = Instant::now();
                    let _ = tx.send(Event::LoadCompleted {
                        sequence,
                        ok: false,
                        latency_ms: started.elapsed().as_millis() as u64,
                        expected: ExpectedTerminal {
                            status: expected_status,
                            verdict: expected_verdict.clone(),
                        },
                        actual: ActualTerminal {
                            status: SubmissionStatus::Pending,
                            verdict: None,
                        },
                    });
                }
            }
        });

        last_post = Some(Instant::now());
    }

    while let Some(joined) = join_set.join_next().await {
        if let Err(e) = joined {
            let msg = format!("load worker task panicked: {}", e);
            errors.lock().await.push((0, msg.clone()));
            let _ = tx.send(Event::Error {
                phase: Some(Phase::Load),
                message: msg,
            });
        }
    }

    let last_post_at = last_post.unwrap_or_else(Instant::now);
    let last_completion_at = *last_completion.lock().await;
    let drain_time = last_completion_at
        .checked_duration_since(last_post_at)
        .unwrap_or(Duration::ZERO);

    let histogram = Arc::try_unwrap(histogram)
        .expect("no other strong refs after JoinSet drains")
        .into_inner();
    let errors = Arc::try_unwrap(errors)
        .expect("no other strong refs after JoinSet drains")
        .into_inner();
    let completed = *completed.lock().await;
    let passed = *passed.lock().await;

    let p95 = if histogram.is_empty() {
        0
    } else {
        histogram.value_at_quantile(0.95)
    };
    let passed_budget = p95 <= config.p95_budget_ms;
    let passed_overall =
        completed == config.total && passed == config.total && errors.is_empty() && passed_budget;

    let _ = tx.send(Event::PhaseFinished {
        phase: Phase::Load,
        ok: passed_overall,
    });

    LoadOutcome {
        completed,
        passed,
        histogram,
        drain_time,
        errors,
        passed_budget,
        passed_overall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{AuthCreds, Client};
    use crate::scenarios::SCENARIOS;
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn login_body(token: &str) -> Value {
        json!({
            "token": token,
            "id": 1,
            "username": "admin",
            "roles": ["admin"],
            "permissions": []
        })
    }

    async fn build_client_with_login(server: &MockServer, token: &str) -> Client {
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(login_body(token)))
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

    const TEST_CONTEST_ID: i32 = 9001;

    fn make_state() -> BootstrapState {
        BootstrapState {
            contest_type: "icpc".into(),
            problem_type: "batch".into(),
            contest_id: TEST_CONTEST_ID,
            problem_ids_by_scenario: SCENARIOS
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, 100 + i as i32))
                .collect(),
        }
    }

    fn submission_json(id: i32, status: &str, verdict: Option<&str>) -> Value {
        let result = match verdict {
            Some(v) => json!({
                "verdict": v,
                "score": 100.0,
                "time_used": 10,
                "memory_used": 1024,
                "compile_output": null,
                "error_message": null,
                "judged_at": "2026-05-01T00:00:00Z",
                "test_case_results": []
            }),
            None => Value::Null,
        };
        json!({
            "id": id,
            "language": "cpp",
            "status": status,
            "user_id": 1,
            "username": "admin",
            "problem_id": 100,
            "problem_title": "stress-test",
            "contest_id": null,
            "contest_type": "icpc",
            "judge_epoch": 0,
            "created_at": "2026-05-01T00:00:00Z",
            "result": result,
        })
    }

    fn verdict_wire(v: &Verdict) -> &'static str {
        match v {
            Verdict::Accepted => "Accepted",
            Verdict::WrongAnswer => "WrongAnswer",
            Verdict::TimeLimitExceeded => "TimeLimitExceeded",
            Verdict::MemoryLimitExceeded => "MemoryLimitExceeded",
            Verdict::RuntimeError => "RuntimeError",
            Verdict::SystemError => "SystemError",
            Verdict::Skipped => "Skipped",
            Verdict::Other(_) => "Other",
        }
    }

    fn submission_status_wire(s: SubmissionStatus) -> &'static str {
        match s {
            SubmissionStatus::Pending => "Pending",
            SubmissionStatus::Compiling => "Compiling",
            SubmissionStatus::Running => "Running",
            SubmissionStatus::Judged => "Judged",
            SubmissionStatus::CompilationError => "CompilationError",
            SubmissionStatus::SystemError => "SystemError",
        }
    }

    async fn mount_per_scenario_mocks(
        server: &MockServer,
        state: &BootstrapState,
        outcomes: &HashMap<&'static str, (SubmissionStatus, Option<Verdict>)>,
    ) {
        for (idx, scenario) in SCENARIOS.iter().enumerate() {
            let problem_id = state.problem_ids_by_scenario[scenario.id];
            let submission_id = 1000 + idx as i32;
            let post_path = format!(
                "/api/v1/contests/{}/problems/{}/submissions",
                TEST_CONTEST_ID, problem_id
            );
            let get_path = format!("/api/v1/submissions/{}", submission_id);

            Mock::given(method("POST"))
                .and(path(post_path.clone()))
                .respond_with(ResponseTemplate::new(201).set_body_json(submission_json(
                    submission_id,
                    "Pending",
                    None,
                )))
                .mount(server)
                .await;

            let (status, verdict) = outcomes
                .get(scenario.id)
                .cloned()
                .unwrap_or_else(|| (scenario.expected_status, scenario.expected_verdict.clone()));
            let v_wire = verdict.as_ref().map(verdict_wire);
            Mock::given(method("GET"))
                .and(path(get_path))
                .respond_with(ResponseTemplate::new(200).set_body_json(submission_json(
                    submission_id,
                    submission_status_wire(status),
                    v_wire,
                )))
                .mount(server)
                .await;
        }
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    #[tokio::test]
    async fn every_submission_terminates_and_matches_expectation() {
        let server = MockServer::start().await;
        let client = build_client_with_login(&server, "tok").await;
        let state = make_state();

        let outcomes = HashMap::new();
        mount_per_scenario_mocks(&server, &state, &outcomes).await;

        let cfg = LoadConfig {
            total: 20,
            rate: 50,
            concurrency: 5,
            per_job_timeout: Duration::from_secs(5),
            p95_budget_ms: 10_000,
            seed: 7,
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(&client, &state, SCENARIOS, &cfg, &tx).await;
        drop(tx);

        assert_eq!(outcome.completed, 20, "all should complete");
        assert_eq!(outcome.passed, 20, "all verdicts should match");
        assert!(outcome.errors.is_empty(), "no errors: {:?}", outcome.errors);
        assert!(outcome.passed_budget, "p95 within budget");
        assert!(outcome.passed_overall, "overall pass");

        let events = drain(&mut rx);
        let submitted = events
            .iter()
            .filter(|e| matches!(e, Event::LoadSubmitted { .. }))
            .count();
        let completed = events
            .iter()
            .filter(|e| matches!(e, Event::LoadCompleted { .. }))
            .count();
        assert_eq!(submitted, 20);
        assert_eq!(completed, 20);

        assert!(matches!(
            events.first().unwrap(),
            Event::PhaseStarted { phase: Phase::Load }
        ));
        assert!(matches!(
            events.last().unwrap(),
            Event::PhaseFinished {
                phase: Phase::Load,
                ok: true
            }
        ));
    }

    #[tokio::test]
    async fn deterministic_with_same_seed() {
        let scenarios = SCENARIOS;
        let a = build_mix(scenarios, 200, 42)
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        let b = build_mix(scenarios, 200, 42)
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        assert_eq!(a, b, "same seed must produce identical sequence");

        let c = build_mix(scenarios, 200, 43)
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        assert_ne!(
            a, c,
            "different seed must produce different sequence (probabilistic; very unlikely to match)"
        );

        assert!(
            a.iter().all(|id| *id != "ab-cpp-ce"),
            "ce must be excluded from mix",
        );

        let ac_count = a
            .iter()
            .filter(|id| {
                matches!(
                    **id,
                    "ab-cpp-ac" | "ab-py-ac" | "ab-cpp-igncase" | "ab-cpp-multi"
                )
            })
            .count();
        assert!(
            ac_count > 100,
            "AC bucket should dominate (got {}/200)",
            ac_count,
        );
    }

    #[tokio::test]
    async fn concurrency_cap_is_respected() {
        let server = MockServer::start().await;
        let client = build_client_with_login(&server, "tok").await;
        let state = make_state();

        for (idx, scenario) in SCENARIOS.iter().enumerate() {
            let problem_id = state.problem_ids_by_scenario[scenario.id];
            let submission_id = 1000 + idx as i32;
            let post_path = format!(
                "/api/v1/contests/{}/problems/{}/submissions",
                TEST_CONTEST_ID, problem_id
            );
            let get_path = format!("/api/v1/submissions/{}", submission_id);

            Mock::given(method("POST"))
                .and(path(post_path))
                .respond_with(ResponseTemplate::new(201).set_body_json(submission_json(
                    submission_id,
                    "Pending",
                    None,
                )))
                .mount(&server)
                .await;

            let v_wire = scenario.expected_verdict.as_ref().map(verdict_wire);
            Mock::given(method("GET"))
                .and(path(get_path))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(100))
                        .set_body_json(submission_json(
                            submission_id,
                            submission_status_wire(scenario.expected_status),
                            v_wire,
                        )),
                )
                .mount(&server)
                .await;
        }

        let cfg = LoadConfig {
            total: 20,
            rate: 100,
            concurrency: 5,
            per_job_timeout: Duration::from_secs(5),
            p95_budget_ms: 10_000,
            seed: 1,
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(&client, &state, SCENARIOS, &cfg, &tx).await;
        drop(tx);

        assert_eq!(outcome.completed, 20);
        assert_eq!(outcome.passed, 20);

        let events = drain(&mut rx);
        let mut in_flight: i64 = 0;
        let mut max_in_flight: i64 = 0;
        for e in &events {
            match e {
                Event::LoadSubmitted { .. } => {
                    in_flight += 1;
                    if in_flight > max_in_flight {
                        max_in_flight = in_flight;
                    }
                }
                Event::LoadCompleted { .. } => in_flight -= 1,
                _ => {}
            }
        }
        assert!(
            max_in_flight <= cfg.concurrency as i64,
            "in-flight exceeded concurrency cap: {} > {}",
            max_in_flight,
            cfg.concurrency,
        );
    }

    #[tokio::test]
    async fn p95_budget_violation_flagged() {
        let server = MockServer::start().await;
        let client = build_client_with_login(&server, "tok").await;
        let state = make_state();

        for (idx, scenario) in SCENARIOS.iter().enumerate() {
            let problem_id = state.problem_ids_by_scenario[scenario.id];
            let submission_id = 1000 + idx as i32;
            let post_path = format!(
                "/api/v1/contests/{}/problems/{}/submissions",
                TEST_CONTEST_ID, problem_id
            );
            let get_path = format!("/api/v1/submissions/{}", submission_id);

            Mock::given(method("POST"))
                .and(path(post_path))
                .respond_with(ResponseTemplate::new(201).set_body_json(submission_json(
                    submission_id,
                    "Pending",
                    None,
                )))
                .mount(&server)
                .await;

            let v_wire = scenario.expected_verdict.as_ref().map(verdict_wire);
            Mock::given(method("GET"))
                .and(path(get_path))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(200))
                        .set_body_json(submission_json(
                            submission_id,
                            submission_status_wire(scenario.expected_status),
                            v_wire,
                        )),
                )
                .mount(&server)
                .await;
        }

        let cfg = LoadConfig {
            total: 10,
            rate: 50,
            concurrency: 10,
            per_job_timeout: Duration::from_secs(5),
            p95_budget_ms: 50,
            seed: 99,
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(&client, &state, SCENARIOS, &cfg, &tx).await;
        drop(tx);
        let _ = drain(&mut rx);

        assert_eq!(outcome.completed, 10);
        assert_eq!(outcome.passed, 10, "verdicts still correct");
        assert!(
            !outcome.passed_budget,
            "p95 must exceed 50ms budget; histogram p95 = {}",
            outcome.histogram.value_at_quantile(0.95),
        );
        assert!(
            !outcome.passed_overall,
            "overall must fail when budget violated"
        );
    }

    #[tokio::test]
    async fn http_errors_recorded_and_fail_overall() {
        let server = MockServer::start().await;
        let client = build_client_with_login(&server, "tok").await;
        let state = make_state();

        for (idx, scenario) in SCENARIOS.iter().enumerate() {
            let problem_id = state.problem_ids_by_scenario[scenario.id];
            let submission_id = 1000 + idx as i32;
            let post_path = format!(
                "/api/v1/contests/{}/problems/{}/submissions",
                TEST_CONTEST_ID, problem_id
            );
            let get_path = format!("/api/v1/submissions/{}", submission_id);

            Mock::given(method("POST"))
                .and(path(post_path))
                .respond_with(ResponseTemplate::new(201).set_body_json(submission_json(
                    submission_id,
                    "Pending",
                    None,
                )))
                .mount(&server)
                .await;

            if scenario.id == "ab-cpp-wa" {
                Mock::given(method("GET"))
                    .and(path(get_path))
                    .respond_with(ResponseTemplate::new(500).set_body_string("kaboom"))
                    .mount(&server)
                    .await;
            } else {
                let v_wire = scenario.expected_verdict.as_ref().map(verdict_wire);
                Mock::given(method("GET"))
                    .and(path(get_path))
                    .respond_with(ResponseTemplate::new(200).set_body_json(submission_json(
                        submission_id,
                        submission_status_wire(scenario.expected_status),
                        v_wire,
                    )))
                    .mount(&server)
                    .await;
            }
        }

        let cfg = LoadConfig {
            total: 200,
            rate: 200,
            concurrency: 20,
            per_job_timeout: Duration::from_secs(2),
            p95_budget_ms: 60_000,
            seed: 1,
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(&client, &state, SCENARIOS, &cfg, &tx).await;
        drop(tx);
        let _ = drain(&mut rx);

        assert!(
            !outcome.errors.is_empty(),
            "at least one HTTP error must be recorded",
        );
        assert!(
            !outcome.passed_overall,
            "passed_overall must be false when errors present",
        );
        assert!(
            outcome.errors.iter().any(|(_, m)| m.contains("ab-cpp-wa")),
            "errors should mention the failing scenario; got {:?}",
            outcome.errors,
        );
    }
}
