//! TLS trust config. Mode from `$BROCCOLI_TLS` (takes precedence) or the `tls`
//! config key; default is rustls with bundled webpki roots.

use std::sync::Once;
use std::time::Duration;

use console::style;
use ureq::tls::{RootCerts, TlsConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    WebPki,
    Platform,
    Insecure,
}

/// TLS mode from `$BROCCOLI_TLS`, then config, else the secure default.
pub fn resolve_tls_mode() -> TlsMode {
    let raw = std::env::var("BROCCOLI_TLS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| crate::config::load_user_config().tls);
    parse_tls_mode(raw.as_deref())
}

/// Unknown or empty values fall back to the secure default.
fn parse_tls_mode(raw: Option<&str>) -> TlsMode {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("system" | "platform" | "native" | "os") => TlsMode::Platform,
        Some("insecure" | "danger" | "danger-accept-invalid" | "no-verify" | "skip") => {
            TlsMode::Insecure
        }
        _ => TlsMode::WebPki,
    }
}

/// Non-2xx returns `Ok` so callers can read the error body.
pub fn build_agent(connect: Option<Duration>, global: Option<Duration>) -> ureq::Agent {
    let mut builder = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(connect)
        .timeout_global(global);

    match resolve_tls_mode() {
        TlsMode::WebPki => {}
        TlsMode::Platform => {
            builder = builder.tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            );
        }
        TlsMode::Insecure => {
            warn_insecure_once();
            builder = builder.tls_config(TlsConfig::builder().disable_verification(true).build());
        }
    }

    builder.build().into()
}

fn warn_insecure_once() {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        eprintln!(
            "{} TLS certificate verification is DISABLED (BROCCOLI_TLS=insecure). \
             Your connection is not secure.",
            style("warning:").yellow().bold()
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{TlsMode, parse_tls_mode};

    #[test]
    fn defaults_to_webpki() {
        assert_eq!(parse_tls_mode(None), TlsMode::WebPki);
        assert_eq!(parse_tls_mode(Some("")), TlsMode::WebPki);
        assert_eq!(parse_tls_mode(Some("webpki")), TlsMode::WebPki);
        assert_eq!(parse_tls_mode(Some("nonsense")), TlsMode::WebPki);
    }

    #[test]
    fn recognizes_platform_synonyms_case_insensitively() {
        for v in ["system", "Platform", " NATIVE ", "os"] {
            assert_eq!(parse_tls_mode(Some(v)), TlsMode::Platform, "{v}");
        }
    }

    #[test]
    fn recognizes_insecure_synonyms() {
        for v in ["insecure", "no-verify", "skip", "DANGER"] {
            assert_eq!(parse_tls_mode(Some(v)), TlsMode::Insecure, "{v}");
        }
    }
}
