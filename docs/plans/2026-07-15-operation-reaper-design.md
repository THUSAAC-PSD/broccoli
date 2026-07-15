# Dead-worker operation reaper (MQ #2 / MQ #3 leftover)

Status: approved 2026-07-15. Scope: private queues + shared `_processing`.
Rollout: active by default.

## Problem

The MQ broker (vendored `broccoli_queue 0.4.6`, Redis) has no visibility timeout
and no reaper. When a worker crashes mid-operation, the broker-internal task id
it was processing stays wedged in `<queue>_processing` forever; nothing ever
redelivers it. The same dead worker's queued-but-unstarted tasks likewise strand
in its private queue ZSET `<queue>:worker:<id>`. Today both are recovered only
by the operation waiter timeout (`.max(30 min)`) or the batch reaper at 7200s.
On the judging path the submission lease/steal engine recovers work quickly; the
_operation_ MQ path has no equivalent, so a crashed worker stalls every
in-flight operation for up to half an hour.

MQ #3 already routes _new_ pinned publishes off a dead worker to the shared
queue. This reaper handles the tasks that were _already_ sitting in a dead
worker's private queue and processing list when it died, plus shared-queue
processing strands.

## Why this is safe (the part that made it "risk-prone")

1. **Requeue uses only the public `mq.publish` API.** Operations publish with
   `disambiguator = None`, so the private/shared queues are plain ZSETs with no
   fairness sub-queues. We never re-implement broker-internal ZADD scoring or
   fairness-set maintenance. We read the stranded task's payload hash,
   deserialize the domain `Task`, and republish it through the same path a
   plugin would.

2. **Double-execution is already gated by the existing heartbeat-aware dedup.**
   The worker's `RedisTaskDedup.try_claim` returns `HeldByOther` (skip) when a
   redelivered task's original owner still has a live heartbeat, and `Stolen`
   (re-run) only when that owner's heartbeat is gone. The system _already_
   re-runs operations on suspected death via the normal retry path; the reaper
   reuses that trusted mechanism and adds no new double-run risk.

3. **Operations are self-contained / blob-portable.** An evaluate-batch
   operation carries its `test_input`, `expected_output`, `solution_source`,
   `solution_language`, and `checker_config`. `target_worker_id` is a locality
   optimization (compile once, reuse cache warmth on one worker), not session
   affinity. Interactive/communication judging, which does hold per-worker
   session state, runs on the separate lease-recovered submission path, never
   here. So requeuing a pinned operation to the shared queue after its worker
   dies yields worse cache locality and an identical result.

## Mechanism

New module `packages/server/src/dispatcher/operation_reaper.rs`, sibling to
`sweeper.rs`, spawned from `dispatcher::Dispatcher::spawn` when a Redis client
and an MQ handle are both present and `operation_reaper_enabled` is set.

Each tick (`operation_reaper_interval_secs`):

1. **Fail closed on absent liveness.** SCAN `broccoli:worker:heartbeat:*`. If
   zero worker heartbeats exist fleet-wide, the liveness signal itself is
   missing (Redis flush, heartbeat writer down) — that is not "every worker
   died". Refuse to reap and do not start debounce clocks. Otherwise, the set of
   live worker ids is exactly the SCAN result.

2. **Private queues.** SCAN `<shared>:worker:*`, group by worker id. For each
   owner NOT in the live set, past its per-worker `dead_since` debounce
   (`operation_reaper_grace_secs`): requeue every uuid in its private ZSET
   (`ZRANGE`) and private `_processing` LIST (`LRANGE`) to the shared queue,
   removing each from its source and deleting the orphaned payload hash.
   Ownership is unambiguous from the `:worker:<id>` key name — no attribution
   needed.

3. **Shared `_processing`.** `LRANGE <shared>_processing`. For each uuid:
   `HGETALL` the payload hash, parse the `Task`, read the dedup owner
   (`GET broccoli:dedup:<Task.id>`), and if that owner is not live (past its
   debounce) requeue + clean. Entries with no dedup owner (expired key, dedup
   disabled) are unattributable: leave them for the waiter-timeout backstop and
   count them. A live owner's in-flight entry is never touched.

Per-tick requeue budget (`operation_reaper_max_requeues_per_tick`) caps blast
radius; the remainder is logged (no silent truncation) and picked up next tick.
`dry_run` logs intended requeues without mutating. Dangling uuids (payload hash
already gone) and undeserializable payloads are removed-or-skipped and counted,
never silently lost.

## Requeue primitive

```
requeue(uuid):
  fields = HGETALL <uuid>                      # broker InternalBrokerMessage hash
  task   = serde_json::from_str::<Task>(fields["payload"])
  mq.publish::<Task>(shared_queue, None, &task, None)   # fresh uuid + hash + ZADD
  <remove uuid from its source list/zset>
  DEL <uuid>                                    # old orphaned payload hash
```

The domain `Task.id` is preserved across the republish, so dedup keys on it and
idempotency holds regardless of the broker generating a fresh internal uuid.

## Config (`server.*`)

- `operation_reaper_enabled` (default true) — master switch.
- `operation_reaper_interval_secs` (default 30) — tick period.
- `operation_reaper_grace_secs` (default 30) — per-worker dead debounce before
  requeuing, on top of the 15s heartbeat TTL.
- `operation_reaper_max_requeues_per_tick` (default 1000) — blast-radius cap.
- `operation_reaper_dry_run` (default false) — log-only.

## Testing

Unit: private-queue key parsing, family-key derivation, debounce decision.
Integration (testcontainers Redis): private-queue requeue to shared + strand
cleanup; shared-processing requeue via dedup attribution; live-owner entry
survives; fail-closed when zero heartbeats; dangling-uuid cleanup.

## Deliberately deferred

Draining a dead worker's private `_failed` list (genuine exhausted-retry
dead-letters, not recoverable work) is left to the existing reply-queue
sweeper's family-deletion model rather than requeued.
