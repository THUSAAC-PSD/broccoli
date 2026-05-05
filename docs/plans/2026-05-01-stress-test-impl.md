# Stress Test Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development to dispatch a fresh implementer
> subagent per task with two-stage review.

**Goal:** Implement the stress test binary specified in
`docs/plans/2026-05-01-stress-test-design.md` — a portable, single-file Rust
binary that admins run on a freshly provisioned contest box to verify the
Broccoli platform is correct and capable under load.

**Architecture:** New workspace member `packages/stress-test/`. Tokio + reqwest
(rustls-tls only, no OpenSSL). DTOs mirrored from the server. Three phases —
correctness, load, optional pass-through — driven by a background runtime that
emits typed `Event`s; either an htop-style ratatui TUI or a non-TTY line mode
consumes them. All fixtures embedded via `include_bytes!`. Static musl builds
for distribution.

**Tech stack:** `tokio`, `reqwest` (rustls-tls), `clap`, `serde`, `serde_json`,
`ratatui`, `crossterm`, `hdrhistogram`, `anyhow`, `thiserror`, `tracing`. Tests
use `wiremock` for HTTP mocking and ratatui's `TestBackend` for TUI snapshots.

**Reference:** Always read the design doc at
`docs/plans/2026-05-01-stress-test-design.md` for context. The design has been
corrected post-Explore and is canonical.

---

## Phasing

The plan is organised into three phases. Each phase ships something usable:

- **Phase A (MVP)** — Tasks 1–15. Functional binary that runs all three phases
  and produces a plain-text PASS/FAIL report. No TUI yet, no JSON, no
  cross-builds. After Phase A, the tool is usable on a Linux contest box for its
  core purpose.
- **Phase B (TUI)** — Tasks 16–19. The htop-style live UI on top of the Phase A
  event stream. Phase A's plain-text output remains the non-TTY fallback.
- **Phase C (Polish)** — Tasks 20–25. JSON output, cleanup, portability harness,
  real-server e2e test, docs.

Subagents may discover scope creep mid-phase. Surface it; don't silently expand.

---

## Phase A — MVP

### Task 1: Workspace scaffold

**Files:**

- Create: `packages/stress-test/Cargo.toml`
- Create: `packages/stress-test/src/main.rs`
- Create: `packages/stress-test/src/lib.rs`
- Create: `packages/stress-test/README.md` (placeholder; full README is Task 25)
- Modify: `Cargo.toml` (root) — add to `[workspace.members]` and exclude from
  `[workspace.default-members]`

**Step 1: Add workspace dependencies needed by the new crate**

Pull these into root `[workspace.dependencies]` if not already present (check
first; many already exist):

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls", "multipart"] }
clap = { version = "4", features = ["derive"] }
ratatui = "0.28"
crossterm = "0.28"
hdrhistogram = "7"
wiremock = "0.6"            # dev-dependencies only
```

`tokio`, `serde`, `serde_json`, `anyhow`, `thiserror`, `tracing`,
`tracing-subscriber`, `chrono` are already in workspace deps — reuse via
`{ workspace = true }`.

**Step 2: Create `packages/stress-test/Cargo.toml`**

```toml
[package]
name = "stress-test"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "broccoli-stress-test"
path = "src/main.rs"

[dependencies]
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
clap = { workspace = true }
ratatui = { workspace = true }
crossterm = { workspace = true }
hdrhistogram = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
wiremock = { workspace = true }
```

**Step 3: Create minimal `src/main.rs` and `src/lib.rs`**

```rust
// src/main.rs
fn main() {
    println!("broccoli-stress-test (scaffolding)");
}
```

```rust
// src/lib.rs
//! Broccoli stress test — see docs/plans/2026-05-01-stress-test-design.md.
```

**Step 4: Update root `Cargo.toml`**

Add `"packages/stress-test"` to `[workspace.members]`. Set or update
`[workspace.default-members]` to list **all current members except**
`packages/stress-test` (research the existing list first; if there's no
`default-members` key, add one explicitly listing every other member).

**Step 5: Verify build and dependency hygiene**

Run:

```bash
cargo build -p stress-test
```

Expected: clean build.

```bash
cargo tree -p stress-test 2>/dev/null | grep -i openssl
```

Expected: empty output. **If anything matches, fail this task.** This means a
transitive dep pulled in OpenSSL and the `default-features = false` /
`rustls-tls` configuration is wrong.

```bash
cargo build
```

Expected: builds the rest of the workspace **without** building stress-test
(verify by checking the output doesn't mention `stress-test`).

**Step 6: Commit**

```
feat(stress-test): scaffold workspace member

Empty crate, rustls-tls-only reqwest (no OpenSSL), excluded from
default-members so plain `cargo build` stays fast.
```

---

### Task 2: CLI definitions

**Files:**

- Create: `packages/stress-test/src/cli.rs`
- Modify: `packages/stress-test/src/main.rs`
- Modify: `packages/stress-test/src/lib.rs` (re-export)
- Test: `packages/stress-test/tests/cli.rs`

**Step 1: Write a failing CLI test**

```rust
// tests/cli.rs
use clap::Parser;
use stress_test::cli::Cli;

#[test]
fn parses_minimal_url_plus_token() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url", "http://localhost:3000",
        "--admin-token", "abc",
    ]).unwrap();
    assert_eq!(cli.url, "http://localhost:3000");
    assert_eq!(cli.admin_token.as_deref(), Some("abc"));
    assert_eq!(cli.total, 200);
    assert_eq!(cli.rate, 20);
    assert_eq!(cli.concurrency, 50);
}

#[test]
fn requires_url() {
    let r = Cli::try_parse_from(["broccoli-stress-test", "--admin-token", "abc"]);
    assert!(r.is_err());
}

#[test]
fn rejects_token_and_username_password_both_missing() {
    // Validation happens in Cli::validate() — write that helper too.
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url", "http://localhost:3000",
    ]).unwrap();
    assert!(cli.validate().is_err());
}
```

Run: `cargo test -p stress-test --test cli`. Expected: compile error on missing
`Cli` type.

**Step 2: Implement `cli.rs`**

Use `clap` derive. Match flags exactly to the design doc's CLI section.
Required: `--url`. Either `--admin-token` or both `--admin-username` and
`--admin-password`. Defaults: `--total 200`, `--rate 20`, `--concurrency 50`,
`--per-job-timeout 60`, `--p95-budget-ms 15000`, `--contest-concurrency 20`,
`--seed 0` (deterministic). Boolean flags: `--skip-correctness`, `--skip-load`,
`--keep-fixtures`, `--json`.

Add a `pub fn validate(&self) -> Result<(), String>` method that returns `Err`
if neither token nor (username AND password) are present, and returns `Err` if
both `--skip-correctness` and `--skip-load` are set (the run would have nothing
to do).

**Step 3: Wire into main.rs**

```rust
fn main() {
    let cli = stress_test::cli::Cli::parse();
    if let Err(e) = cli.validate() {
        eprintln!("error: {e}");
        std::process::exit(64); // EX_USAGE
    }
    println!("{cli:#?}");
}
```

**Step 4: Re-export from lib.rs**

```rust
pub mod cli;
```

**Step 5: Verify**

```bash
cargo test -p stress-test --test cli
cargo run -p stress-test -- --help
```

Expected: tests pass; `--help` lists every flag from the design with
descriptions.

**Step 6: Commit**

```
feat(stress-test): add CLI definitions
```

---

### Task 3: DTO mirrors

**Files:**

- Create: `packages/stress-test/src/dto.rs`
- Modify: `packages/stress-test/src/lib.rs` (`pub mod dto;`)
- Test: inline `#[cfg(test)]` in `dto.rs` is fine for this task

**Background:** Read these source files first to mirror types accurately:

- `packages/server/src/models/auth.rs` — `LoginRequest`, `LoginResponse`
- `packages/server/src/models/submission.rs` — `CreateSubmissionRequest`,
  `SubmissionFileDto`, `SubmissionResponse`, `JudgeResultResponse`,
  `TestCaseResultResponse`
- `packages/server/src/models/problem.rs` — `CreateProblemRequest`,
  `CreateTestCaseRequest`, `ProblemResponse`
- `packages/common/src/submission_status.rs` — `SubmissionStatus`, `Verdict`
- `packages/server/src/handlers/admin.rs` — multipart shape for plugin upload
  (we don't need a struct, just verify field names)
- `packages/server/src/error.rs` — `ErrorBody` (`code`, `message`)

**Step 1: Define types with `#[derive(Serialize, Deserialize, Debug)]`**

For each DTO, mirror only the fields the stress test reads or writes. Add a
`// Mirrors `packages::path::Type` (file:line)` doc comment so a future
contributor can audit drift.

Specific care:

- `Verdict` and `SubmissionStatus` are enums. Use
  `#[serde(rename_all = "PascalCase")]` if the server emits PascalCase. Verify
  against the actual `impl Serialize` / `impl Display` (the Explore agent noted
  `Other(String)` renders as just the inner string — the stress test does
  **not** need to match `Other` against any known scenario, so treat any
  unrecognised string as `Verdict::Unknown(String)` to avoid serde errors when a
  plugin returns a custom verdict).

- `ErrorBody`: `{ code: String, message: String }`.

- For `SubmissionStatus`, replicate `is_terminal()`:

  ```rust
  impl SubmissionStatus {
      pub fn is_terminal(&self) -> bool {
          matches!(self, Self::Judged | Self::CompilationError | Self::SystemError)
      }
  }
  ```

**Step 2: Write roundtrip tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submission_status_round_trip() {
        for s in [SubmissionStatus::Pending, SubmissionStatus::Judged,
                  SubmissionStatus::CompilationError, SubmissionStatus::SystemError] {
            let j = serde_json::to_string(&s).unwrap();
            let back: SubmissionStatus = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn parses_real_submission_response() {
        let json = r#"{
            "id": 1, "files": [...], "language": "cpp",
            "status": "Judged", "user_id": 1, "username": "admin",
            "problem_id": 1, "problem_title": "ab", "contest_id": null,
            "contest_type": "stress", "judge_epoch": 0,
            "created_at": "2026-05-01T00:00:00Z",
            "result": { "verdict": "Accepted", "score": 100.0, ... }
        }"#;
        let r: SubmissionResponse = serde_json::from_str(json).unwrap();
        assert!(r.status.is_terminal());
    }
    // ...one parse test per DTO, with realistic JSON
}
```

**Step 3: Run tests, ensure they pass**

```bash
cargo test -p stress-test dto
```

**Step 4: Commit**

```
feat(stress-test): add DTO mirrors of server types
```

---

### Task 4: HTTP client with auth + 401 retry

**Files:**

- Create: `packages/stress-test/src/client.rs`
- Create: `packages/stress-test/src/error.rs`
- Modify: `packages/stress-test/src/lib.rs`

**Step 1: Define `StressError`**

```rust
// src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum StressError {
    #[error("HTTP {status}: {code} — {message}")]
    Api { status: u16, code: String, message: String },
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("decode error: {0}")]
    Decode(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type StressResult<T> = Result<T, StressError>;
```

**Step 2: Verify JWT lifetime in source**

Open `packages/server/src/handlers/auth.rs` (or wherever the JWT is signed) and
find the literal expiry duration. Note it in a doc comment on the `Client`
struct. This resolves design Open Question #5.

**Step 3: Implement `Client`**

```rust
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    creds: AuthCreds,
    token: tokio::sync::RwLock<String>,
}

pub enum AuthCreds {
    Token(String),
    UsernamePassword { username: String, password: String },
}

impl Client {
    pub async fn new(base_url: String, creds: AuthCreds) -> StressResult<Self> { ... }
    pub async fn login(&self) -> StressResult<()> { ... }
    pub async fn create_problem(&self, req: &CreateProblemRequest) -> StressResult<ProblemResponse> { ... }
    pub async fn create_test_case(&self, problem_id: i32, req: &CreateTestCaseRequest) -> StressResult<TestCaseResponse> { ... }
    pub async fn delete_problem(&self, problem_id: i32) -> StressResult<()> { ... }
    pub async fn create_submission(&self, problem_id: i32, req: &CreateSubmissionRequest) -> StressResult<SubmissionResponse> { ... }
    pub async fn get_submission(&self, id: i32) -> StressResult<SubmissionResponse> { ... }
    pub async fn upload_plugin_archive(&self, plugin_id: &str, tar_gz_bytes: &[u8]) -> StressResult<()> { ... }
    pub async fn disable_plugin(&self, plugin_id: &str) -> StressResult<()> { ... }
    pub async fn list_contest_problems(&self, contest_id: i32) -> StressResult<Vec<ContestProblemResponse>> { ... }
    pub async fn get_problem(&self, id: i32) -> StressResult<ProblemResponse> { ... }
}
```

Internal helper `send_with_retry`:

1. Send request with current token in `Authorization: Bearer …`.
2. If response is 401 **and** `creds` is `UsernamePassword`, call `login()`,
   replace token, retry **once**.
3. On non-2xx, decode body as `ErrorBody` and return `StressError::Api`.
4. On 2xx, deserialise body as `T`.

The plugin upload uses `reqwest::multipart::Form` with a single `plugin` part
containing the bytes (filename `"plugin.tar.gz"`, content type
`application/gzip`).

**Step 4: Tests with `wiremock`**

```rust
#[tokio::test]
async fn login_uses_post_v1_auth_login_and_stores_token() { ... }

#[tokio::test]
async fn get_submission_attaches_bearer_token() { ... }

#[tokio::test]
async fn re_logs_in_once_on_401_when_creds_available() { ... }

#[tokio::test]
async fn 401_with_token_only_creds_fails_loud() { ... }

#[tokio::test]
async fn upload_plugin_sends_multipart_with_plugin_field() { ... }

#[tokio::test]
async fn api_errors_decode_into_StressError_Api() { ... }
```

Each spins up a wiremock `MockServer` and asserts both behaviour and the exact
request shape (verify path, method, headers, body matchers).

**Step 5: Run + commit**

```bash
cargo test -p stress-test client
```

```
feat(stress-test): add HTTP client with auth + 401 re-login
```

---

### Task 5: Fixture solutions + embed macros

**Files:**

- Create:
  - `packages/stress-test/fixtures/solutions/ab_cpp_ac.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_wa.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_tle.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_mle.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_re.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_ce.cpp`
  - `packages/stress-test/fixtures/solutions/ab_cpp_igncase.cpp`
  - `packages/stress-test/fixtures/solutions/ab_py_ac.py`
  - `packages/stress-test/fixtures/multi-file/solution.cpp`
  - `packages/stress-test/fixtures/multi-file/helper.hpp`
- Create: `packages/stress-test/src/fixtures.rs`
- Modify: `src/lib.rs`

**Step 1: Write each fixture source**

Reference snippets (use these — they were chosen for cross-platform stability):

```cpp
// ab_cpp_ac.cpp — accepted
#include <iostream>
int main() { int a, b; std::cin >> a >> b; std::cout << a + b << "\n"; }
```

```cpp
// ab_cpp_wa.cpp — wrong answer (always prints 42)
#include <iostream>
int main() { int a, b; std::cin >> a >> b; std::cout << 42 << "\n"; }
```

```cpp
// ab_cpp_tle.cpp — busy loop
int main() { volatile long x = 0; while (true) { x += 1; } }
```

```cpp
// ab_cpp_mle.cpp — allocate 80 million ints (~320 MB)
#include <vector>
#include <iostream>
int main() {
    std::vector<int> v(80'000'000, 1);
    int a, b; std::cin >> a >> b;
    std::cout << a + b + v[0] - 1 << "\n";
}
```

```cpp
// ab_cpp_re.cpp — null pointer deref
int main() { int a, b; *((int*)0) = 1; (void)a; (void)b; return 0; }
```

```cpp
// ab_cpp_ce.cpp — syntax error
int main(  // intentionally unterminated
```

```cpp
// ab_cpp_igncase.cpp — prints lowercase "yes"
#include <iostream>
int main() { std::cout << "yes\n"; }
```

```python
# ab_py_ac.py
a, b = map(int, input().split())
print(a + b)
```

```cpp
// multi-file/solution.cpp
#include <iostream>
#include "helper.hpp"
int main() { int a, b; std::cin >> a >> b; std::cout << add(a, b) << "\n"; }
```

```cpp
// multi-file/helper.hpp
#pragma once
inline int add(int a, int b) { return a + b; }
```

**Step 2: Write `src/fixtures.rs`**

```rust
pub const SOLUTION_AB_CPP_AC: &str = include_str!("../fixtures/solutions/ab_cpp_ac.cpp");
pub const SOLUTION_AB_CPP_WA: &str = include_str!("../fixtures/solutions/ab_cpp_wa.cpp");
// ...
pub const MULTI_FILE_SOLUTION_CPP: &str = include_str!("../fixtures/multi-file/solution.cpp");
pub const MULTI_FILE_HELPER_HPP: &str = include_str!("../fixtures/multi-file/helper.hpp");

// Plugin tar.gz lives here once Task 7 produces it:
// pub const STRESS_TEST_PLUGIN_TAR_GZ: &[u8] = include_bytes!("../fixtures/plugin/broccoli-stress-test.tar.gz");
```

**Step 3: Tests**

Trivial — assert each constant is non-empty:

```rust
#[test]
fn all_fixtures_loaded() {
    assert!(!SOLUTION_AB_CPP_AC.is_empty());
    // ...
}
```

**Step 4: Commit**

```
feat(stress-test): add fixture sources and embed macros
```

---

### Task 6: Plugin discovery vs build — research spike

**Files:**

- Create: `docs/plans/2026-05-01-stress-test-plugin-decision.md`

This is a research-only task. No code yet. Read:

- The existing plugins under `packages/plugins/` — how they declare contest
  types and evaluators in `plugin.toml`, what host functions they call from
  `register()`, how `on_submission` is implemented.
- The plugin-core registry (`packages/plugin-core/src/registry.rs`) and the
  related host functions (`packages/plugin-core/src/host_funcs/`).
- The plugin manifest discovery in `packages/server/src/manager.rs`.
- The example reference plugin (likely `plugins/broccoli-zh-cn` or similar in
  the repo root `plugins/` dir).

Decide one of:

- **A) Build a minimal fixture plugin.** Necessary if no existing plugin is
  guaranteed-loaded on a fresh server and we need deterministic contest-type /
  evaluator names. Pros: deterministic, predictable. Cons: more code, has to
  track plugin SDK changes.
- **B) Discover at runtime.** `GET /plugins/active` (verify exists) returns
  loaded plugins; pick the first contest type and first evaluator. Pros: zero
  new plugin code. Cons: behaviour differs across deployments, harder to assert
  verdicts deterministically.
- **C) Require admin to specify.** New flags `--contest-type` and
  `--problem-type`; admin tells us what to use. Pros: simplest. Cons: more
  friction for the admin.

Write the decision doc with: findings (what was read, with file:line citations),
the three options' pros/cons in concrete terms for _this_ codebase, the
recommendation, and the rationale. The decision drives Task 7.

Commit:

```
docs(stress-test): plugin strategy decision

Result: <chosen option> — <one-line rationale>.
```

---

### Task 7: Bundled fixture plugin (conditional on Task 6)

**Skip this entire task if Task 6 chose B or C.** Update Task 8 accordingly in
that case (no plugin upload step; instead fetch the registered types from the
server or take them from flags).

**If Task 6 chose A:**

**Files:**

- Create: `packages/stress-test/fixtures/plugin-src/Cargo.toml` (separate Cargo
  project, NOT in workspace)
- Create: `packages/stress-test/fixtures/plugin-src/src/lib.rs`
- Create: `packages/stress-test/fixtures/plugin-src/plugin.toml`
- Create: `packages/stress-test/fixtures/plugin/broccoli-stress-test.tar.gz`
  (committed binary artifact)
- Modify: root `justfile` — add `stress-test-rebuild-plugin` recipe
- Modify: `packages/stress-test/src/fixtures.rs` — add the
  `STRESS_TEST_PLUGIN_TAR_GZ` constant

**Step 1: Author the plugin**

The plugin registers exactly:

- One contest type, e.g. `stress-test/passthrough`. Its `on_submission` is the
  simplest possible implementation that delegates to the registered batch
  evaluator and reports the aggregated result. Look at an existing
  passthrough/ICPC plugin for the pattern.
- One evaluator type, e.g. `stress-test/batch`. Its job is to evaluate every
  test case sequentially and return verdicts as the worker reports them.

Both ids namespaced under `stress-test/` so they can never collide with admin
plugins.

**Step 2: Build to wasm32-wasip1**

```bash
cd packages/stress-test/fixtures/plugin-src
cargo build --target wasm32-wasip1 --release
```

**Step 3: Package as tar.gz**

```bash
cd packages/stress-test/fixtures
mkdir -p staging/broccoli-stress-test
cp plugin-src/plugin.toml staging/broccoli-stress-test/
cp plugin-src/target/wasm32-wasip1/release/*.wasm staging/broccoli-stress-test/
tar -czf plugin/broccoli-stress-test.tar.gz -C staging broccoli-stress-test
rm -rf staging
```

(Wrap this in a `justfile` recipe.)

**Step 4: Embed bytes in fixtures.rs**

```rust
pub const STRESS_TEST_PLUGIN_TAR_GZ: &[u8] =
    include_bytes!("../fixtures/plugin/broccoli-stress-test.tar.gz");
pub const STRESS_TEST_PLUGIN_ID: &str = "broccoli-stress-test";
pub const STRESS_TEST_CONTEST_TYPE: &str = "stress-test/passthrough";
pub const STRESS_TEST_PROBLEM_TYPE: &str = "stress-test/batch";
```

**Step 5: Test that the archive is well-formed**

```rust
#[test]
fn plugin_tar_gz_unpacks_with_expected_layout() {
    use flate2::read::GzDecoder;
    let gz = GzDecoder::new(STRESS_TEST_PLUGIN_TAR_GZ);
    let mut tar = tar::Archive::new(gz);
    let names: Vec<_> = tar.entries().unwrap()
        .map(|e| e.unwrap().path().unwrap().display().to_string())
        .collect();
    assert!(names.iter().any(|n| n.contains("broccoli-stress-test/plugin.toml")));
    assert!(names.iter().any(|n| n.ends_with(".wasm")));
}
```

(`flate2` and `tar` go in `dev-dependencies`.)

**Step 6: Commit (two commits, since the .wasm/.tar.gz are binaries)**

```
feat(stress-test): bundled fixture plugin source

Registers stress-test/passthrough contest type + stress-test/batch
evaluator. Source is a separate Cargo project under fixtures/plugin-src;
not in the workspace.
```

```
chore(stress-test): commit pre-built fixture plugin tar.gz

Built with `cargo build --target wasm32-wasip1 --release` and packaged
via `just stress-test-rebuild-plugin`. Same pattern as
packages/server/tests/fixtures/echo-plugin/.
```

---

### Task 8: Bootstrap module

**Files:**

- Create: `packages/stress-test/src/bootstrap.rs`
- Modify: `src/lib.rs`

**Step 1: Define `BootstrapState`**

```rust
pub struct BootstrapState {
    pub plugin_id: Option<String>, // None if Task 6 chose B or C
    pub problem_ids_by_scenario: HashMap<&'static str, i32>,
}
```

**Step 2: Implement `bootstrap()`**

```rust
pub async fn bootstrap(client: &Client, scenarios: &[Scenario]) -> StressResult<BootstrapState> {
    // 1. login() — already handled by Client::new if creds provided.
    // 2. If Task 6 chose A: client.upload_plugin_archive(STRESS_TEST_PLUGIN_ID, STRESS_TEST_PLUGIN_TAR_GZ).await?;
    //    Errors: if plugin already exists, swallow ConflictError and continue.
    // 3. For each scenario: create problem with the scenario's params,
    //    then create exactly one test case (input "1 2\n", expected output per scenario).
    // 4. Return populated BootstrapState.
}
```

**Step 3: wiremock tests**

```rust
#[tokio::test]
async fn happy_path_creates_plugin_then_problems_then_test_cases() { ... }

#[tokio::test]
async fn handles_plugin_already_loaded_409_gracefully() { ... }

#[tokio::test]
async fn surfaces_problem_creation_errors() { ... }
```

**Step 4: Commit**

```
feat(stress-test): bootstrap module — plugin upload + problem creation
```

---

### Task 9: Scenarios as data

**Files:**

- Create: `packages/stress-test/src/scenarios.rs`
- Modify: `src/lib.rs`

**Step 1: Define `Scenario`**

```rust
pub struct Scenario {
    pub id: &'static str,
    pub language: &'static str,
    pub files: &'static [(&'static str, &'static str)], // (filename, content)
    pub time_limit_ms: i32,
    pub memory_limit_kb: i32,
    pub checker_format: &'static str,
    pub test_input: &'static str,
    pub test_expected_output: &'static str,
    pub expected_status: SubmissionStatus,
    pub expected_verdict: Option<Verdict>,
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario { id: "ab-cpp-ac", language: "cpp",
        files: &[("solution.cpp", crate::fixtures::SOLUTION_AB_CPP_AC)],
        time_limit_ms: 1000, memory_limit_kb: 65536,
        checker_format: "exact",
        test_input: "1 2\n", test_expected_output: "3\n",
        expected_status: SubmissionStatus::Judged,
        expected_verdict: Some(Verdict::Accepted),
    },
    // ... 8 more, one per row of the design's scenario table
];
```

**Step 2: Validation tests**

```rust
#[test]
fn scenario_ids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for s in SCENARIOS { assert!(seen.insert(s.id), "duplicate {}", s.id); }
}

#[test]
fn scenarios_cover_design_doc_table() {
    let ids: Vec<_> = SCENARIOS.iter().map(|s| s.id).collect();
    for expected in ["ab-cpp-ac","ab-py-ac","ab-cpp-wa","ab-cpp-tle",
                     "ab-cpp-mle","ab-cpp-re","ab-cpp-ce","ab-cpp-igncase",
                     "ab-cpp-multi"] {
        assert!(ids.contains(&expected), "missing scenario {expected}");
    }
}

#[test]
fn ce_scenario_has_no_verdict() {
    let ce = SCENARIOS.iter().find(|s| s.id == "ab-cpp-ce").unwrap();
    assert_eq!(ce.expected_status, SubmissionStatus::CompilationError);
    assert!(ce.expected_verdict.is_none());
}
```

**Step 3: Commit**

```
feat(stress-test): define correctness scenarios as static data
```

---

### Task 10: Events + plain-mode renderer

**Files:**

- Create: `packages/stress-test/src/events.rs`
- Create: `packages/stress-test/src/ui/mod.rs`
- Create: `packages/stress-test/src/ui/plain.rs`
- Modify: `src/lib.rs`

**Step 1: Define `Event`**

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PhaseStarted { phase: Phase },
    PhaseFinished { phase: Phase, ok: bool },
    ScenarioStarted { id: String },
    ScenarioFinished { id: String, ok: bool, status: SubmissionStatus,
                       verdict: Option<Verdict>, duration_ms: u64 },
    LoadSubmitted { sequence: u64, scenario: String },
    LoadCompleted { sequence: u64, ok: bool, latency_ms: u64,
                    expected: ExpectedTerminal, actual: ActualTerminal },
    PassthroughSkipped { reason: String },
    PassthroughCompleted { ok: bool, count: usize },
    Error { phase: Option<Phase>, message: String },
}

pub enum Phase { Bootstrap, Correctness, Load, Passthrough, Cleanup }
```

`PhaseFinished.ok` is the per-phase pass/fail signal that the runner emits.

**Step 2: Plain-mode renderer**

```rust
// src/ui/plain.rs
pub async fn run(mut rx: mpsc::UnboundedReceiver<Event>) {
    while let Some(ev) = rx.recv().await {
        // Render one structured line per event to stdout, e.g.:
        //   [14:32:18Z] OK   correctness  ab-cpp-ac      Accepted        412ms
        //   [14:32:21Z] ERR  load    #142  expected Accepted, got WrongAnswer
        // No colours, no escape codes.
    }
}
```

**Step 3: Snapshot test**

```rust
#[tokio::test]
async fn plain_renderer_formats_canned_event_stream() {
    let (tx, rx) = mpsc::unbounded_channel();
    // push a deterministic sequence of events, drop tx, run renderer with stdout captured.
    // assert_eq! against a recorded snapshot string.
}
```

For stdout capture in tests, route through a `dyn Write` parameter so the test
can pass a `Vec<u8>`.

**Step 4: Commit**

```
feat(stress-test): events + plain-text renderer
```

---

### Task 11: Correctness phase runner

**Files:**

- Create: `packages/stress-test/src/correctness.rs`
- Modify: `src/lib.rs`

**Step 1: Implement the runner**

```rust
pub async fn run(
    client: &Client,
    state: &BootstrapState,
    scenarios: &[Scenario],
    timeout: Duration,
    tx: &mpsc::UnboundedSender<Event>,
) -> bool {
    tx.send(Event::PhaseStarted { phase: Phase::Correctness }).ok();
    let mut all_ok = true;
    for s in scenarios {
        tx.send(Event::ScenarioStarted { id: s.id.into() }).ok();
        let started = Instant::now();
        let pid = state.problem_ids_by_scenario[s.id];
        let req = CreateSubmissionRequest::from_scenario(s);
        let sub = match client.create_submission(pid, &req).await { ... };
        let final_resp = match poll_until_terminal(client, sub.id, timeout).await { ... };
        let ok = final_resp.status == s.expected_status
            && final_resp.result.as_ref().and_then(|r| r.verdict.as_ref()) == s.expected_verdict.as_ref()
            && final_resp.status != SubmissionStatus::SystemError;
        tx.send(Event::ScenarioFinished { id: s.id.into(), ok,
            status: final_resp.status, verdict: ..., duration_ms: started.elapsed().as_millis() as u64 }).ok();
        if !ok { all_ok = false; break; }  // fail-fast
    }
    tx.send(Event::PhaseFinished { phase: Phase::Correctness, ok: all_ok }).ok();
    all_ok
}
```

`poll_until_terminal` polls `client.get_submission` every 200 ms until
`status.is_terminal()` or timeout.

**Step 2: Tests with wiremock**

```rust
#[tokio::test]
async fn passes_when_verdict_matches() { ... }
#[tokio::test]
async fn fails_fast_on_first_mismatch() { ... }
#[tokio::test]
async fn fails_on_timeout() { ... }
#[tokio::test]
async fn fails_on_system_error_even_if_verdict_matches() { ... }
```

**Step 3: Commit**

```
feat(stress-test): correctness phase runner
```

---

### Task 12: Load phase runner

**Files:**

- Create: `packages/stress-test/src/load.rs`
- Modify: `src/lib.rs`

**Step 1: Define inputs**

```rust
pub struct LoadConfig {
    pub total: u64,
    pub rate: u32,        // submissions per second
    pub concurrency: u32, // max in-flight
    pub per_job_timeout: Duration,
    pub p95_budget_ms: u64,
    pub seed: u64,
}

pub struct LoadOutcome {
    pub completed: u64,
    pub passed: u64,
    pub histogram: hdrhistogram::Histogram<u64>,
    pub drain_time: Duration,
    pub errors: Vec<(u64, String)>, // (sequence, message)
    pub passed_budget: bool,
}
```

**Step 2: Implement scheduling**

- Token bucket: `Interval::new(Duration::from_micros(1_000_000 / rate))`.
- Concurrency gate: `Arc<Semaphore::new(concurrency)>`.
- Mix: deterministic with `StdRng::seed_from_u64(seed)`.
- For each tick: `acquire_owned()` semaphore permit, then `tokio::spawn`.
  Inside: post submission, poll for terminal, emit `LoadCompleted`.
- Histogram is wrapped in `Mutex<Histogram>`; record only on the spawned task's
  terminal-success path.
- After last submission posted, await all in-flight via a `JoinSet`.

**Step 3: Tests**

```rust
#[tokio::test]
async fn fires_total_submissions_at_target_rate() { ... }
#[tokio::test]
async fn enforces_concurrency_cap() { ... }
#[tokio::test]
async fn records_latencies_in_histogram() { ... }
#[tokio::test]
async fn fails_budget_when_p95_exceeds_threshold() { ... }
#[tokio::test]
async fn deterministic_with_same_seed() { ... }
```

**Step 4: Commit**

```
feat(stress-test): load phase runner with rate limiter + hdrhistogram
```

---

### Task 13: Final summary report

**Files:**

- Create: `packages/stress-test/src/report.rs`
- Modify: `src/lib.rs`

**Step 1: Define `RunSummary`**

Aggregate of bootstrap/correctness/load/passthrough/cleanup outcomes. The runner
builds it as phases complete.

**Step 2: Implement
`format_summary(summary: &RunSummary, w: &mut dyn Write, colour: bool)`**

Match the design's "Final summary" text exactly. Two modes: with ANSI colour
(only if `colour` is true and stdout is a TTY), and plain.

**Step 3: Snapshot tests for pass and fail variants**

**Step 4: Commit**

```
feat(stress-test): final summary report formatter
```

---

### Task 14: Wire MVP — main.rs orchestration

**Files:**

- Modify: `packages/stress-test/src/main.rs`
- Modify: `src/lib.rs` — `pub async fn run(cli: Cli) -> ExitCode`

**Step 1: Implement `run()`**

```rust
pub async fn run(cli: Cli) -> u8 {
    // 1. Build Client
    // 2. (mpsc) spawn plain renderer (or, in Phase B, TUI)
    // 3. bootstrap.bootstrap() — exit 4 on failure
    // 4. correctness.run() (unless --skip-correctness) — exit 1 on failure
    // 5. load.run() (unless --skip-load) — exit 2 on failure
    // 6. passthrough.run() (if --contest-id) — exit 3 on failure (Phase C will fill this in)
    // 7. cleanup (Phase C)
    // 8. report.format_summary
    // 9. return exit code
}
```

For Phase A: stub passthrough as "always skipped" and stub cleanup as a no-op
with TODO. Phase C completes both.

**Step 2: End-to-end test against wiremock**

```rust
// tests/e2e_wiremock.rs
#[tokio::test]
async fn full_run_passes_against_canned_server() { ... }
#[tokio::test]
async fn full_run_fails_on_correctness_mismatch() { ... }
```

**Step 3: Commit**

```
feat(stress-test): wire MVP — orchestrate phases through main.rs
```

---

### Task 15: Phase A polish + manual smoke

**Files:**

- Modify: `packages/stress-test/README.md` (minimal usage section only)

**Step 1: Manual smoke test against a real local server**

Start a real Broccoli server with `docker compose up -d` and
`cargo run -p server`. Pre-create an admin user (document the SQL or steps in
the README). Run:

```bash
cargo run -p stress-test -- \
    --url http://localhost:3000 \
    --admin-username admin --admin-password ... \
    --total 10 --rate 5 --concurrency 5 \
    --skip-load   # first
# then with load:
cargo run -p stress-test -- ... --total 50 --rate 10
```

Expected: PASS reports for both runs. Fix anything broken.

**Step 2: Commit only if changes were needed**

```
fix(stress-test): <whatever surfaced during smoke>
```

---

## Phase B — TUI

### Task 16: TUI theme + capability detection

**Files:**

- Create: `packages/stress-test/src/ui/theme.rs`

Detect:

- `NO_COLOR` env var → no colour at all.
- `COLORTERM=truecolor|24bit` → truecolor palette.
- Otherwise `TERM=*-256color` or `screen-256color` → 256-colour palette.
- Otherwise → 16-colour ANSI.
- `LANG`/`LC_CTYPE` containing `UTF-8` (case-insensitive) → Unicode glyphs;
  otherwise ASCII fallback table from the design doc.

Provide a `Theme` struct exposing `border_top_left()`, `phase_glyph(state)`,
`sparkline_chars()`, etc., plus `colour(token: ColourToken) -> ratatui::Color`.

Tests: per-environment-variable matrix.

Commit:

```
feat(stress-test): TUI theme with truecolor/256/16 + ASCII fallback
```

---

### Task 17: TUI widgets

**Files:**

- Create: `packages/stress-test/src/ui/widgets.rs`

Implement five widgets, each as a free function
`fn render_<name>(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme)`:

- `render_phase_ladder` — three rows with status glyph + label + counter.
- `render_throughput_sparkline` — uses ratatui's `Sparkline` widget, driven by a
  60-sample ring buffer in `AppState`.
- `render_latency_bars` — four bars (p50, p95, p99, max) clipped to a display
  max derived from p95 budget.
- `render_verdict_chart` — ratatui `BarChart`, sorted by count descending.
- `render_event_log` — `Table` over a ring buffer, last N rows fitting the area,
  scroll offset state.
- `render_in_flight` — gauge filled to `current/concurrency`.

Each widget gets a snapshot test using `ratatui::backend::TestBackend`,
asserting the buffer's character grid matches a recorded snapshot.

Commit:

```
feat(stress-test): TUI widgets with snapshot tests
```

---

### Task 18: TUI app state + render loop

**Files:**

- Create: `packages/stress-test/src/ui/app.rs`
- Modify: `packages/stress-test/src/ui/mod.rs` — add `pub async fn run_tui`

`AppState`: phase status, latency histogram, throughput ring buffer (60
seconds), verdict counts, event log ring (256 rows), in-flight count, total
elapsed.

`run_tui` runs in `tokio::select!`:

- Crossterm key events (Quit, Pause, ScrollUp, ScrollDown).
- Event channel — update `AppState`.
- 4 Hz tick — call `terminal.draw(|f| layout(f, &state, &theme))`.

On exit, restore terminal and return so the caller can print the final summary
block to plain stdout.

Integration test using `TestBackend`: feed canned events at deterministic times,
render at known ticks, assert the buffer matches a snapshot.

Commit:

```
feat(stress-test): TUI app state + render loop
```

---

### Task 19: Wire TUI into main.rs

**Files:**

- Modify: `packages/stress-test/src/main.rs` (or `src/lib.rs::run`)

If `cli.json` is unset and `std::io::stdout().is_terminal()` and terminal size ≥
80×24: use `ui::run_tui`. Otherwise: use `ui::plain::run`.

When `run_tui` exits (Quit pressed or phases finished), print the plain summary
block to stdout.

End-to-end test: spawn an in-process server (`wiremock`), set
`std::io::stdout()` to a piped child for the test (so it's non-TTY), assert PASS
exit and plain summary. The TUI path is exercised only via `TestBackend` in unit
tests since real TTY testing is fragile.

Commit:

```
feat(stress-test): switch between TUI and plain renderer based on tty
```

---

## Phase C — Polish

### Task 20: JSON output mode

`--json` forces non-TTY (already handled in Task 19). Add `src/ui/json.rs` that
buffers events and emits a single JSON object on phase completion:

```json
{
  "schema_version": 1,
  "result": "pass",
  "duration_seconds": 21.7,
  "phases": {
    "correctness": { "passed": 9, "total": 9, "scenarios": [ ... ] },
    "load": { "passed": 200, "total": 200, "p50_ms": 820, "p95_ms": 2104, ... },
    "passthrough": { "result": "skipped", "reason": "no --contest-id" }
  },
  "errors": []
}
```

Tests: deserialise emitted JSON, validate schema, golden-file comparison for a
deterministic run.

Commit:

```
feat(stress-test): --json output mode
```

---

### Task 21: Pass-through phase

Implement `src/passthrough.rs` per the design's Section "Phase 3". Wire into
`run()` after load phase, before cleanup.

Tests: wiremock with the contest API surface; assert skip behaviour for
custom-checker problems and consistency-check behaviour for normal problems.

Commit:

```
feat(stress-test): pass-through phase against admin's real contest
```

---

### Task 22: Cleanup module

Implement `src/cleanup.rs`:

- Iterate `BootstrapState.problem_ids_by_scenario` and call
  `client.delete_problem(pid)`. Swallow per-problem errors but record them.
- If `state.plugin_id` set, call `client.disable_plugin(&id)`. Swallow.
- Return a `CleanupOutcome { deleted: usize, errors: Vec<String> }`.

Wire after all phases. On a passing run with cleanup errors, exit code is `5`
(degraded pass). On `--keep-fixtures`, skip and print resource IDs.

Tests: idempotent, never panics, doesn't change exit code on a failing run.

Commit:

```
feat(stress-test): cleanup with degraded-pass exit code
```

---

### Task 23: Portability harness

**Files:**

- Create: `packages/stress-test/tests/portability.rs`
- Modify: root `justfile`

`tests/portability.rs` (Linux-only, env-gated):

```rust
#![cfg(target_os = "linux")]
#[test]
fn musl_x86_64_binary_is_static() {
    if std::env::var("STRESS_TEST_PORTABILITY").is_err() { return; }
    // Exec `cross build --target x86_64-unknown-linux-musl --release -p stress-test`
    // Run `file` and `ldd`, assert "statically linked" / "not a dynamic executable".
    // Spin up an Alpine 3.18 container, run the binary with --help, assert exit 0.
}
```

`justfile` recipes:

```make
stress-test-linux-x86_64:
    cross build --target x86_64-unknown-linux-musl --release -p stress-test
    cp target/x86_64-unknown-linux-musl/release/broccoli-stress-test \
       dist/broccoli-stress-test-linux-x86_64

stress-test-linux-aarch64:
    cross build --target aarch64-unknown-linux-musl --release -p stress-test
    cp target/aarch64-unknown-linux-musl/release/broccoli-stress-test \
       dist/broccoli-stress-test-linux-aarch64

stress-test-all: stress-test-linux-x86_64 stress-test-linux-aarch64
    cd dist && sha256sum broccoli-stress-test-linux-* > SHA256SUMS
```

Commit:

```
feat(stress-test): portability test harness + cross-build recipes
```

---

### Task 24: Real-server integration test

**Files:**

- Create: `packages/stress-test/tests/e2e_real.rs`

`#[ignore]`-by-default test that:

1. Brings up a real server. Either reuse
   `packages/server/tests/integration/ common::TestApp` (preferred — already
   battle-tested per CLAUDE.md), or spawn `docker compose up` for a fresh stack.
   The `TestApp` route requires extracting the helper into a publishable form;
   if that's too invasive, fall back to docker-compose.
2. Seeds an admin user.
3. Calls `stress_test::run(cli_for_test_app)`.
4. Asserts exit code `0`.

Run via `cargo test -p stress-test --test e2e_real -- --ignored`.

Commit:

```
test(stress-test): e2e against a real server (ignored by default)
```

---

### Task 25: README + docs

**Files:**

- Modify: `packages/stress-test/README.md` (full version)
- Modify: root `justfile` (already done in Task 23, just confirm)
- Modify: `CLAUDE.md` — add a brief subsection under "Testing" pointing to the
  stress test

`README.md` sections per the design's Section 7 docs subsection:

- **For admins** — five-line download/run instructions.
- **For developers** — how to add a scenario, rebuild the bundled plugin,
  understand the TUI architecture.
- **For releases** — `just stress-test-all`, attach to GitHub release.

Call out explicitly:

- Pre-existing admin requirement.
- macOS dev-server limitations (MLE may flake).
- JWT lifetime caveat for marathon `--total` runs.

Commit:

```
docs(stress-test): comprehensive README + CLAUDE.md mention
```

---

## Sub-skills

Subagents executing this plan use:

- **superpowers:test-driven-development** — for every task with code changes;
  failing-test-first is non-negotiable.
- **superpowers:verification-before-completion** — before marking a task
  complete; never claim a task is done without running and reading the output of
  `cargo test -p stress-test`, `cargo build -p stress-test`, and the explicit
  verification steps in the task.

Reviewers use the prompts in
`.claude/plugins/cache/superpowers-marketplace/superpowers/4.0.3/skills/subagent-driven-development/`.
