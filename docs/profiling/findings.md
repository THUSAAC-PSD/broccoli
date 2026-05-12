# Broccoli Profiling Findings — DO SGP1, 2026-05-11

This is the summary report from the first cloud profiling run. The live journal
is at [`journal.md`](./journal.md); this is the distilled report.

## Setup

- 10 droplets in DigitalOcean SGP1, `default-sgp1` VPC (10.104.0.0/20)
- 9× `s-8vcpu-16gb` Basic, 1× `s-4vcpu-8gb` Basic (gateway)
- Ubuntu 24.04 LTS, Docker CE, UFW, chrony, node-exporter + cAdvisor per host
- Postgres 18 (alpine), Redis 7 (alpine), SeaweedFS 4.15, Caddy 2 (alpine)
- Prometheus 3.4, Grafana 11.4, Loki 3.4.3, Jaeger 2.17, Promtail 3.4.3
- Broccoli server + worker-icpc + 8 plugins from
  `dist/broccoli-platform-v0.0.0-local`

Topology recap:

```
┌──────────────┐   public 80     ┌──────────────────┐
│  gateway     │ ────────────▶   │  api-1 / api-2   │
│  (caddy)     │                 │  broccoli-server │
└──────┬───────┘                 └────────┬─────────┘
       │ vpc                              │ vpc
       │                                  ▼
       │                       ┌──────────────────┐    ┌──────────────────┐
       │                       │   postgres       │    │     redis        │
       │                       │  + pg_stat       │    │    (MQ + cache)  │
       │                       └──────────────────┘    └──────────────────┘
       │                                  ▲
       │                                  │
┌──────▼────────┐                ┌────────┴─────────┐
│  loadgen      │                │ worker-1/worker-2│
│  stress-test  │                │ broccoli-worker  │
└───────────────┘                └──────────────────┘
                                         │
                                         ▼
                                ┌────────────────┐
                                │  storage       │
                                │  seaweedfs S3  │
                                └────────────────┘

observability host (broccoli-observability) scrapes everything.
```

## What the smoke proved

The cluster is functional end-to-end. With a 200-submission judge-mode load at
30 concurrent:

| Subsystem       | State at peak                              | Verdict                                           |
| --------------- | ------------------------------------------ | ------------------------------------------------- |
| Gateway (Caddy) | ~8% CPU                                    | huge headroom                                     |
| API (2× server) | ~15% CPU each                              | huge headroom                                     |
| Postgres        | ~14% CPU, 99.96% cache hit, 2 active conns | huge headroom                                     |
| Redis (MQ)      | ~1% CPU, 15 conns                          | huge headroom                                     |
| Workers         | **95.6% CPU on worker-1**                  | bottleneck under default concurrency              |
| SeaweedFS       | ~2% CPU                                    | underutilized — testcase data too small to stress |

p95 submission latency was **5535 ms** under load, against a 15 s budget. Worker
task duration p95 alone was **1956 ms**. Throughput sustained was **~5.2
submissions/s** with 2 workers — i.e. about **2.6 submissions/s per worker**.

## Real bugs surfaced

1. **`RuntimeError` storm — but it's a stress-test fixture bug, NOT a platform
   bug.** 47/200 submissions returned `RuntimeError`, all
   `Caught fatal signal 11` (SIGSEGV) at program entry, `time_used` 1-3 ms.
   Reading `dmesg` on the worker hosts revealed cgroup OOM kills and segfaults
   at entry — the C++ static-libstdc++ binary's startup memory footprint
   (libstdc++ static init, allocator arenas) is **~65 MB**, exactly at the **64
   MB problem memory limit** the stress-test fixture sets at
   [`packages/stress-test/src/scenarios.rs:27`](../../packages/stress-test/src/scenarios.rs)
   (`DEFAULT_PROBLEM_MEMORY_LIMIT_KB = 65_536`). The actual platform default at
   [`packages/server-sdk/src/types/evaluate.rs:223`](../../packages/server-sdk/src/types/evaluate.rs)
   is **256 MB** — the stress-test runs at 4× tighter. Sequential correctness
   passes because pages map lazily; concurrent boxes each independently allocate
   ~65 MB and some libstdc++ static init allocation throws `std::bad_alloc` →
   `__cxa_throw` → `abort()` → SIGSEGV before main(). isolate writes `status=SG`
   and
   [`server-sdk/src/evaluator/interpret.rs:110-121`](../../packages/server-sdk/src/evaluator/interpret.rs)
   correctly translates that to RuntimeError. **The cluster, worker, isolate,
   and verdict pipeline all did the right thing.** Fix: bump the stress-test
   default to `262_144` (256 MB) to match the SDK default. Re-pin `ab-cpp-mle`
   to a tighter limit so the intentional MLE case still trips.

2. **Postgres 18 mount path.** The current
   `release/docker-compose.infra.yaml.template` mounts `postgres_data` at
   `/var/lib/postgresql/data`. PG 18+ rejects that and wants
   `/var/lib/postgresql`. Patched locally in
   `ops/deploy/postgres/docker-compose.yaml`. The release template should be
   updated.

3. **SeaweedFS volume port collides with cAdvisor.** Default volume server port
   is 8080; cAdvisor is also 8080. Set `-volume.port=18080` on weed. Bootstrap
   should either pick a non-8080 port for cAdvisor (cleaner) or seaweed should
   ship with a non-conflicting volume port.

4. **Loki 3.4.3 requires `delete_request_store` when retention is on.**
   Otherwise tight crash loop on startup. Worth documenting in the observability
   docs.

5. **Jaeger 2.17 dropped CLI flags** like `--query.http.host-port` that worked
   in 1.x. v2 takes a YAML config or relies on Docker port mappings.

6. **DO MCP `droplet-create` ignores VPC.** Cannot place droplets in a custom
   VPC via the MCP tool. Operationally, the custom `broccoli-profiling` VPC the
   user pre-created went unused. Worth filing as a feature request on the DO MCP
   server or documenting the limitation.

## Surprises that were _not_ Broccoli bugs

- **Tsinghua does asymmetric NAT for SSH vs HTTP.** Web traffic egresses as
  `43.239.95.25`, SSH egresses as the raw campus IP `59.66.28.86`. Bootstrap UFW
  rules need the SSH IP.
- **DO API tokens default to read-only-ish scopes via MCP.** Full-scope tokens
  require choosing custom scopes at creation: `droplet`, `ssh_key`, `tag`,
  `firewall`, `block_storage`, `vpc`, `image`, `region` at minimum.
- **Ubuntu 24.04 sshd uses systemd socket activation.** `sshd_config` `Port`
  lines don't change listening ports; you need `ListenStream=` in the socket
  unit.

## Biggest gap

The dist bundle's `broccoli-server` and `broccoli-worker` images were built
_before_ this session's observability instrumentation. So the live cluster only
exposes 2 of the ~20 metric families we added. **Top action item: rebuild the
bundle from current source and redeploy** to unlock route-labeled HTTP metrics,
MQ consume metrics, plugin pool metrics, blob store metrics, queue wait, message
age, worker active tasks, and the rest.

Path:

1. `cargo build -p server -p worker --release` (or whatever produces the dist
   images).
2. `make release` to produce
   `dist/broccoli-platform-vX.Y.Z-local/images/*.tar.gz`.
3. Re-run `./ops/deploy.sh images servers workers`.
4. Re-run the smoke and capture the richer metric set.

## Capacity scaling estimate from this data

With 2 workers at `s-8vcpu-16gb` saturating at ~5.2 submissions/s sustained:

| Target                                                    | Workers needed                                 |
| --------------------------------------------------------- | ---------------------------------------------- |
| 50 submissions/s                                          | ~20 workers (≈10× scale)                       |
| 100 submissions/s                                         | ~40 workers                                    |
| 5000 contestants × 1 submit / 30 min during contest peak  | ~3 submissions/s — current 2 workers handle it |
| 5000 contestants × 1 submit / minute (final-30-min burst) | ~83 submissions/s — needs ~32 workers          |

Caveat: these numbers depend entirely on per-submission compile+exec cost, which
the A+B fixture massively understates compared to real contest problems. Repeat
with `qa-evidence/signpost` (419 MiB testcase data) before quoting these as
contest capacity.

## Recommendations (in order of impact)

1. **Fix the RuntimeError-under-load bug.** This is a hard blocker for a real
   contest; verdict correctness must be 100% under concurrency.
2. **Rebuild the platform images** to capture full observability surface, then
   re-profile.
3. **Patch the release template fixes** (Postgres 18 mount path, SeaweedFS port,
   Loki retention).
4. **Run a real fixture profile** with `signpost` testcase data through the new
   stress-test mixed profile (with `--run-id`, `--duration`, `--contestants`)
   once it's built.
5. **Investigate Postgres `g-8vcpu-32gb` unlock.** File a DO support ticket if
   32 GB Postgres RAM matters for the eventual contest sizing.

## Cluster teardown

All 10 droplets were destroyed at end of session. See journal for the action
timestamp.
