# fault-harness

Fault-injection harness for the broccoli judging stack. Each scenario sets up a
fault, observes the system's reaction, and emits a JSON transcript suitable as a
CI artifact.

The crate is excluded from `default-members`. Build explicitly:

```bash
cargo build -p fault-harness
```

## Usage

Spin up an ephemeral Redis testcontainer and run the cancel-storm scenario:

```bash
cargo run -p fault-harness -- --scenario cancel-storm --out transcript.json
```

Run against an external Redis (e.g. the dev stack):

```bash
cargo run -p fault-harness -- \
  --scenario cancel-storm \
  --redis-url redis://localhost:6379 \
  --batch-count 1000 \
  --readers 64 \
  --out transcript.json
```

Exits non-zero if the scenario's assertions fail; transcript is always written.

## Transcript schema

```json
{
  "scenario": "cancel_storm",
  "run_id": "...",
  "started_at": "...",
  "ended_at": "...",
  "params": { "batch_count": 1000, "readers": 64, "key_ttl_secs": 21600 },
  "events": [
    {
      "at": "...",
      "elapsed_ms": 0,
      "kind": "setup",
      "severity": "info",
      "message": "..."
    },
    {
      "at": "...",
      "elapsed_ms": 12,
      "kind": "inject",
      "severity": "info",
      "message": "...",
      "fields": { "prime_ms": 12 }
    }
  ],
  "summary": {
    "total_checks": 64000,
    "hits": 64000,
    "misses": 0,
    "p95_us": 180
  },
  "passed": true
}
```

Events are append-only and timestamped (UTC + monotonic `elapsed_ms` since
scenario start). The summary surfaces top-line numbers for dashboards.

## Scenarios

- **`cancel_storm`** — implemented. Pre-loads N batch cancel keys, fans out M
  concurrent EXISTS-style readers, and asserts every reader observes every key.
  Exercises the Redis cancel primitive that workers consult before processing a
  task (see `packages/worker/src/cancel.rs`).

The following scenarios are planned but not yet implemented. Adding one is
`impl Scenario for X` — see `src/scenarios/cancel_storm.rs` for the template.

- **`stuck_judge_cascade`** — saturate workers with stuck operations, observe
  recovery via the stuck-job detector and the lease/steal path.
- **`mq_stall_recovery`** — pause Redis container for T seconds; observe worker
  reconnect, MQ depth recovery, and verdict latency post-recovery.
- **`partial_network_partition`** — drop traffic to Postgres or Redis only (via
  `tc qdisc` or container network manipulation); observe which subsystem
  degrades and how.
- **`kill_9_server`** — kill an api replica mid-judgement; observe submission
  pickup via the lease/steal mechanism (UP#15/UP#19).
- **`rolling_worker_restart`** — drain leader mid-compile while followers poll;
  observe leader-election race and verdict consistency.

Each scenario should:

1. Take its parameters via clap fields on `Cli` (or a per-scenario subcommand
   when the parameter set diverges).
2. Build a `Transcript` and emit events for setup, inject, observe, recover,
   assertion phases.
3. Return `ScenarioOutcome { passed, transcript }`.
4. Be runnable both against a testcontainers Redis (no external infra) and an
   external `--redis-url` (so CI can run it against the live dev stack).

## CI integration

Recommended CI step (placeholder — wire when the harness graduates from
prerequisite to full coverage):

```yaml
- name: Fault harness — cancel storm
  run:
    cargo run -p fault-harness --release -- --scenario cancel-storm --out
    artifacts/cancel-storm.json
- name: Upload transcripts
  uses: actions/upload-artifact@v4
  with:
    name: fault-harness-transcripts
    path: artifacts/
```
