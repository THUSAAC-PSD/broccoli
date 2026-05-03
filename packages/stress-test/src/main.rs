use std::process::ExitCode;

use clap::Parser;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = stress_test::cli::Cli::parse();
    if let Err(e) = cli.validate() {
        eprintln!("error: {e}");
        return ExitCode::from(64);
    }

    let code = stress_test::runner::run(cli).await;
    ExitCode::from(code)
}
