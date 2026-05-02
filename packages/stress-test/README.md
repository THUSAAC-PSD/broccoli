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
concurrent burst against your actual contest. Asserts liveness and determinism
rather than verdict correctness (we don't know your problem's right answer).
Skipped automatically for problems with custom checkers.

> **Note:** the pass-through phase is currently stubbed and surfaces as
> `Pass-through skipped (pass-through phase not yet implemented (Phase C))`. The
> flag is accepted but doesn't do anything yet.

### Exit codes

| Code | Meaning                                                    |
| ---- | ---------------------------------------------------------- |
| `0`  | PASS — system is ready for contest.                        |
| `1`  | Correctness phase failed.                                  |
| `2`  | Load phase failed.                                         |
| `3`  | Pass-through phase failed (reserved; not yet implemented). |
| `4`  | Bootstrap or setup error (couldn't even start).            |
| `5`  | Otherwise-passing run had cleanup warnings.                |
| `64` | Bad CLI arguments.                                         |

### What the test creates and tears down

The stress test creates 9 problems titled `stress-test:<scenario-id>` plus one
test case per problem. After every run it deletes them unconditionally
(best-effort) — failures here surface as warnings and exit code `5` rather than
`0`, but never flip a passing run to FAIL.

To inspect what's left after a run, pass `--keep-fixtures`. The summary will
then list the resource ids.

### CI integration

`--json` forces non-TTY output and emits a single JSON object on stdout once the
run finishes. (Currently the JSON output mode is a stub — emitted but minimal.
Phase C delivers the full schema.) Errors from the event stream are written
line-by-line to stdout as they happen, even in JSON mode, prefixed with
timestamps.

The tool always uses stderr for `tracing` log lines (filter via `RUST_LOG`);
structured progress goes to stdout. Pipe-safe.

### Caveats

- **Pre-existing admin required.** Registration cannot create one.
- **macOS dev servers may flake the MLE scenario.** The mock sandbox doesn't
  enforce memory limits as reliably as Linux's isolate. If you're running
  against `cargo run -p server` on a Mac, expect the `ab-cpp-mle` scenario to
  occasionally fail. Run against a Linux server for trustworthy results.
- **Marathon `--total` runs.** The server's bearer JWT is short-lived (5
  minutes; refresh tokens are 7 days). The HTTP client transparently re-logs in
  on 401 when given username/password, so multi-minute load phases still work —
  but this only kicks in for `--admin-username` / `--admin-password`. If you
  supplied `--admin-token`, a 401 mid-run is fatal. Don't use `--admin-token`
  for anything longer than a couple of minutes.

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
├── cleanup.rs        # best-effort delete of created resources
├── report.rs         # final summary block
└── ui/
    ├── mod.rs        # current consumer: plain-text renderer
    └── plain.rs      # one structured line per event, no escape codes
```

### Tests

```sh
cargo test -p stress-test
```

83 tests (Phase A complete) cover the DTO contract, HTTP client behaviour (auth,
401 retry, multipart, error decoding), bootstrap sequencing, scenario validity,
plain-text rendering, every phase runner's pass/fail/timeout/error paths,
cleanup outcomes, and summary formatting. Wiremock drives most tests;
`tokio::time::pause` keeps the timeout/load tests deterministic.

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

## For releases

The plan calls for static musl builds via `cross` for both x86_64 and aarch64
Linux, distributed as single binaries. **This is currently deferred** — see Task
23 of the implementation plan (`docs/plans/2026-05-01-stress-test-impl.md`).
Until then, admins build locally:

```sh
cargo build -p stress-test --release
# Resulting binary: target/release/broccoli-stress-test
```

## Status

**Phase A (MVP):** complete. Bootstrap → correctness → load → cleanup work
end-to-end with a plain-text event stream and PASS/FAIL summary.

**Phase B (TUI):** not started. The htop-style UI from the design doc will live
under `src/ui/` alongside the existing plain renderer.

**Phase C (polish):** partially complete. Cleanup is done. Pass-through, full
JSON schema, portability harness (cross-builds), real-server e2e test, and full
release tooling are pending.
