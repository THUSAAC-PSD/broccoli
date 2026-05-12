use std::time::Duration;

use anyhow::Context;
use server::config::AppConfig;
use server::runtime::ServerRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("broccoli-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let app_config = AppConfig::load().context("Failed to load configuration")?;

    if std::env::args().any(|a| a == "--healthcheck") {
        let exit_code = run_healthcheck(&app_config).await;
        std::process::exit(exit_code);
    }

    let runtime = ServerRuntime::build(app_config).await?;
    runtime.serve().await
}

/// Hits the local `/healthz` endpoint and returns a process exit code:
/// `0` on a 2xx response, `1` on any other status or transport error.
///
/// Used by the Docker `HEALTHCHECK` directive in `Dockerfile.server`. Skips
/// observability initialization so the probe stays fast and silent.
async fn run_healthcheck(app_config: &AppConfig) -> i32 {
    let url = format!("http://127.0.0.1:{}/healthz", app_config.server.port);

    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("healthcheck failed: {e}");
            return 1;
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => 0,
        Ok(resp) => {
            eprintln!("healthcheck failed: status {}", resp.status());
            1
        }
        Err(e) => {
            eprintln!("healthcheck failed: {e}");
            1
        }
    }
}
