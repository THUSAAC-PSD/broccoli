use std::io::Write;
use std::thread;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::Args;
use console::style;
use serde::Deserialize;

use broccoli_cli_core::config;

#[derive(Args)]
pub struct LoginArgs {
    #[arg(long, default_value = "http://localhost:3000", env = "BROCCOLI_URL")]
    pub server: String,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct PollResponse {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

pub fn run(args: LoginArgs) -> anyhow::Result<()> {
    let agent = ureq::Agent::new_with_defaults();

    println!(
        "{}  Requesting device code from {}...",
        style("→").blue().bold(),
        style(&args.server).cyan()
    );

    let resp = agent
        .post(&format!("{}/api/v1/auth/device-code", args.server))
        .send_json(serde_json::json!({}))
        .context("Failed to connect to server. Is it running?")?;

    if resp.status() != 200 {
        let status = resp.status();
        let body = resp
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "(unreadable)".into());
        bail!("Server returned {}: {}", status, body);
    }

    let device_code_resp: DeviceCodeResponse = resp
        .into_body()
        .read_json()
        .context("Failed to parse device code response")?;

    println!();
    println!(
        "  Open {} and enter code:",
        style(&device_code_resp.verification_url)
            .underlined()
            .cyan()
    );
    println!();
    println!("    {}", style(&device_code_resp.user_code).bold().yellow());
    println!();

    let _ = open::that(&device_code_resp.verification_url);

    let interval = Duration::from_secs(device_code_resp.interval);
    let max_polls = device_code_resp.expires_in / device_code_resp.interval;

    print!("  Waiting for authorization");
    std::io::stdout().flush().ok();

    for _ in 0..max_polls {
        thread::sleep(interval);
        print!(".");
        std::io::stdout().flush().ok();

        let poll_resp = agent
            .post(&format!("{}/api/v1/auth/device-token", args.server))
            .send_json(serde_json::json!({
                "device_code": device_code_resp.device_code
            }));

        let poll_resp = match poll_resp {
            Ok(r) => r,
            Err(e) => {
                eprintln!("\n  Connection error: {}. Retrying...", e);
                continue;
            }
        };

        let poll: PollResponse = match poll_resp.into_body().read_json() {
            Ok(p) => p,
            Err(_) => continue,
        };

        if let Some(token) = poll.token {
            println!();
            println!();

            config::save_credentials(&args.server, &token).context("Failed to save credentials")?;

            println!("{}  Logged in successfully!", style("✓").green().bold());
            println!(
                "   Credentials saved to {}",
                style("~/.config/broccoli/credentials.json").dim()
            );

            return Ok(());
        }

        if let Some(ref error) = poll.error {
            match error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                "expired_token" => {
                    println!();
                    bail!("Device code expired. Run `broccoli-dev login` again to get a new code.");
                }
                other => {
                    println!();
                    bail!("Unexpected error from server: {}", other);
                }
            }
        }
    }

    println!();
    bail!("Timed out waiting for authorization. Run `broccoli-dev login` again.");
}
