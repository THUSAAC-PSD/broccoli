//! Server-bundled stress-test binary downloads.
//!
//! Compiled only when the `bundled-stress-test` cargo feature is enabled. See
//! `docs/plans/2026-05-03-stress-test-release-design.md` for the design.

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use sha2::{Digest, Sha256};

use crate::state::AppState;

const LINUX_X86_64: &[u8] = include_bytes!("../../embedded/stress-test/linux-x86_64");
const LINUX_AARCH64: &[u8] = include_bytes!("../../embedded/stress-test/linux-aarch64");
const WINDOWS_X86_64: &[u8] = include_bytes!("../../embedded/stress-test/windows-x86_64.exe");
const MACOS_UNIVERSAL: &[u8] = include_bytes!("../../embedded/stress-test/macos-universal");
// Used by the manifest endpoint (added in a follow-up task); compiled in here so
// the embed list lives in one place.
const MANIFEST_JSON: &[u8] = include_bytes!("../../embedded/stress-test/manifest.json");

/// Returns `(bytes, download_filename)` for a known platform identifier, or `None`.
fn binary_for(platform: &str) -> Option<(&'static [u8], &'static str)> {
    match platform {
        "linux-x86_64" => Some((LINUX_X86_64, "broccoli-stress-test-linux-x86_64")),
        "linux-aarch64" => Some((LINUX_AARCH64, "broccoli-stress-test-linux-aarch64")),
        "windows-x86_64" => Some((WINDOWS_X86_64, "broccoli-stress-test-windows-x86_64.exe")),
        "macos-universal" => Some((MACOS_UNIVERSAL, "broccoli-stress-test-macos-universal")),
        _ => None,
    }
}

const PLATFORMS: &[&str] = &[
    "linux-x86_64",
    "linux-aarch64",
    "windows-x86_64",
    "macos-universal",
];

pub fn router() -> Router<AppState> {
    // axum/matchit forbids mixing a parameter with a literal in the same path
    // segment (`{platform}.sha256` is rejected). Instead we accept a single
    // `{file}` segment and dispatch on the optional `.sha256` suffix in code.
    Router::new()
        .route("/downloads/stress-test/{file}", get(serve))
        .route("/downloads/manifest.json", get(serve_manifest))
        .route("/downloads", get(serve_discovery))
}

fn detect_platform(user_agent: &str) -> Option<&'static str> {
    let ua = user_agent.to_ascii_lowercase();
    if ua.contains("windows") {
        Some("windows-x86_64")
    } else if ua.contains("mac os") || ua.contains("macintosh") {
        Some("macos-universal")
    } else if ua.contains("linux") {
        if ua.contains("aarch64") || ua.contains("arm64") {
            Some("linux-aarch64")
        } else {
            Some("linux-x86_64")
        }
    } else {
        None
    }
}

async fn serve_discovery(headers: axum::http::HeaderMap) -> Response {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let recommended = detect_platform(ua);
    let html = render_discovery_html(env!("CARGO_PKG_VERSION"), recommended);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}

fn render_discovery_html(version: &str, recommended: Option<&str>) -> String {
    let mut rows = String::new();
    for &p in PLATFORMS {
        let badge = if Some(p) == recommended {
            r#" <span style="background:#ffd700;padding:2px 6px;border-radius:3px;font-size:0.85em;">Recommended for your platform</span>"#
        } else {
            ""
        };
        rows.push_str(&format!(
            r#"<tr>
              <td><code>{p}</code>{badge}</td>
              <td><a href="/downloads/stress-test/{p}">download</a></td>
              <td><a href="/downloads/stress-test/{p}.sha256">sha256</a></td>
            </tr>"#
        ));
    }
    format!(
        r#"<!doctype html>
<html><head>
  <meta charset="utf-8">
  <title>Broccoli Stress Test - Downloads</title>
  <style>
    body {{ font-family: system-ui, sans-serif; max-width: 720px; margin: 2em auto; padding: 0 1em; }}
    table {{ border-collapse: collapse; width: 100%; margin: 1em 0; }}
    td, th {{ padding: 0.5em; border-bottom: 1px solid #ddd; text-align: left; }}
    code {{ background: #f4f4f4; padding: 2px 4px; border-radius: 3px; }}
  </style>
</head><body>
  <h1>Broccoli Stress Test</h1>
  <p>Server version: <code>{version}</code></p>
  <p>Pick the binary for your platform. After download, see
     <a href="/downloads/help">trouble running?</a> for OS-specific notes.</p>
  <table>
    <thead><tr><th>Platform</th><th>Binary</th><th>Checksum</th></tr></thead>
    <tbody>{rows}</tbody>
  </table>
  <p>Machine-readable manifest: <a href="/downloads/manifest.json">/downloads/manifest.json</a></p>
</body></html>
"#
    )
}

async fn serve(Path(file): Path<String>) -> Response {
    if let Some(platform) = file.strip_suffix(".sha256") {
        serve_sha256(platform)
    } else {
        serve_binary(&file)
    }
}

fn serve_binary(platform: &str) -> Response {
    let Some((bytes, filename)) = binary_for(platform) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let server_version = env!("CARGO_PKG_VERSION");
    let etag = format!("\"{server_version}-{platform}\"");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header(header::ETAG, HeaderValue::from_str(&etag).unwrap())
        .body(Body::from(bytes))
        .unwrap()
}

fn serve_sha256(platform: &str) -> Response {
    let Some((bytes, filename)) = binary_for(platform) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hex::encode(hasher.finalize());
    let body = format!("{digest}  {filename}\n");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(body))
        .unwrap()
}

async fn serve_manifest() -> Response {
    let manifest = rewrite_manifest_urls(MANIFEST_JSON);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(manifest))
        .unwrap()
}

/// Rewrite the GitHub Releases URLs in the embedded manifest to relative server paths,
/// so air-gapped lab clients never get pointed back at github.com.
fn rewrite_manifest_urls(raw: &[u8]) -> Vec<u8> {
    let mut value: serde_json::Value =
        serde_json::from_slice(raw).expect("embedded manifest.json must be valid JSON");

    if let Some(platforms) = value.get_mut("platforms").and_then(|p| p.as_object_mut()) {
        for (platform, info) in platforms.iter_mut() {
            if let Some(obj) = info.as_object_mut() {
                obj.insert(
                    "url".into(),
                    serde_json::Value::String(format!("/downloads/stress-test/{platform}")),
                );
            }
        }
    }

    serde_json::to_vec_pretty(&value).expect("re-serialize manifest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_platform_returns_none() {
        assert!(binary_for("freebsd-x86_64").is_none());
        assert!(binary_for("../etc/passwd").is_none());
    }

    #[test]
    fn known_platforms_return_named_filenames() {
        assert_eq!(
            binary_for("linux-x86_64").unwrap().1,
            "broccoli-stress-test-linux-x86_64"
        );
        assert_eq!(
            binary_for("windows-x86_64").unwrap().1,
            "broccoli-stress-test-windows-x86_64.exe"
        );
    }

    #[test]
    fn detect_platform_from_user_agent() {
        use super::detect_platform;

        assert_eq!(
            detect_platform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"),
            Some("windows-x86_64")
        );
        assert_eq!(
            detect_platform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)"),
            Some("macos-universal")
        );
        assert_eq!(
            detect_platform("Mozilla/5.0 (Macintosh; Apple Silicon)"),
            Some("macos-universal")
        );
        assert_eq!(
            detect_platform("Mozilla/5.0 (X11; Linux x86_64)"),
            Some("linux-x86_64")
        );
        assert_eq!(
            detect_platform("Mozilla/5.0 (X11; Linux aarch64)"),
            Some("linux-aarch64")
        );
        assert_eq!(detect_platform("curl/8.0.0"), None);
        assert_eq!(detect_platform(""), None);
    }

    #[test]
    fn rewrite_manifest_urls_replaces_github_urls() {
        let raw = br#"{
          "version": "0.2.0",
          "platforms": {
            "linux-x86_64": {
              "url": "https://github.com/x/y/releases/download/v0.2.0/foo",
              "sha256": "abc"
            }
          }
        }"#;
        let out = rewrite_manifest_urls(raw);
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["platforms"]["linux-x86_64"]["url"],
            "/downloads/stress-test/linux-x86_64"
        );
        assert_eq!(v["platforms"]["linux-x86_64"]["sha256"], "abc");
    }
}
