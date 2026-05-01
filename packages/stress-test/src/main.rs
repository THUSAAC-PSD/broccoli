use clap::Parser;

fn main() {
    let cli = stress_test::cli::Cli::parse();
    if let Err(e) = cli.validate() {
        eprintln!("error: {e}");
        std::process::exit(64);
    }
    println!("{cli:#?}");
}
