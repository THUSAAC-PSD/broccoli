# Stress-Test CLI Release & Distribution — Design

**Date:** 2026-05-03 **Status:** Approved **Owner:** Joseph **Implementation
plan:** `2026-05-03-stress-test-release.md`

## Context

`broccoli-stress-test` is a standalone TUI binary contest admins run to verify a
Broccoli deployment's correctness and throughput before a contest. It's used
both for online dry runs and for offline contests held in university computer
labs. Lab machines are often:

- **Snapshotted and reset** between sessions, so the binary must be re-fetchable
  in seconds.
- **Older or stripped-down** Linux distros, so dynamic glibc dependencies bite.
- **Behind flaky China–GitHub connectivity**, so the install path must not
  require reaching `github.com`.
- **Operated by non-developers** the day of the contest.

Today the binary builds via `cargo run -p stress-test` only. There is no release
artifact, no install path, no version-pinning between CLI and server. This
design fixes that.

## Goals

1. Contest admins can obtain a working CLI on a fresh, snapshotted lab machine
   in **under a minute**, with no Rust toolchain, no Docker, no `cargo install`,
   and no required outbound internet beyond the Broccoli server they're testing.
2. The CLI is **automatically version-matched** to the server it's run against,
   so API drift never causes a mid-contest surprise.
3. The release pipeline is a **single tag push**:
   `git tag v0.2.0 && git push --tags`.
4. The system **survives China–GitHub flakiness**: admins inside CN need nothing
   beyond the Broccoli server they're already deploying.

## Non-Goals (v1)

- Code signing / notarization on macOS or Windows. A documented one-line
  workaround (`xattr -d com.apple.quarantine` / SmartScreen "Run anyway") is
  acceptable for v1; signing is the natural follow-up when an Apple Developer ID
  exists.
- A `curl | sh` install script for laptops. The bundled-server path covers 95%
  of need; the bootstrap `wget` from GitHub is a one-time operator task that
  doesn't justify infrastructure.
- Server release. Tracked separately. This design only ensures the embed
  mechanism is in place for when server release lands.
- CLI auto-update or pinning beyond an advisory mismatch warning.
- Telemetry / phone-home of any kind.

## Two-Channel Distribution Model

| Channel                         | Audience                                                                                   | URL                                                                               |
| ------------------------------- | ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| **GitHub Releases**             | Operator doing first-ever bootstrap of a Broccoli server, or external CI fetching binaries | `https://github.com/.../releases/download/v0.2.0/broccoli-stress-test-{platform}` |
| **Server-bundled `/downloads`** | Every contest admin, every lab machine, every recurring use                                | `http://your-broccoli-server/downloads`                                           |

Both channels serve **the same binaries**. The server-bundled channel is the
primary mode of use; GitHub Releases exists so the binaries can get into a
server in the first place and so external CI can fetch them.

## Build Matrix

Five artifacts per release. Linux uses **musl** for static linking — a binary
built on Ubuntu 24.04 must run on a CentOS 7 lab machine.

| Job               | GitHub Runner      | Rust Target                                    | Notes                                   |
| ----------------- | ------------------ | ---------------------------------------------- | --------------------------------------- |
| `linux-x86_64`    | `ubuntu-latest`    | `x86_64-unknown-linux-musl`                    | Static, no libc dep.                    |
| `linux-aarch64`   | `ubuntu-24.04-arm` | `aarch64-unknown-linux-musl`                   | Native ARM runner; no cross-compile.    |
| `windows-x86_64`  | `windows-latest`   | `x86_64-pc-windows-msvc`                       | `.exe` suffix.                          |
| `macos-universal` | `macos-latest`     | `x86_64-apple-darwin` + `aarch64-apple-darwin` | `lipo -create` to one universal binary. |

The stress-test's `reqwest` dependency is already configured
`default-features = false, features = ["json", "stream", "rustls-tls", "multipart"]`
in the workspace `Cargo.toml`, so musl builds need no extra fiddling — pure-Rust
TLS via `rustls`, zero `openssl-sys` involvement.

Each artifact is named `broccoli-stress-test-{platform}` (or `.exe` on Windows)
and accompanied by a `.sha256` file produced by `sha256sum` (or `shasum -a 256`
on macOS).

## Versioning & Release Trigger

**Lockstep, monorepo-wide.** One repo version drives stress-test (today) and
server (later). Push a tag matching `v[0-9]+.[0-9]+.[0-9]+` and CI cuts a
release.

The stress-test's `Cargo.toml` `[package].version` field is updated via a small
step in the release workflow before building, so the tag and the binary's
`--version` always agree. The CLI exposes its compile-time version via
`env!("CARGO_PKG_VERSION")`.

**Why lockstep?** The whole architecture — bundled embeds, version-matched
`/downloads`, advisory mismatch warnings — leans on "the stress-test binary is
an artifact _of_ a server version." Independent versioning would fight that. The
cosmetic cost (bumping stress-test for unrelated server changes) is negligible.

## Version-Compatibility Check (Advisory)

When the CLI starts, before kicking off any scenarios, it does a single GET
against `/api/v1/version` (a new public, unauthenticated endpoint added by this
design). The server returns:

```json
{ "version": "0.2.0", "git_sha": "abc1234" }
```

The CLI compares to its own compile-time version. If they differ, it prints a
warning to stderr:

```
warning: stress-test 0.2.1 is targeting server 0.2.0 — version mismatch.
         For best results, download the matching binary from
         http://<server>/downloads
         (continuing anyway in 3s; pass --no-version-check to skip)
```

…then sleeps 3 seconds and continues. The CLI **does not** abort on mismatch —
admins running ad-hoc tests against staging vs prod shouldn't be blocked. The
3-second pause exists so the admin notices the warning before scenarios scroll
it offscreen.

A `--no-version-check` flag silences both the network call and the warning.

The `/api/v1/version` endpoint is a regular utoipa-annotated handler in the
`Admin` or new `Meta` tag. Public (no JWT). Returns 200 with the server's
`env!("CARGO_PKG_VERSION")` and the git SHA captured at build time.

## `/downloads` Endpoint Design

All routes are **public** (no auth — the binaries are public on GitHub Releases
too) and live on the **plain `axum::Router`** outside `/api`, so they don't
appear in the OpenAPI spec. They're a deployment concern, not an API surface.

### Routes

| Method & Path                                  | Returns                                                                                                                                                   |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /downloads`                               | HTML discovery page (browser-friendly). Auto-detects platform from `User-Agent` and highlights the right download.                                        |
| `GET /downloads/manifest.json`                 | Machine-readable list of platforms, URLs, SHA-256s, sizes, server version.                                                                                |
| `GET /downloads/stress-test/{platform}`        | Raw binary. `Content-Disposition: attachment; filename=...`. `ETag: "{server_version}-{platform}"`. `Cache-Control: public, max-age=31536000, immutable`. |
| `GET /downloads/stress-test/{platform}.sha256` | Plain-text SHA-256 + filename, in `sha256sum -c` format.                                                                                                  |

`{platform}` is one of: `linux-x86_64`, `linux-aarch64`, `windows-x86_64`,
`macos-universal`. Unknown platforms return 404.

### Discovery page

A single static HTML template (no React, no JS framework). The page:

- Shows the server version prominently.
- Lists all four platforms with download links + SHA-256 checksums + file sizes.
- Highlights the `User-Agent`-detected best match with a "Recommended for your
  platform" badge.
- Includes a short "Why does my OS warn about this binary?" section linking to a
  `/downloads/help` doc page that documents the macOS/Windows unsigned-binary
  workarounds.

The HTML is rendered by an `askama` or hand-rolled `format!()` template — small
enough that adding a templating dependency is overkill. Hand-rolled it is.

### Behavior on slim server (no `bundled-stress-test` feature)

When the server is built without the `bundled-stress-test` feature, the
`/downloads/*` routes are **not registered at all**. Hitting them returns axum's
default 404. There is no helpful "stress-test binaries not bundled in this
server" page — slim servers are explicitly opting out of being a distribution
channel.

## Server Cargo Feature: `bundled-stress-test`

A new optional feature on the `server` crate. Default: **off** (slim).

```toml
# packages/server/Cargo.toml
[features]
default = []
bundled-stress-test = []
```

When the feature is enabled:

- `build.rs` asserts five files exist at
  `packages/server/embedded/stress-test/`:
  - `linux-x86_64`
  - `linux-aarch64`
  - `windows-x86_64.exe`
  - `macos-universal`
  - `manifest.json` (containing version + SHA-256s + sizes — produced by the
    release workflow)
- `build.rs` re-runs if any of those files change
  (`cargo:rerun-if-changed=...`).
- A `routes/downloads.rs` module conditionally compiled with
  `#[cfg(feature = "bundled-stress-test")]` registers the four routes above. The
  module uses `include_bytes!` to embed each binary at compile time.
- `build_router()` calls `.merge(downloads::router())` only when the feature is
  on.

Local devs who want to build the bundled server run a one-liner:

```bash
./scripts/fetch-stress-test-binaries.sh v0.2.0
```

…which downloads the five artifacts from GitHub Releases into
`packages/server/embedded/stress-test/`. The directory is `.gitignore`d. Without
that step, `cargo build --features bundled-stress-test` fails with a clear error
from `build.rs`.

## CI Release Workflow

A new `.github/workflows/release.yml` triggered on `v*` tag push.

```
on:
  push:
    tags: [ 'v*' ]

jobs:
  build:
    strategy:
      matrix: [linux-x86_64, linux-aarch64, windows-x86_64, macos-universal]
    steps:
      - checkout
      - install rust + target
      - cargo build -p stress-test --release --target ${{ matrix.target }}
        (macos: build both arches, lipo)
      - rename to broccoli-stress-test-{platform}[.exe]
      - sha256sum > .sha256
      - upload artifact

  release:
    needs: build
    steps:
      - download all artifacts
      - generate manifest.json from artifact metadata
      - softprops/action-gh-release to create GitHub Release with all
        binaries + .sha256 files + manifest.json + auto-generated changelog
```

Total wall time target: **under 10 minutes**. macOS builds are slowest (two
arches + lipo).

Concurrency:
`concurrency: { group: release-${{ github.ref }}, cancel-in-progress: false }` —
never cancel a tagged release mid-flight.

## Manifest format

`manifest.json` (served by `/downloads/manifest.json` and uploaded to GitHub
Releases):

```json
{
  "version": "0.2.0",
  "released_at": "2026-05-03T12:34:56Z",
  "platforms": {
    "linux-x86_64": {
      "url": "https://github.com/.../releases/download/v0.2.0/broccoli-stress-test-linux-x86_64",
      "sha256": "abc123...",
      "size_bytes": 12345678
    },
    "linux-aarch64": { ... },
    "windows-x86_64": { ... },
    "macos-universal": { ... }
  }
}
```

When served by the bundled server, the `url` field is rewritten to point at the
same server's `/downloads/stress-test/{platform}` (so admins inside an
air-gapped lab don't get pointed back to GitHub). The discovery page reads from
this manifest at runtime.

## Help & error UX

- The `/downloads` discovery page includes a small "Trouble running?" link to
  `/downloads/help` (a static HTML page) explaining:
  - **macOS:** `xattr -d com.apple.quarantine ./broccoli-stress-test` after
    download, or right-click → Open the first time.
  - **Windows:** Click "More info → Run anyway" on the SmartScreen warning.
    (Optionally, `Unblock-File` in PowerShell.)
  - **Linux:** `chmod +x ./broccoli-stress-test`.
- The CLI's `--help` output gets a "First time? Get the latest matching binary
  at `<your-server>/downloads`" footer.

## Compression of embedded binaries

Open question with a default: **no compression in v1**. Five binaries × ~10–15MB
= ~60–75MB embedded into the server. Acceptable for a backend service. If
profiling shows memory pressure, switching to `include_bytes!` + `flate2`/`zstd`
decompression on first request is a contained follow-up. Rejected for v1 to keep
`/downloads` a flat-file serve with no decompression latency on first hit.

## Rate limiting

`/downloads/*` routes are **not** behind any custom rate limiter beyond axum's
default body-size protections (binaries are streamed via `include_bytes!` so
they don't allocate). Lab admins downloading binaries are not an attack vector
worth defending against on the server they own. If abuse becomes a concern
(e.g., a public Broccoli deployment hammered by scrapers), add a `tower::limit`
layer at that point.

## Security considerations

- **No code execution path from /downloads.** Binaries are served as
  `application/octet-stream` with `Content-Disposition: attachment` — browsers
  won't execute, can't inline.
- **No path traversal.** `{platform}` is matched against a static enum of four
  known values; anything else returns 404 before any filesystem or memory
  lookup.
- **SHA-256 verification** is documented as an optional admin step
  (`sha256sum -c broccoli-stress-test-linux-x86_64.sha256`). Not enforced —
  admins who don't verify aren't worse off than today (they can't verify
  _anything_ today).
- **Binaries are unsigned.** Documented in the help page. v1 limitation, tracked
  for follow-up.
- **No telemetry.** The CLI's startup version check is a single GET to a public
  endpoint; no extra data is sent. Documented in `--help`.

## Open follow-ups (out of scope for v1)

- macOS notarization + Windows code-signing (requires Apple Developer ID + EV
  cert).
- `curl | sh` install script with mirror support (GitHub → ghproxy fallback) for
  laptop bootstrap.
- Server release pipeline (separate design, will reuse this workflow's
  artifact-staging pattern).
- Aliyun OSS / Qiniu mirror for GitHub Releases (defer until someone reports
  actual pain).
- Compression of embedded binaries if the server's RSS becomes a concern.

## Test strategy

- **Unit tests** on the platform-detection logic in the discovery handler
  (User-Agent → platform mapping).
- **Unit tests** on the manifest URL-rewriting (GitHub URLs → relative server
  URLs when served).
- **Integration test** (in `packages/server/tests/integration/`) that builds the
  server with `--features bundled-stress-test` _only_ if a fixture set of dummy
  embedded binaries is present. The fixture binaries are tiny (a few bytes of
  `0xDEADBEEF`) and committed to git under
  `packages/server/tests/fixtures/embedded-stress-test/`. The test asserts:
  - `GET /downloads` returns 200 HTML containing the version string.
  - `GET /downloads/manifest.json` returns valid JSON with all four platforms.
  - `GET /downloads/stress-test/linux-x86_64` returns the embedded fixture bytes
    with correct headers.
  - `GET /downloads/stress-test/unknown-platform` returns 404.
  - On the slim build, `GET /downloads` returns 404.
- **CLI integration test** that mocks a `/api/v1/version` endpoint via
  `wiremock` and asserts the warning text on mismatch and silence on match and
  on `--no-version-check`.
- **CI smoke test** in the release workflow: after building each artifact, run
  `./broccoli-stress-test --version` to confirm the binary at minimum links and
  starts.
