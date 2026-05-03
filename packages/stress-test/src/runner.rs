use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Mutex;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogTarget {
    Stderr,
    File(PathBuf),
}

pub fn pick_log_target(choice: RendererChoice) -> LogTarget {
    match choice {
        RendererChoice::Tui => LogTarget::File(
            std::env::temp_dir().join(format!("broccoli-stress-test-{}.log", std::process::id())),
        ),
        RendererChoice::Plain | RendererChoice::None => LogTarget::Stderr,
    }
}

fn install_tracing(target: &LogTarget) {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn,stress_test=info".to_string());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match target {
        LogTarget::Stderr => {
            let _ = builder
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .try_init();
        }
        LogTarget::File(path) => {
            if let Ok(file) = std::fs::File::create(path) {
                let _ = builder
                    .with_writer(Mutex::new(file))
                    .with_ansi(false)
                    .try_init();
            }
        }
    }
}

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

    let choice = pick_renderer(
        cli.json,
        io::stdout().is_terminal(),
        crossterm::terminal::size().ok(),
    );
    let log_target = pick_log_target(choice);
    if let LogTarget::File(p) = &log_target {
        let _ = writeln!(io::stderr(), "stress-test: tracing log -> {}", p.display());
    }
    install_tracing(&log_target);

    let creds = build_creds(&cli);
    let client = match Client::new(cli.url.clone(), creds).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("setup error: failed to build client: {e}");
            return exit_code::SETUP_FAIL;
        }
    };

    let (tx, rx) = mpsc::unbounded_channel::<Event>();
    let renderer = match choice {
        RendererChoice::None => None,
        RendererChoice::Plain => Some(spawn_plain_renderer(rx)),
        RendererChoice::Tui => Some(spawn_tui_renderer(rx, &cli)),
    };

    tx.send(Event::PhaseStarted {
        phase: Phase::Bootstrap,
        total: None,
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
                cli.json,
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
        let outcome = crate::correctness::run(&client, &state, SCENARIOS, timeout, &tx).await;
        let ok = outcome.is_ok();
        summary.correctness = Some(CorrectnessSummary {
            total: outcome.total,
            passed: outcome.passed,
            failed_scenarios: outcome.failed_scenarios,
        });
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
            total: None,
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
    finalize(tx, renderer, Some(summary), overall_exit, cli.json).await
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

fn spawn_plain_renderer(rx: mpsc::UnboundedReceiver<Event>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stdout = io::stdout();
        if let Err(e) = crate::ui::plain::run(rx, &mut stdout).await {
            let _ = writeln!(io::stderr(), "renderer error: {e}");
        }
    })
}

fn spawn_tui_renderer(
    rx: mpsc::UnboundedReceiver<Event>,
    cli: &Cli,
) -> tokio::task::JoinHandle<()> {
    let url = cli.url.clone();
    let p95 = cli.p95_budget_ms;
    let concurrency = cli.concurrency as usize;
    tokio::spawn(async move {
        if let Err(e) = crate::ui::tui::run_tui(rx, url, p95, concurrency).await {
            let _ = writeln!(io::stderr(), "tui error: {e}");
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererChoice {
    None,
    Plain,
    Tui,
}

pub fn pick_renderer(json: bool, is_tty: bool, size: Option<(u16, u16)>) -> RendererChoice {
    if json {
        return RendererChoice::None;
    }
    if !is_tty {
        return RendererChoice::Plain;
    }
    match size {
        Some((w, h)) if w >= 80 && h >= 24 => RendererChoice::Tui,
        _ => RendererChoice::Plain,
    }
}

async fn finalize(
    tx: mpsc::UnboundedSender<Event>,
    renderer: Option<tokio::task::JoinHandle<()>>,
    summary: Option<RunSummary>,
    exit_code: u8,
    json: bool,
) -> u8 {
    drop(tx);
    if let Some(r) = renderer {
        let _ = r.await;
    }
    if let Some(s) = summary {
        let mut stdout = io::stdout();
        if json {
            let payload = s.to_json(exit_code);
            let _ = serde_json::to_writer(&mut stdout, &payload);
            let _ = stdout.write_all(b"\n");
        } else {
            let block = format_summary(&s);
            let _ = stdout.write_all(block.as_bytes());
        }
        let _ = stdout.flush();
    }
    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_forces_no_renderer() {
        assert_eq!(
            pick_renderer(true, true, Some((200, 60))),
            RendererChoice::None,
        );
        assert_eq!(pick_renderer(true, false, None), RendererChoice::None);
    }

    #[test]
    fn non_tty_falls_back_to_plain() {
        assert_eq!(
            pick_renderer(false, false, Some((200, 60))),
            RendererChoice::Plain,
        );
    }

    #[test]
    fn small_terminal_falls_back_to_plain() {
        assert_eq!(
            pick_renderer(false, true, Some((79, 30))),
            RendererChoice::Plain,
        );
        assert_eq!(
            pick_renderer(false, true, Some((100, 23))),
            RendererChoice::Plain,
        );
        assert_eq!(pick_renderer(false, true, None), RendererChoice::Plain);
    }

    #[test]
    fn tty_at_minimum_size_picks_tui() {
        assert_eq!(
            pick_renderer(false, true, Some((80, 24))),
            RendererChoice::Tui,
        );
        assert_eq!(
            pick_renderer(false, true, Some((200, 60))),
            RendererChoice::Tui,
        );
    }

    #[test]
    fn tui_choice_logs_to_a_file_under_tempdir() {
        match pick_log_target(RendererChoice::Tui) {
            LogTarget::File(p) => {
                assert!(p.starts_with(std::env::temp_dir()));
                assert!(
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("broccoli-stress-test-")),
                );
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn plain_and_none_choices_log_to_stderr() {
        assert_eq!(pick_log_target(RendererChoice::Plain), LogTarget::Stderr);
        assert_eq!(pick_log_target(RendererChoice::None), LogTarget::Stderr);
    }
}
