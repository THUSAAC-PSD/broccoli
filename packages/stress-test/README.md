# broccoli-stress-test

A single static binary that contest/IT admins run on a freshly provisioned
contest box to verify the Broccoli platform is wired up correctly **and** can
sustain expected contest load. Answers one question with high confidence: **is
this hardware safe to run a contest on right now?**

The tool drives a running Broccoli server purely over HTTP. It needs no DB or
Redis access, no Rust toolchain, no privileged credentials beyond a pre-existing
admin user.

## For admins

### Prerequisites

- A Broccoli server reachable over HTTP and ready to accept submissions.
- An **admin user already provisioned** in that server. The stress test cannot
  create one — registration on Broccoli always assigns the `contestant` role
  only. If you don't have an admin yet, seed one via the DB or whatever
  provisioning tool your deployment uses.
- At least one contest plugin and one evaluator (e.g. `batch-evaluator`) loaded
  on the server. The stress test discovers them via
  `GET /api/v1/plugins/registries`; if either list is empty, the tool fails fast
  with a message naming `batch-evaluator` as the canonical fix.

### Running

```sh
broccoli-stress-test \
    --url https://judge.example.com \
    --admin-username admin \
    --admin-password '<password>'
```

Read the final summary block. If it says
`RESULT: PASS — System is ready for contest.`, you're good. If it says
`RESULT: FAIL — DO NOT RUN CONTEST until these are resolved.`, the issues block
names what broke.

### Output modes

Three renderers, picked automatically:

| Condition                                          | Renderer                                                                                                   |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `--json`                                           | suppresses human output; emits a single JSON object at the end                                             |
| stdout is a TTY ≥ 80×24                            | live TUI dashboard (phases, throughput sparkline, latency bars, verdict chart, in-flight gauge, event log) |
| stdout is piped, redirected, or smaller than 80×24 | plain-text event stream, one structured line per event                                                     |

The TUI auto-detects color (`COLORTERM=truecolor` → 24-bit, `TERM=*-256color` →
256, `NO_COLOR` → none) and Unicode (`LANG`/`LC_CTYPE` containing `UTF-8` →
box-drawing glyphs, otherwise an ASCII fallback table).

TUI key bindings: `q` / `Esc` / `Ctrl-C` quit, `p` toggle event log pause, `↑` /
`↓` scroll the event log. The final summary block always prints to plain stdout
after the TUI exits, so it survives in scrollback.

### Quick post-deploy sanity (~30s)

```sh
broccoli-stress-test --url ... --admin-username ... --admin-password ... \
    --skip-load
```

Just runs the 9-scenario correctness phase. Cheap; useful immediately after a
deploy to confirm nothing's smoke-broken.

### Tuning the load phase

```sh
broccoli-stress-test --url ... --admin-username ... --admin-password ... \
    --total 500 --rate 30 --concurrency 100 --p95-budget-ms 20000
```

| Flag                | Default | Purpose                                               |
| ------------------- | ------- | ----------------------------------------------------- |
| `--total`           | 200     | How many submissions to fire in the load phase.       |
| `--rate`            | 20      | Target submissions per second (token-bucket).         |
| `--concurrency`     | 50      | Maximum in-flight submissions.                        |
| `--per-job-timeout` | 60      | Per-submission timeout in seconds.                    |
| `--p95-budget-ms`   | 15000   | p95 latency ceiling. Exceeding this fails the run.    |
| `--seed`            | 0       | RNG seed for the load mix. Same seed → same sequence. |

### Targeting your real contest plugin

```sh
broccoli-stress-test --url ... --admin-username ... --admin-password ... \
    --contest-id 42 --contest-concurrency 20
```

After the self-contained correctness + load phases pass, this fires a small
concurrent burst against your actual contest. Asserts liveness (every submission
reaches a terminal status within `--per-job-timeout`) and determinism (every
submission produces the same final verdict) rather than verdict correctness — we
don't know your problem's right answer.

The phase auto-skips with a clear reason in any of:

- The contest has zero problems.
- The selected problem has no sample test cases.
- The selected problem uses a Testlib checker (sample-echo cannot reliably match
  a custom checker).

Skipping is **not** a failure. Pass `--problem-id <id>` to target a specific
problem within the contest; otherwise the lowest-position problem is chosen.

### Exit codes

| Code | Meaning                                              |
| ---- | ---------------------------------------------------- |
| `0`  | PASS — system is ready for contest.                  |
| `1`  | Correctness phase failed.                            |
| `2`  | Load phase failed.                                   |
| `3`  | Pass-through phase failed (liveness or determinism). |
| `4`  | Bootstrap or setup error (couldn't even start).      |
| `5`  | Otherwise-passing run had cleanup warnings.          |
| `64` | Bad CLI arguments.                                   |

### What the test creates and tears down

The stress test creates 9 problems titled `stress-test:<scenario-id>` plus one
test case per problem. After every run it deletes them unconditionally
(best-effort) — failures here surface as warnings and exit code `5` rather than
`0`, but never flip a passing run to FAIL.

To inspect what's left after a run, pass `--keep-fixtures`. The summary will
then list the resource ids.

### CI integration

`--json` suppresses the human-readable plain-text renderer and emits a single
JSON object on stdout once the run finishes. The schema is versioned via the
top-level `schema_version` field (currently `1`) so consumers can pin against a
known shape.

```json
{
  "schema_version": 1,
  "result": "pass",
  "exit_code": 0,
  "target_url": "http://localhost:3000",
  "duration_seconds": 21.7,
  "bootstrap": { "ok": true, "error": null },
  "correctness": { "total": 9, "passed": 9, "failed_scenarios": [] },
  "load": {
    "total": 200,
    "completed": 200,
    "passed": 200,
    "p50_ms": 820,
    "p95_ms": 2104,
    "p99_ms": 3401,
    "max_ms": 4012,
    "p95_budget_ms": 15000,
    "passed_budget": true,
    "error_count": 0,
    "passed_overall": true
  },
  "passthrough": {
    "state": "not_run",
    "reason": null,
    "ok": null,
    "count": null
  },
  "cleanup": { "warnings": [] }
}
```

`correctness` and `load` are `null` when the corresponding `--skip-*` flag was
set. `passthrough.state` is one of `not_run` (no `--contest-id`), `skipped`
(opted in but auto-skipped, e.g. Testlib checker), or `completed`. The
`exit_code` field always matches the process exit status.

The tool uses stderr for `tracing` log lines (filter via `RUST_LOG`); the JSON
payload goes to stdout. Pipe-safe.

### Caveats

Don't use `--admin-token` for anything longer than a couple of minutes (might
expire before the run completes). Use `--admin-username` and `--admin-password`
for these runs.

## For developers

### Adding a correctness scenario

Scenarios are static data in [`src/scenarios.rs`](src/scenarios.rs). Each entry
gives a scenario id, a language, source files (plain `&str`s embedded via
`include_str!`), problem limits, a checker format, the test input/output, and
the expected `(SubmissionStatus, Option<Verdict>)` pair. To add one:

1. Drop the source file under `fixtures/solutions/` (or `fixtures/multi-file/`).
2. Add the corresponding `pub const X: &str = include_str!(...)` in
   [`src/fixtures.rs`](src/fixtures.rs).
3. Add a `Scenario { ... }` entry in `SCENARIOS`. Update the
   `scenarios_cover_design_doc_table` test if you want the new ID to be
   asserted.
4. Optional: tweak the load-phase mix in `src/load.rs` if the new scenario
   should appear in the workload.

### Module layout

```
src/
├── main.rs           # `tokio::main` shim — parses CLI, calls `runner::run`
├── runner.rs         # phase orchestration + exit-code mapping
├── cli.rs            # clap definitions + Cli::validate
├── client.rs         # reqwest wrapper + 401 auto-relogin + multipart
├── dto.rs            # mirrored server request/response types
├── error.rs          # StressError / StressResult
├── events.rs         # Event enum sent runners → ui
├── fixtures.rs       # include_str! / include_bytes! of every fixture
├── scenarios.rs      # the 9 correctness scenarios as static data
├── bootstrap.rs      # registry resolution + problem creation
├── correctness.rs    # phase 1 runner
├── load.rs           # phase 2 runner (rate limiter + hdrhistogram)
├── passthrough.rs    # phase 3 runner (sample-echo + liveness + determinism)
├── cleanup.rs        # best-effort delete of created resources
├── report.rs         # final summary block
└── ui/
    ├── mod.rs        # module declarations
    ├── plain.rs      # one structured line per event, no escape codes
    ├── theme.rs      # color + glyph capability detection (NO_COLOR, COLORTERM, TERM, LANG)
    ├── app.rs        # AppState + apply_event + tick + latency histogram
    ├── widgets.rs    # phase ladder, sparkline, latency bars, verdict chart, log table, in-flight gauge, full dashboard layout
    └── tui.rs        # alternate-screen lifecycle + tokio::select! loop on events / keys / 4 Hz tick
```

### Local dev workflow

```sh
# Build only stress-test (the workspace excludes it from default-members):
cargo build -p stress-test
cargo run -p stress-test -- --help

# Run against a local server:
docker compose up -d              # Postgres + Redis
cargo run -p server               # in another terminal
# Seed an admin (one-time): see the server crate's docs.
cargo run -p stress-test -- \
    --url http://localhost:3000 \
    --admin-username admin \
    --admin-password ...
```

### Real-server e2e test

`tests/e2e_real.rs` runs the bin against a live stack and asserts a clean PASS.
It's `#[ignore]`-by-default so it never runs under plain `cargo test`. Bring up
the full Docker stack (server + workers + Postgres + Redis) and seed an admin
first:

```sh
just e2e-docker-up
# seed an admin via your usual provisioning path, then:
STRESS_TEST_E2E_URL=http://127.0.0.1:3000 \
STRESS_TEST_E2E_USERNAME=admin \
STRESS_TEST_E2E_PASSWORD='<password>' \
    cargo test -p stress-test --test e2e_real -- --ignored
```

The test passes `--skip-load --json`, so it covers correctness + cleanup but
keeps wall-clock time down. Any non-zero exit or `result != "pass"` in the JSON
payload fails the test with the full stdout/stderr captured.

## For releases

Static musl binaries for x86_64 and aarch64 Linux ship via `cross`. Recipes live
in the workspace `justfile`:

```sh
just stress-test-linux-x86_64    # → dist/broccoli-stress-test-linux-x86_64
just stress-test-linux-aarch64   # → dist/broccoli-stress-test-linux-aarch64
just stress-test-all             # both targets + dist/SHA256SUMS
```

`tests/portability.rs` (Linux-only, gated on `STRESS_TEST_PORTABILITY=1`)
verifies each artifact is statically linked (`file` + `ldd`) and runs the binary
inside `alpine:3.18` to confirm it boots on a barebones libc-less host. Run it
after building:

```sh
just stress-test-portability
```

The harness is a no-op on non-Linux build hosts (cfg-gated to compile away).

## Releases

Pre-built binaries for Linux (x86_64, aarch64), Windows (x86_64), and macOS
(universal) are published with each tagged version:

- **Recommended:** download from your Broccoli server at `<server>/downloads`.
  The binary served there is automatically version-matched to the server.
- **Alternative:** GitHub Releases at
  <https://github.com/THUSAAC-PSD/broccoli/releases>.

For air-gapped lab environments, the bundled-server distribution path means lab
clients only need to reach the Broccoli server, not GitHub.

### Verifying downloads

Each binary ships with a `.sha256` companion file:

```sh
sha256sum -c broccoli-stress-test-linux-x86_64.sha256
```

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
stderr if its own compile-time version doesn't match the server. Pass
`--no-version-check` to skip the handshake entirely.
