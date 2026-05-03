use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ServerVersion {
    pub version: String,
    #[serde(default)]
    pub git_sha: String,
}

/// Outcome of comparing the CLI's compile-time version to the server's reported version.
#[derive(Debug, PartialEq, Eq)]
pub enum VersionCheck {
    Match,
    Mismatch { server: String, cli: String },
}

pub fn compare(cli_version: &str, server_version: &str) -> VersionCheck {
    if cli_version == server_version {
        VersionCheck::Match
    } else {
        VersionCheck::Mismatch {
            server: server_version.to_string(),
            cli: cli_version.to_string(),
        }
    }
}

pub fn warning_message(server_url: &str, server: &str, cli: &str) -> String {
    format!(
        "warning: stress-test {cli} is targeting server {server} - version mismatch.\n\
         For best results, download the matching binary from\n\
         {server_url}/downloads\n\
         (continuing anyway in 3s; pass --no-version-check to skip)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_versions_return_match() {
        assert_eq!(compare("0.2.0", "0.2.0"), VersionCheck::Match);
    }

    #[test]
    fn differing_versions_return_mismatch() {
        let r = compare("0.2.0", "0.2.1");
        assert_eq!(
            r,
            VersionCheck::Mismatch {
                server: "0.2.1".into(),
                cli: "0.2.0".into()
            }
        );
    }

    #[test]
    fn warning_message_mentions_both_versions_and_downloads_url() {
        let msg = warning_message("https://broccoli.example", "0.2.0", "0.2.1");
        assert!(msg.contains("0.2.0"));
        assert!(msg.contains("0.2.1"));
        assert!(msg.contains("https://broccoli.example/downloads"));
        assert!(msg.contains("--no-version-check"));
    }
}
