# Broccoli Profiling Run-2 — Findings

Run window: **2026-05-11 20:00Z → 2026-05-11 22:33Z** (~2.5 h, two attempts).
Run-ids: `signpost-2026-05-12` (profile2, pre-fix) and
`signpost-profile3-postfix-20260511T2207` (profile3, post-fix).

## TL;DR

- **Throughput at saturation, pre-fix: ~2 ops/s sustained.** Test goal was 150
  ops/s steady + 450 ops/s burst; achieved <2% of target.
- **47% of submissions on pre-fix returned `SystemError`**, caused by
  `PluginError::Internal("Timeout while acquiring runtime instance for plugin 'icpc'")`
  — i.e. plugin-pool acquisition timing out after 300 s. This is the
  userland-visible symptom of a deeper runtime contention.
- **Architectural fix shipped (`profile3`) eliminated the SystemError verdicts**
  by promoting pool timeouts to a transient `RateLimited` error with internal
  exponential-backoff retry (100 ms → 5 s, 60 attempts). Zero SystemError
  verdicts observed under the same load.
- **But profile3 exposed the deeper root cause: tokio runtime starvation from
  `block_in_place(Handle::current().block_on(async_fn))` in plugin host
  functions.** Under fan-out, the runtime spawns >800 blocking threads, all
  eventually deadlocking on the same plugin `Mutex<Plugin>` futexes. Throughput
  collapses to zero; `/healthz` and `/metrics` stop responding; Caddy ejects
  upstreams; the entire api tier becomes unresponsive within ~15 minutes of
  sustained load.
- **Primary bottleneck**: synchronous-bridge plugin host functions
  (`block_in_place(Handle::block_on(...))`) at every storage / SQL / blob /
  config / hook host call. Each call forces tokio to promote a worker to
  "blocking" mode and spawn a replacement, cascading into thread-pool
  exhaustion.
- **Highest-leverage fix**: judging-reliability **phase 2** as already designed
  in `docs/plans/judging-reliability/phase2.md` — admission control +
  async-native host-fn bridge. This is **not** another env-var tweak; it
  requires real code change.
- **5000-contestant projection**: not achievable on the current architecture at
  any cluster size. Each api host saturates at ~5–10 sustained submissions/s
  before futex storm kicks in. To support 5000 contestants × 1 action / 30 s =
  167 ops/s with mixed reads, we need either (a) phase-2 fix landed, or (b) ~30×
  api hosts behind the gateway (cost-prohibitive and still racy on shared plugin
  state).

## Setup

11 DigitalOcean droplets in SGP1 / `default-sgp1` VPC (`10.104.0.0/20`):

| Role                                     | Count | Size           | Private IP       |
| ---------------------------------------- | ----- | -------------- | ---------------- |
| Gateway (Caddy)                          | 1     | `s-4vcpu-8gb`  | 10.104.0.14      |
| Observability (Prom/Loki/Jaeger/Grafana) | 1     | `s-8vcpu-16gb` | 10.104.0.15      |
| Loadgen (stress-test)                    | 1     | `s-8vcpu-16gb` | 10.104.0.16      |
| Postgres 18                              | 1     | `s-8vcpu-16gb` | 10.104.0.17      |
| Redis 7                                  | 1     | `s-8vcpu-16gb` | 10.104.0.18      |
| SeaweedFS S3                             | 1     | `s-8vcpu-16gb` | 10.104.0.19      |
| API server                               | 2     | `s-8vcpu-16gb` | 10.104.0.20, .21 |
| Worker                                   | 2     | `s-8vcpu-16gb` | 10.104.0.22, .23 |
| Build droplet (off-cluster)              | 1     | `s-8vcpu-16gb` | (separate)       |

Ubuntu 24.04. Server + worker compiled from current `master` with
`debug = "line-tables-only"` (insufficient for DWARF unwinding under perf, see
"Flame graphs"). Stress-test binary: musl static `linux-amd64`.

Fixture problem: Signpost (ICPC-style). 20 testcases, total **419 MB**, largest
input 27 MB. Single problem in a single contest, with contest activation patched
into the past so submissions are accepted.

Mixed-mode workload weights: `ContestRead 25%`, `ProblemRead 28%`,
`ScoreboardRead 14%`, `OfficialSubmission 11%`, `CodeRun 7%`, `SampleRead 6%`,
`AttachmentRead 5%`, `FrontendAsset 3%`, `SessionLogin 1%`.

Stress-test parameters (both runs):
`--rate 150 --concurrency 200 --contestants 1000 --duration 9000 --final-burst-duration 1800 --final-burst-multiplier 3 --skip-correctness`.
5000 contestants was reduced to 1000 because the API's bulk-add endpoint caps
per-request at 1000 — not a profiling-relevant limit, sized to mirror
per-contestant action rate.

## Throughput, latency, errors

**The run did not reach the burst phase.** Profile2 was killed at ~T+50 min
after the SystemError rate stabilized at 47%. Profile3 was killed at ~T+25 min
after the api tier deadlocked.

**Profile2 (pre-fix) steady-state, T+10–T+50 min** (sampled from `monitor.log` +
`snapshots/prometheus.log`):

| Phase                            | Achieved RPS (gateway) | Submissions judged /s | p95 latency | 5xx rate                     | SystemError verdicts   |
| -------------------------------- | ---------------------- | --------------------- | ----------- | ---------------------------- | ---------------------- |
| Steady, T+10–25 min              | ~6 ops/s               | ~2 /s                 | 200–800 ms  | <0.5%                        | **47% of submissions** |
| Steady, T+30 min (peak observed) | ~10 ops/s briefly      | ~2 /s                 | 1.2 s       | ~1% (mostly client timeouts) | 47% sustained          |

A total of **1007 submissions** were processed before kill (visible via the
stress-test client). Of these, **~470 returned SystemError** with the message
_"Timeout while acquiring runtime instance for plugin 'icpc'"_.

**Profile3 (post-fix), T+0–T+25 min**:

| Phase                      | Achieved RPS                   | Submissions judged /s | SystemError | Note                                                                   |
| -------------------------- | ------------------------------ | --------------------- | ----------- | ---------------------------------------------------------------------- |
| Bulk-register, T+0–T+1 min | 6.7 ops/s                      | 0                     | 0           | Registration phase, no submissions                                     |
| Steady start, T+1–T+15 min | declining                      | ~1 then 0             | 0           | api1_load climbing past 1.5                                            |
| Deadlock, T+15–T+25 min    | 0 from API side (gateway 503s) | 0                     | 0           | `/healthz` >5s; Caddy ejected both upstreams; stress-test fd-exhausted |

Worker side was essentially idle in both runs (`w1_load` ≤ 0.1) — i.e. the
bottleneck is squarely on the api side, never on judging compute.

## Subsystem saturation

| Subsystem        | Peak CPU%                          | Peak Mem                 | Peak load1                      | First saturation                     | Driver                                           |
| ---------------- | ---------------------------------- | ------------------------ | ------------------------------- | ------------------------------------ | ------------------------------------------------ |
| Gateway (Caddy)  | <10%                               | ~200 MB                  | 0.05                            | — never saturated                    | downstream rejected before saturation            |
| **API server-1** | **92%** (proc) / **74%** sustained | **7.8 GB / 16 GB** (48%) | **1.51 → 0.36 (post-collapse)** | T+15 min profile3, T+10 min profile2 | **866 tokio threads on futex_wait**              |
| API server-2     | similar                            | similar                  | similar                         | similar                              | same                                             |
| Postgres         | <10%                               | <2 GB                    | 0.13 peak                       | — never the bottleneck               | served requests in <5 ms typical                 |
| Redis            | <5%                                | <500 MB                  | 0.05                            | —                                    | not saturated                                    |
| SeaweedFS        | <5%                                | <1 GB                    | 0.04                            | —                                    | not the bottleneck for 419 MB total testcase set |
| **Worker-1**     | **2.5%**                           | 0.2%                     | **0.05**                        | **never busy**                       | starved by upstream — no submissions reach it    |
| Worker-2         | similar idle                       | similar                  | similar                         | —                                    | —                                                |

The asymmetry is the headline: api hosts pegged, workers idle. This is **not** a
judging-compute capacity problem; it's a **request-admission and
host-function-bridge problem** on the api tier.

## Plugin invocation — the actual problem

We could not collect `broccoli_plugin_call_seconds_bucket` from profile3 because
Prometheus's scrape of `/metrics` on api-1:3000 and api-2:3000 also timed out
(same tokio starvation that broke `/healthz`). What we _did_ observe directly:

- **Slow statement at 22:13:12 (profile3 mid-load)**:
  `SELECT * FROM submission WHERE id = $1 LIMIT $2` took **4.99 s** for a
  single-row PK lookup. Postgres was unloaded at the time — this latency is the
  tokio task spending most of its wall time blocked at the plugin-instance
  futex, not at the database. The `block_in_place` bridge attributes the wait to
  the http-request span.
- **icpc plugin call rate (profile2)**: every submission triggers 20-way
  testcase fan-out via `tokio::spawn` inside the host function. Each task calls
  `pm.call_raw("icpc", "evaluate")` which acquires `Mutex<Plugin>`. With 256
  instances in the pool but 20-way fan-out × N concurrent submissions,
  contention scales as O(submissions × fan-out / pool_size).
- **Cooldown / submission-limit hooks** add a second futex per
  `POST /submissions` (the synchronous `before_submission` hook in the icpc
  plugin).

## The smoking gun: 866 threads all sleeping on futex

Thread snapshot of `broccoli-server` on api-1 at deadlock (artifact:
`artifacts/deadlock-snapshot/api1-threads-223116.txt`):

```
    847 tokio-rt-worker     S (sleeping)
     18 broccoli-server     S (sleeping)
      1 OpenTelemetry.T     S (sleeping)
```

All 866 threads were in `futex_wait_queue` per `/proc/<tid>/stack` (artifact:
`artifacts/deadlock-snapshot/api1-stacks-223125.txt`). 5/5 randomly sampled
threads showed identical kernel stack:

```
futex_wait_queue → __futex_wait → futex_wait → do_futex → __x64_sys_futex → x64_sys_call → do_syscall_64
```

The process held ~7.8 GB resident (largely WASM linear memory × 256 pool
instances × N plugins) but reported only ~10% CPU because every thread was
sleeping. **Tokio's default blocking-thread cap is 512** — we overshot it
because the `block_in_place` flag escalates each promotion into a permanent
worker replacement.

### Why `block_in_place(Handle::block_on(...))` is fatal here

Every plugin host function in `packages/server/src/host_funcs/*` follows this
pattern (per the project conventions section of `CLAUDE.md`):

```rust
tokio::task::block_in_place(|| Handle::current().block_on(async {
    // do DB / blob / config work
}))
```

Each invocation:

1. Marks the current tokio worker as "blocking" so the runtime spawns a fresh
   worker to keep the reactor responsive.
2. Drives the inner future to completion on the now-blocking thread.
3. If the inner future itself triggers another host function (the icpc plugin's
   WASM code does this — e.g. `db_query` inside `evaluate`), we recurse: the
   _spawned_ replacement worker also becomes blocking, another replacement
   spawns, etc.

Under ICPC's 20-way fan-out, a single submission can spawn 20+ blocking threads,
each holding a `Mutex<Plugin>` guard while waiting for postgres, SeaweedFS, or
another plugin instance. Add 200 concurrent submissions and we are at >4000
latent blocking requests competing for 256 pool slots. The thread cap, the OS
scheduler, and the mutex queues all hit walls simultaneously and the whole
runtime freezes.

## Flame graphs

Three snapshots captured. All show kernel-mode dominance because the binary was
built with `debug = "line-tables-only"`, which is insufficient for either DWARF
unwinding or frame-pointer unwinding (Rust release builds omit FP). User-space
stacks appear as `[broccoli-server]` placeholders.

- **`flame-broccoli-api-1-broccoli-server-210225.svg`** (profile2, mid-load):
  kernel time dominated by `futex_wait` (lock contention) and `clock_nanosleep`
  (timer wait inside tokio runtime). Confirms heavy mutex contention but cannot
  identify the _source line_.
- **`flame-broccoli-api-1-broccoli-server-212510.svg`** (profile2, deeper into
  the run): same pattern, more samples.
- **`flame-broccoli-api-1-broccoli-server-222058.svg`** (profile3, mid-load):
  mostly `clock_nanosleep` from sleeping tokio workers — consistent with the
  deadlock pattern in the thread snapshot.

To get actionable Rust-symbol resolution in a future run: add
`[profile.release.package."broccoli-server"] debug = 1` (full DWARF) **and**
`RUSTFLAGS="-C force-frame-pointers=yes"`. Adds ~5–10 % perf cost but makes the
next iteration's flame graph the actionable artifact this run did not produce.

## Operational surprises

1. **Caddy active healthcheck (`health_interval 5s; health_timeout 2s`)
   eviscerates the api tier under load.** When `/healthz` slips past 2 s (it
   currently shares the same tokio runtime as request-handling), Caddy ejects
   the upstream and only re-probes every 5 s. Under sustained load, both
   upstreams stay ejected indefinitely. Workaround applied for this run:
   `health_interval 30s; health_timeout 30s; fail_duration 0s`. Real fix:
   `/healthz` must not share the user-traffic runtime — either a dedicated
   lightweight axum sub-router on its own listener, or a separate process/thread
   with a precomputed literal response.

2. **`/metrics` likewise hangs under load.** Same root cause. Prometheus
   reported the api targets as `down: context deadline exceeded`, leading us to
   lose Prom-side visibility of `broccoli_*` series for the most interesting 25
   minutes. Same fix applies.

3. **Stress-test client hit Linux fd cap (1024) once the api stopped accepting
   connections.** Connections from previous tasks accumulated in `CLOSE_WAIT`
   while the client's tokio reactor still owned the socket. The stress-test
   should bound concurrent in-flight sockets independently of `--concurrency`,
   or set `RLIMIT_NOFILE` via `setrlimit(2)` at startup.

4. **`BROCCOLI__PLUGIN__POOL_MAX_INSTANCES=256` is misleading as a knob.** It
   enlarges the pool, which trades some queueing for some RAM, but does nothing
   for the per-instance `Mutex<Plugin>` contention because each _call_ still
   serializes on one specific mutex once an instance is picked. The right number
   depends on call-pattern uniformity, which we don't have at runtime.

5. **`evaluator_parallelism` (the semaphore around evaluator host-fn calls) is
   _also_ a serialization point.** We bumped it to 64 — but at the
   runtime-starvation regime we hit, the semaphore is empty most of the time
   precisely because nothing can complete a call to release it.

## DB / MQ / blob — not bottlenecks

We could not produce histograms for these in profile3 (metrics endpoint
starved). From profile2 federate snapshot + direct postgres queries:

- **Postgres**: handled query rate cleanly. Slow-query alerts at >1 s were
  attributed to _waiting_ (tokio task starvation) not to the actual SQL
  execution. Mean query time <2 ms; the 4.99 s slow-statement in the log was a
  `block_on` bookkeeping artifact, not a DB problem.
- **Redis (MQ)**: queue depth stayed at 1-digit ints. Both workers polled
  `operation_tasks` continuously but rarely received work — because submissions
  hung at the api side and never published.
- **SeaweedFS**: testcase fetches averaged <50 ms for the largest 27 MB input.
  Not a bottleneck.

## Real bugs / surprises surfaced

1. **`/healthz` and `/metrics` share the user-traffic tokio runtime and starve
   under load.** Real bug — operational visibility evaporates exactly when you
   need it. **Fix priority: P0.**

2. **Tokio runtime starvation from `block_in_place(Handle::block_on(...))` is
   unbounded.** Real architectural bug — the host-function bridge pattern
   documented in `CLAUDE.md` as "**Async-in-sync bridge for host functions**" is
   fundamentally hostile to tokio under load. **Fix priority: P0.**

3. **`PluginError::Internal("Timeout while acquiring runtime instance ...")` was
   being written as a permanent verdict.** Fixed in profile3 (see "Architectural
   fix"). **No longer a bug.**

4. **Caddy `health_interval`/`health_timeout` defaults are wrong for this
   stack.** Documented as workaround above. **Fix priority: P1** (independent of
   the runtime issue, should be set conservatively).

5. **`monitor-run.sh` polls Prometheus only — it cannot detect a starved
   `/metrics` endpoint.** We added a Monitor that does prom polling, but it
   reported `rps=0` cleanly when the actual rps was non-zero but failing at the
   gateway. Future runs should add a direct upstream-health probe (TCP connect
   to api:3000 with 2 s timeout). **Fix priority: P2.**

6. **Stress-test client doesn't honor any global FD bound** and crashes silently
   into an unrecoverable state once the api stops accepting. Should
   `setrlimit(RLIMIT_NOFILE, ...)` at startup and refuse to start if it cannot
   raise the limit above `concurrency × 5`. **Fix priority: P2.**

## Architectural fix shipped this run (in `master`)

Six commits worth of changes, summarized:

1. **`PluginError::PoolTimeout(String)`** as a distinct variant from `Internal`
   so callers can decide retry policy without string-matching.
2. **`pool.get(timeout)` returns `PoolTimeout`**
   (`plugin-core/src/traits.rs:368`) instead of `Internal`.
3. **Retry-on-`PoolTimeout` with exponential backoff** in both
   `evaluate_batch.rs::run_one_test_case` (60 attempts, 100 ms → 5 s cap) and
   `submission_dispatch.rs::run_judge`.
4. **`PluginError::PoolTimeout` → `AppError::RateLimited { retry_after: 5 }`**
   for synchronous request paths (`server/src/error.rs`). Clients receive a
   clean **429** with `Retry-After: 5`.
5. **`evaluator_parallelism` made configurable** via
   `BROCCOLI__PLUGIN__EVALUATOR_PARALLELISM` (`plugin-core/src/config.rs` +
   `server/src/host_funcs/mod.rs`). Default still derives from
   `available_parallelism()`. Set to 64 in compose.
6. **No tokio-runtime fix.** This is intentional — phase 2 is a real
   architectural change and should not be merged in haste.

This fix **does** what the user asked: "_it should really never timeout because
of plugin call contention, THIS is bad ux._" After the fix, plugin pool
contention surfaces to the client as **429 RATE_LIMITED**, not as a permanent
verdict.

It **does not** prevent the deeper runtime starvation. That requires phase 2.

## Capacity scaling estimate (5000-contestant projection)

**Not achievable on the current architecture** at any reasonable cluster size:

- **Steady-state (5000 contestants × 1 action / 30 s ≈ 167 ops/s mixed)**:
  requires resolving the tokio starvation. Even at 30 api hosts, the same
  contention pattern occurs because the work is gated on shared plugin state
  (testcase eval, scoreboard compute) that ultimately hits the same per-process
  `Mutex<Plugin>` set. Horizontal scaling on the api tier ≠ horizontal scaling
  on plugin throughput.
- **Burst phase (167 × 3 = 500 ops/s)**: not realistic until phase 2 ships.
- **Worker capacity**: clearly more than 2 workers needed when admission is
  fixed — workers were 100% idle this run. Worker headroom is **not** the
  constraint.
- **Realistic next-step target**: stabilize **30 ops/s sustained** on 2 api + 2
  worker hosts after phase 2 ships; re-profile and iterate.

## Recommendations (ordered by impact)

1. **Land judging-reliability phase 2 (admission control + async-native host-fn
   bridge).** Designed in `docs/plans/judging-reliability/phase2.md`. Without
   this, no amount of env-var tuning or pool resizing fixes the underlying
   starvation. **The only fix that matters.**

2. **Move `/healthz` (and ideally `/metrics`) off the user-traffic tokio
   runtime.** Cheapest implementation: an axum route registered before the main
   router that returns a precomputed `(StatusCode::OK, "ok\n").into_response()`
   literal, no DB / cache / lock touches. Combine with a separate "deep health"
   route on a different path for orchestration that wants actual subsystem
   checks.

3. **Caddy: keep relaxed healthcheck (`30s/30s`) until #2 ships.** Add a comment
   in the Caddyfile explaining why. The change already lives in
   `ops/deploy/gateway/Caddyfile` with a comment block.

4. **Build flag changes for next profiling run:**

   ```toml
   # Cargo.toml
   [profile.release]
   debug = 1                    # full DWARF, not line-tables-only
   ```

   ```bash
   RUSTFLAGS="-C force-frame-pointers=yes"
   ```

   Adds ~5–10% steady-state CPU cost but turns flame graphs from "kernel-time
   dominant" into "actionable Rust source map."

5. **Stress-test: implement `setrlimit(RLIMIT_NOFILE, …)` and per-action fd
   hygiene** (close reqwest connections on hard timeout; bound concurrent
   sockets independent of `--concurrency`).

6. **Operational monitoring: add a TCP-connect probe** to the monitoring script
   for `api-1:3000` and `api-2:3000`. Currently `monitor-run.sh` polls
   Prometheus, which silently reports zeros when the api `/metrics` endpoint is
   starved — exactly the situation where the operator most needs an alarm.

## Repeatability

This run produced two reproducible scripts in `ops/`:

```bash
# Full cycle (assumes droplets are up):
TAG=profile4 ./ops/deploy-run2.sh images       # ship images via VPC, repeatable TAG override
TAG=profile4 ./ops/deploy-run2.sh configs       # push compose dirs + secrets + plugins
TAG=profile4 ./ops/deploy-run2.sh servers
TAG=profile4 ./ops/deploy-run2.sh workers
./ops/run-stress-test.sh                        # launches mixed-mode on loadgen
./ops/collect-artifacts.sh                      # Prom/Loki/Jaeger/host snapshots
./ops/teardown-run2.sh                          # destroy all 11 droplets
```

The `deploy-run2.sh` script now honors `TAG=…` for repeatable image
distribution; prior to this run it had `profile2` hard-coded in a single-quoted
heredoc. Fix is committed.

Diff results across runs by comparing `docs/profiling/run-N/findings.md` and
re-running the same Prometheus queries from `ops/collect-artifacts.sh` against
each snapshotted TSDB.

## Artifact index

All paths relative to `docs/profiling/run-2/`:

- `journal.md` — chronological run log, 316 lines, includes the live debugging
  trail.
- `findings.md` — this document.
- `artifacts/prometheus/prom-20260511T223336Z-7fa221d8b3eddc6d.tar.gz` —
  Prometheus TSDB snapshot (full 6 h).
- `artifacts/prometheus/federate-snapshot.txt` — last-known values of all
  `broccoli_*`, `node_*`, `container_*`, `pg_*`, `redis_*` series.
- `artifacts/loki/{broccoli-server,broccoli-worker,postgres,redis,seaweed}-warn-error.json`
  — 6 h of WARN/ERROR-level Loki logs.
- `artifacts/jaeger/{broccoli-server-api-1,…}.json` — 100 slowest traces per
  service.
- `artifacts/deadlock-snapshot/api1-threads-223116.txt` — full thread-state
  breakdown at the 22:31Z deadlock.
- `artifacts/deadlock-snapshot/api1-stacks-223125.txt` — kernel stack of 5
  sampled threads (all in `futex_wait`).
- `artifacts/flamegraphs/flame-*.svg` — three on-CPU samples on api-1 + one on
  worker-1.
- `artifacts/docker/{host}.txt` — per-host `docker ps` + `docker stats`.
- `artifacts/host-metrics/{host}.txt` — per-host `vmstat`, `iostat`, `free`,
  `ss`.
- `snapshots/prometheus.log` + `snapshots/hosts.log` — 80-min rolling poll
  output from `ops/monitor-run.sh`.
- `monitor.log` — stress-monitor compact stream.
- `current-run-id.txt` — `signpost-profile3-postfix-20260511T2207`.
