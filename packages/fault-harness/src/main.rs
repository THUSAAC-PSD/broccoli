use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use fault_harness::{Scenario, ScenarioContext, scenarios::cancel_storm::CancelStorm};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScenarioKind {
    CancelStorm,
}

#[derive(Parser, Debug)]
#[command(name = "broccoli-fault-harness", about = "Fault-injection harness")]
struct Cli {
    #[arg(long, value_enum)]
    scenario: ScenarioKind,
    #[arg(long, default_value_t = 1000)]
    batch_count: usize,
    #[arg(long, default_value_t = 64)]
    readers: usize,
    #[arg(long, default_value_t = 21600)]
    key_ttl_secs: u64,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long, default_value = "transcript.json")]
    out: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let (redis_url, _container) = match cli.redis_url.clone() {
        Some(url) => (url, None),
        None => {
            tracing::info!("no --redis-url provided; starting ephemeral Redis testcontainer");
            let container = Redis::default().start().await?;
            let port = container.get_host_port_ipv4(6379).await?;
            (format!("redis://127.0.0.1:{port}"), Some(container))
        }
    };

    let ctx = ScenarioContext {
        redis_url: redis_url.clone(),
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
