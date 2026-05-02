
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::bootstrap::BootstrapConfig;
use crate::cli::Cli;
use crate::client::{AuthCreds, Client};
use crate::events::{Event, Phase};
use crate::report::{
    CorrectnessSummary, LoadSummary, PassthroughSummary, RunSummary, format_summary,
};
use crate::scenarios::SCENARIOS;

pub mod exit_code {
    pub const PASS: u8 = 0;
    pub const CORRECTNESS_FAIL: u8 = 1;
    pub const LOAD_FAIL: u8 = 2;
    pub const PASSTHROUGH_FAIL: u8 = 3;
    pub const SETUP_FAIL: u8 = 4;
    pub const CLEANUP_DEGRADED: u8 = 5;
}

pub async fn run(cli: Cli) -> u8 {
    let started = Instant::now();

    let creds = build_creds(&cli);
    let client = match Client::new(cli.url.clone(), creds).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("setup error: failed to build client: {e}");
            return exit_code::SETUP_FAIL;
        }
    };

    let (tx, rx) = mpsc::unbounded_channel::<Event>();

    let renderer = if cli.json {
        None
    } else {
        Some(spawn_plain_renderer(rx))
    };

    tx.send(Event::PhaseStarted {
        phase: Phase::Bootstrap,
    })
    .ok();

    let bootstrap_config = BootstrapConfig {
        contest_type: cli.contest_type.clone(),
        problem_type: cli.problem_type.clone(),
    };
    let state = match crate::bootstrap::bootstrap(&client, SCENARIOS, &bootstrap_config).await {
        Ok(s) => s,
        Err(e) => {
            tx.send(Event::Error {
                phase: Some(Phase::Bootstrap),
                message: format!("{e}"),
            })
            .ok();
            tx.send(Event::PhaseFinished {
                phase: Phase::Bootstrap,
                ok: false,
            })
            .ok();
            return finalize(
                tx,
                renderer,
                Some(RunSummary {
                    target_url: cli.url.clone(),
                    duration: started.elapsed(),
                    bootstrap_error: Some(format!("{e}")),
                    correctness: None,
                    load: None,
                    passthrough: PassthroughSummary::NotRun,
                    cleanup_warnings: vec![],
                }),
                exit_code::SETUP_FAIL,
            )
            .await;
        }
    };
    tx.send(Event::PhaseFinished {
        phase: Phase::Bootstrap,
        ok: true,
    })
    .ok();

    let mut summary = RunSummary {
        target_url: cli.url.clone(),
        duration: Duration::ZERO,
        bootstrap_error: None,
        correctness: None,
        load: None,
        passthrough: PassthroughSummary::NotRun,
        cleanup_warnings: vec![],
    };
    let mut overall_exit = exit_code::PASS;

    if !cli.skip_correctness {
        let timeout = Duration::from_secs(cli.per_job_timeout);
        let ok = crate::correctness::run(&client, &state, SCENARIOS, timeout, &tx).await;
        summary.correctness = Some(build_correctness_summary(SCENARIOS.len(), ok));
        if !ok {
            overall_exit = exit_code::CORRECTNESS_FAIL;
        }
    }

    if overall_exit == exit_code::PASS && !cli.skip_load {
        let load_config = crate::load::LoadConfig {
            total: cli.total,
            rate: cli.rate,
            concurrency: cli.concurrency,
            per_job_timeout: Duration::from_secs(cli.per_job_timeout),
            p95_budget_ms: cli.p95_budget_ms,
            seed: cli.seed,
        };
        let outcome = crate::load::run(&client, &state, SCENARIOS, &load_config, &tx).await;
        summary.load = Some(LoadSummary::from_outcome(
            &outcome,
            cli.total,
            cli.p95_budget_ms,
        ));
        if !outcome.passed_overall {
            overall_exit = exit_code::LOAD_FAIL;
        }
    }

    if let Some(contest_id) = cli.contest_id {
        let pt_config = crate::passthrough::PassthroughConfig {
            contest_id,
            problem_id: cli.problem_id,
            concurrency: cli.contest_concurrency,
            per_job_timeout: Duration::from_secs(cli.per_job_timeout),
        };
        match crate::passthrough::run(&client, &pt_config, &tx).await {
            Ok(outcome) => {
                summary.passthrough = passthrough_summary_from_outcome(&outcome);
                if !outcome.passed() && overall_exit == exit_code::PASS {
                    overall_exit = exit_code::PASSTHROUGH_FAIL;
                }
            }
            Err(e) => {
                summary.passthrough = PassthroughSummary::Completed {
                    ok: false,
                    count: 0,
                };
                if overall_exit == exit_code::PASS {
                    overall_exit = exit_code::PASSTHROUGH_FAIL;
                }
                summary
                    .cleanup_warnings
                    .push(format!("pass-through setup error: {e}"));
            }
        }
    } else {
        tx.send(Event::PassthroughSkipped {
            reason: "no --contest-id".into(),
        })
        .ok();
        summary.passthrough = PassthroughSummary::NotRun;
    }

    if cli.keep_fixtures {
        for (scenario_id, problem_id) in &state.problem_ids_by_scenario {
            summary.cleanup_warnings.push(format!(
                "kept (--keep-fixtures): problem {problem_id} for scenario {scenario_id}"
            ));
        }
    } else {
        tx.send(Event::PhaseStarted {
            phase: Phase::Cleanup,
        })
        .ok();
        let outcome = crate::cleanup::run(&client, &state).await;
        tx.send(Event::PhaseFinished {
            phase: Phase::Cleanup,
            ok: outcome.is_clean(),
        })
        .ok();
        summary.cleanup_warnings = outcome.warnings;
        if overall_exit == exit_code::PASS && !summary.cleanup_warnings.is_empty() {
            overall_exit = exit_code::CLEANUP_DEGRADED;
        }
    }

    summary.duration = started.elapsed();
    finalize(tx, renderer, Some(summary), overall_exit).await
}

fn build_creds(cli: &Cli) -> AuthCreds {
    if let Some(t) = cli.admin_token.clone() {
        AuthCreds::Token(t)
    } else {
        AuthCreds::UsernamePassword {
            username: cli.admin_username.clone().unwrap_or_default(),
            password: cli.admin_password.clone().unwrap_or_default(),
        }
    }
}

fn passthrough_summary_from_outcome(
    outcome: &crate::passthrough::PassthroughOutcome,
) -> PassthroughSummary {
    match outcome {
        crate::passthrough::PassthroughOutcome::Skipped { reason } => PassthroughSummary::Skipped {
            reason: reason.clone(),
        },
        crate::passthrough::PassthroughOutcome::Completed { count, .. } => {
            PassthroughSummary::Completed {
                ok: outcome.passed(),
                count: *count,
            }
        }
    }
}

fn build_correctness_summary(total: usize, all_ok: bool) -> CorrectnessSummary {
    CorrectnessSummary {
        total,
        passed: if all_ok { total } else { total - 1 },
        failed_scenarios: if all_ok {
            vec![]
        } else {
            vec!["(see event log)".into()]
        },
    }
}

fn spawn_plain_renderer(rx: mpsc::UnboundedReceiver<Event>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stdout = io::stdout();
        if let Err(e) = crate::ui::plain::run(rx, &mut stdout).await {
            let _ = writeln!(io::stderr(), "renderer error: {e}");
        }
    })
}

async fn finalize(
    tx: mpsc::UnboundedSender<Event>,
    renderer: Option<tokio::task::JoinHandle<()>>,
    summary: Option<RunSummary>,
    exit_code: u8,
) -> u8 {
    drop(tx);
    if let Some(r) = renderer {
        let _ = r.await;
    }
    if let Some(s) = summary {
        let block = format_summary(&s);
        let mut stdout = io::stdout();
        if stdout.is_terminal() {
            let _ = stdout.write_all(block.as_bytes());
        } else {
            let _ = stdout.write_all(block.as_bytes());
        }
        let _ = stdout.flush();
    }
    exit_code
}
