use std::process::ExitCode;

use clap::Parser;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = stress_test::cli::Cli::parse();
    if let Err(e) = cli.validate() {
        eprintln!("error: {e}");
        return ExitCode::from(64);
    }

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn,stress_test=info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let code = stress_test::runner::run(cli).await;
    ExitCode::from(code)
}
