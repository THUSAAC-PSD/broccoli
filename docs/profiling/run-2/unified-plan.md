# Broccoli Unified Delivery Plan — Judging Reliability + Run-2 Follow-Up

**Date:** 2026-05-12 **Status:** consolidation of two parallel planning efforts
(audit-driven seven-phase reliability plan and run-2 profiling follow-up),
revised to incorporate Agent C's critical review (Phase A.5 added; Claim C
refuted in §D.5) and the user's integrated-release directive (no intermediate
deployment between phases — see §E). Phase 0 has shipped to `master`. Phases 1,
1b, and 9 of 11 Phase 2 PRs are implemented in the
`judging-reliability-phase1-recovered` worktree but **not yet PR-staged, merged,
or covered by integration tests for the new behavior**. Phase A.5 items 14b–14h
and all Phase D / E / F / G work is genuinely new. **Scope:** all numbered audit
findings (§1.0 – §1.19), all PRs from the Phase 1b and Phase 2 implementation
plans, and every numbered item in `follow-up-plan.md`. Where the two source
documents disagreed, §D names the resolution. **Release model:** all phases land
sequentially in a single integrated release. No intermediate canary or
deployment between phases. §E discusses what that constraint changes about
review priorities and the integration test plan.

---

## A. Context and reconciliation

Two independent planning efforts have been running in parallel:

1. **Judging-reliability audit + impl plans.** The audit
   (`docs/plans/2026-05-07-judging-reliability-audit.md`, 2253 lines) reads the
   tree and enumerates 19 numbered findings in §1, defines a 7-phase staged plan
   (Phases 0, 1, 1b, 2, 3, 4, 5), and is followed by the Phase 1b implementation
   plan (`docs/plans/2026-05-08-phase-1b-impl.md`, 9 PRs A–I) and the Phase 2
   implementation plan (`docs/plans/2026-05-08-phase-2-impl.md`, 11 PRs J–DD).
   Observability has its own two-phase plan (Phase 0/1 in
   `docs/plans/2026-05-07-observability-phase-{0,1}-impl.md`).
2. **Run-2 profiling follow-up.** After the audit was written, a multi-droplet
   stress run on Signpost (`docs/profiling/run-2/findings.md`) reproduced a
   tokio futex deadlock at ~6 ops/s, with the api tier saturated on 866
   tokio-rt-worker threads in `futex_wait_queue` and workers idle at 0.05 load.
   The synthesis in `follow-up-plan.md` identifies five corrections to the
   original narrative (its §0 A–F) and lands 38 numbered remediation items
   across phases A/B/C/D/E.

**Where the two efforts converge.** Both plans agree on the strategic shape:
bounded per-worker concurrency, fleet-aware dispatcher admission, per-submission
windowing, durable submission accept, worker-side cancel primitive, and a
beefed-up observability surface. The two plans almost word-for-word agree on the
seven items at the core of the reliability story.

**Where the run-2 plan adds new findings.** The run-2 follow-up surfaces five
items the audit doesn't address because they are below the SDK layer:

- The `block_in_place(Handle::current().block_on(async))` cascade at
  `packages/plugin-core/src/traits.rs:333` — the single line whose fix collapses
  the 866-thread futex storm into a no-op. Correction A and D of the run-2 doc.
- The `operation_result` consumer at
  `packages/server/src/consumers/operation_result.rs:9-64` running with
  `concurrency = None` (strictly serial), capping return-path throughput at ~200
  msg/s/replica. Correction E.
- Worker-side performance wins (reflink for cache→sandbox copy, cache size,
  `exists()` short-circuit, parallel input load) that the audit does not touch.
- Cross-worker compile coordination via a leader-election primitive in the
  worker cache subsystem (not in the audit at all).
- The choice of bridge fix shape (`spawn_blocking` vs every host-fn rewrite) —
  the run-2 verification rounds D and F show only one line at `traits.rs:333`
  needs to change; all nested `block_in_place` calls inside `host_funcs/*.rs`
  collapse to no-ops automatically once their caller is no longer on a
  multi-thread runtime worker.

**Where the two plans disagree.** Three substantive disagreements, all resolved
in §D below: (1) durable enqueue mechanism (new Redis MQ queue vs PG SKIP LOCKED
on the existing row); (2) dispatcher admission shape (per-plugin-id semaphore vs
fleet-aware single semaphore); (3) cross-worker compile coordination (PG
advisory lock vs Redis-leader-election in the cache subsystem). For each, the
consolidated plan picks one explicitly.

**Relationship to current `master`.** Phase 0 of the audit has shipped: the
`release/` env templates default to `object_storage`, Postgres
`max_connections=400`, server pool 50, worker pool 5, Redis 6 GiB / noeviction,
and the install.sh / README guidance is inverted. Nothing past Phase 0 has
landed on `master`: worker still calls `mq.process_messages(.., None, ..)`, no
lease columns exist on `submission`/`code_run`, the Verdict enum has no
`Cancelled` variant, no `traits.rs:333` fix is in tree, no dispatcher semaphore.

**Worktree status (audited).** The `judging-reliability-phase1-recovered`
worktree contains ~86 uncommitted files implementing Phase 1, Phase 1b, and 9 of
11 Phase 2 PRs. That work is functionally present but **not PR-staged, not
merged, not covered by integration tests for the new behavior**. The integrated
release sequencing in §E therefore begins with a staging step: decompose the
worktree into ~20 atomic PRs matching the PR-A through PR-Y identifiers in the
source impl plans, write integration tests as each PR is staged, then merge in
dependency order. Phase A.5 items (14b–14h) and all Phase D / E / F / G work is
genuinely new code, not in any worktree.

---

## B. Audit findings — cross-reference

The audit's §1 enumerates 19 numbered findings; each must be touched by exactly
one item below or flagged as an open gap. Numeric §1.X refers to the audit;
numeric item N refers to §C.

| Finding | One-line restatement                                                                                                          | Addressed by                                                                                                                                                                                                                |
| ------- | ----------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| §1.0    | Default deployment routed blob traffic through Postgres.                                                                      | **Shipped in Phase 0.** Codification only — no item in §C.                                                                                                                                                                  |
| §1.1    | Worker effective concurrency is 1 task per process, `max_concurrency: None` in heartbeat.                                     | Items **6, 7** (Phase A worker config + heartbeat plumb-through).                                                                                                                                                           |
| §1.2    | 9 raw `tokio::spawn(dispatch_to_plugin)` sites with no semaphore.                                                             | Item **27** (Phase C, PR-S).                                                                                                                                                                                                |
| §1.3    | Testcase fan-out spawns N futures per submission, no per-submission window.                                                   | Items **14b** (Phase A.5 server-side host-fanout semaphore) **+ 28** (Phase C plugin-side windowing, PR-T) — see §D.5.                                                                                                      |
| §1.4    | Operation fan-out is one Redis publish per testcase, no batching.                                                             | Items **14c** (publish parallelism), **28** (windowing reduces inbound rate), **34** (bulk `insert_results` reduces back-side churn). Architectural batching of _outbound_ publishes deferred to Phase G as optional.       |
| §1.5    | `result_timeout_ms` floor of 15 min is the _only_ execution timeout — queue wait is silently charged against testcase budget. | Item **40** (Phase D, separate queue-wait from execution timeout).                                                                                                                                                          |
| §1.6    | Multi-server result routing is correct but in-memory only; result consumer is also serial.                                    | Items **3** (consumer concurrency=8, Phase A), **21–22** (Phase B sweeper for ghost reply queues).                                                                                                                          |
| §1.7    | DB pressure: pool sizes and per-operation churn.                                                                              | **Phase 0 shipped.** No §C item; future scaling captured in §4 of the audit.                                                                                                                                                |
| §1.8    | Worker file cache size 512 MiB is too small for realistic working sets.                                                       | Item **10** (bump default to 4 GiB, Phase A).                                                                                                                                                                               |
| §1.9    | Compile cache is per-step but `toolchain_fingerprint` is the empty string.                                                    | Item **8** (Phase A correctness fix).                                                                                                                                                                                       |
| §1.10   | `Pending`/`Running` semantics are wrong for queueing; both treated identically by stuck detector.                             | Item **42** (Phase E status semantics, audit Phase 4).                                                                                                                                                                      |
| §1.11   | Stuck-job detector is reactive, 60s scan, 2h timeout, no automatic retry.                                                     | Items **5** (Phase A interim re-dispatch), **14f** (Phase A.5 `updated_at` bug fix), **19** (Phase B PR-E: raise to 6h wide-net), **43** (Phase E per-state thresholds).                                                    |
| §1.12   | Plugin `Mutex<Plugin>` is per-instance; evaluator pool small.                                                                 | Items **27, 29** (Phase C admission caps inbound pressure; fleet-aware admission via heartbeats). Deep fix (contest-plugin instance pool) deferred to Phase H item 61.                                                      |
| §1.13   | Heartbeat sound, but `max_concurrency: None` hard-coded.                                                                      | Item **7** (Phase A).                                                                                                                                                                                                       |
| §1.14   | No backpressure at API.                                                                                                       | Items **27** (PR-S 503 on dispatcher overflow), **39** (Phase D queue-depth backpressure on POST).                                                                                                                          |
| §1.15   | Server death has no recovery beyond 2-h timeout; reply queues ghost.                                                          | Items **15–23** (entire Phase B / audit Phase 1b).                                                                                                                                                                          |
| §1.16   | Cancel paths are server-side only; in-flight worker compute is uninterruptible.                                               | Items **24, 25, 26** (Phase C cancel primitive: PR-K, PR-L, PR-M). Item **30** (ICPC plugin-side fast-path) closes the loop.                                                                                                |
| §1.17   | IOI compile-error path leaves missing test_case_result rows; ICPC fills them.                                                 | Item **44** (Phase E — audit folds this into the Phase 1 SDK/plugin "ride this phase" bundle; consolidating into Phase E because it groups naturally with §1.18).                                                           |
| §1.18   | Submission score misleading for non-terminal states under Sum scoring.                                                        | Item **45** (Phase E — audit Phase 4 score-display fix).                                                                                                                                                                    |
| §1.19   | No execution-time skipping — multiple sub-issues (a/b/c/d).                                                                   | Items **31** (§1.19a IOI testcase filter, PR-X), **32** (§1.19b IOI per-subtask short-circuit, PR-Y), **24–26, 30** (§1.19c covered by cancel primitive + ICPC short-circuit), **46** (§1.19d doc-only clarifying comment). |

No audit finding is left without an item. Items 44 and 45 are highlighted as
**previously not in either source plan** — the audit cites them but the impl
plans don't pull them in, and the run-2 plan is silent on them. They get
explicit Phase E rows.

---

## C. Unified delivery plan

Each item is numbered continuously through the document. Effort tags: **small**
≈ hours; **medium** ≈ 1–3 days; **large** ≈ 1+ weeks. "Source" cites both parent
plans where both touch the work.

### Phase A — runtime-cascade fix and mechanical wins (1–2 days)

These are the items merged first (in PR-review order) because they unblock the
integration test harness — without item 1, the integration test wave can't run
long enough to exercise items 11+. Per §E, "merged first" does not mean "shipped
first"; the integrated release lands all phases together.

| #   | Title                                                                                                                                                                                                                                       | Source                                                                                                                                 | File:line target                                                                                                                                                           | Depends on                                                      | Effort | Rationale                                                                                                                                                                                                                                    |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Replace `block_in_place(Handle::block_on(...))` with `spawn_blocking` at the plugin call boundary. Propagate `Span::current()` into the closure. Map `JoinError::is_panic()` to `PluginError::ExecutionFailed`.                             | run-2 Phase A #1; audit §1.2 mentions the lock-contention symptom but not the bridge.                                                  | `packages/plugin-core/src/traits.rs:333`                                                                                                                                   | —                                                               | small  | The single line that breaks the 866-thread futex cascade. Per run-2 Correction D, nested `block_in_place(..)` calls in `host_funcs/*.rs` auto-collapse to no-ops once their caller is on the blocking pool, so this is genuinely one change. |
| 2   | Move from `#[tokio::main]` to an explicit `tokio::runtime::Builder` in `packages/server/src/main.rs` to control `max_blocking_threads=1024`.                                                                                                | run-2 Phase A #2.                                                                                                                      | `packages/server/src/main.rs` (entrypoint), new helper in `packages/common/src/runtime.rs`                                                                                 | —                                                               | small  | We can't raise the cap without owning the builder. 1024 gives 2× headroom over our worst-observed blocking-thread count after item 1 makes the cascade impossible.                                                                           |
| 3   | `operation_result` consumer `concurrency = Some(8)` so result-path throughput is no longer 200 msg/s/replica.                                                                                                                               | run-2 Phase A #3; audit §1.6 acknowledges the serial consumer.                                                                         | `packages/server/src/consumers/operation_result.rs:9`                                                                                                                      | —                                                               | small  | Handler is concurrency-safe (DashMap waiter table + oneshot send). Drops the api-side bottleneck identified in run-2 Correction E.                                                                                                           |
| 4   | Drop the `PluginError::PoolTimeout → AppError::RateLimited (429)` mapping. Plugin-pool contention is an internal concern.                                                                                                                   | run-2 Phase A #4; audit §2.1 ("the synchronous-hook path is the one place left where contention can surface as a user-visible error"). | `packages/server/src/error.rs:206-209`                                                                                                                                     | item 1 (so contention is genuinely rare before this is exposed) | small  | Once item 1 lands and the retry-loop ships from the prior incident, 429 from contention is wrong shape. Keep 429 only for actual rate-limit policy.                                                                                          |
| 5   | Replace `stuck.rs:167-173`'s `mark_submission_system_error` with `tokio::spawn(dispatch_submission_to_plugin(...))` re-dispatch (quick interim form; superseded by Phase B lease/steal items 17–18 and Phase D durable accept items 36–38). | run-2 Phase A #5; audit §1.11.                                                                                                         | `packages/server/src/dlq/stuck.rs:167-173`                                                                                                                                 | —                                                               | small  | Audit Phase 1b will replace this with lease/steal. In the interim, even an in-process re-dispatch is better than tagging SystemError after 2h.                                                                                               |
| 6   | Add `worker.max_concurrency: usize` to `WorkerConfig` (default `1` in all configurations, per audit §3.12).                                                                                                                                 | audit Phase 1; run-2 Phase A #6.                                                                                                       | `packages/worker/src/config.rs`                                                                                                                                            | —                                                               | small  | Per run-2 Correction F, effective concurrency _is already 1_ — this item makes it explicit and configurable, not changes behavior.                                                                                                           |
| 7   | Plumb `max_concurrency`, `current_in_flight`, `fairness_mode` through the worker heartbeat (`HeartbeatConfig.max_concurrency` is no longer `None`).                                                                                         | audit Phase 1; run-2 Phase A #6–7.                                                                                                     | `packages/worker/src/heartbeat.rs:75`, `packages/worker/src/main.rs:111`                                                                                                   | item 6                                                          | small  | Needed for fleet-aware admission (item 29). The field exists in the heartbeat schema; only the populate site is missing.                                                                                                                     |
| 8   | Populate `toolchain_fingerprint` from `g++ --version`, `python3 --version`, etc. at worker boot; hash into cache key.                                                                                                                       | audit §1.9; run-2 Phase A #7.                                                                                                          | `packages/worker/src/models/operation/executor.rs:27`                                                                                                                      | —                                                               | small  | **Correctness bug today** — compiler upgrade silently serves stale cached binaries. Ship before anything that exercises compile cache.                                                                                                       |
| 9   | Reflink/hardlink fast-path in `file_cacher.rs:213-220`. Try `std::os::unix::fs::link` first; fall back to `tokio::fs::copy`.                                                                                                                | run-2 Phase A #8.                                                                                                                      | `packages/worker/src/models/file_cacher.rs:213`                                                                                                                            | —                                                               | small  | Biggest worker-side win for Signpost (27 MB → 1 ms vs 200 ms).                                                                                                                                                                               |
| 10  | Bump `max_cache_size` default to 4 GiB.                                                                                                                                                                                                     | audit §1.8; run-2 Phase A #11.                                                                                                         | `packages/worker/src/config.rs:79-82`                                                                                                                                      | —                                                               | small  | Disk is plentiful on the 16-GB worker droplets. 512 MiB thrashes on Signpost-class working sets.                                                                                                                                             |
| 11  | `exists()` short-circuit in `upload_from_path` (HEAD probe before stream).                                                                                                                                                                  | run-2 Phase A #12.                                                                                                                     | `packages/worker/src/models/file_cacher.rs:278-314`                                                                                                                        | —                                                               | small  | Saves SeaweedFS bandwidth when worker re-uploads an already-present blob.                                                                                                                                                                    |
| 12  | Cache hit/miss + size + eviction metrics (`blob_cache_hits_total`, `blob_cache_misses_total`, `blob_cache_size_bytes`, `blob_cache_evictions_total`).                                                                                       | run-2 §4.2 worker section; audit §3.10 (observability).                                                                                | `packages/common/src/metrics.rs`, `packages/worker/src/models/file_cacher.rs:155-220`                                                                                      | —                                                               | small  | Run-2 had no visibility into cache hit ratio. These metrics tell us whether item 10 sized the cache right.                                                                                                                                   |
| 13  | `host_fn_duration` + `host_fn_calls_total` + `host_fn_block_in_place_total` (regression guard — should be zero after item 1).                                                                                                               | run-2 §4.2 plugin call hot path; audit §3.10.                                                                                          | `packages/common/src/metrics.rs`, every `host_funcs/*.rs` `_fn` callback.                                                                                                  | —                                                               | small  | After item 1, any `host_fn_block_in_place_total > 0` reading means a regression. Cheap CI guard.                                                                                                                                             |
| 14  | `Verdict::Cancelled` enum variant + stress-test mirror gap fix (PR-J wire-up only; no consumer logic yet).                                                                                                                                  | audit §1.16; Phase 2 PR-J.                                                                                                             | `packages/server-sdk/src/types/verdict.rs:5-15`, `packages/common/src/submission_status.rs:114`, `packages/stress-test/src/dto.rs:205`, plus 5 match sites listed in PR-J. | —                                                               | small  | Land additively in Phase A so all downstream PRs can `match Verdict::Cancelled` without compile errors. No worker behavior changes here.                                                                                                     |

Phase A total effort: ~1–2 days for one engineer.

#### Phase A silent-error remediation

The submission dispatch path has 10
`let _ = mark_submission_dispatch_system_error(...)` sites in
`packages/server/src/services/submission_dispatch.rs` where dispatch failures
silently bin a submission as SystemError with no operator-visible signal. Bundle
their remediation with the Phase A.5 hoist below so the new failure modes opened
by the cascade fix don't immediately re-fail silently.

| #   | Title                                                                                                                                                                                                                             | File:line target                                                 | Effort |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- | ------ |
| 14a | Replace each `let _ = mark_submission_dispatch_system_error(...)` with explicit `match` arms that log at `error!` with `submission.id`, `error.code`, `error.message`, and `submission_dispatch_failure_total` counter increment. | `packages/server/src/services/submission_dispatch.rs` (10 sites) | small  |

### Phase A.5 — server-side fan-out hoist and stabilization (~3–5 days)

**Why this section exists.** Agent C's critical-review pass refuted a
load-bearing assumption in the original consolidation: that SDK-level windowing
(item 28, Phase C) gates the server-side fan-out and thus relieves
batch-evaluator pool saturation. It does not.
`packages/server/src/services/evaluate_batch.rs:84-185` spawns N futures
_immediately_ at the host-fn entry, before any plugin-visible windowing can
intervene. The actual saturation driver is server-side, and the fix has to live
in the server. This phase lands the server-side hoist plus the supporting
stabilization that pairs naturally with the Phase A cascade fix.

| #   | Title                                                                                                                                                                                                                                                                                                                               | Source                                  | File:line target                                                                                                                              | Depends on                                                                            | Effort | Rationale                                                                                                                                                                                                                                                            |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 14b | Server-side bounded fan-out in `start_evaluate_batch`. Acquire from a per-batch-or-fleet `Semaphore` _before_ spawning each per-testcase future. Pass acquired permits into the spawned task and release on result publish. Default `server.batch_evaluator_fanout_concurrency = 64` per server.                                    | run-2 §2.3; Agent C critical review §C. | `packages/server/src/services/evaluate_batch.rs:84-185` (the spawn loop), new helper in `packages/server/src/dispatcher/fanout.rs`            | item 1 (cascade fix must land before the hoist exposes a different contention point)  | medium | This is the actual fix for batch-evaluator pool saturation. Item 28's per-submission windowing reduces _plugin-side_ polling pin time; this reduces _host-side_ fan-out pressure. They are independent corrections, both needed.                                     |
| 14c | Publish parallelism in `packages/server/src/services/operation_batch.rs:70-136`. Replace serial publish loop with `try_join_all` of `publish_one(op)` calls; bound to `server.operation_batch_publish_concurrency` (default 32).                                                                                                    | Agent C critical review §C.             | `packages/server/src/services/operation_batch.rs:70-136`                                                                                      | item 14b (so the fan-out semaphore is in place before publish becomes the bottleneck) | small  | Publish path was implicitly serial because each op was awaited before the next was published. With item 14b reducing the cap on concurrent ops, serial publish bounds throughput unnecessarily.                                                                      |
| 14d | Transactional waiter insertion. Insert the result-waiter entry into the `DashMap` registry _before_ publishing the operation to the MQ, not after — so a fast worker response cannot arrive before the waiter is registered.                                                                                                        | Agent C critical review §C.             | `packages/server/src/services/operation_batch.rs:73` (publish_one), `packages/server/src/host_funcs/dispatch.rs` (waiter table mutation site) | —                                                                                     | small  | Today the publish happens, _then_ the waiter is inserted. With a hot worker pool and a slow tokio scheduler tick, the result can arrive at the consumer before the waiter exists, causing a `WaiterNotFound` log and a 60 s timeout. The fix is purely a reordering. |
| 14e | `/healthz` and `/metrics` on a dedicated tokio runtime + listener. Moved up from Phase C item 35 so the Phase A.5 stress run can prove the cascade fix actually worked.                                                                                                                                                             | run-2 §4.1; Agent C §C.                 | `packages/server/src/main.rs`, `packages/server/src/observability.rs`                                                                         | item 2 (explicit runtime builder is the natural seam)                                 | small  | Run-2 lost 25 min of metrics + healthz during the deadlock. Even after item 1 makes deadlock unlikely, segregating observability traffic from user traffic is correct architecture and lets us trust the post-fix measurements.                                      |
| 14f | Stuck-detector switched from `created_at` to `updated_at`. Fix the long-standing bug at `packages/server/src/dlq/stuck.rs:54` that causes a submission with active retries to still be flagged stuck purely because creation was long ago.                                                                                          | Agent C critical review §C.             | `packages/server/src/dlq/stuck.rs:54`                                                                                                         | —                                                                                     | small  | One-line bug. Caught in audit too but never scheduled — pulling it forward because it interacts with the new lease columns landing in Phase B and we don't want two semantics co-existing.                                                                           |
| 14g | CI regression-guard: `host_fn_block_in_place_total` Prometheus counter must read `0` after a 60 s integration-test stress wave. If non-zero, fail the build with file/line offender hint.                                                                                                                                           | Agent C critical review §C.             | `packages/common/src/metrics.rs` (definition lands in item 13), `packages/server/tests/integration/regression_block_in_place.rs` (new)        | items 1, 13                                                                           | small  | Prevents accidental reintroduction of `block_in_place(Handle::block_on(...))` in a future host-fn refactor. Cheap and catches the highest-cost regression vector.                                                                                                    |
| 14h | Bundle the sync-hook retry-with-backoff change with item 4 (`PoolTimeout → RateLimited` removal). Without retry, item 4 alone widens the 5xx window because every contended hook call now surfaces as a 500 instead of a 429. The retry loop already exists for the synchronous-evaluator path; copy that pattern to the hook path. | Agent C critical review §C.             | `packages/server/src/host_funcs/hooks.rs` (or wherever the sync-hook caller lives), shared retry helper in `packages/common/src/retry.rs`     | item 4                                                                                | small  | Without this bundling, Phase A item 4 is a small regression. With it, the hook path matches the synchronous-evaluator path's contention behavior and the public API contract is maintained.                                                                          |

Phase A.5 total effort: ~3–5 working days. Items 14b–14d are the load-bearing
trio; 14e–14h are smaller corrections that ride naturally.

### Phase B — lease/steal recovery (audit Phase 1b, ~1 week)

Restores correctness after a server restart in under 75 s instead of 2 h. All
items below are PRs from `2026-05-08-phase-1b-impl.md`; the table preserves the
PR-A through PR-I sequencing.

| #   | Title                                                                                                                                                                                                                                                                                          | Source                                                              | File:line target                                                                                                                                                                        | Depends on       | Effort | Rationale                                                                                                                                                  |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 15  | Schema migration: add `owner_server_id varchar(128) NULL`, `lease_heartbeat_at timestamptz NULL`, `retry_count int NOT NULL DEFAULT 0` to `submission` and `code_run`. Index on `(status, lease_heartbeat_at)`.                                                                                | audit Phase 1b PR-A.                                                | `packages/server/src/entity/submission.rs`, `packages/server/src/entity/code_run.rs`                                                                                                    | —                | small  | SeaORM schema-sync handles it. Additive; rollback = stop reading the columns.                                                                              |
| 16  | Server heartbeat module (`broccoli:server:heartbeat:<id>` SET EX 30 s, refresh every 10 s). Tighten `resolve_server_id` with `expects_multi_replica` flag.                                                                                                                                     | audit Phase 1b PR-B; ghost-reply-queue handling per audit §3.5.     | `packages/server/src/heartbeat.rs` (new), `packages/server/src/main.rs`, `packages/server/src/config.rs:198-261`                                                                        | item 15          | medium | Mirror of `packages/worker/src/heartbeat.rs`. Needed for ghost-queue debounce in items 21–22 (sweeper).                                                    |
| 17  | Lease refresher (background task, every 10 s, one batch UPDATE per server).                                                                                                                                                                                                                    | audit Phase 1b PR-C.                                                | `packages/server/src/dispatcher/{mod,lease}.rs` (new), `packages/server/src/main.rs`                                                                                                    | items 15, 16     | medium | One UPDATE per tick; cheap. Needed before steal scanner (item 18) so we can verify owned-by-self rows are not stolen.                                      |
| 18  | Steal scanner with `SELECT ... FOR UPDATE SKIP LOCKED LIMIT 8`, in-txn `DELETE FROM test_case_result WHERE submission_id IN (...)`, retry_count++, re-dispatch via existing plugin path. `retry_count > 5` → SystemError + DLQ with `DISPATCH_RETRY_EXHAUSTED`.                                | audit §3.5 + Phase 1b PR-D.                                         | `packages/server/src/dispatcher/steal.rs` (new), `packages/server/src/handlers/submission.rs` (refactor `dispatch_to_plugin` to be callable from outside the create-submission handler) | items 15, 16, 17 | large  | The single most complex Phase B PR. PG-side `SKIP LOCKED` requires raw SQL via `Statement::from_sql_and_values` — SeaORM doesn't generate it.              |
| 19  | Stuck-detector wide-net: raise `stuck_job_timeout_secs` default to 21600 (6h). Add `Queued` query for `owner_server_id IS NULL` older than 5 min → `DISPATCHER_DEAD`. **Builds on item 14f's `updated_at` fix** — that lands first so the new timeout interacts with correct timing semantics. | audit Phase 1b PR-E.                                                | `packages/server/src/dlq/stuck.rs`, `packages/common/src/config.rs`                                                                                                                     | items 14f, 18    | small  | Replaces the time-based primary recovery role; lease/steal is the primary path.                                                                            |
| 20  | Worker dedup TTL decoupled from `stuck_job_timeout_secs`. Add `worker.dedup_ttl_secs` (default 600 s).                                                                                                                                                                                         | audit Phase 1b PR-F + Phase 2 PR-DD (Phase 1b inheritance cleanup). | `packages/common/src/config.rs`, `packages/worker/src/dedup.rs`, `packages/worker/src/main.rs:68-77`                                                                                    | —                | small  | If item 19 raises the stuck-detector timer to 6 h without this split, dedup detects stuck/dead other-workers in 6 h instead of minutes. Pure config split. |
| 21  | Reply-queue sweeper scaffolding: SCAN MATCH `operation_results.*`, two-stage `dead_since` debounce, log-only (no DEL). Add `sweeper_dry_run: bool` (default true).                                                                                                                             | audit Phase 1b PR-G.                                                | `packages/server/src/dispatcher/sweeper.rs` (new)                                                                                                                                       | item 16          | medium | Lets us soak the discovery + debounce logic before risking a wrong delete.                                                                                 |
| 22  | Reply-queue sweeper DEL path (flip dry-run flag once soaked).                                                                                                                                                                                                                                  | audit Phase 1b PR-H.                                                | `packages/server/src/dispatcher/sweeper.rs`                                                                                                                                             | item 21          | small  | Two-stage rollout: 21 ships discovery; 22 ships activation.                                                                                                |
| 23  | Feature flag flip in `release/.env.server.example`: `server.dispatcher.lease_steal_enabled = true`.                                                                                                                                                                                            | audit Phase 1b PR-I.                                                | `release/.env.{,server}.example`, `release/docs/operator-runbook.md`                                                                                                                    | items 15–22      | small  | Operations decision, not a code change. Per-deployment opt-in via custom env files.                                                                        |

Phase B total effort: ~5–8 working days.

### Phase C — admission, windowing, cancel primitive (audit Phase 2, ~1–2 weeks)

This phase removes the 1000-spike OOM risk and makes ICPC short-circuit / Phase
1b steal recovery actually save worker compute. The items below are PRs from
`2026-05-08-phase-2-impl.md` plus three additions from run-2 / Agent C review
(item 33 leader-election, item 34 bulk `insert_results`, item 35 recursive
checker hazard).

| #   | Title                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Source                                                                                          | File:line target                                                                                                                                                        | Depends on                                       | Effort         | Rationale                                                                                                                                                                                                                                                                                    |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 24  | Cancel host functions: `cancel_evaluate_batch_fn` now sets `broccoli:cancel:batch:<batch_id>` (EX 21600) **before** mutating the in-process map and walks `EvaluateBatchOpsRegistry` to bulk-set `cancel:op:<task_id>`. New `cancel_evaluate_test_cases_fn(batch_id, task_ids)` writes per-op cancel keys via one Lua `EVAL`. Validates batch ownership. Add new `EvaluateBatchOpsRegistry` module.                                                                                                                                                                                                                                                                                                                                     | audit §1.16 + Phase 2 PR-K.                                                                     | `packages/server/src/host_funcs/{evaluate,dispatch}.rs`, `packages/server/src/host_funcs/evaluate_ops_registry.rs` (new)                                                | item 14                                          | large          | The primitive that makes §1.16, §1.19b, and Phase 1b steal recovery share one mechanism. Lua `EVAL` is atomic from the worker's perspective.                                                                                                                                                 |
| 25  | Worker EXISTS check + `Cancelled` fast-path: pipeline `EXISTS broccoli:cancel:batch:<batch_id>` and `EXISTS broccoli:cancel:op:<task_id>` with the existing dedup claim before sandbox setup. If either is set, emit `TaskResult { verdict: Cancelled }` and skip execution.                                                                                                                                                                                                                                                                                                                                                                                                                                                            | audit §1.16 + Phase 2 PR-L.                                                                     | `packages/worker/src/cancel.rs` (new), `packages/worker/src/models/operation/handler.rs`, `packages/server-sdk/src/evaluator/interpret.rs`                              | items 14, 24                                     | medium         | Must benchmark to confirm <1 ms p99 added to op-start. Use criterion.                                                                                                                                                                                                                        |
| 26  | SDK wrapper `host.eval.cancel_evaluate_test_cases(batch_id, task_ids)`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Phase 2 PR-M.                                                                                   | `packages/server-sdk/src/host/raw.rs`, `packages/server-sdk/src/sdk/eval.rs`                                                                                            | item 24                                          | small          | Mirror of existing `cancel_batch` shape.                                                                                                                                                                                                                                                     |
| 27  | Dispatcher semaphore + 503 OVERLOADED. Wrap all 9 outer `tokio::spawn(dispatch_to_plugin*)` sites with `DispatcherSemaphore::acquire_owned()`. Add `server.dispatcher_concurrency` (default 16) and `server.max_queued_submissions` (default 100 × n_machines). Return 503 with `Retry-After` when exceeded.                                                                                                                                                                                                                                                                                                                                                                                                                            | audit Phase 2 (a); Phase 2 PR-S. **NB the impl plan revises the audit's "8 sites" count to 9.** | 5 sites in `packages/server/src/handlers/submission.rs`, 2 in `code_run.rs`, 2 in `dlq.rs`; new `packages/server/src/dispatcher/permits.rs`                             | —                                                | medium         | First-week ship for OOM protection. Pick over run-2's per-plugin-id semaphore (see §D) because fleet-wide admission generalizes cleanly to multiple contest plugins without per-id bookkeeping.                                                                                              |
| 28  | Windowing SDK helper `host.eval.start_windowed(input, window_size)`. ICPC opts in with `W=1` (sequential); IOI opts in with `W=4`. Plugins not opted-in keep current behavior (`window_size = u32::MAX` is the fallback). **Pairs with Phase A.5 item 14b — windowing alone does not reduce batch-evaluator host-side fan-out (see §D.5 below).**                                                                                                                                                                                                                                                                                                                                                                                       | audit Phase 2 (b); Phase 2 PR-T.                                                                | `packages/server-sdk/src/evaluator/run.rs`, `packages/server-sdk/src/sdk/eval.rs`, `plugins/{ioi,icpc}/src/evaluate*.rs`                                                | item 14b                                         | medium         | The plan ships windowing per-submission and stays fleet-aware via item 29. Windowing's actual win is reducing the in-plugin polling pin (`next_result` slot-time) and bounding the _plugin-visible_ number of in-flight testcases — not the _host-visible_ fan-out, which is item 14b's job. |
| 29  | Fleet-aware admission. `FleetCapacityMonitor` polls `broccoli:worker:heartbeat:*` every 5 s, sums live workers' `max_concurrency`, atomically resizes `evaluator_slots` (replaces `available_parallelism()`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | audit §3.3; Phase 2 PR-V.                                                                       | `packages/server/src/dispatcher/fleet_capacity.rs` (new), `packages/server/src/host_funcs/mod.rs:235-239`, `packages/server/src/main.rs`                                | item 7 (heartbeat must report `max_concurrency`) | medium         | Picks the fleet-wide shape over the per-plugin-id semaphore variant — same rationale as item 27.                                                                                                                                                                                             |
| 30  | ICPC plugin: treat `Verdict::Cancelled` like `Verdict::Skipped` in the short-circuit fill loop. Don't insert duplicate rows.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | Phase 2 PR-W.                                                                                   | `plugins/icpc/src/evaluate.rs:185-203`                                                                                                                                  | item 14                                          | small          | The plugin side of item 25's fast-path.                                                                                                                                                                                                                                                      |
| 31  | IOI testcase filtering: `compute_scoring_test_case_ids(subtask_defs, test_cases)` excludes `is_sample=false ∧ score=0.0 ∧ tc.id ∉ ⋃ subtask_defs[*].test_cases`. Samples kept (diagnostic value).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | audit §1.19a; Phase 2 PR-X.                                                                     | `plugins/ioi/src/subtasks.rs`, `plugins/ioi/src/judge.rs`                                                                                                               | —                                                | small          | Saves <10% of worker compute typically, but cheap fix.                                                                                                                                                                                                                                       |
| 32  | IOI per-subtask short-circuit (GroupMin/GroupMul). Track per-subtask running tally; on threshold hit, build cancellable task_ids (excluding ops still needed by sibling active subtasks), call `cancel_evaluate_test_cases`, insert `Skipped` rows.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | audit §1.19b; Phase 2 PR-Y.                                                                     | `plugins/ioi/src/evaluate_batch.rs`, `plugins/ioi/src/judge.rs`, `plugins/ioi/src/subtasks.rs`                                                                          | items 24, 25, 26, 28, 31                         | large          | Most complex Phase C PR. The "still-needed-by-active-sibling" check is the subtle correctness condition; cover with nested-subtask test.                                                                                                                                                     |
| 33  | Leader-election in worker cache subsystem for cross-worker compile coordination (Redis SETNX with TTL, leader heartbeat every 5 s, followers wait on `task_cache` poll).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | run-2 §3.2 C + Phase B #17 (revised from PG advisory lock or "compile on server").              | `packages/worker/src/models/operation/{handler,task_cache}.rs`                                                                                                          | —                                                | medium         | See §D for why this design replaces the PG advisory lock and "compile-on-server" proposals from earlier drafts. Generic over cache step type.                                                                                                                                                |
| 34  | Bulk `insert_results` in plugins (currently calls multi-row endpoint with single-element slices).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | run-2 §2.5 + Phase B #18; audit §1.4 calls out the publish-per-testcase pattern.                | `plugins/icpc/src/evaluate.rs:303`, `plugins/ioi/src/evaluate_batch.rs` similar site, `packages/server-sdk/src/sdk/submissions.rs:158-164` (already supports multi-row) | —                                                | small          | Cuts 20 INSERTs per submission down to 2–3. Multi-row endpoint already exists.                                                                                                                                                                                                               |
| 35  | **Recursive plugin invocation hazard at `packages/server/src/host_funcs/checker.rs:85`.** When the contest plugin calls into the checker plugin during evaluation, the checker pool size _must be ≥ contest pool size_ to avoid a self-deadlock (contest holds its slot, requests a checker slot, but checker pool exhausted by other in-flight checker calls). Until this is sized correctly, item 25's `Cancelled` fast-path can deadlock if it invokes a checker. **Architectural decision (config-bound this release; structural move-to-worker deferred):** size pools accordingly via config (1-line change), document the constraint in `release/docs/operator-runbook.md`. Move-to-worker refactor remains a Phase H candidate. | Agent C critical review §C; related to audit §1.12 (plugin Mutex sizing).                       | `packages/server/src/host_funcs/checker.rs:85`, `packages/server/src/config.rs` (pool sizing), `release/docs/operator-runbook.md`                                       | —                                                | small (config) | The hazard is real but quiescent under current workloads. The config bound ships this release; the Phase H move-to-worker refactor is scheduled only if we ever ship a contest plugin that calls checker on every testcase under high cancel pressure.                                       |

Phase C total effort: ~8–14 working days. With parallelization (cancel chain
items 24–26 + admission items 27/29 + windowing item 28 on separate engineers):
~6–8 days.

### Phase D — durable submission accept (audit Phase 3, ~1 week)

Submission persisted in PG with `status='Queued'`; polling fiber dispatches via
`SKIP LOCKED`. Builds entirely on the Phase B lease columns — see §D for why we
chose PG `SKIP LOCKED` over a new Redis MQ queue.

| #   | Title                                                                                                                                                                                                                                                                                 | Source                | File:line target                                                                                                                                                                 | Depends on                                                                                              | Effort | Rationale                                                                                                                                                                                                                   |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 36  | Add `Queued` to `SubmissionStatus` enum; update `IN_PROGRESS_STATUSES` in stuck detector.                                                                                                                                                                                             | audit Phase 3.        | `packages/common/src/submission_status.rs`, `packages/server/src/dlq/stuck.rs:17-21`                                                                                             | item 19 (so stuck-detector treats `Queued` distinctly)                                                  | small  | Additive enum variant; existing rows never contain it.                                                                                                                                                                      |
| 37  | Submission API: replace `tokio::spawn(dispatch_to_plugin)` with `INSERT INTO submission(..., status, ...) VALUES (..., 'Queued', ...)`, return 201. No in-process spawn.                                                                                                              | audit Phase 3.        | 5 sites in `packages/server/src/handlers/submission.rs`, 2 in `code_run.rs`, 2 in `dlq.rs` (already inventoried by PR-S).                                                        | item 36, item 27 (dispatcher already bounded so the in-memory spawn is a safe fallback during flag-off) | medium | Closes the silent-loss gap: an api process death after `tokio::spawn` no longer loses the submission.                                                                                                                       |
| 38  | Claim fiber: per-server background task polls every 1 s with `SELECT ... FROM submission WHERE status='Queued' ORDER BY created_at LIMIT $bounded FOR UPDATE SKIP LOCKED`. In-txn: UPDATE to `Pending`, set `owner_server_id`, bump `retry_count`, commit, dispatch through plugin.   | audit Phase 3.        | `packages/server/src/dispatcher/claim.rs` (new), `packages/server/src/main.rs`                                                                                                   | items 36, 37                                                                                            | medium | Bounded by `dispatcher_concurrency` (item 27). Reuses lease columns from item 15.                                                                                                                                           |
| 39  | Backpressure on POST: if `COUNT(*) WHERE status='Queued' > max_queued_submissions` (default 5000), return 503 with `Retry-After`. Cached every 5 s.                                                                                                                                   | audit §3.8 + Phase 3. | `packages/server/src/handlers/submission.rs`, new helper in `packages/server/src/state.rs`                                                                                       | item 38                                                                                                 | small  | Explicit reject is the user-stated requirement: "1000 simultaneous → server stable, durable queue grows, drain over time." 503 is intentional, not surprising.                                                              |
| 40  | Separate queue-wait from execution timeout for `result_timeout_ms`. Plugins' `next_result` polling sees only execution time; queue wait is invisible from the plugin's perspective.                                                                                                   | audit §1.5.           | `packages/server-sdk/src/types/evaluate.rs:7-8` (timeout policy doc + implementation), `packages/server/src/host_funcs/evaluate.rs:564` (clock semantics for the result-rx wait) | item 27 (admission ensures queue stays bounded)                                                         | medium | Without items 27, 28, 38, queue wait is silently charged against the testcase budget — the audit's §0 flaw #3. Once admission bounds queue depth, this becomes a small SDK-policy change rather than a correctness rewrite. |
| 41  | Stuck-job detector → re-enqueuer (durable version of item 5). When the steal scanner hits `retry_count > 5`, the row goes to SystemError + DLQ; otherwise the lease/steal path handles re-dispatch automatically. The wide-net 6 h backstop in item 19 catches plugin-hang pathology. | audit §3.5 + Phase 3. | `packages/server/src/dlq/stuck.rs`                                                                                                                                               | items 18, 36, 38                                                                                        | small  | Item 5's in-process re-dispatch is replaced by Phase D's natural lease-based recovery flow.                                                                                                                                 |

Phase D total effort: ~5–7 working days.

### Phase E — status semantics + score correctness (audit Phase 4 + leftover audit findings)

The audit findings that haven't appeared in any other phase yet — §1.10, §1.11
(partial), §1.17, §1.18, §1.19d. All UX/diagnostic in nature.

| #   | Title                                                                                                                                                                                                                                                          | Source                                                                               | File:line target                                                                                                                                     | Depends on   | Effort | Rationale                                                                                                                                                                                 |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 42  | Redefine `Pending` → "queued for dispatch"; `Running` → "at least one exec op claimed by a worker". Set `Running` only after first exec result observed (per-server SDK helpers `host.submission.set_compiling()` / `set_running()`, or dispatcher inference). | audit §1.10 + §3.7 + Phase 4.                                                        | `plugins/{icpc,ioi}/src/evaluate*.rs` (where Running is set today), new SDK helpers in `packages/server-sdk/src/sdk/submissions.rs`                  | item 36      | medium | Today `Running` is set when `start_evaluate_batch` returns — long before any worker has claimed a single op. Operators reading the dashboard see "everything running" during queue waits. |
| 43  | Per-state stuck timeouts: `Queued` → 5 min no claim attempt; `Pending` → 5 min with `owner_server_id IS NULL`; `Compiling`/`Running` → no time-based limit (covered by lease). 6 h wide-net (item 19) remains the catch-all.                                   | audit §1.11 + §3.7.                                                                  | `packages/server/src/dlq/stuck.rs`                                                                                                                   | items 36, 42 | small  | Replaces the single 2-h global timeout with per-state thresholds matching the new semantics.                                                                                              |
| 44  | **§1.17 fix.** IOI `evaluate_batch.rs::evaluate_all` on `Verdict::CompileError` fills remaining testcases with `Verdict::Skipped` (matching ICPC's `is_known_failure` branch). Same fix for `code-run`'s `run.rs:95-99`.                                       | audit §1.17 — **not addressed by either impl plan as written; pulled into Phase E.** | `plugins/ioi/src/evaluate_batch.rs:136-148`, `packages/server-sdk/src/evaluator/run.rs:95-99`                                                        | —            | small  | Diagnostic-row consistency; final scoring is unchanged. Audit Phase 1 mentions "ride this phase" — we land it here so it groups with §1.18.                                               |
| 45  | **§1.18 fix.** Render submission_score as "—" / "in progress" when `submission.status != Judged`. Optionally do not persist `submission_score` until the plugin calls `set_submission_verdict`.                                                                | audit §1.18 — **not addressed by either impl plan as written; pulled into Phase E.** | `packages/web/src/...` (display helper), `packages/server/src/handlers/submission.rs`, `packages/server/src/models/submission.rs` (response shaping) | —            | small  | A "98% Running" submission visually resembling "98% Judged" is the only audit-flagged UX hazard. Cheap to fix once we own the response shape.                                             |
| 46  | **§1.19d doc-only fix.** Add a clarifying comment near `plugins/ioi/src/lib.rs:472` noting that the "Skipped" rewrite is _output redaction_ for feedback levels, not _execution skipping_. Cross-link §1.16/§1.19a/b in the comment.                           | audit §1.19d.                                                                        | `plugins/ioi/src/lib.rs:472`                                                                                                                         | —            | small  | Doc-only; rides item 44.                                                                                                                                                                  |

Phase E total effort: ~2–3 working days.

### Phase F — observability (audit Phase 4 obs + run-2 §4)

Concrete metric and tracing-span list, build-flag changes for the next profiling
run, and dashboard updates.

| #   | Title                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Source                   | File:line target                                                                                                                                                  | Depends on       | Effort | Rationale                                                                                                                                                     |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 47  | Plugin call hot-path metrics: `plugin_evaluator_semaphore_wait_duration`, `plugin_pool_contention_total` (with latency buckets), verify `plugin_instance_acquire_duration` is emitted on every `pool.get(timeout)` path including the retry loop at `services/evaluate_batch.rs:138-141`.                                                                                                                                                                                     | run-2 §4.2; audit §3.10. | `packages/common/src/metrics.rs:28`, `packages/server/src/services/evaluate_batch.rs:103-141`, `packages/plugin-core/src/traits.rs`                               | item 13          | small  | Items 12/13 added cache + host-fn metrics; this fills in the plugin-pool side.                                                                                |
| 48  | Submission lifecycle metrics: `submission_state_transition_duration` (labels `from`, `to`, `problem_type`); `submission_in_flight` by state; `submission_judge_queue_depth` (LLEN of new MQ queue / COUNT of `status='Queued'` rows); `submission_age_in_pending_seconds`.                                                                                                                                                                                                    | run-2 §4.2; audit §3.10. | `packages/common/src/metrics.rs`, `packages/server/src/handlers/submission.rs` (transition events), `packages/server/src/dispatcher/claim.rs` (queue depth gauge) | items 36, 42     | medium | Audit-Phase-4 deliverable; lets operators answer "how many submissions are stuck in `Queued`?"                                                                |
| 49  | Worker side: `worker_permits_in_flight`, `worker_permits_max`, `sandbox_init_duration`, `sandbox_cleanup_duration` (`enable_cgroups` label), `file_materialization_copy_seconds` (`path_kind={input,output,source}`), `worker_compile_cache_redundancy_total`, `operation_result_e2e_duration` (with `task.enqueued_at_unix_ms` → delivered timing).                                                                                                                          | run-2 §4.2; audit §3.10. | `packages/common/src/metrics.rs`, `packages/worker/src/models/operation/{handler,sandbox/isolate}.rs`, `packages/worker/src/main.rs`                              | items 6, 7       | medium | Per-testcase wallclock decomposition is the run-3 ask.                                                                                                        |
| 50  | Tracing spans: every host-fn `_fn` callback opens an `info_span!("host_fn", name=..., plugin_id=...).entered()` _with explicit parent linkage_ (since after item 1 they run on blocking-pool threads, not tokio workers — `Span::current()` is thread-local). Add `#[instrument]` to `isolate_init`, `isolate_cleanup`, `file_cacher_fetch`, `file_cacher_upload`. Per-submission span in `services/submission_dispatch.rs` with `submission.id`, `worker.id`, `judge_epoch`. | run-2 §4.3; audit §3.10. | every `host_funcs/*.rs` `_fn`, `packages/worker/src/models/operation/sandbox/isolate.rs:271-332`, `packages/worker/src/models/file_cacher.rs`                     | item 1           | medium | Item 1's `spawn_blocking` breaks thread-local span context — explicit propagation is required to keep Jaeger traces continuous.                               |
| 51  | Build flags for next profiling run: `[profile.release] debug = 1` (full DWARF), `RUSTFLAGS="-C force-frame-pointers=yes"`.                                                                                                                                                                                                                                                                                                                                                    | run-2 §4.4.              | root `Cargo.toml`, `.cargo/config.toml` (or CI env), `release/Dockerfile`                                                                                         | —                | small  | Run-2's `debug = "line-tables-only"` flame graphs collapsed all Rust frames to `[broccoli-server]`. Costs ~5–10% CPU steady-state; worth it for the next run. |
| 52  | Dashboard update: add panels for host-fn duration P50/P95/P99 (per-fn), plugin instance acquire P99, plugin pool contention rate, worker permits in-flight vs max, submission state transition durations, operation-result e2e duration, blob cache hit ratio + size.                                                                                                                                                                                                         | run-2 §4.5; audit §3.10. | `config/grafana/provisioning/dashboards/broccoli-overview.json`                                                                                                   | items 47, 48, 49 | small  | The dashboard is the operator's first window into "what's happening." Phase 4 deliverable per the audit.                                                      |

Phase F total effort: ~3–5 working days. Most items rideable in parallel with
Phase E.

### Phase G — Sketch D coalescing (run-2 Phase C, ~2–3 weeks)

This is the major architectural change run-2 proposed: replace the WASM-side
`while next_result()` polling loop with host-driven callbacks.

**Originally written as a conditional gate** ("measure post-Phase-C, then
schedule"). Per §E, the user's no-intermediate-deployment constraint dissolves
this gate — Phase G ships unconditionally in the integrated release.

The justification: §D.5 refuted the prior claim that windowing (item 28)
suffices to fix host-side batch-evaluator saturation. Item 14b's server-side
fan-out semaphore bounds the in-flight pressure to `fanout_concurrency=64` per
server, but each in-flight op still pins a batch-evaluator instance for the full
1–30 s testcase duration. Phase G's callback model reduces that pin to ~100 ms
slot-time (K=4 coalescing × ~20 ms per callback), a 10–300× reduction
independent of item 14b's cap. Both bound the same resource from different
sides; both ship.

| #   | Title                                                                                                                                                     | Source                           | File:line target                                                              | Depends on  | Effort | Rationale                                                                                                                                                                        |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------- | ----------------------------------------------------------------------------- | ----------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 53  | Sketch D coalescing for batch-evaluator: replace `host.operations.get_next_operation_result` polling with host-driven callback-on-result. K=4 coalescing. | run-2 Phase C #22.               | `packages/server/src/host_funcs/dispatch.rs`, batch-evaluator plugin          | items 24–28 | large  | Per Correction C, batch-evaluator's per-testcase pin is the actual saturation driver. K=4 callbacks at ~20 ms each = 100 ms slot-time per testcase vs 1–5 s pin. **20–50× win.** |
| 54  | Sketch D coalescing for ICPC: replace `while next_result()` with `host.eval.run_batch(batch_id, on_K_results_handler)`.                                   | run-2 Phase C #22 (second half). | `plugins/icpc/src/evaluate.rs:120-174`, `packages/server-sdk/src/sdk/eval.rs` | item 53     | large  | 100 ms slot-time per submission vs 30 s pin. **300× win.** Preserves ICPC short-circuit (server fires immediate callback on failing result).                                     |

Phase G total effort: ~10–15 working days.

### Phase H — optional / future

Items intentionally left unscheduled — audit Phase 5 work, run-2's "move
dispatch consumer out of api process," and other forward-looking items.

| #   | Title                                                                                                            | Source                                 | Effort | Rationale                                                                                                                                 |
| --- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 55  | Restore `perf_event_open` instruction counting; transition selected problem types to instruction-count TLE.      | audit §3.12 + Phase 5.                 | large  | The only thing that unlocks `max_concurrency > 1` as a fair default. Until then, scale `S` (machines), not `c` (per-machine slots).       |
| 56  | Move blocking hooks to async post-accept; new `Rejected` terminal status.                                        | run-2 §2.1 alt; audit §2.1 "Option A". | medium | Changes API contract — needs operator/contestant communication.                                                                           |
| 57  | Move dispatch consumer out of api process and into the worker tier.                                              | run-2 Phase C #24.                     | large  | Makes the api process a pure HTTP front-end; api never holds plugin pools again.                                                          |
| 58  | Cgroup pool / per-testcase fresh-sandbox optimization (replaces rejected "sandbox-reuse-across-testcases" idea). | run-2 §3.2 B replacement.              | medium | ~30–80 ms saving per testcase; preserves per-testcase isolation contract. Lower priority than items 53/54.                                |
| 59  | Compile-once-per-submission (server-side compile + cache-keyed 50 run ops).                                      | audit Phase 5.                         | medium | Subsumed by item 33's leader-election in cache subsystem in 95% of cases; only worth scheduling if measured cache hit rate is still poor. |
| 60  | Checker-in-operation: move standard-checkers from server-side host fn to worker-side step.                       | audit Phase 5.                         | medium | Saves server CPU + memory under burst. Not on the critical path.                                                                          |
| 61  | Replace contest-plugin `Mutex<Plugin>` with `EvaluatorPool`-style instance pool.                                 | audit Phase 5 + §1.12.                 | medium | Real but subsumed by items 28 + 53/54 for the common case.                                                                                |
| 62  | IOI-style resume-from-partial (opt-in plugin-level resume after steal).                                          | audit Phase 5.                         | large  | Recovers compute on stolen submissions that already judged N-1 of N tests. Plugin-author opt-in.                                          |

---

## D. Conflicts resolved, with rationale

The two source plans disagreed in four places. The consolidated plan picks one
in each case.

### D.1 — Durable enqueue mechanism

- **run-2 proposed:** new Redis MQ queue `submission_judge_queue`, with a pool
  of consumers running `dispatch_submission_to_plugin_with_judgement`
  (`follow-up-plan.md §2.1`).
- **Audit Phase 3 proposed:** build on Phase 1b's lease columns, status field
  goes to `Queued`, polling fiber claims via `SKIP LOCKED`.
- **Consolidated plan picks:** _audit's PG `SKIP LOCKED` shape._ Items 36–38.
- **Rationale:** the audit's argument is canonical (audit §3.1): submission
  lifecycle must survive Redis flush, so it must be PG-backed regardless. Once
  PG is the system-of-record, having a _second_ MQ queue with the same semantics
  is duplication — every submission would write to both `submission` and the MQ
  queue, and the steal scanner already gives `SKIP LOCKED` claim semantics for
  free. Redis remains correct for _ephemeral_ operation tasks (where worker
  dedup + DLQ already exist), but the submission lifecycle is durable PG.

### D.2 — Dispatcher admission shape

- **run-2 proposed:** per-plugin-id semaphores keyed by `plugin_id`, sized to
  `pool_max_instances` for each plugin (`follow-up-plan.md §2.3 (a)`).
- **Audit Phase 2 / PR-S proposed:** single fleet-wide `DispatcherSemaphore`
  with `server.dispatcher_concurrency` (default 16), 503 OVERLOADED on overflow.
- **Consolidated plan picks:** _audit PR-S — single fleet-wide semaphore._ Items
  27 + 29.
- **Rationale:** the audit's shape generalizes cleanly to N contest plugins
  without per-id bookkeeping or operator-visible tuning. The run-2 design's
  per-plugin-id strength was that it bounded the pool _exactly_ — but item 29
  (fleet-aware admission via heartbeat sums) achieves the same thing at a higher
  level by sizing the dispatcher to actual fleet capacity, not to arbitrary pool
  sizes. Per-plugin-id can be added later as a refinement if measured per-plugin
  saturation diverges; until then it's premature.

### D.3 — Cross-worker compile coordination

- **Run-2 first proposed:** PG advisory lock keyed by compile cache_key inside
  `try_cache_hit` (`follow-up-plan.md §3.2 C`).
- **Run-2 revised proposal:** Redis SETNX leader-election in the worker cache
  subsystem (`follow-up-plan.md §17`, Phase B item).
- **Audit:** §1.9 names the `toolchain_fingerprint` correctness bug and
  recommends fixing it; it doesn't propose a coordination primitive.
- **Consolidated plan picks:** _run-2's leader-election in cache subsystem._
  Item 33; correctness fix is item 8.
- **Rationale:** PG advisory lock is session-scoped and would hold a worker DB
  connection for the 2–5 s of a compile. With worker pool size 5 and multiple
  concurrent submissions, this cascades — the worker is waiting on its own pool
  to free a slot for a coordination primitive. Compile-on-server was rejected
  because it leaks plugin-domain knowledge ("which step is the compile?") into
  the server abstraction. Leader-election in the cache subsystem keeps the host
  plugin-agnostic — the worker just knows "this step has a cache key; coordinate
  via Redis SETNX before recomputing." Generalizes to any future cacheable
  expensive step.

### D.5 — Windowing as a batch-evaluator saturation fix (REFUTED)

- **Original consolidation claimed (Claim C):** Per-submission windowing (`W=4`)
  materially reduces batch-evaluator pool saturation, making Phase G's Sketch D
  coalescing potentially unnecessary.
- **Agent C critical-review refutation:** Windowing operates at the SDK layer
  inside the contest plugin's evaluator loop. The actual fan-out happens at
  `packages/server/src/services/evaluate_batch.rs:84-185`, which spawns N
  per-testcase futures _immediately_ when the plugin calls
  `start_evaluate_batch`. The plugin can only "see" windowing via `next_result`,
  which gates _processing_ of results, not _publication_ of operations. So
  windowing reduces the in-plugin polling pin but not the host-side in-flight
  pressure on the batch-evaluator pool.
- **Consolidated plan picks:** the original Claim C is REFUTED. The
  batch-evaluator pool fix is item 14b (Phase A.5 — server-side bounded
  fan-out). Windowing (item 28) still ships for its independent value: reducing
  per-submission slot-time in the polling pin, and bounding the plugin-visible
  parallelism for testing/short-circuit logic.
- **Implication for Phase G:** Because windowing alone doesn't fix host-side
  saturation, the Phase G coalescing redesign retains its full value — the
  20–300× slot-time reductions described in items 53/54 are independent
  improvements to item 14b's fan-out bound.

### D.4 — Sandbox reuse across testcases

- **Run-2 proposed:** batch K testcases into one OperationTask with shared
  compile step + K exec steps inside one sandbox `--init`/`--cleanup`
  (`follow-up-plan.md §3.2 B`).
- **Run-2 revised (Phase B #16):** rejected as written — cross-testcase
  contamination is a fairness/correctness contract change. Replacement is a
  cgroup-pool variant: pre-allocate cgroup hierarchies at worker boot, claim on
  `--init`, return on `--cleanup` (after wipe). Smaller win (~30–80 ms vs
  ~50–130 ms) but preserves per-testcase-fresh isolation.
- **Audit:** doesn't address sandbox reuse.
- **Consolidated plan picks:** _cgroup-pool variant; deferred to Phase H
  item 58._
- **Rationale:** the contract change (cross-testcase isolation) is too costly
  for the ~50–130 ms × (K-1) saving. Higher-leverage items (items 1, 27, 28)
  cover more ground. The cgroup-pool variant is scheduled but not on the
  critical path.

---

## E. Sequencing under "all phases land before next deployment"

**Constraint update (user, post-consolidation):** all phases ship sequentially
in a single integrated release, with no intermediate deployment to production.
This rules out the conventional "ship the minimum-viable fix, measure, then
decide the rest" strategy and changes what §E is for.

What this constraint means for the plan:

- **No measurement gates.** Phase G's conditional "measure batch-evaluator pin
  durations post-Phase-C, then schedule" check (originally written as a
  staged-rollout safety) cannot fire because there is no production measurement
  between phases. Phase G ships unconditionally; the slot-time argument in items
  53/54 is taken as sufficient justification.
- **Item dependency ordering still matters.** A code dependency like "item 38
  depends on item 36's enum variant landing first" is unaffected — the ordering
  applies to merge order, not release order. The dependency columns in each
  phase table remain authoritative for review/merge sequencing.
- **§E becomes a review-priority guide, not a ship-priority guide.** The three
  "minimum-viable" items below remain the highest-leverage code changes; review
  them most carefully and write the most tests against them. Everything else can
  be reviewed at normal effort.
- **One large integration test wave replaces N small canary windows.** The
  integration test plan (§F open gaps) must cover all phases together —
  particularly the interactions between Phase A.5's fan-out semaphore, Phase B's
  lease columns, Phase C's cancel primitive, and Phase D's durable accept. A
  fault-injection harness exercising "server restart with N=1000 in-flight
  submissions" is the load-bearing test.

### E.1 — Highest-leverage items (review carefully)

**Item 1 (Phase A): replace `block_in_place` with `spawn_blocking` at
`traits.rs:333`.** This collapses the 866-thread tokio futex cascade observed in
run-2 profile3 into a no-op. The api process stops dying under sustained load;
`/healthz` and `/metrics` stay responsive; Caddy stops ejecting upstreams; and
_every nested `block_in_place` in `host_funcs/_.rs` auto-collapses to plain
function execution\* (run-2 Correction D). One line, single largest reliability
win available.

**Item 14b (Phase A.5): server-side bounded fan-out in
`services/evaluate_batch.rs`.** The fix Agent C's refutation surfaced — the
batch-evaluator pool saturation driver that windowing was wrongly claimed to
address. Without item 14b, item 1 makes the api process survive but the
batch-evaluator pool still saturates under 1000-submission bursts.

**Phase B (items 15–23): lease + steal + sweeper.** Restores correctness after a
server restart in ~75 s instead of 2 h. Without this, item 1 makes the api
process _survive_ under load, but a single restart of any LB replica still
strands every in-flight submission for 2 h. After Phase B, server death is
recoverable within a single steal cycle and ghost reply queues are GC'd within 1
h.

**Phase C items 27 + 28 + 29 (dispatcher semaphore + windowing + fleet-aware
admission).** Caps the in-flight pressure on each contest plugin and on the
plugin-visible parallelism so that 1000 simultaneous submissions cannot OOM any
server. With windowing W=4 _and_ item 14b's host-side semaphore, a
1000-submission spike puts 4000 testcases on the queue but at most
`fanout_concurrency=64` active operations per server — well within the pool's
`pool_max_instances=256`.

### E.2 — Items reviewable at normal effort

Phase D (durable accept) closes a smaller silent-loss gap and is largely
additive on top of Phase B's lease columns. Phase E (status semantics + §1.17,
§1.18) is operator UX. Phase F (observability) lets us _measure_ the system
during the next stress run but doesn't change its reliability. Phase G is a
throughput optimization with large per-item complexity but well-contained blast
radius. Phase H is forward-looking.

Total effort for the integrated release: **~4–6 weeks of one engineer's time, or
~2–3 weeks with two engineers parallelizing**. The integrated release pays for
tighter coupling between phases (no staged rollback) with a single sustained
engineering effort instead of a series of smaller ships.

---

## F. Open gaps and uncertainty

- **§1.17 and §1.18** are flagged in the audit but not picked up by either impl
  plan. They're scheduled in Phase E (items 44, 45) — small, isolated fixes that
  ride naturally with the status-semantics work.
- **Worktree status (audited).** The team's
  `judging-reliability-phase1-recovered` worktree contains ~86 uncommitted files
  implementing Phase 1, Phase 1b, and 9 of 11 Phase 2 PRs. The work is
  functionally present but **not yet PR-staged and not yet covered by
  integration tests for the new behavior**. The integrated-release sequencing in
  §E therefore requires a pre-step: decompose the worktree into ~20 atomic PRs
  matching the PR-A through PR-Y identifiers in the source impl plans, and write
  the integration tests as each PR is staged. Phase 3+ work has not been started
  in any worktree.
- **Item 14b (server-side fan-out semaphore)** is not in any worktree yet; this
  was the load-bearing finding from Agent C's critical review. It must be added
  to the Phase A.5 PR batch and is genuinely new code, not staging.
- **Item 33 (leader-election in worker cache subsystem)** has not been
  prototyped against a real load pattern; the SETNX + heartbeat + follower-poll
  loop is a design, not a measurement. Because there is no intermediate
  deployment to validate against, the integration test must include a
  multi-worker concurrent-compile scenario to exercise the leader-election path
  before merge.
- **Integration test fault-injection harness** is the single largest test gap.
  Per §E, an integrated release rules out canary measurement, so the test suite
  must cover the cross-phase interactions: server restart with N=1000 in-flight
  submissions (Phase B × Phase D), cancel storm under load (Phase C item 24 ×
  Phase A.5 14b), and a leader-election race during worker fleet rolling restart
  (item 33 × Phase 1b lease columns). None of these tests exist today.
- **Audit §1.19c** ("ICPC short-circuit doesn't actually save worker compute")
  is technically a cross-reference to §1.16; both are closed by items 24–26.
  Listed here for completeness.
- **Recursive plugin invocation (Phase C item 35)** is documented as a hazard
  but not fixed. If the integrated release ships without sizing the checker pool
  ≥ contest pool, the §1.16 fast-path may deadlock under cancel-storm
  conditions. Pool-sizing config is a 1-line change; the decision to keep
  checker on the server side or move it to a worker operation is deferred but
  the config bound should land in this release.
