pub mod login;
pub mod plugin;

use clap::Subcommand;

use self::login::LoginArgs;
use self::plugin::PluginArgs;

#[derive(Subcommand)]
pub enum Command {
    /// Manage plugins
    Plugin(PluginArgs),
    /// Log in to a Broccoli server
    Login(LoginArgs),
}
