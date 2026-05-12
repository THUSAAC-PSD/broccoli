# Broccoli Profiling Run-2 — Findings (TEMPLATE — fill in after collection)

Run window: `<start>` → `<end>` (UTC). Run-id: `signpost-2026-05-12`.

## TL;DR

- Headline result (throughput, p95 latency, error rate, queue dynamics):
- Primary bottleneck:
- Highest-leverage fix:
- 5000-contestant projection vs steady-state and burst:

## Setup

- 11 droplets in DigitalOcean SGP1 / default-sgp1 VPC (10.104.0.0/20).
- 9× `s-8vcpu-16gb`, 1× `s-4vcpu-8gb` (gateway), 1× `s-8vcpu-16gb` (build).
- Ubuntu 24.04 LTS. Postgres 18, Redis 7, SeaweedFS 4.15, Caddy 2.
- Prometheus 3.4, Grafana 11.4, Loki 3.4, Jaeger 2.17, Promtail 3.4.
- Server + worker built from current source (HEAD on master).
  `debug = "line-tables-only"` for perf-resolvable flame graphs.
- Fixture: Signpost (20 testcases, total 419 MB; largest input 27 MB).
- Load:
  `stress-test --profile mixed --duration 9000 --rate 150 --concurrency 200 --contestants 5000 --final-burst-duration 1800 --final-burst-multiplier 3`.

## Throughput, latency, errors

(filled from `stress-output.jsonl` + Prometheus federate)

| Phase            | Duration | Achieved rate | p50     | p95     | p99     | Error rate |
| ---------------- | -------- | ------------- | ------- | ------- | ------- | ---------- |
| Steady           | 9000 s   | \_\_ / s      | \_\_ ms | \_\_ ms | \_\_ ms | \_\_%      |
| Final burst (3x) | 1800 s   | \_\_ / s      | \_\_ ms | \_\_ ms | \_\_ ms | \_\_%      |

Per-MixedAction breakdown (steady-state means):

| Action             | RPS | p95 (ms) | error rate |
| ------------------ | --- | -------- | ---------- |
| ContestRead        |     |          |            |
| ProblemRead        |     |          |            |
| ScoreboardRead     |     |          |            |
| OfficialSubmission |     |          |            |
| CodeRun            |     |          |            |
| SampleRead         |     |          |            |
| AttachmentRead     |     |          |            |
| FrontendAsset      |     |          |            |
| SessionLogin       |     |          |            |

## Subsystem saturation

For each: peak CPU%, peak Mem%, peak network out, peak disk IO, and the _moment
of saturation_ relative to T0.

| Subsystem       | Peak CPU% | Peak Mem | Peak Net | Peak Disk | First saturation | Driver |
| --------------- | --------- | -------- | -------- | --------- | ---------------- | ------ |
| Gateway (Caddy) |           |          |          |           |                  |        |
| API server-1    |           |          |          |           |                  |        |
| API server-2    |           |          |          |           |                  |        |
| Postgres        |           |          |          |           |                  |        |
| Redis           |           |          |          |           |                  |        |
| SeaweedFS       |           |          |          |           |                  |        |
| Worker-1        |           |          |          |           |                  |        |
| Worker-2        |           |          |          |           |                  |        |

## DB connection pool dynamics

(broccoli_database_pool_size, broccoli_database_pool_idle,
broccoli_database_pool_acquire_seconds histograms)

- Steady-state pool acquire p95: \_\_ ms (budget: ?)
- Burst acquire p95: \_\_ ms
- Pool exhaustion incidents (acquire timeout): \_\_ count

## MQ pipeline

(broccoli*mq_consume*_, broccoli*mq_publish*_, broccoli_mq_message_age_seconds)

- Steady consume rate: ** /s (vs publish rate ** /s)
- Queue depth peak: \_\_
- Message age p95 peak: \_\_ s
- Burst behavior: did consume keep up? Did queue drain after burst?

## Blob store (SeaweedFS S3)

(broccoli_blob_fetch_seconds, broccoli_blob_put_seconds)

- p95 fetch latency for testcase input: \_\_ ms (size?)
- p95 fetch latency for submission code: \_\_ ms
- Peak ops/s

## Plugin invocation

(broccoli_plugin_call_seconds_bucket per plugin/function)

- icpc plugin call p95: \_\_ ms
- batch-evaluator p95: \_\_ ms
- Plugin pool wait time: \_\_

## Worker pipeline

(broccoli_worker_active_tasks, broccoli_worker_task_duration_seconds,
broccoli_isolate_run_seconds, broccoli_compilation_seconds)

- Compile p95 (cpp): \_\_ s
- Execution p95: \_\_ s
- Worker task duration p95: \_\_ s
- Worker active concurrency: ** avg, ** peak

## Flame graphs

- broccoli-api-1 @ T+15min:
  `flamegraphs/flame-broccoli-api-1-broccoli-server-*.svg`
  - Top hot path: \_\_\_
- broccoli-api-1 @ T+1h45min: hot path: \_\_\_
- broccoli-api-1 @ T+mid-burst: hot path: \_\_\_
- broccoli-worker-1 (same): \_\_\_

## Real bugs / surprises surfaced

1. _N/A so far_

## Capacity scaling estimate (5000-contestant projection)

- Steady-state (1 action / 30 s per contestant ≈ 150 ops/s):
  - Required worker fleet: \_\_
  - Required API instances: \_\_
  - Required DB pool size: \_\_
- Final-30-min burst (1 action / 10 s per contestant ≈ 450 ops/s):
  - Required worker fleet: \_\_
  - Required API instances: \_\_

## Recommendations (ordered by impact)

1.
2.
3.

## Repeatability

Future runs from same baseline:

```bash
./ops/profile-runbook.sh build
./ops/profile-runbook.sh deploy
./ops/profile-runbook.sh seed
./ops/profile-runbook.sh monitor &
./ops/profile-runbook.sh flames &
./ops/profile-runbook.sh stress
./ops/profile-runbook.sh collect
```

Diff results across runs by comparing `docs/profiling/run-N/findings.md` and
re-running the same Prometheus queries from `ops/collect-artifacts.sh` against
the snapshotted TSDB.
