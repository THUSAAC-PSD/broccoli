pub mod login;
pub mod plugin;

use clap::Subcommand;

use self::login::LoginArgs;
use self::plugin::PluginArgs;

#[derive(Subcommand)]
pub enum Command {
    Plugin(PluginArgs),
    Login(LoginArgs),
}
