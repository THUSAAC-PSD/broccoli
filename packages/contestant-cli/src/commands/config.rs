use broccoli_cli_core::config;
use clap::{Args, Subcommand};
use console::style;

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: Option<ConfigCommand>,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show current configuration
    #[command(visible_alias = "sh", aliases = ["get", "ls"])]
    Show,
    /// Set a configuration value
    Set(SetArgs),
    /// Unset a configuration value
    #[command(alias = "rm")]
    Unset(UnsetArgs),
}

#[derive(Args)]
pub struct SetArgs {
    /// Config key (contest, language, server)
    pub key: String,
    /// Config value
    pub value: String,
}

#[derive(Args)]
pub struct UnsetArgs {
    /// Config key (contest, language, server)
    pub key: String,
}

pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command.unwrap_or(ConfigCommand::Show) {
        ConfigCommand::Show => {
            let cfg = config::load_user_config();
            println!(
                "{}  Configuration ({:?}/config.toml):\n",
                style("→").blue().bold(),
                config::config_dir()
            );
            println!(
                "  contest  = {}",
                cfg.contest.as_deref().unwrap_or("(not set)")
            );
            println!(
                "  language = {}",
                cfg.language.as_deref().unwrap_or("(auto-detect)")
            );
            println!(
                "  server   = {}",
                cfg.server.as_deref().unwrap_or("(from credentials)")
            );
            if !cfg.runtimes.is_empty() {
                println!("\n  Runtimes:");
                for (ext, cmd) in &cfg.runtimes {
                    println!("    {} = \"{}\"", ext, cmd);
                }
            }
        }
        ConfigCommand::Set(a) => {
            let mut cfg = config::load_user_config();
            match a.key.as_str() {
                "contest" => cfg.contest = Some(a.value.clone()),
                "language" => cfg.language = Some(a.value.clone()),
                "server" => cfg.server = Some(a.value.clone()),
                _ => {
                    anyhow::bail!(
                        "Unknown config key '{}'. Valid keys: contest, language, server",
                        a.key
                    );
                }
            }
            config::save_user_config(&cfg)?;
            println!("{}  {} = {}", style("✓").green(), a.key, a.value);
        }
        ConfigCommand::Unset(a) => {
            let mut cfg = config::load_user_config();
            match a.key.as_str() {
                "contest" => cfg.contest = None,
                "language" => cfg.language = None,
                "server" => cfg.server = None,
                _ => {
                    anyhow::bail!(
                        "Unknown config key '{}'. Valid keys: contest, language, server",
                        a.key
                    );
                }
            }
            config::save_user_config(&cfg)?;
            println!("{}  {} unset", style("✓").green(), a.key);
        }
    }
    Ok(())
}
