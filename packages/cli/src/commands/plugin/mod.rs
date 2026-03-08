pub mod build;
pub mod new;

use clap::{Args, Subcommand};

use self::build::BuildPluginArgs;
use self::new::NewPluginArgs;

#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Create a new plugin from template
    New(NewPluginArgs),
    /// Build a plugin
    Build(BuildPluginArgs),
}
