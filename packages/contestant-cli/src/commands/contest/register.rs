use anyhow::Context;
use clap::Args;
use console::style;

use broccoli_cli_core::client::Client;
use broccoli_cli_core::{config, resolve};

#[derive(Args)]
pub struct RegisterArgs {
    /// Contest ID or name (e.g. 3 or "Spring Round")
    #[arg(required = true)]
    pub contest: String,
}

pub fn run(args: RegisterArgs, unregister: bool) -> anyhow::Result<()> {
    let creds = config::resolve_credentials(None, None)?;
    let client = Client::new(creds);
    let cid = resolve::contest_id(&client, &args.contest)?;

    if unregister {
        println!(
            "{}  Unregistering from contest {}...",
            style("→").blue().bold(),
            style(&cid).yellow()
        );
        client
            .unregister_from_contest(&cid)
            .context("Failed to unregister from contest")?;
        println!(
            "{}  Unregistered from contest {}",
            style("\u{2713}").green().bold(),
            style(&cid).yellow()
        );
    } else {
        println!(
            "{}  Registering for contest {}...",
            style("→").blue().bold(),
            style(&cid).yellow()
        );
        client
            .register_for_contest(&cid)
            .context("Failed to register for contest")?;
        println!(
            "{}  Registered for contest {}",
            style("\u{2713}").green().bold(),
            style(&cid).yellow()
        );
    }

    Ok(())
}
