use std::time::Duration;

use anyhow::Context;
use server::config::AppConfig;
use server::runtime::ServerRuntime;
use tracing::info;

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("broccoli-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let app_config = AppConfig::load().context("Failed to load configuration")?;

    let max_blocking_threads = app_config.server.effective_max_blocking_threads();
    info!(
        max_blocking_threads,
        configured = ?app_config.server.max_blocking_threads,
        "Sizing tokio blocking-thread pool"
    );

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(max_blocking_threads)
        .thread_name("broccoli-server")
        .build()
        .context("Failed to build tokio runtime")?;

    if std::env::args().any(|a| a == "--healthcheck") {
        let exit_code = tokio_runtime.block_on(run_healthcheck(&app_config));
        std::process::exit(exit_code);
    }

    tokio_runtime.block_on(async move {
        let server_runtime = ServerRuntime::build(app_config).await?;
        server_runtime.serve().await
    })
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
