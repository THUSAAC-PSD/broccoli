# Stress-Test CLI Release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to
> implement this plan task-by-task.

**Goal:** Ship `broccoli-stress-test` v0.2.0 as five platform binaries via
GitHub Releases and a server-bundled `/downloads` endpoint, with lockstep
versioning and an advisory CLI/server version check.

**Architecture:** Add a `bundled-stress-test` Cargo feature on the server that
embeds the five musl/native binaries via `include_bytes!` and exposes them at
`/downloads`. A new `release.yml` GitHub Actions workflow triggered on `v*` tag
builds the matrix, uploads to GitHub Releases, and produces a `manifest.json`.
The CLI does a startup version handshake against a new `/api/v1/version`
endpoint and prints an advisory warning on mismatch.

**Tech Stack:** Rust (axum, utoipa, reqwest, rustls), GitHub Actions (matrix
build, ubuntu/windows/macos runners + ARM runner), `lipo` for macOS universal
binaries, `sha256sum` / `shasum -a 256` for checksums.

**Design doc:** `docs/plans/2026-05-03-stress-test-release-design.md`

---

## Conventions

- Each task ends in a single `git commit`. Use Conventional Commits (`feat`,
  `fix`, `chore`, `ci`, `docs`, `test`, `refactor`).
- Run `cargo fmt --all` before each commit. Run
  `cargo clippy --workspace --all-targets --locked -- -D warnings` before
  commits that touch Rust.
- All new handlers follow the **utoipa → instrument → auth → validate → DB →
  response** order in `CLAUDE.md` even when there's no auth (use `security(())`
  to mark public).
- DTOs derive `Serialize, utoipa::ToSchema`. Request DTOs additionally derive
  `Deserialize`; update DTOs additionally derive `Default, PartialEq`.
- The `/downloads` routes are intentionally NOT on the OpenApi router — they're
  a deployment concern, not an API surface. They live on the plain
  `axum::Router` in `build_router`.

---

## Task 1: Add `/api/v1/version` public endpoint

**Files:**

- Create: `packages/server/src/handlers/meta.rs`
- Modify: `packages/server/src/handlers/mod.rs`
- Modify: `packages/server/src/routes/v1.rs`
- Modify: `packages/server/src/lib.rs` (ApiDoc tags)

**Step 1: Write the integration test (TDD)**

Add to `packages/server/tests/integration/main.rs`:

```rust
mod meta;
```

Create `packages/server/tests/integration/meta.rs`:

```rust
use crate::common::TestApp;

#[tokio::test]
async fn version_endpoint_returns_server_version() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/api/v1/version").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await;
    let version = body.get("version").and_then(|v| v.as_str()).unwrap();
    assert!(!version.is_empty(), "version should be non-empty");
    assert!(
        version.chars().next().unwrap().is_ascii_digit(),
        "version should start with a digit, got {version}"
    );
}

#[tokio::test]
async fn version_endpoint_is_public() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/api/v1/version").await;
    assert_eq!(resp.status(), 200);
}
```

**Step 2: Verify the test fails**

Run: `cargo test -p server --test integration meta:: --locked` Expected: FAIL
with 404 for `/api/v1/version`.

**Step 3: Add the handler**

Create `packages/server/src/handlers/meta.rs`:

```rust
use axum::Json;
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Serialize, ToSchema)]
pub struct VersionResponse {
    /// Server version, matching the `Cargo.toml` `[package].version`.
    #[schema(example = "0.2.0")]
    pub version: String,
    /// Short Git SHA captured at build time (or `"unknown"` if not built from a git checkout).
    #[schema(example = "abc1234")]
    pub git_sha: String,
}

#[utoipa::path(
    get,
    path = "/version",
    tag = "Meta",
    operation_id = "getVersion",
    summary = "Server version and build info",
    description = "Public, unauthenticated. Used by the stress-test CLI for an advisory \
                   client/server version check.",
    responses(
        (status = 200, description = "Server version info", body = VersionResponse),
    ),
)]
#[instrument]
pub async fn get_version() -> Result<Json<VersionResponse>, AppError> {
    Ok(Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: option_env!("BROCCOLI_GIT_SHA").unwrap_or("unknown").to_string(),
    }))
}
```

Modify `packages/server/src/handlers/mod.rs` to add:

```rust
pub mod meta;
```

Modify `packages/server/src/routes/v1.rs` — at the top of the `routes()`
function, add after `.nest("/auth", auth_routes())`:

```rust
.routes(routes!(handlers::meta::get_version))
```

Modify `packages/server/src/lib.rs` — in the `tags(...)` block of the `ApiDoc`
macro, add:

```rust
(name = "Meta", description = "Server metadata (version, build info)"),
```

**Step 4: Verify the test passes**

Run: `cargo test -p server --test integration meta:: --locked` Expected: PASS
(both tests).

**Step 5: Commit**

```bash
cargo fmt --all
git add packages/server/src/handlers/ packages/server/src/routes/v1.rs packages/server/src/lib.rs packages/server/tests/integration/main.rs packages/server/tests/integration/meta.rs
git commit -m "feat(server): add public /api/v1/version endpoint for cli handshakes"
```

---

## Task 2: Capture git SHA at server build time

**Files:**

- Create: `packages/server/build.rs`
- Modify: `packages/server/Cargo.toml`

**Step 1: Add the build script**

Create `packages/server/build.rs`:

```rust
use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=BROCCOLI_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
```

Modify `packages/server/Cargo.toml` — add at the top of `[package]`:

```toml
build = "build.rs"
```

**Step 2: Verify the SHA is captured**

Run:
`cargo build -p server --locked && cargo test -p server --test integration meta:: --locked`
Expected: PASS. Then manually verify with `cargo run -p server` (or by reading
the test output) that the `git_sha` field is now a 7-char hex string, not
`"unknown"`.

**Step 3: Commit**

```bash
git add packages/server/build.rs packages/server/Cargo.toml
git commit -m "feat(server): capture git short sha at build time for /version endpoint"
```

---

## Task 3: Add `--no-version-check` flag to CLI

**Files:**

- Modify: `packages/stress-test/src/cli.rs`

**Step 1: Find the CLI struct**

Read `packages/stress-test/src/cli.rs` to confirm the `Cli` struct shape and
existing flags.

**Step 2: Add the flag**

Add the field to the existing `#[derive(Parser)] pub struct Cli`:

```rust
/// Skip the startup version handshake against the server.
#[arg(long, default_value_t = false)]
pub no_version_check: bool,
```

**Step 3: Verify it compiles and `--help` shows it**

Run: `cargo build -p stress-test --locked` Run:
`cargo run -p stress-test -- --help` Expected: `--no-version-check` appears in
the help output.

**Step 4: Commit**

```bash
cargo fmt --all
git add packages/stress-test/src/cli.rs
git commit -m "feat(stresstest): add --no-version-check flag for advisory handshake opt-out"
```

---

## Task 4: Implement the version handshake in the CLI

**Files:**

- Create: `packages/stress-test/src/version_check.rs`
- Modify: `packages/stress-test/src/lib.rs`
- Modify: `packages/stress-test/src/runner.rs`

**Step 1: Write the unit tests (TDD)**

Create `packages/stress-test/src/version_check.rs`:

```rust
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
            VersionCheck::Mismatch { server: "0.2.1".into(), cli: "0.2.0".into() }
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
```

Modify `packages/stress-test/src/lib.rs` — add at the top with the other
`pub mod`s:

```rust
pub mod version_check;
```

**Step 2: Verify the unit tests pass**

Run: `cargo test -p stress-test version_check:: --locked` Expected: PASS.

**Step 3: Wire the handshake into the runner**

Read `packages/stress-test/src/runner.rs` to find where
`pub async fn run(cli: Cli) -> u8` does its first network call (likely a
`Client::new` or similar).

Add a helper at the top of `runner.rs`:

```rust
use crate::version_check::{ServerVersion, VersionCheck, compare, warning_message};

async fn perform_version_check(base_url: &str) {
    let url = format!("{}/api/v1/version", base_url.trim_end_matches('/'));
    let result = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .and_then(|r| r.error_for_status());

    let server_version = match result {
        Ok(resp) => match resp.json::<ServerVersion>().await {
            Ok(v) => v.version,
            Err(e) => {
                eprintln!("warning: could not parse /api/v1/version response: {e}");
                return;
            }
        },
        Err(e) => {
            eprintln!(
                "warning: could not reach {url} for version handshake: {e} \
                 (continuing without check)"
            );
            return;
        }
    };

    let cli_version = env!("CARGO_PKG_VERSION");
    if let VersionCheck::Mismatch { server, cli } = compare(cli_version, &server_version) {
        eprintln!("{}", warning_message(base_url, &server, &cli));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
```

In `pub async fn run(cli: Cli) -> u8`, near the top (before any scenario kicks
off), add:

```rust
if !cli.no_version_check {
    perform_version_check(&cli.url).await;
}
```

(Replace `cli.url` with the actual field name if different — read the struct
first.)

**Step 4: Verify all stress-test tests pass**

Run: `cargo test -p stress-test --locked` Expected: PASS.

**Step 5: Commit**

```bash
cargo fmt --all
git add packages/stress-test/src/version_check.rs packages/stress-test/src/lib.rs packages/stress-test/src/runner.rs
git commit -m "feat(stresstest): advisory cli/server version handshake on startup"
```

---

## Task 5: Add `bundled-stress-test` Cargo feature scaffold

**Files:**

- Modify: `packages/server/Cargo.toml`
- Modify: `packages/server/build.rs`
- Create: `packages/server/embedded/.gitkeep`
- Modify: `.gitignore`

**Step 1: Add the feature**

Modify `packages/server/Cargo.toml` — add a new section:

```toml
[features]
default = []
bundled-stress-test = []
```

**Step 2: Make build.rs assert binaries when feature is on**

Replace `packages/server/build.rs` with:

```rust
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=BROCCOLI_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    if std::env::var("CARGO_FEATURE_BUNDLED_STRESS_TEST").is_ok() {
        check_embedded_binaries();
    }
}

fn check_embedded_binaries() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("embedded")
        .join("stress-test");

    let required = [
        "linux-x86_64",
        "linux-aarch64",
        "windows-x86_64.exe",
        "macos-universal",
        "manifest.json",
    ];

    for name in required {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!(
                "\nerror: feature `bundled-stress-test` requires {} to exist.\n\
                 Run scripts/fetch-stress-test-binaries.sh <version> to fetch them \
                 from GitHub Releases, or unset the feature.\n",
                path.display()
            );
            std::process::exit(1);
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
```

**Step 3: Add `.gitkeep` and gitignore the binaries**

Create empty file: `packages/server/embedded/.gitkeep`

Append to `.gitignore`:

```
# Embedded stress-test binaries (fetched at build time, not committed)
packages/server/embedded/stress-test/*
!packages/server/embedded/stress-test/.gitkeep
```

Create `packages/server/embedded/stress-test/.gitkeep` (empty file).

**Step 4: Verify slim build still works**

Run: `cargo build -p server --locked` Expected: PASS (feature is off by
default).

Run: `cargo build -p server --locked --features bundled-stress-test` Expected:
FAIL with the helpful error message about missing binaries.

**Step 5: Commit**

```bash
git add packages/server/Cargo.toml packages/server/build.rs packages/server/embedded/ .gitignore
git commit -m "feat(server): add bundled-stress-test cargo feature scaffold"
```

---

## Task 6: Build the dummy fixture binaries for tests

**Files:**

- Create: `packages/server/tests/fixtures/embedded-stress-test/linux-x86_64`
- Create: `packages/server/tests/fixtures/embedded-stress-test/linux-aarch64`
- Create:
  `packages/server/tests/fixtures/embedded-stress-test/windows-x86_64.exe`
- Create: `packages/server/tests/fixtures/embedded-stress-test/macos-universal`
- Create: `packages/server/tests/fixtures/embedded-stress-test/manifest.json`
- Modify: `.gitignore` (negation rule for fixture dir)

**Step 1: Create deterministic fixture binaries**

Each binary file is exactly 16 bytes containing platform-identifying ASCII so
tests can assert content. Use `printf` to write them (so they're deterministic
and committable):

```bash
mkdir -p packages/server/tests/fixtures/embedded-stress-test
printf 'FIXTURE-LINUX-X86' > packages/server/tests/fixtures/embedded-stress-test/linux-x86_64
printf 'FIXTURE-LINUX-ARM' > packages/server/tests/fixtures/embedded-stress-test/linux-aarch64
printf 'FIXTURE-WIN-X86_X' > packages/server/tests/fixtures/embedded-stress-test/windows-x86_64.exe
printf 'FIXTURE-MAC-UNIVE' > packages/server/tests/fixtures/embedded-stress-test/macos-universal
```

Create `packages/server/tests/fixtures/embedded-stress-test/manifest.json`:

```json
{
  "version": "test-fixture",
  "released_at": "2026-05-03T00:00:00Z",
  "platforms": {
    "linux-x86_64": {
      "url": "https://github.com/example/broccoli/releases/download/test/broccoli-stress-test-linux-x86_64",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000001",
      "size_bytes": 17
    },
    "linux-aarch64": {
      "url": "https://github.com/example/broccoli/releases/download/test/broccoli-stress-test-linux-aarch64",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000002",
      "size_bytes": 17
    },
    "windows-x86_64": {
      "url": "https://github.com/example/broccoli/releases/download/test/broccoli-stress-test-windows-x86_64.exe",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000003",
      "size_bytes": 17
    },
    "macos-universal": {
      "url": "https://github.com/example/broccoli/releases/download/test/broccoli-stress-test-macos-universal",
      "sha256": "0000000000000000000000000000000000000000000000000000000000000004",
      "size_bytes": 17
    }
  }
}
```

**Step 2: Add gitignore negation so fixtures are committed**

Append to `.gitignore`:

```
# Fixture binaries are committed (used by integration tests)
!packages/server/tests/fixtures/embedded-stress-test/
!packages/server/tests/fixtures/embedded-stress-test/**
```

**Step 3: Verify fixtures are tracked**

Run:
`git check-ignore -v packages/server/tests/fixtures/embedded-stress-test/linux-x86_64`
Expected: NO output (file is NOT ignored).

Run: `git status --short packages/server/tests/fixtures/embedded-stress-test/`
Expected: Five `??` (untracked) entries.

**Step 4: Commit**

```bash
git add packages/server/tests/fixtures/embedded-stress-test/ .gitignore
git commit -m "test(server): add deterministic fixture binaries for downloads endpoint"
```

---

## Task 7: Implement the downloads route module (binary endpoints only)

**Files:**

- Create: `packages/server/src/routes/downloads.rs`
- Modify: `packages/server/src/routes/mod.rs`
- Modify: `packages/server/src/lib.rs`

**Step 1: Write integration tests (TDD)**

Add to `packages/server/tests/integration/main.rs`:

```rust
#[cfg(feature = "bundled-stress-test")]
mod downloads;
```

Create `packages/server/tests/integration/downloads.rs`:

```rust
use crate::common::TestApp;

#[tokio::test]
async fn downloads_serves_linux_x86_64_binary() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/stress-test/linux-x86_64").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    let cd = resp.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(cd.contains("attachment"));
    assert!(cd.contains("broccoli-stress-test-linux-x86_64"));
    let body = resp.bytes().await;
    assert_eq!(&body[..], b"FIXTURE-LINUX-X86");
}

#[tokio::test]
async fn downloads_serves_windows_binary_with_exe_filename() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/stress-test/windows-x86_64").await;
    assert_eq!(resp.status(), 200);
    let cd = resp.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(cd.contains("broccoli-stress-test-windows-x86_64.exe"));
}

#[tokio::test]
async fn downloads_unknown_platform_returns_404() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/stress-test/freebsd-x86_64").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn downloads_serves_sha256_file() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/stress-test/linux-x86_64.sha256").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; charset=utf-8"
    );
    let body = resp.text().await;
    assert!(body.contains("broccoli-stress-test-linux-x86_64"));
    assert_eq!(body.split_whitespace().next().unwrap().len(), 64); // sha256 hex
}
```

**Step 2: Verify the tests fail**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
Expected: FAIL — likely a compile error because the module doesn't exist yet.

To get past the build.rs check, first create symlinks from the embedded dir to
the fixture dir for local dev:

```bash
mkdir -p packages/server/embedded/stress-test
cp packages/server/tests/fixtures/embedded-stress-test/* packages/server/embedded/stress-test/
```

(These copies are gitignored from Task 5.)

Re-run the test — now expect 404 on the downloads paths.

**Step 3: Implement the binary + sha256 endpoints**

Create `packages/server/src/routes/downloads.rs`:

```rust
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloads/stress-test/{platform}", get(serve_binary))
        .route("/downloads/stress-test/{platform}.sha256", get(serve_sha256))
}

async fn serve_binary(Path(platform): Path<String>) -> Response {
    let Some((bytes, filename)) = binary_for(&platform) else {
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

async fn serve_sha256(Path(platform): Path<String>) -> Response {
    let Some((bytes, filename)) = binary_for(&platform) else {
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
        assert_eq!(binary_for("linux-x86_64").unwrap().1, "broccoli-stress-test-linux-x86_64");
        assert_eq!(binary_for("windows-x86_64").unwrap().1, "broccoli-stress-test-windows-x86_64.exe");
    }

    #[allow(dead_code)]
    fn _manifest_is_compiled_in() {
        let _ = MANIFEST_JSON;
    }
}
```

Add `sha2` and `hex` to `packages/server/Cargo.toml` `[dependencies]` if they're
not already workspace deps. Check workspace `Cargo.toml` first; if present, use
`.workspace = true`. (`hex` is already used by server per CLAUDE.md context —
verify with `grep`.)

Modify `packages/server/src/routes/mod.rs`:

```rust
mod v1;
#[cfg(feature = "bundled-stress-test")]
pub mod downloads;
```

Modify `packages/server/src/lib.rs` `build_router()` — after the
`.merge(Scalar::with_url(...))` line, add:

```rust
#[cfg(feature = "bundled-stress-test")]
let router = router.merge(routes::downloads::router().with_state(state.clone()));
```

(Note: `with_state` placement varies — read the surrounding code to slot it in
correctly. The downloads router needs `AppState`.)

**Step 4: Verify the integration tests pass**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
Expected: PASS (all four).

Run:
`cargo test -p server --features bundled-stress-test --lib downloads:: --locked`
Expected: PASS (the in-module unit tests).

**Step 5: Verify slim build still has no /downloads**

Run: `cargo build -p server --locked` (no feature) Expected: PASS, builds the
slim variant.

Verify the slim test file:

Add a slim-only test to `packages/server/tests/integration/main.rs`:

```rust
#[cfg(not(feature = "bundled-stress-test"))]
mod downloads_slim;
```

Create `packages/server/tests/integration/downloads_slim.rs`:

```rust
use crate::common::TestApp;

#[tokio::test]
async fn slim_server_returns_404_for_downloads() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/stress-test/linux-x86_64").await;
    assert_eq!(resp.status(), 404);
}
```

Run: `cargo test -p server --test integration downloads_slim:: --locked`
Expected: PASS.

**Step 6: Commit**

```bash
cargo fmt --all
git add packages/server/src/routes/downloads.rs packages/server/src/routes/mod.rs packages/server/src/lib.rs packages/server/Cargo.toml packages/server/tests/integration/main.rs packages/server/tests/integration/downloads.rs packages/server/tests/integration/downloads_slim.rs
git commit -m "feat(server): bundled-stress-test downloads endpoints (binaries + sha256)"
```

---

## Task 8: Manifest endpoint with URL rewriting

**Files:**

- Modify: `packages/server/src/routes/downloads.rs`
- Modify: `packages/server/tests/integration/downloads.rs`

**Step 1: Write the integration tests (TDD)**

Append to `packages/server/tests/integration/downloads.rs`:

```rust
#[tokio::test]
async fn manifest_endpoint_returns_json_with_all_platforms() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/manifest.json").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await;
    assert!(body.get("version").is_some());
    let platforms = body.get("platforms").and_then(|p| p.as_object()).unwrap();
    for key in ["linux-x86_64", "linux-aarch64", "windows-x86_64", "macos-universal"] {
        assert!(platforms.get(key).is_some(), "manifest missing platform {key}");
    }
}

#[tokio::test]
async fn manifest_rewrites_urls_to_relative_server_paths() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/manifest.json").await;
    let body: serde_json::Value = resp.json().await;
    let url = body
        .pointer("/platforms/linux-x86_64/url")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(url, "/downloads/stress-test/linux-x86_64", "got {url}");
    assert!(!url.contains("github.com"), "manifest leaked github URL: {url}");
}
```

**Step 2: Verify the tests fail**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads::manifest --locked`
Expected: FAIL with 404.

**Step 3: Implement the manifest handler**

Add to `packages/server/src/routes/downloads.rs`:

In the `router()` function, add a new route:

```rust
.route("/downloads/manifest.json", get(serve_manifest))
```

Add the handler:

```rust
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
                let suffix = if platform == "windows-x86_64" { ".exe" } else { "" };
                let _ = suffix; // path uses the matchit param without .exe
                obj.insert(
                    "url".into(),
                    serde_json::Value::String(format!("/downloads/stress-test/{platform}")),
                );
            }
        }
    }

    serde_json::to_vec_pretty(&value).expect("re-serialize manifest")
}
```

Add a unit test to the in-module `mod tests` block:

```rust
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
```

**Step 4: Verify all downloads tests pass**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
Run:
`cargo test -p server --features bundled-stress-test --lib downloads:: --locked`
Expected: PASS (all).

**Step 5: Commit**

```bash
cargo fmt --all
git add packages/server/src/routes/downloads.rs packages/server/tests/integration/downloads.rs
git commit -m "feat(server): /downloads/manifest.json with relative-url rewriting"
```

---

## Task 9: Discovery HTML page with platform detection

**Files:**

- Modify: `packages/server/src/routes/downloads.rs`
- Modify: `packages/server/tests/integration/downloads.rs`

**Step 1: Write the unit + integration tests (TDD)**

Add to the `mod tests` block in `packages/server/src/routes/downloads.rs`:

```rust
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
```

Append to `packages/server/tests/integration/downloads.rs`:

```rust
#[tokio::test]
async fn discovery_page_renders_with_all_platforms() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let body = resp.text().await;
    for platform in ["linux-x86_64", "linux-aarch64", "windows-x86_64", "macos-universal"] {
        assert!(body.contains(platform), "discovery page missing {platform}");
    }
    assert!(body.to_lowercase().contains("stress"));
}
```

**Step 2: Verify the tests fail**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads::discovery --locked`
Expected: FAIL with 404.

**Step 3: Implement the discovery page**

Add to `packages/server/src/routes/downloads.rs`:

In `router()`, add:

```rust
.route("/downloads", get(serve_discovery))
```

Add helpers:

```rust
const PLATFORMS: &[&str] = &[
    "linux-x86_64",
    "linux-aarch64",
    "windows-x86_64",
    "macos-universal",
];

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
```

**Step 4: Verify all tests pass**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
Run:
`cargo test -p server --features bundled-stress-test --lib downloads:: --locked`
Expected: PASS (all).

**Step 5: Commit**

```bash
cargo fmt --all
git add packages/server/src/routes/downloads.rs packages/server/tests/integration/downloads.rs
git commit -m "feat(server): /downloads discovery html page with platform detection"
```

---

## Task 10: `/downloads/help` page

**Files:**

- Modify: `packages/server/src/routes/downloads.rs`
- Modify: `packages/server/tests/integration/downloads.rs`

**Step 1: Write integration test**

Append to `packages/server/tests/integration/downloads.rs`:

```rust
#[tokio::test]
async fn help_page_documents_macos_workaround() {
    let app = TestApp::spawn().await;
    let resp = app.get_without_token("/downloads/help").await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await;
    assert!(body.contains("xattr"));
    assert!(body.to_lowercase().contains("smartscreen") || body.to_lowercase().contains("windows"));
    assert!(body.contains("chmod +x"));
}
```

**Step 2: Verify it fails**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads::help --locked`
Expected: FAIL with 404.

**Step 3: Implement the help page**

Add to `packages/server/src/routes/downloads.rs`:

In `router()`, add `.route("/downloads/help", get(serve_help))`.

Add the handler:

```rust
async fn serve_help() -> Response {
    let html = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Trouble Running Broccoli Stress Test</title>
<style>body{font-family:system-ui,sans-serif;max-width:720px;margin:2em auto;padding:0 1em;}
code,pre{background:#f4f4f4;padding:2px 4px;border-radius:3px;}
pre{padding:1em;overflow-x:auto;}</style></head><body>
<h1>Trouble running?</h1>
<p>The binaries are not code-signed in v1. Each OS has a one-line workaround:</p>

<h2>macOS</h2>
<p>If macOS says "cannot be opened because the developer cannot be verified":</p>
<pre>xattr -d com.apple.quarantine ./broccoli-stress-test-macos-universal</pre>
<p>Or right-click the binary in Finder, choose Open, then click Open in the dialog.</p>

<h2>Windows</h2>
<p>If SmartScreen warns "Microsoft Defender prevented an unrecognized app from starting":</p>
<ol><li>Click <strong>More info</strong>.</li>
<li>Click <strong>Run anyway</strong>.</li></ol>
<p>Or in PowerShell: <code>Unblock-File .\broccoli-stress-test-windows-x86_64.exe</code></p>

<h2>Linux</h2>
<pre>chmod +x ./broccoli-stress-test-linux-x86_64
./broccoli-stress-test-linux-x86_64 --help</pre>

<p><a href="/downloads">&larr; back to downloads</a></p>
</body></html>
"#;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}
```

**Step 4: Verify the test passes**

Run:
`cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
Expected: PASS (all downloads tests).

**Step 5: Commit**

```bash
cargo fmt --all
git add packages/server/src/routes/downloads.rs packages/server/tests/integration/downloads.rs
git commit -m "feat(server): /downloads/help page documenting unsigned-binary workarounds"
```

---

## Task 11: `scripts/fetch-stress-test-binaries.sh`

**Files:**

- Create: `scripts/fetch-stress-test-binaries.sh`

**Step 1: Write the script**

Create `scripts/fetch-stress-test-binaries.sh`:

```bash
#!/usr/bin/env bash
# Fetches stress-test binaries from GitHub Releases into
# packages/server/embedded/stress-test/, so `cargo build -p server
# --features bundled-stress-test` can include_bytes! them.
#
# Usage: scripts/fetch-stress-test-binaries.sh <version-tag>
# Example: scripts/fetch-stress-test-binaries.sh v0.2.0
#
# Set BROCCOLI_RELEASES_BASE to override the GitHub Releases base URL
# (e.g., to use a mirror). Defaults to upstream GitHub.

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version-tag>  e.g. v0.2.0" >&2
  exit 64
fi

VERSION="$1"
BASE="${BROCCOLI_RELEASES_BASE:-https://github.com/JosephJoshua/broccoli/releases/download}"
OUT_DIR="packages/server/embedded/stress-test"

PLATFORMS=(
  "linux-x86_64"
  "linux-aarch64"
  "windows-x86_64.exe"
  "macos-universal"
)

mkdir -p "$OUT_DIR"

for p in "${PLATFORMS[@]}"; do
  url="$BASE/$VERSION/broccoli-stress-test-$p"
  out="$OUT_DIR/$p"
  echo "fetching $url"
  curl -fSL --retry 3 -o "$out" "$url"

  sha_url="$url.sha256"
  sha_file="$out.sha256"
  curl -fSL --retry 3 -o "$sha_file" "$sha_url"
  pushd "$OUT_DIR" > /dev/null
  if command -v sha256sum > /dev/null; then
    sha256sum -c "$(basename "$sha_file")"
  else
    # macOS fallback
    expected=$(awk '{print $1}' "$(basename "$sha_file")")
    actual=$(shasum -a 256 "$(basename "$out")" | awk '{print $1}')
    if [[ "$expected" != "$actual" ]]; then
      echo "checksum mismatch for $out: expected $expected got $actual" >&2
      exit 1
    fi
    echo "$(basename "$out"): OK"
  fi
  rm -f "$(basename "$sha_file")"
  popd > /dev/null
done

manifest_url="$BASE/$VERSION/manifest.json"
echo "fetching $manifest_url"
curl -fSL --retry 3 -o "$OUT_DIR/manifest.json" "$manifest_url"

echo
echo "done. Binaries staged in $OUT_DIR."
echo "Build with: cargo build -p server --release --features bundled-stress-test"
```

**Step 2: Make it executable and verify shellcheck if available**

```bash
chmod +x scripts/fetch-stress-test-binaries.sh
shellcheck scripts/fetch-stress-test-binaries.sh || echo "(shellcheck not installed, skipping)"
```

Expected: PASS (or skipped if shellcheck is missing).

**Step 3: Commit**

```bash
git add scripts/fetch-stress-test-binaries.sh
git commit -m "chore: add fetch-stress-test-binaries.sh for local bundled builds"
```

---

## Task 12: GitHub Actions release workflow

**Files:**

- Create: `.github/workflows/release.yml`

**Step 1: Write the workflow**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'

permissions:
  contents: write

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false

env:
  CARGO_TERM_COLOR: always
  CARGO_INCREMENTAL: 0

jobs:
  build:
    name: Build ${{ matrix.platform }}
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: linux-x86_64
            runner: ubuntu-latest
            target: x86_64-unknown-linux-musl
            ext: ''
          - platform: linux-aarch64
            runner: ubuntu-24.04-arm
            target: aarch64-unknown-linux-musl
            ext: ''
          - platform: windows-x86_64
            runner: windows-latest
            target: x86_64-pc-windows-msvc
            ext: '.exe'
          - platform: macos-universal
            runner: macos-latest
            target: universal2-apple-darwin
            ext: ''
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly
          targets:
            ${{ matrix.target == 'universal2-apple-darwin' &&
            'x86_64-apple-darwin,aarch64-apple-darwin' || matrix.target }}

      - name: Install musl tools (linux)
        if: contains(matrix.target, 'musl')
        run: |
          sudo apt-get update
          sudo apt-get install -y musl-tools

      - uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ matrix.platform }}

      - name: Build (non-mac)
        if: matrix.platform != 'macos-universal'
        run:
          cargo build -p stress-test --release --locked --target ${{
          matrix.target }}

      - name: Build (mac, both arches + lipo)
        if: matrix.platform == 'macos-universal'
        run: |
          cargo build -p stress-test --release --locked --target x86_64-apple-darwin
          cargo build -p stress-test --release --locked --target aarch64-apple-darwin
          mkdir -p target/universal2-apple-darwin/release
          lipo -create \
            target/x86_64-apple-darwin/release/broccoli-stress-test \
            target/aarch64-apple-darwin/release/broccoli-stress-test \
            -output target/universal2-apple-darwin/release/broccoli-stress-test

      - name: Stage artifact
        shell: bash
        run: |
          mkdir -p staging
          src="target/${{ matrix.target }}/release/broccoli-stress-test${{ matrix.ext }}"
          dest="staging/broccoli-stress-test-${{ matrix.platform }}${{ matrix.ext }}"
          cp "$src" "$dest"
          if [[ "${{ runner.os }}" == "macOS" ]]; then
            shasum -a 256 "$dest" > "$dest.sha256"
          else
            sha256sum "$dest" > "$dest.sha256"
          fi

      - name: Smoke test (--version)
        if:
          matrix.platform == 'linux-x86_64' || matrix.platform ==
          'macos-universal'
        shell: bash
        run: |
          chmod +x "staging/broccoli-stress-test-${{ matrix.platform }}${{ matrix.ext }}"
          "./staging/broccoli-stress-test-${{ matrix.platform }}${{ matrix.ext }}" --version

      - uses: actions/upload-artifact@v4
        with:
          name: stress-test-${{ matrix.platform }}
          path: staging/*

  release:
    name: Publish GitHub Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true

      - name: Build manifest.json
        shell: bash
        run: |
          version="${GITHUB_REF_NAME#v}"
          released_at="$(date -u +%FT%TZ)"
          base_url="https://github.com/${{ github.repository }}/releases/download/${GITHUB_REF_NAME}"
          python3 - <<PY > dist/manifest.json
          import hashlib, json, os, sys
          version="$version"
          released_at="$released_at"
          base_url="$base_url"
          platforms = {
              "linux-x86_64":  "broccoli-stress-test-linux-x86_64",
              "linux-aarch64": "broccoli-stress-test-linux-aarch64",
              "windows-x86_64":"broccoli-stress-test-windows-x86_64.exe",
              "macos-universal":"broccoli-stress-test-macos-universal",
          }
          out = {"version": version, "released_at": released_at, "platforms": {}}
          for key, fname in platforms.items():
              path = os.path.join("dist", fname)
              with open(path, "rb") as f:
                  data = f.read()
              out["platforms"][key] = {
                  "url": f"{base_url}/{fname}",
                  "sha256": hashlib.sha256(data).hexdigest(),
                  "size_bytes": len(data),
              }
          json.dump(out, sys.stdout, indent=2)
          PY
          cat dist/manifest.json

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: |
            dist/broccoli-stress-test-*
            dist/manifest.json
```

**Step 2: Lint with `actionlint` if available**

```bash
which actionlint && actionlint .github/workflows/release.yml || echo "(actionlint not installed, skipping)"
```

Expected: PASS (or skipped).

**Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: release workflow for stress-test cli (5 platforms + manifest)"
```

---

## Task 13: Extend CI to test the bundled feature

**Files:**

- Modify: `.github/workflows/ci.yml`

**Step 1: Add a job for the bundled-feature build/test path**

Modify `.github/workflows/ci.yml` — append a new job:

```yaml
bundled-server:
  name: Server (bundled-stress-test feature)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4

    - uses: dtolnay/rust-toolchain@master
      with:
        toolchain: nightly

    - uses: Swatinem/rust-cache@v2
      with:
        key: bundled-server

    - name: Stage fixture binaries as embedded
      run: |
        mkdir -p packages/server/embedded/stress-test
        cp packages/server/tests/fixtures/embedded-stress-test/* \
           packages/server/embedded/stress-test/

    - name: Build (bundled feature)
      run: cargo build -p server --features bundled-stress-test --locked

    - name: Test (bundled feature)
      run: cargo test -p server --features bundled-stress-test --locked
```

**Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add bundled-server job to verify bundled-stress-test feature builds"
```

---

## Task 14: Update README and CLI --help footer

**Files:**

- Modify: `packages/stress-test/README.md`
- Modify: `packages/stress-test/src/cli.rs`
- Modify: `README.md` (root)

**Step 1: Update CLI help footer**

In `packages/stress-test/src/cli.rs`, on the existing `#[derive(Parser)]` `Cli`
struct, add or update the `#[command(...)]` attribute to include an
`after_help`:

```rust
#[command(
    name = "broccoli-stress-test",
    version,
    about = "Stress and correctness testing for Broccoli online judge deployments.",
    after_help = "First time? Get the matching binary at <your-server>/downloads."
)]
pub struct Cli { ... }
```

Verify: `cargo run -p stress-test -- --help` Expected: footer line appears.

**Step 2: Append a "Releases" section to packages/stress-test/README.md**

Append:

````markdown
## Releases

Pre-built binaries for Linux (x86_64, aarch64), Windows (x86_64), and macOS
(universal) are published with each tagged version:

- **Recommended:** download from your Broccoli server at `<server>/downloads`.
  The binary served there is automatically version-matched to the server.
- **Alternative:** GitHub Releases at
  <https://github.com/JosephJoshua/broccoli/releases>.

For air-gapped lab environments, the bundled-server distribution path means lab
clients only need to reach the Broccoli server, not GitHub.

### Verifying downloads

Each binary ships with a `.sha256` companion file:

```bash
sha256sum -c broccoli-stress-test-linux-x86_64.sha256
```
````

### Unsigned binaries

The v1 release does not code-sign the macOS or Windows artifacts. After
download:

- **macOS:**
  `xattr -d com.apple.quarantine ./broccoli-stress-test-macos-universal`
- **Windows:** Click "More info → Run anyway" on SmartScreen, or `Unblock-File`
  in PowerShell.
- **Linux:** `chmod +x ./broccoli-stress-test-linux-x86_64`.

### CLI/server version handshake

On startup, the CLI fetches `<server>/api/v1/version` and prints a warning to
stderr if its own compile-time version doesn't match. Pass `--no-version-check`
to skip.

````

**Step 3: Add a brief mention to root README (if it has a "Distribution" or "Stress test" section, otherwise skip)**

Check `README.md` first. If a stress-test section exists, add a one-liner pointing at `packages/stress-test/README.md#releases`. If none exists, skip this step.

**Step 4: Commit**

```bash
git add packages/stress-test/src/cli.rs packages/stress-test/README.md README.md
git commit -m "docs(stresstest): document release artifacts and version handshake"
````

---

## Task 15: Verify the whole pipeline (final check)

**Step 1: Full slim build + test**

```bash
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
```

Expected: ALL PASS.

**Step 2: Bundled build + test (with fixtures)**

```bash
mkdir -p packages/server/embedded/stress-test
cp packages/server/tests/fixtures/embedded-stress-test/* packages/server/embedded/stress-test/
cargo build -p server --features bundled-stress-test --locked
cargo test -p server --features bundled-stress-test --locked
```

Expected: PASS.

**Step 3: Manual smoke test**

```bash
cargo run -p server --features bundled-stress-test &
sleep 2
curl -i http://localhost:3000/downloads
curl -i http://localhost:3000/downloads/manifest.json
curl -i http://localhost:3000/downloads/stress-test/linux-x86_64
curl -i http://localhost:3000/downloads/stress-test/freebsd-x86_64  # 404
curl -i http://localhost:3000/api/v1/version
kill %1
```

Expected:

- `/downloads` → 200 HTML.
- `/downloads/manifest.json` → 200 JSON, with `/downloads/stress-test/...` URLs
  (no github).
- `/downloads/stress-test/linux-x86_64` → 200,
  `Content-Type: application/octet-stream`, body is the fixture bytes.
- `/downloads/stress-test/freebsd-x86_64` → 404.
- `/api/v1/version` → 200 JSON with version + git_sha.

**Step 4: No commit needed for this verification task.**

---

## Done

The release pipeline is now ready. To cut v0.2.0:

```bash
# update Cargo.toml versions if needed (workspace + stress-test + server)
git tag v0.2.0
git push origin v0.2.0
```

CI will build, checksum, and publish the GitHub Release. Operators wanting the
bundled server then run:

```bash
./scripts/fetch-stress-test-binaries.sh v0.2.0
cargo build -p server --release --features bundled-stress-test
```

…and ship that binary to their Broccoli deployment, where lab admins will fetch
from `/downloads`.
