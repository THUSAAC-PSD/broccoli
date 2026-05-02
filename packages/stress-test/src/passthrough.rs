
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::client::Client;
use crate::dto::{
    ContestProblemResponse, CreateSubmissionRequest, SubmissionFileDto, SubmissionResponse,
    SubmissionStatus, Verdict,
};
use crate::error::{StressError, StressResult};
use crate::events::{Event, Phase};

const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone)]
pub struct PassthroughConfig {
    pub contest_id: i32,
    pub problem_id: Option<i32>,
    pub concurrency: u32,
    pub per_job_timeout: Duration,
}

#[derive(Debug)]
struct SubResult {
    verdict_label: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
pub enum PassthroughOutcome {
    Skipped { reason: String },
    Completed {
        count: usize,
        terminal: usize,
        verdict_counts: HashMap<String, usize>,
        deterministic: bool,
        all_live: bool,
        errors: Vec<String>,
    },
}

impl PassthroughOutcome {
    pub fn passed(&self) -> bool {
        match self {
            Self::Skipped { .. } => true,
            Self::Completed {
                count,
                deterministic,
                all_live,
                ..
            } => *count > 0 && *all_live && *deterministic,
        }
    }

    pub fn count(&self) -> usize {
        match self {
            Self::Skipped { .. } => 0,
            Self::Completed { count, .. } => *count,
        }
    }
}

pub async fn run(
    client: &Client,
    config: &PassthroughConfig,
    tx: &mpsc::UnboundedSender<Event>,
) -> StressResult<PassthroughOutcome> {
    let _ = tx.send(Event::PhaseStarted {
        phase: Phase::Passthrough,
    });

    let outcome = match run_inner(client, config, tx).await {
        Ok(o) => o,
        Err(e) => {
            let _ = tx.send(Event::Error {
                phase: Some(Phase::Passthrough),
                message: format!("{e}"),
            });
            let _ = tx.send(Event::PhaseFinished {
                phase: Phase::Passthrough,
                ok: false,
            });
            return Err(e);
        }
    };

    match &outcome {
        PassthroughOutcome::Skipped { reason } => {
            let _ = tx.send(Event::PassthroughSkipped {
                reason: reason.clone(),
            });
        }
        PassthroughOutcome::Completed { count, .. } => {
            let _ = tx.send(Event::PassthroughCompleted {
                ok: outcome.passed(),
                count: *count,
            });
        }
    }
    let _ = tx.send(Event::PhaseFinished {
        phase: Phase::Passthrough,
        ok: outcome.passed(),
    });
    Ok(outcome)
}

async fn run_inner(
    client: &Client,
    config: &PassthroughConfig,
    tx: &mpsc::UnboundedSender<Event>,
) -> StressResult<PassthroughOutcome> {
    let contest = client.get_contest(config.contest_id).await?;

    let problems = client.list_contest_problems(config.contest_id).await?;
    if problems.is_empty() {
        return Ok(skip(format!(
            "contest {} has no problems",
            config.contest_id
        )));
    }

    let chosen = pick_problem(&problems, config.problem_id, config.contest_id)?;
    let problem = client.get_problem(chosen.problem_id).await?;
    if problem.checker_format == "testlib" {
        return Ok(skip(format!(
            "problem {} uses a Testlib checker; pass-through requires standard output matching",
            chosen.problem_id,
        )));
    }

    let test_cases = client.list_test_cases(chosen.problem_id).await?;
    let sample_summary = test_cases
        .iter()
        .filter(|tc| tc.is_sample)
        .min_by_key(|tc| tc.position)
        .cloned();
    let sample_summary = match sample_summary {
        Some(s) => s,
        None => {
            return Ok(skip(format!(
                "problem {} has no sample test cases",
                chosen.problem_id,
            )));
        }
    };
    let full_sample = client
        .get_test_case(chosen.problem_id, sample_summary.id)
        .await?;

    let request = build_sample_echo_request(&full_sample.expected_output, contest.contest_type);

    info!(
        contest_id = config.contest_id,
        problem_id = chosen.problem_id,
        sample_test_case_id = full_sample.id,
        concurrency = config.concurrency,
        "starting pass-through fan-out",
    );

    let results = fan_out(
        client.clone(),
        chosen.problem_id,
        request,
        config.concurrency,
        config.per_job_timeout,
        tx,
    )
    .await;

    Ok(aggregate(results))
}

fn skip(reason: String) -> PassthroughOutcome {
    PassthroughOutcome::Skipped { reason }
}

fn pick_problem(
    problems: &[ContestProblemResponse],
    explicit: Option<i32>,
    contest_id: i32,
) -> StressResult<ContestProblemResponse> {
    if let Some(pid) = explicit {
        problems
            .iter()
            .find(|cp| cp.problem_id == pid)
            .cloned()
            .ok_or_else(|| {
                StressError::Other(anyhow!(
                    "--problem-id {} is not part of contest {}",
                    pid,
                    contest_id,
                ))
            })
    } else {
        Ok(problems
            .iter()
            .min_by_key(|cp| cp.position)
            .cloned()
            .expect("non-empty problems checked above"))
    }
}

fn build_sample_echo_request(
    expected_output: &str,
    contest_type: Option<String>,
) -> CreateSubmissionRequest {
    CreateSubmissionRequest {
        files: vec![SubmissionFileDto {
            filename: "main.py".into(),
            content: build_python_echo_source(expected_output),
        }],
        language: "python3".into(),
        contest_type,
    }
}

pub(crate) fn build_python_echo_source(expected_output: &str) -> String {
    let mut escaped = String::with_capacity(expected_output.len() * 4);
    for &b in expected_output.as_bytes() {
        escaped.push_str(&format!("\\x{:02x}", b));
    }
    format!("import sys\nsys.stdout.buffer.write(b\"{escaped}\")\n")
}

async fn fan_out(
    client: Client,
    problem_id: i32,
    request: CreateSubmissionRequest,
    concurrency: u32,
    per_job_timeout: Duration,
    tx: &mpsc::UnboundedSender<Event>,
) -> Vec<SubResult> {
    let semaphore = Arc::new(Semaphore::new(concurrency as usize));
    let mut joinset: JoinSet<SubResult> = JoinSet::new();

    for index in 0..concurrency {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore not closed");
        let client = client.clone();
        let request = request.clone();
        let tx = tx.clone();
        joinset.spawn(async move {
            let _permit = permit;
            run_one(&client, problem_id, &request, per_job_timeout, index, &tx).await
        });
    }

    let mut results = Vec::with_capacity(concurrency as usize);
    while let Some(joined) = joinset.join_next().await {
        match joined {
            Ok(r) => results.push(r),
            Err(join_err) => {
                results.push(SubResult {
                    verdict_label: None,
                    error: Some(format!("submitter task panicked: {join_err}")),
                });
            }
        }
    }
    results
}

async fn run_one(
    client: &Client,
    problem_id: i32,
    request: &CreateSubmissionRequest,
    timeout: Duration,
    index: u32,
    tx: &mpsc::UnboundedSender<Event>,
) -> SubResult {
    let submitted = match client.create_submission(problem_id, request).await {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(Event::Error {
                phase: Some(Phase::Passthrough),
                message: format!("pass-through #{index}: submit failed: {e}"),
            });
            return SubResult {
                verdict_label: None,
                error: Some(format!("submit: {e}")),
            };
        }
    };

    match poll_until_terminal(client, submitted.id, timeout).await {
        Ok(final_resp) => {
            let label = label_for(final_resp.status, terminal_verdict(&final_resp));
            SubResult {
                verdict_label: Some(label),
                error: None,
            }
        }
        Err(e) => {
            let _ = tx.send(Event::Error {
                phase: Some(Phase::Passthrough),
                message: format!("pass-through #{index}: poll failed: {e}"),
            });
            SubResult {
                verdict_label: None,
                error: Some(format!("poll: {e}")),
            }
        }
    }
}

fn terminal_verdict(resp: &SubmissionResponse) -> Option<&Verdict> {
    resp.result.as_ref().and_then(|r| r.verdict.as_ref())
}

fn label_for(status: SubmissionStatus, verdict: Option<&Verdict>) -> String {
    match status {
        SubmissionStatus::Judged => match verdict {
            Some(v) => format!("Judged/{}", verdict_str(v)),
            None => "Judged/?".to_string(),
        },
        s => format!("{s:?}"),
    }
}

fn verdict_str(v: &Verdict) -> String {
    match v {
        Verdict::Accepted => "Accepted".into(),
        Verdict::WrongAnswer => "WrongAnswer".into(),
        Verdict::TimeLimitExceeded => "TimeLimitExceeded".into(),
        Verdict::MemoryLimitExceeded => "MemoryLimitExceeded".into(),
        Verdict::RuntimeError => "RuntimeError".into(),
        Verdict::SystemError => "SystemError".into(),
        Verdict::Skipped => "Skipped".into(),
        Verdict::Other(s) => format!("Other({s})"),
    }
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
                "submission {} did not reach terminal status within {:?} (last status = {:?})",
                submission_id,
                timeout,
                resp.status,
            )));
        }
        sleep(POLL_INTERVAL).await;
    }
}

fn aggregate(results: Vec<SubResult>) -> PassthroughOutcome {
    let count = results.len();

    let mut verdict_counts: HashMap<String, usize> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();
    let mut terminal = 0usize;

    for r in &results {
        if let Some(label) = &r.verdict_label {
            *verdict_counts.entry(label.clone()).or_insert(0) += 1;
            terminal += 1;
        }
        if let Some(err) = &r.error {
            errors.push(err.clone());
        }
    }

    let all_live = terminal == count && count > 0;
    let deterministic = terminal > 0 && verdict_counts.len() == 1;

    if !all_live {
        warn!(
            attempted = count,
            terminal, "pass-through liveness violation",
        );
    }
    if terminal > 0 && !deterministic {
        warn!(
            distinct_verdicts = verdict_counts.len(),
            ?verdict_counts,
            "pass-through determinism violation",
        );
    }

    PassthroughOutcome::Completed {
        count,
        terminal,
        verdict_counts,
        deterministic,
        all_live,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::AuthCreds;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn login_body(token: &str) -> Value {
        json!({
            "token": token,
            "id": 1,
            "username": "admin",
            "roles": ["admin"],
            "permissions": [],
        })
    }

    async fn build_client(server: &MockServer) -> Client {
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(login_body("tok")))
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
        .expect("client built")
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
                "test_case_results": [],
            }),
            None => Value::Null,
        };
        json!({
            "id": id,
            "language": "python3",
            "status": status,
            "user_id": 1,
            "username": "admin",
            "problem_id": 5,
            "problem_title": "passthrough-target",
            "contest_id": 42,
            "contest_type": "icpc",
            "judge_epoch": 0,
            "created_at": "2026-05-01T00:00:00Z",
            "result": result,
        })
    }

    fn contest_json(id: i32, contest_type: Option<&str>) -> Value {
        let ct = contest_type.map(Value::from).unwrap_or(Value::Null);
        json!({
            "id": id,
            "title": "Test Contest",
            "description": "",
            "activate_time": null,
            "start_time": "2026-05-01T00:00:00Z",
            "end_time": "2026-05-01T03:00:00Z",
            "deactivate_time": null,
            "is_public": true,
            "submissions_visible": true,
            "show_compile_output": true,
            "show_participants_list": true,
            "contest_type": ct,
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:00Z",
        })
    }

    fn problem_json(id: i32, checker_format: &str) -> Value {
        json!({
            "id": id,
            "title": "Pass-through Target",
            "time_limit": 1000,
            "memory_limit": 262144,
            "problem_type": "batch",
            "checker_format": checker_format,
            "default_contest_type": "icpc",
        })
    }

    fn test_case_list_item(id: i32, is_sample: bool, position: i32) -> Value {
        json!({
            "id": id,
            "score": 100,
            "label": "01",
            "description": null,
            "is_sample": is_sample,
            "position": position,
            "input_preview": "1\n",
            "output_preview": "1\n",
            "problem_id": 5,
            "created_at": "2026-05-01T00:00:00Z",
        })
    }

    fn test_case_full(id: i32, expected: &str) -> Value {
        json!({
            "id": id,
            "input": "1\n",
            "expected_output": expected,
            "score": 100,
            "label": "01",
            "description": null,
            "is_sample": true,
            "position": 0,
            "problem_id": 5,
            "created_at": "2026-05-01T00:00:00Z",
        })
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    #[test]
    fn python_echo_round_trips_arbitrary_bytes() {
        let raw: Vec<u8> = (0u8..=255u8).collect();
        let lossy = String::from_utf8_lossy(&raw).to_string();
        let src = build_python_echo_source(&lossy);

        assert!(src.contains("import sys\nsys.stdout.buffer.write(b\""));
        assert!(src.ends_with("\")\n"));

        for &b in lossy.as_bytes() {
            let needle = format!("\\x{b:02x}");
            assert!(
                src.contains(&needle),
                "byte 0x{b:02x} should appear as {needle:?}",
            );
        }
    }

    #[test]
    fn python_echo_quotes_and_newlines_do_not_break_literal() {
        let src = build_python_echo_source("hello\n\"world\"");
        assert!(!src.contains("hello\n\"world\""));
        assert!(src.contains("\\x68"));
        assert!(src.contains("\\x0a"));
        assert!(src.contains("\\x22"));
    }

    #[test]
    fn label_for_collapses_judged_with_verdict() {
        assert_eq!(
            label_for(SubmissionStatus::Judged, Some(&Verdict::Accepted)),
            "Judged/Accepted",
        );
        assert_eq!(
            label_for(SubmissionStatus::Judged, Some(&Verdict::WrongAnswer)),
            "Judged/WrongAnswer",
        );
        assert_eq!(
            label_for(SubmissionStatus::CompilationError, None),
            "CompilationError",
        );
    }

    #[test]
    fn aggregate_passes_when_all_terminal_and_consistent() {
        let results = vec![
            SubResult {
                verdict_label: Some("Judged/Accepted".into()),
                error: None,
            },
            SubResult {
                verdict_label: Some("Judged/Accepted".into()),
                error: None,
            },
            SubResult {
                verdict_label: Some("Judged/Accepted".into()),
                error: None,
            },
        ];
        let outcome = aggregate(results);
        match &outcome {
            PassthroughOutcome::Completed {
                count,
                terminal,
                deterministic,
                all_live,
                ..
            } => {
                assert_eq!(*count, 3);
                assert_eq!(*terminal, 3);
                assert!(*all_live);
                assert!(*deterministic);
            }
            _ => panic!("expected Completed"),
        }
        assert!(outcome.passed());
    }

    #[test]
    fn aggregate_fails_when_verdicts_split() {
        let results = vec![
            SubResult {
                verdict_label: Some("Judged/Accepted".into()),
                error: None,
            },
            SubResult {
                verdict_label: Some("Judged/WrongAnswer".into()),
                error: None,
            },
        ];
        let outcome = aggregate(results);
        match &outcome {
            PassthroughOutcome::Completed {
                deterministic,
                all_live,
                ..
            } => {
                assert!(*all_live, "both reached terminal status");
                assert!(!*deterministic, "two distinct verdicts");
            }
            _ => panic!("expected Completed"),
        }
        assert!(!outcome.passed());
    }

    #[test]
    fn aggregate_fails_when_any_timed_out() {
        let results = vec![
            SubResult {
                verdict_label: Some("Judged/Accepted".into()),
                error: None,
            },
            SubResult {
                verdict_label: None,
                error: Some("poll timeout".into()),
            },
        ];
        let outcome = aggregate(results);
        match &outcome {
            PassthroughOutcome::Completed {
                count,
                terminal,
                all_live,
                deterministic,
                errors,
                ..
            } => {
                assert_eq!(*count, 2);
                assert_eq!(*terminal, 1);
                assert!(!*all_live);
                assert!(*deterministic);
                assert_eq!(errors.len(), 1);
            }
            _ => panic!("expected Completed"),
        }
        assert!(!outcome.passed());
    }

    #[tokio::test]
    async fn skips_when_contest_has_no_problems() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: None,
                concurrency: 5,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await
        .expect("setup ok");
        drop(tx);

        match &outcome {
            PassthroughOutcome::Skipped { reason } => {
                assert!(reason.contains("no problems"), "reason: {reason}");
            }
            _ => panic!("expected Skipped, got {outcome:?}"),
        }
        assert!(outcome.passed(), "skip == pass");

        let events = drain(&mut rx);
        assert!(matches!(
            events.first(),
            Some(Event::PhaseStarted {
                phase: Phase::Passthrough
            })
        ));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PassthroughSkipped { reason } if reason.contains("no problems")
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PhaseFinished {
                phase: Phase::Passthrough,
                ok: true
            }
        )));
    }

    #[tokio::test]
    async fn skips_when_problem_has_no_samples() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "contest_id": 42,
                    "problem_id": 5,
                    "label": "A",
                    "position": 0,
                    "problem_title": "P",
                }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_json(5, "exact")))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5/test-cases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                test_case_list_item(11, false, 0),
                test_case_list_item(12, false, 1),
            ])))
            .mount(&server)
            .await;

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: None,
                concurrency: 3,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await
        .expect("setup ok");
        drop(tx);

        match &outcome {
            PassthroughOutcome::Skipped { reason } => {
                assert!(reason.contains("sample"), "reason: {reason}");
            }
            _ => panic!("expected Skipped"),
        }

        let events = drain(&mut rx);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PassthroughSkipped { reason } if reason.contains("sample")
        )));
    }

    #[tokio::test]
    async fn skips_when_checker_is_testlib() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "contest_id": 42,
                    "problem_id": 5,
                    "label": "A",
                    "position": 0,
                    "problem_title": "P",
                }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_json(5, "testlib")))
            .mount(&server)
            .await;

        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: None,
                concurrency: 3,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await
        .expect("setup ok");
        drop(tx);

        match &outcome {
            PassthroughOutcome::Skipped { reason } => {
                assert!(reason.contains("Testlib"), "reason: {reason}");
            }
            _ => panic!("expected Skipped"),
        }
        assert!(outcome.passed());
    }

    #[tokio::test]
    async fn explicit_problem_id_not_in_contest_errors() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "contest_id": 42,
                    "problem_id": 5,
                    "label": "A",
                    "position": 0,
                    "problem_title": "P",
                }
            ])))
            .mount(&server)
            .await;

        let (tx, _rx) = mpsc::unbounded_channel();
        let result = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: Some(999),
                concurrency: 3,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await;
        let err = result.expect_err("missing --problem-id is a setup error");
        let msg = format!("{err}");
        assert!(msg.contains("999"), "msg: {msg}");
        assert!(msg.contains("contest 42"), "msg: {msg}");
    }

    #[tokio::test]
    async fn happy_path_all_consistent_passes() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "contest_id": 42,
                    "problem_id": 5,
                    "label": "A",
                    "position": 0,
                    "problem_title": "P",
                }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_json(5, "exact")))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5/test-cases"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([test_case_list_item(11, true, 0),])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5/test-cases/11"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_case_full(11, "1\n")))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/problems/5/submissions"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(submission_json(700, "Pending", None)),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/submissions/700"))
            .respond_with(ResponseTemplate::new(200).set_body_json(submission_json(
                700,
                "Judged",
                Some("Accepted"),
            )))
            .mount(&server)
            .await;

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: None,
                concurrency: 4,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await
        .expect("setup ok");
        drop(tx);

        match &outcome {
            PassthroughOutcome::Completed {
                count,
                terminal,
                verdict_counts,
                deterministic,
                all_live,
                ..
            } => {
                assert_eq!(*count, 4);
                assert_eq!(*terminal, 4);
                assert!(*all_live);
                assert!(*deterministic);
                assert_eq!(verdict_counts.get("Judged/Accepted").copied(), Some(4));
            }
            _ => panic!("expected Completed"),
        }
        assert!(outcome.passed());

        let events = drain(&mut rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::PassthroughCompleted { ok: true, count: 4 }))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PhaseFinished {
                phase: Phase::Passthrough,
                ok: true,
            }
        )));
    }

    #[tokio::test]
    async fn nondeterministic_verdicts_fail_phase() {
        let server = MockServer::start().await;
        let client = build_client(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(contest_json(42, Some("icpc"))))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/contests/42/problems"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {
                    "contest_id": 42,
                    "problem_id": 5,
                    "label": "A",
                    "position": 0,
                    "problem_title": "P",
                }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(problem_json(5, "exact")))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5/test-cases"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([test_case_list_item(11, true, 0),])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/problems/5/test-cases/11"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_case_full(11, "1\n")))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/problems/5/submissions"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(submission_json(900, "Pending", None)),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/problems/5/submissions"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(submission_json(901, "Pending", None)),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v1/submissions/900"))
            .respond_with(ResponseTemplate::new(200).set_body_json(submission_json(
                900,
                "Judged",
                Some("Accepted"),
            )))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/submissions/901"))
            .respond_with(ResponseTemplate::new(200).set_body_json(submission_json(
                901,
                "Judged",
                Some("WrongAnswer"),
            )))
            .mount(&server)
            .await;

        let (tx, mut rx) = mpsc::unbounded_channel();
        let outcome = run(
            &client,
            &PassthroughConfig {
                contest_id: 42,
                problem_id: None,
                concurrency: 2,
                per_job_timeout: Duration::from_secs(5),
            },
            &tx,
        )
        .await
        .expect("setup ok");
        drop(tx);

        match &outcome {
            PassthroughOutcome::Completed {
                count,
                terminal,
                verdict_counts,
                deterministic,
                all_live,
                ..
            } => {
                assert_eq!(*count, 2);
                assert_eq!(*terminal, 2);
                assert!(*all_live);
                assert!(!*deterministic, "split verdicts must be flagged");
                assert_eq!(verdict_counts.len(), 2);
            }
            _ => panic!("expected Completed"),
        }
        assert!(!outcome.passed(), "non-deterministic must fail");

        let events = drain(&mut rx);
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PassthroughCompleted {
                ok: false,
                count: 2
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            Event::PhaseFinished {
                phase: Phase::Passthrough,
                ok: false,
            }
        )));
    }
}
