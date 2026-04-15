pub mod build;
pub mod new;
mod wasm;
pub mod watch;

use clap::{Args, Subcommand};

use self::build::BuildPluginArgs;
use self::new::NewPluginArgs;
use self::watch::WatchPluginArgs;

#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    New(NewPluginArgs),
    Build(BuildPluginArgs),
    Watch(WatchPluginArgs),
}
