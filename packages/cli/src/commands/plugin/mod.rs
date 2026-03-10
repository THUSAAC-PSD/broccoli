pub mod build;
pub mod new;
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
    /// Create a new plugin from template
    New(NewPluginArgs),
    /// Build a plugin's backend and/or frontend components.
    ///
    /// Customize the frontend build via `broccoli.dev.toml` in the
    /// plugin directory. See `broccoli plugin build --help` for details.
    Build(BuildPluginArgs),
    /// Watch a plugin for changes, auto-build and upload.
    ///
    /// Watches plugin source files and automatically rebuilds, packages,
    /// and uploads the plugin to the server on each change.
    ///
    /// Place a `broccoli.dev.toml` in the plugin directory to customize
    /// watch behavior, frontend directory, and build commands. Run
    /// `broccoli plugin watch --help` for details.
    Watch(WatchPluginArgs),
}
