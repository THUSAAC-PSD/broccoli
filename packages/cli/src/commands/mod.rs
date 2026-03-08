pub mod plugin;

use clap::Subcommand;

use self::plugin::PluginArgs;

#[derive(Subcommand)]
pub enum Command {
    /// Manage plugins
    Plugin(PluginArgs),
}
