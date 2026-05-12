# Profiling Run-2 Journal — Signpost / 5000 contestants / mixed-mode

Started: 2026-05-11 19:30Z. 11-host cluster in DO SGP1 (10 deploy + 1 build).

## Goals vs run-1

Run-1 (last session) found:

- 47/200 RuntimeError verdicts traced to a stress-test fixture bug (64 MB memory
  limit too tight for g++ static-libstdc++ startup) — _not_ a platform bug.
- p95 5535 ms under a 200-submission / 30-concurrent judge-mode load, against a
  15 s budget. Worker-1 hit 95.6% CPU.
- Throughput: ~5.2 submissions/s with 2 workers; ~2.6 sub/s per worker.
- **Biggest miss**: the shipped image bundle predated the observability
  instrumentation — only 2 of ~20 metric families exported.

Run-2 goals:

1. **Rebuild from current source** to expose every instrumented metric.
2. Use the **Signpost** problem (419 MB of testcases) — exercises blob store +
   DB → worker fetch paths the toy A+B fixture missed.
3. **Mixed mode** (`stress-test --profile mixed`) so the test exercises page
   reads, scoreboard polls, code-runs, and submissions in realistic proportion —
   not just judge-mode submission spam.
4. **5000 contestants** target with a 30-min final burst at 3× rate.
5. Multi-hour duration so steady-state queue dynamics emerge.
6. Capture **perf flame graphs** on api & worker hosts at 3 time windows so we
   can root-cause CPU hot paths post-hoc.
7. Snapshot Prometheus TSDB, Loki query exports, and Jaeger trace samples for
   offline analysis.
8. Repeatable: every step scripted in `ops/profile-runbook.sh` so future
   "compare before/after optimization" runs are a single command sequence.

## Cluster topology (run-2)

Same shape as run-1, plus one builder. Private IP slots shifted to .13–.23
(run-1 used .3–.12).

| Role          | Name                   | Public IP       | Private IP  |
| ------------- | ---------------------- | --------------- | ----------- |
| build         | broccoli-build         | 209.97.162.248  | 10.104.0.13 |
| gateway       | broccoli-gateway       | 206.189.84.60   | 10.104.0.14 |
| observability | broccoli-observability | 165.22.102.240  | 10.104.0.15 |
| loadgen       | broccoli-loadgen       | 168.144.101.175 | 10.104.0.16 |
| postgres      | broccoli-postgres      | 159.89.196.18   | 10.104.0.17 |
| redis         | broccoli-redis         | 165.232.174.195 | 10.104.0.18 |
| storage       | broccoli-storage       | 68.183.226.122  | 10.104.0.19 |
| api-1         | broccoli-api-1         | 209.97.170.94   | 10.104.0.20 |
| api-2         | broccoli-api-2         | 139.59.119.194  | 10.104.0.21 |
| worker-1      | broccoli-worker-1      | 168.144.104.193 | 10.104.0.22 |
| worker-2      | broccoli-worker-2      | 159.65.137.233  | 10.104.0.23 |

All 11 droplets: Ubuntu 24.04 LTS, monitoring on, in default-sgp1 VPC
(10.104.0.0/20). All except gateway are s-8vcpu-16gb @ $0.143/hr; gateway is
s-4vcpu-8gb @ $0.071/hr.

## Build configuration

- Tag: `broccoli-server:profile2` / `broccoli-worker:profile2-icpc` (later
  re-tagged to `:v0.0.0-local` on each deploy host so the existing compose
  templates work unmodified).
- **Release profile** patched to keep `debug = "line-tables-only"` — adds
  ~15-20% binary size, zero runtime overhead, lets `perf` resolve frame symbols
  for flame graphs.
- `CARGO_BUILD_JOBS=8` passed as build-arg (without it cargo-chef panics on
  empty env var — first attempt failed; second attempt succeeded after patch).
- Image build performed on the dedicated `broccoli-build` droplet
  (s-8vcpu-16gb), then images pushed to deploy hosts over VPC (10 Gb/s LAN) —
  avoids slow Tsinghua→DO transfer of the ~1.4 GB image bundle.

## Stress-test fixture fix

Bumped `packages/stress-test/src/scenarios.rs::DEFAULT_PROBLEM_MEMORY_LIMIT_KB`
from 64 MB → 256 MB (matches SDK default). The 64 MB ceiling was the root cause
of the spurious RuntimeError storm in run-1.

## Tsinghua → SGP1 network surprises

- `broccoli-postgres` public IP (159.89.196.18) is unreachable on port 22 from
  the Tsinghua egress (Operation timed out). VPC routing fine. Worked around by
  adding `ProxyJump broccoli-build` for postgres in `ops/ssh_config`. Same
  asymmetry phenomenon as run-1 (different IP this time).
- Direct SSH from Tsinghua to public IPs is otherwise flaky for some droplets;
  the bootstrap allows admin SSH from 59.66.0.0/16 (campus block) and
  unrestricted SSH on port 2222 as fallback.
- Build droplet's pubkey was distributed to all 10 deploy hosts so VPC scp can
  fan out images without Mac↔DO round trips.

## Stress test parameters

```
--profile mixed
--duration 9000             # 2h30m steady state
--rate 150                  # ~5000 contestants × 1 action / 33s
--concurrency 200           # max in-flight requests
--contestants 5000          # accounts bulk-registered up front
--final-burst-duration 1800 # 30-min burst
--final-burst-multiplier 3  # 450 actions/s during burst
--per-job-timeout 60
--p95-budget-ms 15000
--contest-id <signpost>
--problem-id <signpost>
--run-id signpost-2026-05-12
--skip-correctness          # correctness was run-1's concern; we want load
--json
```

Mixed-mode action mix (weights from `mixed.rs`):

- ContestRead 25%, ProblemRead 28%, ScoreboardRead 14%, OfficialSubmission 11%,
  CodeRun 7%, login/sample/attachment/static 5% each.
- So at 150 ops/s steady → ~16 sub/s + ~10 coderuns/s. Burst at 450 ops/s → ~50
  sub/s + ~30 coderuns/s.
- Worker capacity (2× s-8vcpu-16gb running ICPC build pipeline) was ~5.2 sub/s
  in run-1 with A+B. Signpost (3 s TL, 256 MB ML, 419 MB testdata) is heavier
  per submission. Expect catastrophic queue growth under steady-state load —
  that's the point: we want to see _what queues, where, and how it cascades_.

## What we will measure (active findings to fill in)

- p50/p95/p99 per MixedAction
- HTTP route latency p95 per handler (broccoli-server)
- DB connection pool utilization + wait time
- MQ consume rate, queue depth, message age
- Worker active tasks gauge, plugin pool waits, blob fetch time
- Postgres: tx/s, lock contention, buffer hit rate, WAL volume
- Redis: ops/s, latency, connection count
- SeaweedFS: GET/PUT rate, p95 latency, disk IO
- Per-host CPU/Mem/Net/Disk saturation curves (Prometheus + cAdvisor +
  node-exporter)
- Per-process flame graphs at T+15min, T+1h45min, T+2h45min (mid-burst)

## Progress log

- 19:30Z provisioned 11 droplets.
- 19:46Z bootstrap completed on 10/11 hosts; postgres needed ProxyJump retry.
- 19:50Z source rsync'd to broccoli-build (110 MB).
- 19:55Z Signpost data rsync'd to broccoli-build (419 MB at ~2.5 MB/s).
- 20:05Z docker build first attempt failed: empty CARGO_BUILD_JOBS → cargo-chef
  parse error. Patched and restarted.
- 20:09Z second build attempt running (cargo chef cook in progress).
- … [continues during run]

## Issues hit at run start (and fixes)

1. **5000 contestants exceeds bulk_add cap.** `validate_bulk_add_participants`
   caps at 1000 entries (server-side, see
   `packages/server/src/models/contest.rs:380`). Dropped `--contestants` to
   **1000**; raised `--rate` so per-second load is preserved. 5000-contestant
   projection done via extrapolation in the report.
2. **Auth rate limit blocking contestant login burst.**
   `BROCCOLI__SERVER__RATE_LIMIT_AUTH=true` in compose throttled the bulk-login
   phase (stress-test logs each of the N contestants serially → ~10 logins/s ×
   axum-governor cap). Set to `false` for the profile run and redeployed
   api-1/api-2 (~2 min).
3. **Stress-test received 404 on every submission for ~10 min.** Caused by
   `activate_time` missing on the contest. `check_contest_access` requires
   `contest.activate_time <= now` (else 404 for non-admins). Patched the contest
   with `PATCH /contests/1 {"activate_time": "..."}` — submissions immediately
   succeeded.
4. **perf-flamegraph script picked tini PID** (the docker container's PID 1)
   instead of the real `broccoli-server` PID (a child of tini). Fixed by
   `pgrep -x broccoli-server | head -1`. Also relaxed
   `kernel.perf_event_paranoid=-1` and `kernel.kptr_restrict=0` on api/worker
   hosts.
5. **`--call-graph=dwarf` produced 0 stack samples** because the binary was
   built with `debug = "line-tables-only"` (DWARF line tables but no DIE entries
   that dwarf unwinding needs). Switched to `--call-graph=fp` — got 5944 useful
   samples in 60s on api-1.
6. **Worker flame graph picked up only 18 samples in 60s.** The broccoli-worker
   process itself spends ~all of its time waiting on child subprocesses (g++,
   the sandboxed binary). `perf -p PID` doesn't follow children. Workaround for
   next sample: `perf record -a` (system-wide) on the worker host during a
   sampling window. The 18-sample SVG is preserved as evidence of this
   observation.
7. **Stress-test compile cache hits inflate worker throughput.** All submissions
   for a given scenario use the same code →
   `Cache hit — skipped execution, step_id=compile`. Real contestants submit
   unique code each time. **Worker throughput numbers below are upper-bound
   estimates** for the cached-compile case.

## Live snapshot at T+18min (21:09Z, into Mixed load)

| Metric                             | Value                                    |
| ---------------------------------- | ---------------------------------------- |
| Total HTTP RPS                     | 541                                      |
| Submission POST 201/s              | 1.6                                      |
| Task completed/s (worker pipeline) | 5.5                                      |
| Task in-flight                     | 2                                        |
| Submission 4xx/s                   | 0.2 (401 from token-expired contestants) |
| broccoli-api-1 load1               | 5.16 (≈64% of 8 cores)                   |
| broccoli-api-2 load1               | 2.46                                     |
| broccoli-worker-1 load1            | 1.24                                     |
| broccoli-worker-2 load1            | 0.93                                     |
| broccoli-postgres load1            | 2.22                                     |

api-1 is hot (load1 5.16 of 8 cores). api-2 less so (load balancer not
round-robin perfectly — Caddy's `lb_policy round_robin` apparently isn't fully
even, or one client sticks).

## Flame graphs captured

- T+09min api-1 broccoli-server (60s, 5944 samples, fp call-graph):
  `flame-broccoli-api-1-broccoli-server-210225.svg`
- T+09min worker-1 broccoli-worker (60s, 18 samples — process mostly off-CPU):
  `flame-broccoli-worker-1-broccoli-worker-210342.svg`

Scheduled next flame captures at T+1h (mid steady) and T+2h45min (mid burst).
Worker capture will switch to `perf -a` system-wide.

## Mid-steady-state findings (T+30min, 21:23Z)

**Submission flow** (cumulative since T0): | Status | Count | % | |---|---|---|
| Judged (verdict assigned) | 180 | 18% | | SystemError | 469 | 47% | | Pending
(queued, not started) | 279 | 28% | | Running (worker in progress) | 44 | 4% | |
CompilationError | 28 | 3% | | **Total submissions** | **1007** | |

**47% of all submissions fail with `SystemError` — primary bottleneck
identified.**

Error message from a SystemError submission:
`Internal error: Timeout while acquiring runtime instance for plugin 'icpc'`.

### Bottleneck: WASM plugin instance pool

- `plugin_core::config::PluginConfig::pool_max_instances` defaults to **32**.
- icpc plugin instance acquire **p99 = 5 s** (clipped at upper histogram bucket
  — actually higher).
- icpc plugin call duration **p95 = 5 s** (also clipped).
- icpc plugin call failure rate: **2.5/s**.
- Each plugin instance is `Mutex<Plugin>` — one call at a time per instance.
- Each submission to /submissions invokes `BeforeSubmissionEvent` synchronously
  → icpc plugin's hook handler → plugin instance acquire.

The `create_contest_submission` handler (server) holds a DB transaction while
waiting for the icpc plugin instance. Under sustained load:

1. Pool of 32 icpc instances saturates immediately at ~17 sub/s POSTs.
2. Subsequent POSTs queue inside the API for up to 300 s (call_timeout_secs
   default).
3. After timeout → `PluginError::Internal` → submission written to DB with
   `status=SystemError`.

### End-to-end submission latency (successful path)

| Stage                          | p50    | p95                             | p99        |
| ------------------------------ | ------ | ------------------------------- | ---------- |
| API ingest (POST /submissions) | ~70 ms | ~250 ms (when not pool-blocked) | n/a        |
| MQ + queue_wait                | 0.4 s  | **9.5 s**                       | **24.6 s** |
| Worker task_process            | 0.07 s | **3.4 s**                       | **4.7 s**  |
| Result write back (estimate)   | ~50 ms | ~200 ms                         | —          |
| **Total e2e**                  | ~0.5 s | **~13 s**                       | **~30 s**  |

Worker process time itself (3-5 s) tracks compile-cached A+B execution against
Signpost data — the stress-test fixture uses canned solutions that semantically
don't match Signpost, but the platform still runs them through all 20 testcases.

### Blob store

| op                                 | rate   | p95    | p99    |
| ---------------------------------- | ------ | ------ | ------ |
| put_stream (submissions + results) | 11.3/s | 48 ms  | 50 ms  |
| get_range (testcase data)          | 3.8/s  | 158 ms | 232 ms |

`get_range` is used for partial testcase reads — workers fetch only the byte
ranges they need. p99 of 232 ms is reasonable for 1-27 MB partial reads.

### Effective throughput

- **API accepts** ~1 successful submission/s under load (most return SystemError
  after timeout).
- **Workers complete** ~6.7 task/s but many "tasks" are dependency-failure
  cleanups, not real evaluations.
- **Actual judged submissions** ≈ 180 over ~25 min = **0.12 Judged sub/s**.
- Compare to run-1: 5.2 sub/s with the toy A+B fixture and a working plugin
  path. Run-2's apparent regression is _not_ a worker regression — it's the
  plugin pool synchronously gating API ingest.

### What this means for 5000 contestants

- Theoretical max with current config: pool_max_instances=32 / ~5 s per icpc
  call ≈ 6.4 icpc calls/s. Each submission needs ≥1 icpc call.
- So **upper bound = ~6 sub/s** without bumping the pool.
- 5000 contestants × 1 sub/min = 83 sub/s required.
- **Plugin pool must be bumped 13×** (to ~400 instances) just to satisfy
  steady-state — and that's assuming the icpc plugin code is actually fast and
  the 5 s call duration is wait time, not work. Need to verify by reducing
  contention.

## Architectural fix designed and implemented (overnight)

### Diagnosis recap

The user-visible symptom: **47% of submissions ended as `SystemError` with
`error_message: "Internal error: Timeout while acquiring runtime instance for plugin 'icpc'"`.**

Root cause, in layers:

1. **Plugin pool exhaustion under fan-out load.** Every Signpost submission
   spawns 20 parallel tokio tasks (one per testcase) inside
   `start_evaluate_batch` (`packages/server/src/services/evaluate_batch.rs:83`).
   Each task acquires:
   - an `evaluator_slots` semaphore permit (size = number of CPU cores = 8)
   - then a `plugin pool` instance via `pm.call_raw` → `pool.get(timeout)`
     (`packages/plugin-core/src/traits.rs:335`)
2. **Default pool size (32) too small for fan-out scale.** With 100 submissions
   in flight, 100×20=2000 desired-concurrent eval permits vs. 8 cores × 1 server
   × 1 = 8 semaphore slots, hitting the plugin pool from queued spawns.

3. **Pool acquisition timeout produced a permanent verdict, not transient
   backpressure.** When `pool.get(timeout)` returned `Ok(None)` (i.e. 300 s
   elapsed without a free instance), `evaluate_batch.rs:147` and
   `submission_dispatch.rs:467` called `send_system_error` /
   `mark_submission_dispatch_system_error`, **writing a final SystemError row to
   the DB**. The contestant's submission was lost.

4. **Metric histograms clipped at 5 s upper bound.** `PLUGIN_BUCKETS_SECONDS` in
   `packages/common/src/metrics.rs` topped at 5 s, so p95/p99 reports were
   lower-bounds. Real values during the contention storm were 10s+, climbing.

### Fix layers shipped tonight

**Layer 1 — Env-var quick fix (deployed at 21:55Z, no rebuild):**

- `BROCCOLI__PLUGIN__POOL_MAX_INSTANCES=256` on api+worker. Each WASM instance
  is ~few MB; 256 × 8 plugins × ~3 MB ≈ 6 GB / server (fits 16 GB).
- `BROCCOLI__PLUGIN__CALL_TIMEOUT_SECS=3600` so contention queues instead of
  giving up at 5 minutes.
- `BROCCOLI__OBSERVABILITY__LOG_FILTER="info,sqlx=warn,sea_orm=warn,sea_query=warn,broccoli_queue=warn"`
  — every DB query was being logged as JSON to stdout (and Loki). Burns CPU +
  bandwidth + storage. Filtered noise.

Observed effect: SystemError rate dropped to **0/s** in the next monitor sample
window. Throughput climbed from ~1/s to ~2/s.

**Layer 2 — Code change (built as `broccoli-server:profile3`):**

1. **New `PluginError::PoolTimeout(plugin_id)` variant**
   (`packages/plugin-core/src/error.rs`) — distinct from `Internal`, so callers
   can decide retry policy without string-matching.

2. **`pool.get(timeout)` returns `PoolTimeout`** instead of `Internal`
   (`packages/plugin-core/src/traits.rs:368`).

3. **Retry-on-PoolTimeout in `evaluate_batch.rs`** with exponential backoff 100
   ms → 5 s cap, up to 60 attempts. Pool contention is transient; the verdict is
   preserved.

4. **Same retry pattern in `submission_dispatch.rs::run_judge`** so the icpc
   plugin's submission-level call also rides through transient contention.

5. **`PluginError::PoolTimeout` → `AppError::RateLimited { retry_after: 5 }`**
   (`packages/server/src/error.rs:206`) — when contention DOES surface to a
   synchronous request (e.g. `before_submission` hook on POST /submissions),
   client receives a clean **429 Too Many Requests** with `Retry-After: 5`, not
   an opaque 500.

6. **Made `evaluator_parallelism` configurable** via
   `BROCCOLI__PLUGIN__EVALUATOR_PARALLELISM`
   (`packages/plugin-core/src/config.rs` +
   `packages/server/src/host_funcs/mod.rs`). Default still derives from
   `available_parallelism()`. Set to **64** in compose so a fanned-out
   submission no longer slams an 8-slot semaphore.

### Why this is the right fix (per user directive)

User: _"it should really never timeout because of plugin call contention, THIS
is bad ux"_.

After fix, the only contention path that still surfaces to the user is the
synchronous `before_submission` hook on `POST /submissions`. That now returns
**429 RATE_LIMITED**, which the stress-test client correctly retries
(retry_after=5). The eval/judging path retries internally and never produces a
SystemError from contention.

### Caveat — retry loop is bounded at 60 attempts

If contention is truly permanent (e.g. a plugin is OOM-stuck and never frees the
pool), the retry loop terminates after ~5 min of backoff and writes a
SystemError so we don't loop forever. That's a real failure mode worth
investigating, not transient backpressure.

---

## 2026-05-12 22:00–22:30Z — profile3 deployed; new bottleneck emerged

Deployed `broccoli-server:profile3` + new env vars across api-1, api-2,
worker-1, worker-2 with `docker compose up -d --force-recreate`. Logs confirm
both code-path changes loaded:

- `evaluator_parallelism: 64` (new configurable semaphore picked up env var).
- icpc plugin registered + activated successfully.

Started fresh stress run `signpost-profile3-postfix-20260511T2207` on
broccoli-loadgen with same parameters as profile2 (1000 contestants, 9000 s
steady at 150 rps, 1800 s burst at 3×). PID 15589, detached via nohup.

### Outcome: HTTP listener deadlock at api-1/api-2

Within ~15 minutes the entire api tier became unresponsive. Findings, in order:

1. **Caddy ejected both upstreams.** All gateway requests returned
   `503 no upstreams available`. With
   `health_uri /healthz; health_interval 5s; health_timeout 2s`,
   broccoli-server's `/healthz` started exceeding 2 s under load (verified:
   direct `curl http://10.104.0.20:3000/healthz` from gateway takes >5 s). Caddy
   yanked both backends and didn't recover them.

2. **Relaxed the healthcheck** (`health_interval 30s`, `health_timeout 30s`,
   `fail_duration 0s`) and pushed via `caddy reload`. Upstreams came back in
   rotation immediately (`/reverse_proxy/upstreams` showed `num_requests:100`
   each within seconds). **But traffic then re-stalled** — caddy upstream
   counter froze at 100 and `api1_load` climbed but `rps=0` from Prometheus's
   perspective.

3. **Direct `localhost:3000/healthz` from inside api-1 also times out (exit
   28).** The HTTP listener is **fully deadlocked** even from loopback.

4. **broccoli-server thread state on api-1** (snapshot
   `docs/profiling/run-2/artifacts/deadlock-snapshot/api1-threads-223116.txt`):

   ```
        847 tokio-rt-worker     S (sleeping)
         18 broccoli-server     S (sleeping)
          1 OpenTelemetry.T     S (sleeping)
   ```

   All 866 threads are `S (sleeping)` on `futex_wait_queue`. Tokio's default
   blocking-thread cap is 512; the runtime spawned past it because every
   `block_in_place(Handle::current().block_on(async_fn))` in plugin host
   functions promoted a runtime worker into a "blocking" thread and forced a
   replacement worker to be spawned. Process is at ~10 % CPU doing background
   work but accepts no new HTTP connections.

5. **stress-test client hit file-descriptor cap** (1024 sockets open against
   gateway, never closed because gateway 503'd then api hung). Stress-test
   threads are all `futex_wait` / `epoll_pwait` (idle). 0 requests being sent to
   gateway. No throughput.

### Root cause

The `block_in_place(Handle::current().block_on(...))` bridge used by every
plugin host function (SQL, storage, blob_read_range, config:read, …) is
**architecturally hostile to tokio**. Each call:

- Tells the runtime "this worker is now blocking — please spawn a replacement to
  keep my reactor responsive."
- The replacement spawns; the original worker runs the inner async block on the
  same thread but as if it were sync code.
- Under high fan-out (ICPC plugin × 20-way testcase parallel × `block_on` inside
  host fns), the runtime spawns hundreds of replacement workers, each of which
  also calls `block_in_place`.
- Eventually all 512 blocking slots are saturated, then we hit OS thread limits,
  then the **`Mutex<Plugin>` contention** becomes a futex storm that none of the
  now-1000 threads can win.
- `/healthz` and `/metrics` (just simple async handlers) can't even acquire a
  free tokio worker.

### Implication for the architectural fix

The `PoolTimeout` retry loop from this morning _does_ prevent the SystemError
verdict cascade — under the same load that produced 47 % SystemError on
profile2, profile3 produced 0. But the retries themselves are now blocked at the
same futex contention. Retries pile up; in-flight count grows; thread pool
explodes; everything stalls.

**The retry-loop is a necessary correctness fix (no false SystemError), but it
does not increase capacity.** Throughput is still bounded by host-fn-bridge
contention. Worse, retries trade throughput for queue depth, masking the true
ceiling.

### What ships next (judging-reliability phase 2)

1. **Admission control** — middleware reads `broccoli_mq_consume_inflight` (or a
   local atomic counter) and rejects `POST /submissions` with 429 when above a
   configurable threshold. Stops new work from entering the queue once we're
   saturated. Already designed in `docs/plans/judging-reliability/phase2.md` per
   the worktree survey.
2. **Async-native host-fn bridge** — replace
   `block_in_place(Handle::block_on(async_fn))` with either (a) genuine
   `spawn_blocking` returning a oneshot to a sync wrapper, or (b) Extism's async
   host-fn API where the runtime owns the polling. Eliminates the tokio worker →
   blocking-thread promotion.
3. **Operational**: caddy `health_uri` should poll a `/healthz/static` route
   that does **not** hit the tokio runtime (e.g. a separate `axum` route
   registered before the main router, served from a lightweight reactor or just
   a fixed 200 OK literal).

### Artifacts captured this iteration

- `docs/profiling/run-2/artifacts/deadlock-snapshot/api1-threads-223116.txt` —
  full thread-state breakdown at deadlock.
- `docs/profiling/run-2/artifacts/deadlock-snapshot/api1-stacks-223125.txt` —
  kernel stacks of 5 sampled threads (all `futex_wait`).
- `docs/profiling/run-2/artifacts/flamegraphs/flame-broccoli-api-1-broccoli-server-222058.svg`
  — 60 s on-CPU sample. 160 samples; binary lacks frame pointers so user-space
  stacks unresolved, but kernel time dominantly `clock_nanosleep` (idle tokio
  workers) consistent with the futex finding.

### Next step

Kill the deadlocked broccoli-server containers, kill the stuck stress-test,
write up the findings report, then teardown — no more profile-iteration cycles
tonight; the necessary fix is a real code change (admission control), not
another env-var tweak.
