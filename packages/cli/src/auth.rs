use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Resolved credentials for authenticating with a Broccoli server.
pub struct Credentials {
    pub server: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Default)]
struct CredentialsFile {
    entries: Vec<CredentialEntry>,
}

#[derive(Serialize, Deserialize)]
struct CredentialEntry {
    server: String,
    token: String,
}

fn credentials_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("broccoli")
        .join("credentials.json")
}

pub fn resolve_credentials(
    server: Option<&str>,
    token: Option<&str>,
) -> anyhow::Result<Credentials> {
    // If both are provided explicitly, use them
    if let (Some(s), Some(t)) = (server, token) {
        return Ok(Credentials {
            server: s.to_string(),
            token: t.to_string(),
        });
    }

    // Check env vars
    let env_server = std::env::var("BROCCOLI_URL").ok();
    let env_token = std::env::var("BROCCOLI_TOKEN").ok();

    let resolved_server = server.map(String::from).or(env_server);
    let resolved_token = token.map(String::from).or(env_token);

    if let (Some(s), Some(t)) = (resolved_server.as_ref(), resolved_token.as_ref()) {
        return Ok(Credentials {
            server: s.clone(),
            token: t.clone(),
        });
    }

    // Load saved credentials
    let creds_path = credentials_path();
    if creds_path.exists() {
        let content =
            std::fs::read_to_string(&creds_path).context("Failed to read credentials file")?;
        let file: CredentialsFile =
            serde_json::from_str(&content).context("Failed to parse credentials file")?;

        if let Some(target_server) = &resolved_server {
            if let Some(entry) = file.entries.iter().find(|e| &e.server == target_server) {
                return Ok(Credentials {
                    server: entry.server.clone(),
                    token: resolved_token.unwrap_or_else(|| entry.token.clone()),
                });
            }
        } else if let Some(entry) = file.entries.first() {
            return Ok(Credentials {
                server: entry.server.clone(),
                token: resolved_token.unwrap_or_else(|| entry.token.clone()),
            });
        }
    }

    bail!(
        "No credentials found.\n\
         Run `broccoli login` to authenticate, or pass --server and --token."
    );
}

/// Saves credentials for a server to ~/.config/broccoli/credentials.json.
pub fn save_credentials(server: &str, token: &str) -> anyhow::Result<()> {
    let creds_path = credentials_path();

    let mut file = if creds_path.exists() {
        let content = std::fs::read_to_string(&creds_path).unwrap_or_default();
        serde_json::from_str::<CredentialsFile>(&content).unwrap_or_default()
    } else {
        CredentialsFile::default()
    };

    if let Some(entry) = file.entries.iter_mut().find(|e| e.server == server) {
        entry.token = token.to_string();
    } else {
        file.entries.push(CredentialEntry {
            server: server.to_string(),
            token: token.to_string(),
        });
    }

    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    let content = serde_json::to_string_pretty(&file)?;
    std::fs::write(&creds_path, &content).context("Failed to write credentials file")?;

    // Set file permissions to 0600 (owner-only read/write)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&creds_path, perms)?;
    }

    Ok(())
}
