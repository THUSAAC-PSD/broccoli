use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use fault_harness::{
    Scenario, ScenarioContext,
    scenarios::{cancel_storm::CancelStorm, kill_server_recovery::KillServerRecovery},
};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScenarioKind {
    CancelStorm,
    KillServerRecovery,
}

#[derive(Parser, Debug)]
#[command(name = "broccoli-fault-harness", about = "Fault-injection harness")]
struct Cli {
    #[arg(long, value_enum)]
    scenario: ScenarioKind,

    // --- cancel_storm flags ---
    #[arg(long, default_value_t = 1000)]
    batch_count: usize,
    #[arg(long, default_value_t = 64)]
    readers: usize,
    #[arg(long, default_value_t = 21600)]
    key_ttl_secs: u64,

    // --- shared / kill_server_recovery flags ---
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long)]
    db_url: Option<String>,
    #[arg(long)]
    server_url: Option<String>,
    #[arg(long)]
    admin_token: Option<String>,
    #[arg(long)]
    kill_command: Option<String>,
    #[arg(long, default_value_t = 10)]
    submission_count: usize,
    #[arg(long, default_value_t = 75)]
    observe_timeout_secs: u64,
    #[arg(long, default_value_t = 1)]
    problem_id: i32,

    #[arg(long, default_value = "transcript.json")]
    out: PathBuf,
}

fn require_kill_recovery_flags(cli: &Cli) -> Result<(), String> {
    let mut missing = Vec::new();
    if cli.db_url.is_none() {
        missing.push("--db-url");
    }
    if cli.server_url.is_none() {
        missing.push("--server-url");
    }
    if cli.admin_token.is_none() {
        missing.push("--admin-token");
    }
    if cli.kill_command.is_none() {
        missing.push("--kill-command");
    }
    if !missing.is_empty() {
        return Err(format!(
            "scenario kill-server-recovery requires: {}",
            missing.join(", ")
        ));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    if let ScenarioKind::KillServerRecovery = cli.scenario
        && let Err(e) = require_kill_recovery_flags(&cli)
    {
        eprintln!("error: {e}");
        std::process::exit(2);
    }

    let (redis_url, _container) = match cli.redis_url.clone() {
        Some(url) => (url, None),
        None => {
            // The KillServerRecovery scenario presumes operator-managed
            // infrastructure; spinning up an ephemeral Redis would defeat
            // the purpose. Only cancel_storm gets the auto-container.
            if matches!(cli.scenario, ScenarioKind::KillServerRecovery) {
                eprintln!("error: --redis-url is required for kill-server-recovery");
                std::process::exit(2);
            }
            tracing::info!("no --redis-url provided; starting ephemeral Redis testcontainer");
            let container = Redis::default().start().await?;
            let port = container.get_host_port_ipv4(6379).await?;
            (format!("redis://127.0.0.1:{port}"), Some(container))
        }
    };

    let ctx = ScenarioContext {
        redis_url: redis_url.clone(),
        db_url: cli.db_url.clone(),
        server_url: cli.server_url.clone(),
        admin_token: cli.admin_token.clone(),
        kill_command: cli.kill_command.clone(),
    };

    let outcome = match cli.scenario {
        ScenarioKind::CancelStorm => {
            CancelStorm {
                batch_count: cli.batch_count,
                readers: cli.readers,
                key_ttl_secs: cli.key_ttl_secs,
            }
            .run(&ctx)
            .await?
        }
        ScenarioKind::KillServerRecovery => {
            KillServerRecovery {
                submission_count: cli.submission_count,
                observe_timeout_secs: cli.observe_timeout_secs,
                problem_id: cli.problem_id,
            }
            .run(&ctx)
            .await?
        }
    };

    outcome.transcript.write_json(&cli.out)?;
    tracing::info!(
        path = %cli.out.display(),
        passed = outcome.passed,
        "wrote transcript"
    );

    if !outcome.passed {
        std::process::exit(1);
    }

    Ok(())
}
