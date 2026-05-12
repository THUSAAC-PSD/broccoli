# Broccoli Profiling Run — Live Journal

Run started 2026-05-11 17:42 UTC. Region: DigitalOcean SGP1. Cluster: 10
droplets in `default-sgp1` VPC. This file is appended in real time as
discoveries land. Treat newest entries first within each section.

## Cluster layout (final)

| Role          | Name                   | Size         | Private IP  | Public IP       |
| ------------- | ---------------------- | ------------ | ----------- | --------------- |
| gateway       | broccoli-gateway       | s-4vcpu-8gb  | 10.104.0.3  | 178.128.218.138 |
| loadgen       | broccoli-loadgen       | s-8vcpu-16gb | 10.104.0.4  | 168.144.102.17  |
| observability | broccoli-observability | s-8vcpu-16gb | 10.104.0.5  | 174.138.18.132  |
| api-1         | broccoli-api-1         | s-8vcpu-16gb | 10.104.0.6  | 159.65.138.32   |
| api-2         | broccoli-api-2         | s-8vcpu-16gb | 10.104.0.7  | 178.128.123.6   |
| redis         | broccoli-redis         | s-8vcpu-16gb | 10.104.0.8  | 178.128.103.154 |
| storage       | broccoli-storage       | s-8vcpu-16gb | 10.104.0.9  | 167.99.73.31    |
| worker-1      | broccoli-worker-1      | s-8vcpu-16gb | 10.104.0.10 | 68.183.233.129  |
| worker-2      | broccoli-worker-2      | s-8vcpu-16gb | 10.104.0.11 | 167.172.73.105  |
| postgres      | broccoli-postgres      | s-8vcpu-16gb | 10.104.0.12 | 159.223.67.113  |

Burn rate: **~$1.36/hr** (~$33/day). Tracked teardown task #19 must run when
done.

## Deployment findings

### Surprises that wasted real time

1. **DO MCP `droplet-create` cannot place droplets into a non-default VPC.** No
   `vpc_uuid` parameter in the tool schema, so the user-created
   `broccoli-profiling` VPC (10.104.16.0/20) sat unused. Cluster landed in
   `default-sgp1` (10.104.0.0/20). Functionally identical for profiling, but the
   custom VPC is a wasted setup step.
2. **First DO API token was missing scopes.** `key-list`, `key-create`,
   `balance-get` all 403. `droplet-create` failed with "missing ssh_key:read and
   tag:create". User regenerated a full-scope token and reloaded MCP plugins —
   clean recovery.
3. **Tsinghua campus does asymmetric NAT for SSH vs HTTP.** `ipify.org`,
   `ifconfig.co`, and `ifconfig.me` all reported `43.239.95.25` for the Mac's
   HTTP egress. The actual SSH egress IP, confirmed from `/var/log/auth.log` on
   a droplet, is `59.66.28.86` (campus LAN /16, no NAT). Bootstrap had to be
   re-run with the correct admin IP. Direct SSH-to-DO worked the whole time on
   port 22 — there was no campus filter, just a UFW rule for the wrong source
   IP.
4. **Earlier in this run we mis-diagnosed UFW vs campus filtering.** Bash
   `/dev/tcp` "succeeded" on the closed port because `head -1` returned 0 on
   EOF; the connection was actually being dropped. Lesson: when testing TCP
   reachability through bash builtins, check the exit of the `exec` itself, not
   pipe consumers.
5. **Ubuntu 24.04 uses systemd socket activation for sshd (`ssh.socket`).**
   Editing `Port` in `sshd_config` has no effect; the listening ports come from
   `ListenStream=` in the socket unit. Workaround used:
   `[Socket] ListenStream=N` drop-in under `/etc/systemd/system/ssh.socket.d/`.
6. **DO web console requires a root password the droplet doesn't have** when the
   droplet was created with SSH keys only. Recovery flow needs an explicit
   "Reset Root Password" step which emails a one-time password.

### Image bundle surprises

7. **`pg_stat_statements` extension must be created in every database**, not
   just declared via `shared_preload_libraries`. We added an `init.sql` with
   `CREATE EXTENSION IF NOT EXISTS pg_stat_statements;` mounted to
   `/docker-entrypoint-initdb.d/`.
8. **PostgreSQL 18 changed its data directory contract.** The image refuses to
   start when an existing data dir is mounted at the legacy
   `/var/lib/postgresql/data` — PG18 expects `/var/lib/postgresql` and places
   data in a versioned subdirectory. We had to wipe the named volume and remount
   at the new path.
9. **SeaweedFS volume server defaults to bind 8080.** We had already given
   cAdvisor 8080 on every host in the bootstrap. Set `-volume.port=18080` on the
   seaweed command to avoid collision.

### Cost-side observations

- Originally planned `g-8vcpu-32gb` (General Purpose, 32 GB RAM) for Postgres.
  **DO restricted that size on this account tier**; the create call returned 422
  "please open a ticket to increase your account tier". We substituted
  `s-8vcpu-16gb` to keep the deploy moving. Postgres has 16 GB RAM instead of
  32, which will affect shared_buffers / page-cache pressure during the run —
  flagged for analysis.
- 500 GiB block volumes were skipped. With every droplet at `s-8vcpu-16gb` we
  get 320 GiB local NVMe for free, which is faster than DO's network-attached
  block storage for the same use cases (Postgres data, observability TSDB,
  SeaweedFS blobs).

### Observability gotchas observed during bring-up

- cAdvisor containers report **"unhealthy"** on all 10 hosts. Pre-existing
  issue, will dig into during smoke run.
- **Loki 3.4.3 retention requires `delete_request_store` to be set** when
  `retention_enabled: true`. The config rejected by Loki was producing a tight
  crash-restart loop. Added `delete_request_store: filesystem` under
  `compactor`.
- **Jaeger v2.17 dropped the legacy `--query.http.host-port`,
  `--collector.otlp.grpc.host-port`, and `--collector.otlp.http.host-port`
  flags.** The container would crash with `unknown flag` and never bind.
  Workaround: drop the CLI overrides entirely and use Docker `ports:` mappings
  bound to the private IP instead. (Otherwise jaeger v2 expects a YAML config
  file.)
- **Both api-1 and api-2 servers are emitting traces to Jaeger.**
  `/api/services` returns `broccoli-server-api-1` and `broccoli-server-api-2`,
  confirming OTLP propagation works in the cluster.

## Smoke run

### `smoke-judge-20260511-1916` (judge-mode default load)

Run shape: 200 submissions, 30 concurrent, 20/s offered rate. Two workers
(worker-icpc image) on separate `s-8vcpu-16gb` hosts. Two API replicas behind
Caddy LB.

**Results:**

- `correctness` phase: **9/9 scenarios passed** (single-shot, sequential).
- `load` phase: 200/200 completed, but **39 verdicts wrong** (`RuntimeError`
  instead of expected `Accepted` or other expected).
- Wall time: **38.46s for 200** → effective throughput ~5.2/s sustained against
  30 concurrent.
- Latency: p50 **2871 ms**, p95 **5535 ms**, p99 **6307 ms**, max 6415 ms — well
  under the 15s p95 budget.

**Mid-run resource snapshot:**

- worker-worker-1 container: **95.6% CPU** (clearly the bottleneck)
- API hosts: ~15% CPU per host
- Postgres host: ~14% CPU
- Redis host: ~1% CPU
- Storage (SeaweedFS): ~2% CPU
- Gateway: ~8% CPU
- Loadgen: ~2% CPU

**Critical finding — verdict instability under load:**

The shipped stress-test only exercises judge-mode load against synthetic A+B
problems. Of 200 submissions:

- 161 passed (correct verdict matched)
- 39 produced `RuntimeError` when the scenario expected `Accepted` (or differed
  similarly)

Affected scenarios include `ab-cpp-ac` (a simple "print a+b" accepted
submission) — a program that has no reason to fail at runtime, failing
intermittently. The single-shot correctness phase BEFORE the load phase passed
all 9 scenarios cleanly. So this only manifests under concurrency.

Most likely causes to investigate:

1. **isolate sandbox concurrency / cgroup contention.** When the worker spawns
   multiple isolate sandboxes back-to-back, slot reuse or cgroup ID collision
   can produce flaky failures.
2. **Insufficient file descriptor / process limits** in the worker container.
3. **Process group cleanup race** in the worker — a previous run leaving a
   zombie that the next run trips on.
4. **/tmp full or sandbox dir collision.**

This is precisely the kind of bug profiling was meant to surface, so I'm
treating it as a primary finding rather than a smoke-blocker. The platform is
otherwise behaving — gateway routing is fine, MQ shuttles tasks, both workers
pick up jobs, the verdict pipeline does return, just with wrong content.

### Why the shipped stress-test binary was outdated

The bundle at
`dist/broccoli-platform-v0.0.0-local/stress-test/broccoli-stress-test` doesn't
have the new `--profile mixed`, `--run-id`, `--duration`, `--contestants`, or
`--final-burst-*` flags. Those were added later in this same conversation; the
dist binary predates them. We started a fresh build of the new stress-test on
`broccoli-loadgen` in parallel with the smoke run.

### Observability surface was a baseline only

A bigger version of the above: the entire dist `server.tar.gz` and
`worker-icpc.tar.gz` were built before this session's instrumentation work.
Prometheus enumerates only **2 broccoli\_\* metric families** end-to-end on the
cluster:

- `broccoli_task_process_duration_seconds` — worker task duration histogram,
  labeled by `task_type` (only value seen: `operation`)
- `broccoli_step_duration_seconds` — per-step duration histogram inside the
  worker, labeled by `outcome` ∈ {success, failure, cache_hit, skipped}

The work we did in this session that is _not_ yet observable on the cluster
because the images predate it:

- HTTP route-labeled request count / duration / inflight on the server
- MQ publish/consume duration, queue depth sampling, message age
- Plugin pool acquire duration, plugin call duration, plugin acquire failures
- Blob store get/put/delete/exists timing and byte counters
- Worker active task count, queue wait time, compile/testcase/checker breakdown
  labels
- HTTP request body / response body size
- OTLP service names per server replica

To unlock the full observability surface we built, we need to:

1. Rebuild the bundle with the current source: `make release` (or whatever
   target produces `dist/broccoli-platform-*/images/*.tar.gz`).
2. Re-run
   `./ops/deploy.sh images && ./ops/deploy.sh servers && ./ops/deploy.sh workers`
   to redistribute and restart.

This is the most important platform-side action item that came out of this run.

### Baseline numbers from old instrumentation

Even with the limited metric surface, useful baselines emerged from the
200-submission judge smoke:

**Worker side:**

- Worker-1 processed 112 tasks, worker-2 processed 97. Load balanced reasonably
  (53/47 split).
- Worker task p95: **1956 ms** (mostly compile+exec).
- Step outcomes: **132 success, 84 failure, 173 cache_hit, 1 skipped**. The 84
  failures correspond roughly to the 39 wrong-verdict submissions (each
  submission emits ~2 steps).
- The cache-hit count (173) outpacing success (132) suggests the worker is
  caching compile artifacts across submissions of the same scenario, which is
  good.

**Database side (Postgres v18 on `s-8vcpu-16gb`):**

- **Cache hit ratio: 0.9996** — DB working set fits in shared_buffers; not the
  bottleneck at this load.
- Peak active connections: **2** (out of 400 configured max).
- 49 idle connections held by the pool. Plenty of headroom.
- 26,654 total commits during the smoke window — lots of writes per submission
  (verdicts, results, judgements, audits).

**Redis side (`s-8vcpu-16gb`):**

- 35,816 commands processed during the run, 15 connected clients. Below capacity
  by orders of magnitude.

**SeaweedFS side:**

- No `SeaweedFS_volume_server_request_total` time series appeared — either the
  SeaweedFS metrics endpoint at `:9327` doesn't expose what I queried, or no S3
  traffic flowed (probable, since the smoke fixture's testcase data is small
  enough to live inline in Postgres). Needs a follow-up to verify the metrics
  path and to retry with `signpost`-sized blobs.

**API side:**

- API replicas at ~15% CPU each during smoke; far from saturated.
- Caddy gateway at ~8% CPU.

**Workers were the obvious bottleneck** (95.6% CPU on worker-1 mid-run). Both
workers were CPU-bound; the queue would absorb more load only with more worker
hosts or more concurrent isolate slots per worker.

### Live-monitoring subagent findings (3-min window covering and following the smoke)

A monitor subagent sampled Prometheus every 30 s through and after the smoke.
Highlights:

- **Postgres connection pool jumped 15 → 51 in < 30 s and stayed pinned at 52**
  even after commit rate collapsed by 99%. Pool isn't shrinking — server's DB
  pool min-idle / reaper config is worth a look. Not a problem at this scale but
  at 5000-contestant peak this could matter.
- **Postgres commit rate fell 467/s → 104/s → 3/s → 3/s over four consecutive 30
  s windows.** Workload was effectively a single short write burst (correctness
  phase seed + load phase) then idle. Suggests the shipped stress-test load
  profile front-loads writes and never sustains submission throughput long
  enough for percentile windows to stabilize.
- **Effective work finished in the first ~30 s.** Loadgen RX dropped from 3.2
  MB/s in window 1 to ~0.1 KB/s by window 4. Total 200-submission load completes
  well under our 3-minute monitoring window. To get useful percentile data we
  need a longer-duration / sustained-rate profile mode.
- No host came close to saturating during the monitoring window's aggregate
  average. Peak per-host CPU averaged over a 30-s window was **api-1 10.9%**,
  **worker-1 9.1%**. (Spot reading during the smoke peak showed worker-1 at
  95.6%, so the 30-s averaging window is hiding the actual saturation.)
- Memory available stayed >91% on every host for the whole run.

This confirms the picture: the dist binary load mode is too short, and the
metric surface we built isn't on the running image, so percentile data is
limited. Both blockers are unblocked by rebuilding the bundle from current
source.

### Correctness "bug" — root cause (subagent report, 19:35 UTC)

**Updated finding: this is a stress-test fixture issue, not a platform bug.**

A deeper dive (reading worker logs, `dmesg`, sandbox meta files, source code) by
a subagent pinned the cause precisely:

- The `test_case_result` rows show `checker_output = "Caught fatal signal 11"`
  (SIGSEGV) for failing submissions, with `memory_used` clustered at **328-576
  KB** and `time_used` 1-3 ms.
- `dmesg` on both workers shows two distinct patterns:
  1. **Cgroup OOM kills** inside `/.../isolate/box-NN`:
     `Memory cgroup out of memory: Killed process ... (solution) total-vm:318152kB, anon-rss:65152kB ...`
     — limit `65536kB`. These produce the 10 `MemoryLimitExceeded` rows.
  2. **Segfaults at entry**:
     `solution[PID]: segfault at 0 ip ... error 6 in solution[...+1000]` —
     segfault before main() runs. These map to the 47 SIGSEGVs which become
     RuntimeError verdicts.

**Why under concurrency, but not sequentially:**

The stress-test default problem memory limit is **64 MB** —
[packages/stress-test/src/scenarios.rs:27](../../packages/stress-test/src/scenarios.rs)
sets `DEFAULT_PROBLEM_MEMORY_LIMIT_KB = 65_536`. Compare to
[packages/server-sdk/src/types/evaluate.rs:223](../../packages/server-sdk/src/types/evaluate.rs)
where the SDK default is **256 MB** (262144). So the stress-test fixture imposes
a 4× tighter limit than the real platform default.

The compiled g++ binary (statically linked libstdc++) has `total-vm ≈ 318 MB`
and resident usage near 64 MB at startup — `std::ios_base::Init` constructors,
allocator arenas, etc. In the sequential correctness pass, pages can be lazily
mapped and the working set just barely fits under 64 MB. Under 30 concurrent
boxes each cgroup independently allocates ~65 MB, and:

- Some cross the ceiling → `SIGKILL` from the cgroup → 10 MLE rows.
- More commonly, an allocation in libstdc++ static init throws `std::bad_alloc`.
  With no catch-handler at that point in init, it unwinds into `__cxa_throw` →
  `std::terminate` → `abort()`, raising signal 11 at the trap stub (`0f 0b`
  bytes in dmesg) before `main()`. isolate writes
  `status:SG message:"Caught fatal signal 11"` and
  [packages/server-sdk/src/evaluator/interpret.rs:110-121](../../packages/server-sdk/src/evaluator/interpret.rs)
  translates that to RuntimeError. → 47 RuntimeError rows.

Python (heap not pre-committed in a single process), igncase (lighter codegen),
and TLE (no iostream include) all stay below the threshold and are unaffected.

**Conclusion: not a Broccoli platform bug.** The cluster, isolate runtime, and
worker pipeline are doing exactly the right thing under the constraints they
were given. The bug is in the **stress-test fixture** itself — 64 MB is
unrealistically tight for the C++ toolchain shipped in
`broccoli-worker:v0.0.0-local-icpc`. No real-contest deployment would set
per-problem limits this low for C++.

**Recommended fix:**

1. Bump `DEFAULT_PROBLEM_MEMORY_LIMIT_KB` in
   `packages/stress-test/src/scenarios.rs:27` from `65_536` (64 MB) to
   **`262_144`** (256 MB) — matching the SDK default at
   `packages/server-sdk/src/types/evaluate.rs:223`.
2. Re-pin the deliberate MLE scenario `ab-cpp-mle` (`scenarios.rs:83`) so it
   stays intentionally over-limit — e.g., set the problem limit to 128 MB with a
   solution that allocates 256 MB.
3. Re-run the smoke; expect verdict-mismatch count to drop to ~0 for
   `ab-cpp-ac`, `ab-cpp-multi`, `ab-cpp-wa`.

**Secondary observation (latent):**

- `interpret.rs:110-121` currently always maps `status=SG` to `RuntimeError`. It
  could optionally inspect `sandbox.cg_oom_killed` and reclassify a SIGSEGV that
  coincides with a cgroup OOM as MLE. But since the SIGSEGV here is libc-abort,
  not a cgroup kill, the existing mapping is correct; the real fix is the memory
  limit.
- The earlier "Collect target not found" warning on worker-1 was a separate,
  much rarer event — not the dominant cause of the 47 RT failures. My earlier
  journal entry attributing the bulk of failures to that warning was wrong.

### Earlier (now-superseded) preliminary read

Manual investigation (running in parallel with subagent) surfaced this very
fast:

**Worker log evidence (worker-1):**

```
{"level":"WARN","message":"Collect target not found, skipping","path":"/var/local/lib/isolate/4/box/solution"}
{"level":"WARN","message":"Skipping task due to dependency failure","task_id":"exec"}
```

The compile step's output binary `/var/local/lib/isolate/<box>/box/solution` was
missing when the subsequent exec step tried to collect it. Exec was skipped —
which produces a phantom RuntimeError-shaped verdict because the contestant
program "didn't run".

**But — only 1 such warning on worker-1 across the whole smoke**, with 0
occurrences on worker-2. Yet `test_case_result` shows **47 RuntimeError rows**.
So 46 of 47 failure causes are not being logged at all. The worker has a
**diagnostic gap**: it writes RuntimeError to the DB without emitting any
log/trace explaining why.

**Verdict distribution in `submission_judgement`** for the 200-job smoke: |
Verdict | Count | |---|---:| | Accepted | 105 | | RuntimeError | 47 | |
TimeLimitExceeded | 26 | | WrongAnswer | 19 | | MemoryLimitExceeded | 10 | |
(null) | 2 |

Looking at the `test_case_result` rows that are `RuntimeError`: `time_used` is
**1–119 ms** with mean **4 ms** — i.e. the program is dying _immediately_, not
after running for any meaningful time. That's consistent with one of:

- Process never reached `main()` (exec failure, ld.so failure, seccomp deny,
  prlimit hit)
- Missing binary (the "Collect target not found" pattern, but at scale)
- isolate prlimit / cgroup setup race
- File descriptor / inode pressure on worker box during concurrent box setup

The smoke ran with only 30 concurrent submissions and 2 worker hosts. The
worker-icpc image uses `BROCCOLI__WORKER__SANDBOX_BACKEND=isolate` with
`BROCCOLI__WORKER__ENABLE_CGROUPS=true`. Isolate is well-known to have lock-step
issues if the same box-id is rapidly recycled across overlapping tasks.

**Recommended follow-ups (in priority order):**

1. **Instrument the failure path.** When the worker decides "Skipping task due
   to dependency failure" → it must emit a log/span/metric naming the
   dependency, the box-id, the step kind, the submission id, and the inferred
   contestant verdict it's about to record. Today it silently writes
   RuntimeError.
2. **Audit isolate box-id allocation.** Verify that concurrent worker tasks
   don't share box-ids. The `4` in the path suggests a small box-id pool.
   Confirm there's no path where two concurrent tasks both grab the same box.
3. **Re-run with sandbox_backend=mock to confirm.** If mock backend has zero
   RuntimeError, then the bug is _definitely_ in the isolate path. (Currently
   can't tell because we lack instrumentation.)
4. **Increase worker tracing verbosity for compile step**, including
   stderr-on-failure, isolate meta file contents, and exit-code reason.

Both bugs (the silent dependency-skip and the underlying isolate race) are
pre-existing — they would happen in any deployment running this image at this
concurrency level, not just our profiling cluster. Worth filing as platform
issues.

## Outstanding action items

- Investigate cAdvisor unhealthy state on all hosts.
- File DO support ticket if 32-GB-RAM Postgres profile is desired.
- Re-test with a customer-grade non-campus network to confirm whether the
  SSH-vs-HTTP NAT split is Tsinghua-specific or generalizable.

## Teardown

Cluster destroyed via DO MCP `droplet-delete` calls at **2026-05-11 19:34 UTC**.
`droplet-list` afterwards returned only the unrelated pre-existing
`docker-s-1vcpu-1gb-sgp1-01`. Billing for the profiling cluster stopped at that
point.

**Total session cost estimate:** the 10 droplets ran from ~17:42 UTC to ~19:34
UTC, ~1h 52m at $1.357/hr ≈ **$2.55** in compute. No block volumes were
attached, no spaces consumed. Public egress was minimal (image pulls + a few
hundred MB of bundle scp during deploy).
