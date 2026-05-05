# Broccoli Stress Test — Design

**Status:** design, not yet implemented.

**Goal:** Give contest/IT admins a single portable binary they can run on a
freshly provisioned contest box to verify, before the contest starts, that the
Broccoli platform is wired up correctly _and_ can sustain expected contest load.
The tool's job is to answer one question with high confidence: **is this
hardware safe to run a contest on right now?**

**Audience:** contest/IT operators. They are not expected to know Rust, the
codebase, or the plugin model. They run one command, watch a TUI, and read a
PASS/FAIL line at the end.

---

## Scope

**In scope.** A new workspace member `packages/stress-test/` that builds into a
single static binary. The binary:

- Drives the running server over HTTP (no DB or Redis access).
- Bootstraps its own minimal contest plugin and synthetic problems.
- Runs a fast correctness phase, a longer load phase, and an optional
  pass-through against the admin's real contest.
- Renders a real-time TUI inspired by htop, with a non-TTY fallback and a
  machine-readable JSON mode.
- Cleans up everything it created (best-effort) before exiting.

**Out of scope.** The stress test does not benchmark individual subsystems
(sandbox in isolation, MQ in isolation, etc.), does not exercise the frontend,
and does not generate sustained synthetic load over hours. It is a pre-contest
gate, not a load-testing platform.

## Success criteria

A passing run gives the admin justified confidence that:

1. Every hop of the judging pipeline (server → MQ → worker → sandbox → checker →
   DB) is functional end-to-end.
2. The sandbox enforces time and memory limits correctly on this hardware.
3. The platform handles the configured concurrency without crashing, stalling,
   or producing incorrect verdicts.
4. The admin's specific contest plugin (if provided) loads and routes
   submissions deterministically under load.

A failing run produces a clear, actionable list of what broke. The tool refuses
to soft-pass; either every check holds or the result is FAIL.

---

## High-level architecture

```
┌──────────────────────┐         HTTP            ┌─────────────────────┐
│ broccoli-stress-test │ ───────────────────────>│  Broccoli server    │
│  (single binary)     │ <────────────────────── │  (already running)  │
└──────────┬───────────┘                         └─────────┬───────────┘
           │                                               │
           │ structured Events (mpsc)                      │ existing pipeline
           v                                               v
       ┌────────┐                                  ┌──────────────┐
       │  TUI   │                                  │    Worker    │
       │ thread │                                  └──────────────┘
       └────────┘
```

The binary is a pure HTTP client. It needs no privileged access to the contest
box beyond the ability to reach the server's HTTP port. All state mutation
happens through documented API endpoints — the same surface a real admin would
touch.

A background tokio runtime drives the test phases and emits typed `Event`s over
an `mpsc` channel. The TUI consumes events and a 4 Hz tick to repaint. This
decoupling keeps the TUI snappy under high event rates and lets the non-TTY mode
reuse the exact same event stream.

### Crate layout

```
packages/stress-test/
├── Cargo.toml
├── README.md                # admin-facing usage
├── justfile-additions       # documented in root justfile
├── fixtures/
│   ├── plugin/
│   │   └── stress_test_plugin.wasm     # committed, like echo-plugin
│   ├── plugin-src/                     # source for the bundled plugin (rebuildable)
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── solutions/
│   │   ├── ab_cpp_ac.cpp
│   │   ├── ab_cpp_wa.cpp
│   │   ├── ab_cpp_tle.cpp
│   │   ├── ab_cpp_mle.cpp
│   │   ├── ab_cpp_re.cpp
│   │   ├── ab_cpp_ce.cpp
│   │   ├── ab_cpp_testlib.cpp
│   │   └── ab_py_ac.py
│   └── multi-file/
│       ├── solution.cpp
│       └── helper.hpp
├── src/
│   ├── main.rs              # CLI parsing, top-level run, exit code mapping
│   ├── cli.rs               # clap definitions
│   ├── client.rs            # thin HTTP client over reqwest, typed DTOs
│   ├── dto.rs               # request/response shapes (mirrored from server)
│   ├── fixtures.rs          # include_bytes!/include_str! of all fixtures
│   ├── scenarios.rs         # the correctness scenarios as data
│   ├── bootstrap.rs         # admin login, plugin load, problem creation
│   ├── correctness.rs       # phase 1 runner
│   ├── load.rs              # phase 2 runner
│   ├── passthrough.rs       # phase 3 runner
│   ├── cleanup.rs           # delete created resources
│   ├── events.rs            # Event enum sent runners → ui
│   ├── report.rs            # final summary block (TTY + plain text)
│   ├── ui/
│   │   ├── mod.rs           # entry: run_tui / run_plain
│   │   ├── app.rs           # AppState (phases, latency histogram, log ring)
│   │   ├── widgets.rs       # phase ladder, sparkline, verdict bar, log table
│   │   └── theme.rs         # color tokens, glyphs, border styles
│   └── lib.rs               # re-exports for the integration tests
└── tests/
    └── portability.rs       # cross-build + ldd/file assertions (Linux + opt-in)
```

The crate is added to `[workspace.members]` but **excluded from
`[workspace.default-members]`** so plain `cargo build` from the root doesn't
pull it in. Day-to-day development builds stay fast; CI and release builds opt
in with `-p stress-test`.

### Dependencies

Pinned at design time; final versions selected at implementation:

```toml
[dependencies]
tokio = { workspace = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
serde = { workspace = true }
serde_json = { workspace = true }
clap = { version = "4", features = ["derive"] }
ratatui = "0.28"
crossterm = "0.28"
hdrhistogram = "7"          # latency percentiles
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
chrono = { workspace = true }
```

`reqwest` with `rustls-tls` and **no** `default-features` keeps OpenSSL out of
the dep tree entirely. `cargo tree -p stress-test | grep -i openssl` must return
nothing — verified in CI.

---

## Phase 1: Correctness

**When:** always runs first unless `--skip-correctness` is set. Bails out the
whole run on any failure — the load phase is meaningless if basic correctness is
broken.

### Scenarios

Nine fixed scenarios, defined as data in `scenarios.rs`. Each scenario asserts
on a `(SubmissionStatus, Option<Verdict>)` pair. Most scenarios reach status
`Judged` with a specific verdict; the compilation-error scenario reaches status
`CompilationError` with no verdict (verdict is `None` when the submission never
makes it past compile). This split matches the actual server data model —
`SubmissionStatus` and `Verdict` are different types.

| ID               | Source                                 | Language | Problem shape                | Expected status    | Expected verdict      | What it proves                        |
| ---------------- | -------------------------------------- | -------- | ---------------------------- | ------------------ | --------------------- | ------------------------------------- |
| `ab-cpp-ac`      | `solutions/ab_cpp_ac.cpp`              | C++      | a+b, `checker_format=exact`  | `Judged`           | `Accepted`            | Compile + sandbox + exact checker     |
| `ab-py-ac`       | `solutions/ab_py_ac.py`                | Python   | a+b, exact match             | `Judged`           | `Accepted`            | Interpreter path, no compile cache    |
| `ab-cpp-wa`      | `solutions/ab_cpp_wa.cpp`              | C++      | a+b, prints `42`             | `Judged`           | `WrongAnswer`         | Exact-checker rejection path          |
| `ab-cpp-tle`     | `solutions/ab_cpp_tle.cpp`             | C++      | a+b with 1s limit, busy loop | `Judged`           | `TimeLimitExceeded`   | Sandbox time limit enforcement        |
| `ab-cpp-mle`     | `solutions/ab_cpp_mle.cpp`             | C++      | a+b with 64MB limit          | `Judged`           | `MemoryLimitExceeded` | Sandbox memory limit enforcement      |
| `ab-cpp-re`      | `solutions/ab_cpp_re.cpp`              | C++      | a+b, null-pointer deref      | `Judged`           | `RuntimeError`        | Sandbox exit-code handling            |
| `ab-cpp-ce`      | `solutions/ab_cpp_ce.cpp`              | C++      | a+b, syntax error            | `CompilationError` | `None`                | Compile-pipeline error path           |
| `ab-cpp-igncase` | `solutions/ab_cpp_igncase.cpp`         | C++      | `checker_format=ignore_case` | `Judged`           | `Accepted`            | Non-default checker format wired up   |
| `ab-cpp-multi`   | `multi-file/{solution.cpp,helper.hpp}` | C++      | a+b across two files         | `Judged`           | `Accepted`            | Multi-file compile + sorted-name hash |

All sources are ≤ 20 lines and target the exact failure mode. The MLE source
allocates `1024 * 1024 * 80` ints in a vector to trip a 64 MB cap; the TLE
source busy-loops with a `volatile` counter; the RE source dereferences a null
pointer (`*((int*)0) = 1`) for a cross-platform-stable SIGSEGV. The
`ignore_case` scenario prints `"yes"` while the expected output is `"YES"` —
verifies a non-default `checker_format` actually changes behaviour.

**Testlib scenario was dropped.** The original design assumed a public
`CheckerSpec`/`cache_key` API surface, but the real server represents the
checker as a single `checker_format` string with built-in modes plus a separate
"checker source" upload endpoint. Testlib correctness is already covered by the
existing integration tests; the stress test exercises the checker pipeline
through the four built-in formats instead.

### Execution

Sequential, fail-fast. For each scenario:

1. `POST /api/v1/problems/{id}/submissions/` with the scenario's source files
   and language. The server returns the new submission's id.
2. Poll `GET /api/v1/submissions/{id}` at 200 ms intervals until
   `SubmissionStatus::is_terminal()` (i.e. `Judged`, `CompilationError`, or
   `SystemError`) or `--per-job-timeout` (default 60 s) elapses.
3. Compare the resulting `(status, result.verdict)` pair against
   `Scenario::expected`. A `SystemError` is **always** a failure regardless of
   the scenario — it indicates the worker itself crashed. Any other mismatch →
   record failure, abort phase, fail the run.

The phase emits `ScenarioStarted` and
`ScenarioFinished { status, verdict, duration }` events. The TUI updates the
phase ladder and event log live.

### Bootstrap (run once before the phase)

The stress test **requires a pre-existing admin user**. The server's
registration endpoint always assigns the `contestant` role; admin must be seeded
out-of-band (via DB or a separate admin tool). The README flags this
prominently. The admin supplies credentials via `--admin-token <jwt>` or
`--admin-username <u> --admin-password <p>`.

1. **Authenticate.** `POST /api/v1/auth/login` if username/password were given;
   stash the JWT plus the original credentials. If `--admin-token` was given,
   use it directly and skip refresh logic. JWT lifetime is short on this server,
   so the HTTP client treats any 401 on a previously-working request as
   "re-login needed" and retries once. Without credentials available (token-only
   mode) a 401 mid-run is fatal — surface it clearly.
2. **Upload the bundled fixture plugin.** `POST /api/v1/admin/plugins/upload`
   accepts a multipart `plugin` field containing a `.tar.gz` archive whose
   single top-level directory is `{plugin_id}/` and contains `plugin.toml` plus
   the compiled `.wasm`. The server extracts to disk and immediately activates
   the plugin. The fixture archive is embedded in the binary via
   `include_bytes!`; the stress test posts it as multipart bytes. Plugin id:
   `broccoli-stress-test`. Permission required: `plugin:manage`.
3. **Create one problem per scenario.** `POST /api/v1/problems` with
   `time_limit`, `memory_limit`, `checker_format` (`exact` or `ignore_case`),
   `default_contest_type` (the contest type registered by the fixture plugin),
   and `submission_format` (a `{language: [filenames]}` map listing the file
   names the scenario will submit). For each problem,
   `POST /api/v1/problems/{id}/test-cases/` with one test case
   (`input = "1 2\n"`, `expected_output = "3\n"` or `"YES\n"` per scenario,
   `score = 100`).
4. Track every created resource ID in `BootstrapState` for cleanup.

### Pass criteria

All nine `(status, verdict)` pairs match expectation, no submission times out,
no HTTP 5xx, no `SystemError` status.

---

## Phase 2: Load

**When:** after correctness passes, unless `--skip-load`.

### Workload shape

- `--total <N>` total submissions (default `200`).
- `--rate <R>` target submission rate per second (default `20`).
- `--concurrency <C>` max in-flight cap (default `50`).
- Submissions drawn from the correctness-phase problems with a fixed weighted
  distribution: `Accepted` 70%, `WrongAnswer` 10%, `TimeLimitExceeded` 10%,
  `RuntimeError` 5%, `MemoryLimitExceeded` 5%. Compilation-error submissions are
  deliberately _excluded_ from the load mix because they short-circuit the
  pipeline and aren't representative of contest workload.
- The mix is seeded so two runs with the same flags produce identical submission
  sequences. Helps debugging "it failed once but passed on retry."

### Scheduling

A token-bucket rate limiter at `R/s` paces submission posts. A semaphore caps
in-flight at `C`. Each submission spawns a tokio task that posts, polls for
verdict, and emits `LoadCompleted { id, expected, actual, latency_ms }`.

### Metrics

- **End-to-end latency**: submit → terminal verdict, in `hdrhistogram` for
  accurate p50/p95/p99/max.
- **Drain time**: wall clock from "last submission posted" to "last verdict
  received."
- **Throughput**: completed submissions per second, sampled at 1 Hz for the
  sparkline; sustained throughput = `total / (post_start → drain_end)`.
- **Verdict accuracy**: count of `actual == expected`.
- **Error counts**: HTTP 5xx, network errors, judge errors, timeouts.

### Pass criteria

All must hold:

| Metric                                | Threshold                             |
| ------------------------------------- | ------------------------------------- |
| Submissions reaching terminal verdict | 100% within `--per-job-timeout`       |
| Verdict accuracy                      | 100%                                  |
| HTTP 5xx                              | 0                                     |
| `JudgeError` verdicts                 | 0                                     |
| p95 latency                           | ≤ `--p95-budget-ms` (default `15000`) |

The latency budget is intentionally generous and configurable. Hardware varies
wildly, and the goal isn't to enforce a performance SLA — it's to flag a clear
regression like "the worker is wedged" or "the box is so slow this contest will
be unusable."

---

## Phase 3: Optional contest pass-through

**When:** after load passes, if `--contest-id <id>` was provided.

### What it does

1. `GET /api/v1/contests/{id}` to confirm access and read the contest's type.
2. `GET /api/v1/contests/{id}/problems` to list problems.
3. Pick a problem (first by position, or `--problem-id <id>` if given).
4. Fetch its first sample test case. Construct a Python solution that simply
   prints the sample's expected output literally. This works for problems with
   deterministic, exact-match output and is the smallest possible "real"
   submission against the admin's actual plugin.
5. If the problem has a custom checker we can't match against trivially, or has
   no samples, **skip with a clear reason logged**. Skipping is not a failure.
6. Otherwise, fire `--contest-concurrency` (default `20`) submissions of that
   solution in parallel.
7. Assert: every submission terminates within `--per-job-timeout`, and the
   verdict is _consistent across submissions_. We do not assert AC because
   sample-echo may not satisfy a custom checker — we assert determinism and
   liveness, which is what we actually care about for "the plugin works under
   load."

The phase emits `PassthroughSkipped { reason }` or `PassthroughCompleted` and
contributes its own block to the final summary.

---

## TUI design

### Aesthetic direction

**Mission Control.** Dense, dark-respecting, monospaced ops console. Steals
three concrete patterns from htop / `bottom`:

1. **Fixed-position phase ladder** (top-left). The eye learns where to look.
2. **Live throughput sparkline**. Anomalies (stalls, spikes) become visible at a
   glance.
3. **Proportional verdict bar chart**. A wall of green = healthy. Red bars =
   investigate. Glanceable across a room.

### Layout (80 cols × 24 rows minimum)

```
╔═ BROCCOLI STRESS TEST ════════════════ http://localhost:3000 ═══ 00:21 ═╗
║                                                                          ║
║  ┌─ PHASES ────────────────┐  ┌─ THROUGHPUT (subs/sec, last 60s) ─────┐ ║
║  │ ✓ Correctness  9/9  8s  │  │     ▂▄▆█▇▆▅▄▃▂▂▃▄▅▆█▇▆▅▄▃▄▅▆▇█▇▆▅▄▃▂ │ ║
║  │ ▶ Load      142/200     │  │ peak 21.4 / sustained 16.1            │ ║
║  │ ○ Pass-through  ─       │  └───────────────────────────────────────┘ ║
║  └─────────────────────────┘                                             ║
║                                                                          ║
║  ┌─ LATENCY (ms) ───────────┐  ┌─ VERDICTS ──────────────────────────┐  ║
║  │  p50  ████░░░░░░░    820 │  │ Accepted           ███████████  98  │  ║
║  │  p95  ████████░░░   2104 │  │ WrongAnswer        ██            12 │  ║
║  │  p99  ██████████░   3401 │  │ TimeLimitExceeded  █              8 │  ║
║  │  max  ███████████   4012 │  │ MemoryLimitExceed  ▌              4 │  ║
║  │  budget p95 < 15000      │  │ RuntimeError       ▌              5 │  ║
║  └──────────────────────────┘  │ CompilationError                  0 │  ║
║                                │ JudgeError                        0 │  ║
║  ┌─ IN-FLIGHT 45/50 ────────┐  └──────────────────────────────────────┘ ║
║  │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░ │                                           ║
║  └──────────────────────────┘                                            ║
║                                                                          ║
║  ┌─ EVENT LOG ────────────────────────────────────────────────────────┐ ║
║  │ 14:32:18  OK   correctness  ab-cpp-ac      Accepted        412ms   │ ║
║  │ 14:32:19  OK   correctness  ab-py-ac       Accepted        611ms   │ ║
║  │ 14:32:21  ERR  load    #142  expected AC, got WrongAnswer          │ ║
║  │ 14:32:22  OK   load    #143  Accepted                      820ms   │ ║
║  └────────────────────────────────────────────────────────────────────┘ ║
║                                                                          ║
║  [q] quit    [p] pause stream    [↑↓] scroll log                        ║
╚══════════════════════════════════════════════════════════════════════════╝
```

### Visual system

- **Borders:** double-line `═ ║ ╔ ╗ ╚ ╝` for the outer frame, single-line
  `─ │ ┌ ┐ └ ┘` for inner panels. Hierarchy through line weight, not color.
- **Color tokens** (truecolor, with 16-color and ASCII fallbacks; see
  Portability):

  | Token    | Truecolor         | Used for                                       |
  | -------- | ----------------- | ---------------------------------------------- |
  | `accent` | `#5fd7ff` (cyan)  | Title bar, active phase pulse, current log row |
  | `ok`     | `#5faf5f` (green) | AC verdicts, completed phases, OK log lines    |
  | `warn`   | `#d7af5f` (amber) | In-flight bar, throughput peak markers         |
  | `err`    | `#d75f5f` (red)   | Failed verdicts, FAIL banners, ERR log lines   |
  | `dim`    | `#6c6c6c` (gray)  | Borders, axis labels, finished/skipped phases  |

- **Glyph language:** `○` pending · `▶` running · `✓` passed · `✗` failed · `─`
  skipped. The 3-char `OK ` / `ERR` / `WRN` log column never uses glyphs, so
  misaligned terminals don't ruin column alignment.

- **Animation:** the active phase's left border alternates `▏ ▎` at 2 Hz.
  Sparkline shifts left every second. Log auto-scrolls unless the user presses
  `↑` to detach.

### Refresh model

- Background runtime drives phases; emits `Event` over an
  `mpsc::UnboundedSender<Event>`.
- TUI thread `tokio::select!`s on (a) crossterm key events, (b) `Event` channel,
  (c) a 4 Hz tick. The tick redraws; events update state. Decoupling redraw rate
  from event rate keeps the screen smooth at 200+ events/sec.

### Final summary

When phases finish (or fail), ratatui restores the terminal and prints a
plain-text summary block to stdout so it survives in scrollback:

```
─────────────────────────────────────────────────────────
 BROCCOLI STRESS TEST — RESULT: PASS
─────────────────────────────────────────────────────────
 Target           http://localhost:3000
 Duration         21.7s
 Correctness      9/9   passed
 Load             200/200 passed   p95 2104ms / budget 15000ms
 Pass-through     skipped (no --contest-id)
 Verdict accuracy 100.0%
 Errors           0

 System is ready for contest.
─────────────────────────────────────────────────────────
```

On failure: red `RESULT: FAIL`, plus an `Issues:` list, plus
`DO NOT RUN CONTEST until these are resolved.`

### Non-TTY fallback

Triggered by any of: `!stdout.is_terminal()`, `--json`, terminal smaller than
80×24. Same event stream, rendered as one structured line per event:

```
[14:32:18Z] OK   correctness  ab-cpp-ac      Accepted        412ms
```

Final summary block is identical (without colors).

`--json` emits a single JSON object on stdout at the end with all phase results,
the latency histogram, error list, and resource IDs created/cleaned. Schema
versioned (`"schema_version": 1`) so CI consumers can pin.

---

## CLI

```
broccoli-stress-test
  --url <URL>                       # required
  (--admin-token <JWT> | --admin-username <U> --admin-password <P>)

  [--total 200]                     # phase 2 total submissions
  [--rate 20]                       # phase 2 target submissions/sec
  [--concurrency 50]                # phase 2 max in-flight
  [--per-job-timeout 60]            # seconds before a submission is a fail
  [--p95-budget-ms 15000]           # phase 2 latency ceiling

  [--contest-id <ID>]               # enables phase 3
  [--problem-id <ID>]               # phase 3 specific problem
  [--contest-concurrency 20]        # phase 3 fan-out

  [--skip-correctness]              # iterate on load only
  [--skip-load]                     # quick post-deploy sanity (~30s)

  [--keep-fixtures]                 # disable cleanup; print resource IDs
  [--seed <U64>]                    # deterministic load mix
  [--json]                          # forces non-TTY, emits final JSON
```

Exit codes:

| Code | Meaning                                                                       |
| ---- | ----------------------------------------------------------------------------- |
| 0    | PASS                                                                          |
| 1    | Correctness phase failed                                                      |
| 2    | Load phase failed                                                             |
| 3    | Pass-through phase failed                                                     |
| 4    | Bootstrap/setup error (couldn't even start)                                   |
| 5    | Cleanup error after a passing run (degraded but not a real fail; prints WARN) |

Distinct codes so a CI script can dispatch on what kind of failure it was.

---

## Portability

### Build targets

- `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. Fully static, no
  glibc dependency, runs on every Linux distro from RHEL 7 to current Alpine.
- macOS dev builds use the host's default target and are not shipped.
- Cross-compilation via `cross` (the `cross-rs/cross` Docker tool) so devs on
  any host can produce shippable binaries.

### TLS

`reqwest` configured with `default-features = false` and the `rustls-tls`
feature. Pure Rust TLS, no OpenSSL anywhere in the dependency tree. CI step:

```sh
cargo tree -p stress-test 2>/dev/null | grep -i openssl && exit 1 || true
```

### What the host needs

- TCP access to the server URL.
- Stdin/stdout/stderr.

That is the complete list. No Postgres client, no Redis client, no compilers, no
Rust toolchain, no environment variables (everything via flags).

### Embedded fixtures

All fixtures embedded into the binary at compile time via `include_bytes!` /
`include_str!`. No `--fixtures-dir`, no risk of "I copied the binary but not the
WASM." The committed `.wasm` follows the same pattern as
`packages/server/tests/fixtures/echo-plugin/` — built and committed by a
developer, not built on the admin's box. CI rebuilds the plugin and verifies the
committed binary matches.

### Terminal compatibility

- **Color:** truecolor by default; detect `COLORTERM=truecolor|24bit` and
  `TERM=*-256color`. If unsupported, downgrade to standard 16-color ANSI. Tested
  matrix: `dumb`, `xterm`, `xterm-256color`, `screen`, `tmux`.
- **Unicode:** detect `LANG`/`LC_CTYPE` containing `UTF-8`. If absent, swap
  glyph table to ASCII:

  | UTF-8                  | ASCII fallback       |
  | ---------------------- | -------------------- |
  | `═ ║ ╔ ╗ ╚ ╝`          | `= \| + + + +`       |
  | `─ │ ┌ ┐ └ ┘`          | `- \| + + + +`       |
  | `○ ▶ ✓ ✗`             | `[ ] [*] [x] [!]`    |
  | `▁▂▃▄▅▆▇█` (sparkline) | `_.-=^"` (six-level) |

- **Width:** minimum 80×24. Below that, the TUI prints a friendly stderr message
  and falls back to the line-based mode rather than rendering garbage. Above 80,
  panels expand proportionally and the sparkline grows to fill width.
- **Non-TTY:** `--json`, piped stdout, or sub-minimum terminal triggers
  fallback. Same event stream, no escape codes.

### Verification

`packages/stress-test/tests/portability.rs` (gated by
`STRESS_TEST_PORTABILITY=1` and `cfg(target_os = "linux")`):

1. `cross build --target x86_64-unknown-linux-musl --release -p stress-test`
2. `file target/.../broccoli-stress-test` contains `statically linked`.
3. `ldd target/.../broccoli-stress-test` reports `not a dynamic executable`.
4. Run `--help` inside an Alpine 3.18 container; assert exit 0.

Skipped on macOS dev. Runs in CI on Linux.

### Distribution

`just stress-test-all` produces:

```
dist/broccoli-stress-test-linux-x86_64
dist/broccoli-stress-test-linux-aarch64
dist/SHA256SUMS
```

Attached to GitHub releases. Admins download the artifact for their arch and run
it. No installer, no package manager, no dependencies.

---

## Cleanup contract

Cleanup is best-effort and runs in a `Drop`-style finalizer regardless of phase
outcome (unless `--keep-fixtures`):

- `DELETE /api/v1/problems/{id}` for every problem created during bootstrap.
- Disable the stress-test plugin via `POST /api/v1/admin/plugins/{id}/disable`
  (the inverse of the `enable`/`upload` activation). The on-disk extracted files
  are left in place — the server's plugin discovery handles restoration if the
  admin re-enables. Full uninstall is out of scope for cleanup.
- Best-effort: if the server is unreachable during cleanup, log resource IDs to
  stderr so the admin can clean up manually.
- Never block the final result on cleanup. Cleanup failures produce exit code
  `5` (degraded pass) on an otherwise-passing run, but never turn a PASS into a
  FAIL.

`--keep-fixtures` skips deletion entirely and prints a list of resource IDs for
the admin to inspect or clean up manually.

---

## Open questions

### Resolved during pre-implementation Explore (2026-05-01)

1. **Submission status endpoint.** ✅ Resolved. `GET /api/v1/submissions/{id}`
   exists (`packages/server/src/handlers/submission.rs:888`). Response is
   `SubmissionResponse` with a `status: SubmissionStatus` field plus an optional
   `result: JudgeResultResponse` containing `verdict: Option<Verdict>` when
   terminal. `SubmissionStatus::is_terminal()` returns true for `Judged`,
   `CompilationError`, and `SystemError`
   (`packages/common/src/submission_status.rs:34-39`).

2. **Testlib provisioning.** ✅ Resolved by dropping the scenario. The public
   API has no `CheckerSpec`/`cache_key` surface — checkers are represented as a
   single `checker_format` string with built-in modes (`exact`, `ignore_case`,
   `ignore_whitespace`, `floating_point`) plus a separate "checker source"
   upload endpoint. Testlib correctness is already covered by integration tests;
   the stress test exercises the standard checker pipeline through `exact` and
   `ignore_case` instead.

3. **Plugin loading mechanism.** ✅ Resolved.
   `POST /api/v1/admin/plugins/upload` accepts a multipart `plugin` field
   carrying a tar.gz archive; the server extracts and activates atomically
   (`packages/server/src/handlers/admin.rs:369`). No separate "load" call
   needed. The endpoint imagined in the original design did not exist.

4. **Admin bootstrap.** ✅ Resolved by requiring pre-existing admin.
   Registration always assigns the `contestant` role only. Creating an admin
   requires either DB seeding or an out-of-band admin tool. The stress test
   accepts admin credentials via flags and the README documents this as a
   prerequisite.

### Still open (resolve during implementation)

5. **JWT lifetime.** CLAUDE.md says 7-day expiry; the Explore agent reported a
   5-minute literal in the auth handler. The implementer must read the actual
   auth code at the start of Task 4 (HTTP client) and confirm. Regardless of the
   answer, the client implements automatic re-login on 401 when
   username/password are available, so the run survives even short tokens. If
   the answer is "5 minutes", the load phase still completes in a single token
   lifetime under default flags but would risk expiry on marathon
   `--total 100000` runs — flag in README.

6. **Default contest type and evaluator.** The fixture plugin must register a
   contest type and an evaluator for problems to be created with sensible
   defaults. The implementer will read the existing plugins
   (`packages/plugins/`) to understand the registration API, then either (a)
   build a minimal fixture plugin, or (b) if an existing plugin is suitable and
   always-loaded, skip the fixture upload and use what's there. Decision
   captured in Task 6 of the implementation plan.

7. **MLE on macOS dev.** Sandbox memory enforcement on macOS via the mock
   sandbox may not produce `MemoryLimitExceeded` reliably. The stress test
   should be runnable against a Linux server only; running against a macOS dev
   server is best-effort and may legitimately fail the MLE scenario.
   `--skip-correctness` exists partly for this reason; the README calls it out
   so devs don't think the tool is broken.

8. **Workspace member or stand-alone crate.** Default decision: workspace member
   with `default-members` exclusion. Sharing types and patterns is worth the
   dep-bump churn.

---

## Implementation plan

A separate plan document (`docs/plans/YYYY-MM-DD-stress-test-impl.md`) will
break this design into ordered tasks, beginning with: (1) confirm the
submission-status API surface, (2) scaffold the workspace member, (3) build the
bundled plugin, (4) implement bootstrap + correctness, (5) load, (6) TUI, (7)
pass-through, (8) cleanup, (9) portability harness, (10) docs.
