use anyhow::Result;
use clap::Parser;

use broccoli_dev_cli::commands::Command;
use broccoli_dev_cli::commands::plugin::PluginCommand;

#[derive(Parser)]
#[command(name = "broccoli-dev", about = "Broccoli plugin developer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plugin(args) => match args.command {
            PluginCommand::New(new_args) => broccoli_dev_cli::commands::plugin::new::run(new_args),
            PluginCommand::Build(build_args) => {
                broccoli_dev_cli::commands::plugin::build::run(build_args)
            }
            PluginCommand::Watch(watch_args) => {
                broccoli_dev_cli::commands::plugin::watch::run(watch_args)
            }
        },
        Command::Login(login_args) => broccoli_dev_cli::commands::login::run(login_args),
    }
}
