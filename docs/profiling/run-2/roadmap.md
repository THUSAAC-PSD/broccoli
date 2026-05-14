# Judging-Reliability Integrated-Release Roadmap

**Companion to:** `docs/profiling/run-2/unified-plan.md` (the _what_). This file
is the _when_ and _who_ — a tickable checklist for agents and engineers picking
up work in the integrated-release model (no intermediate deployment between
phases).

**How to use this file.** Tick `- [x]` when the item lands on its target branch
_with passing integration tests_. Tick `- [~]` if the code lands but tests are
still being written. Add the PR URL on the same line in parentheses. Each item
references `unified-plan.md` by item number (e.g. `UP#14b`) so the spec lives in
one place.

**Estimated total effort:** 6–9 weeks single-engineer, 3–5 weeks two-engineer.

---

## 0. Hard prerequisites (block all merges to integration branch)

These three items are not in the unified-plan numbering but block everything
downstream. They must be ticked before Tranche 1 merges to the integration
branch.

- [~] **Integration test harness for fault injection.** Crate
  `packages/fault-harness/` (6ee0190b, 6e7d9d63) emits transcript JSON; only
  cancel-storm sub-scenario implemented.
  - [ ] N=1000 in-flight submissions with a configurable problem-type mix (ICPC,
        IOI).
  - [ ] Server restart mid-judgement (kill -9 one api replica, observe recovery
        via lease/steal).
  - [x] Cancel storm (rewritten to exercise real `RedisCancelChecker` with
        hit/miss/DEL toggle phases — 6e7d9d63).
  - [ ] Rolling worker restart with leader-election race (drain leader
        mid-compile, observe follower-poll path).
  - **Acceptance:** harness can produce a failure-and-recovery transcript JSON
    usable as a CI artifact. ✓ shape; missing 3 sub-scenarios above.
- [x] **Toolchain-fingerprint correctness fix lands first** (UP#8 — 8d8edcb7).
      One-shot probe at worker startup via `toolchain_fingerprint::compute`,
      threaded through `compute_cache_key` at `task_cache.rs:137`.
  - **Acceptance:** `worker_toolchain_fingerprint` info-logged at boot; hash
    visible in `task_cache` keys (verified via
    cache_key_differs_for_different_fingerprints test).
- [x] **Worker-pool sizing for recursive-checker hazard** (UP#35 — e2c5017a +
      347b716e). Auto-bumps `pool_max_instances` to `evaluator_parallelism` at
      startup with warn-log; runbook accurately separates queueing (independent
      plugins) from deadlock (self-checking plugins).
  - **Acceptance:** env example committed, runbook section visible at
    `release/docs/operator-runbook.md#plugin-pool-sizing`. ✓

---

## Tranche 1 — PR-stage the existing worktree (~5–7 days)

The `judging-reliability-phase1-recovered` worktree holds ~86 uncommitted files.
**No greenfield work in this tranche** — it's review, test, and merge of code
that already exists. Decompose into atomic PRs matching the PR-A through PR-Y
identifiers in the source impl plans.

### Phase A — runtime-cascade fix + mechanical wins

- [x] **PR: cascade-fix.** UP#1 — `spawn_blocking` at
      `packages/plugin-core/src/traits.rs:343`. Inner `block_in_place` wrappers
      in `host_funcs/*.rs` collapsed to plain `Handle::current().block_on(...)`
      in 7b612064 (they were no-ops on blocking-pool threads).
  - [ ] Integration test: 60s stress wave, assert no thread-count blow-up.
  - [x] No `block_in_place` regressions in `host_funcs/*.rs` —
        `grep -rn     block_in_place packages/server/src/host_funcs/` returns
        only the regression-guard doc comment.
- [x] **PR: runtime-builder.** UP#2 — explicit `tokio::runtime::Builder` in
      `packages/server/src/main.rs` (739b034c + 0651c664).
      `max_blocking_threads` is config-driven with `available_parallelism * 16`
      clamp [512, 8192] auto-default, override via
      `BROCCOLI__SERVER__MAX_BLOCKING_THREADS`.
- [x] **PR: result-consumer-concurrency.** UP#3 —
      `process_messages(.., Some(8),     ..)` at
      `packages/server/src/consumers/operation_result.rs:21` (19ffd8d3).
- [ ] **PR: drop-pool-timeout-429.** UP#4 — remove
      `PluginError::PoolTimeout →     AppError::RateLimited` mapping at
      `packages/server/src/error.rs:206-209`. **Bundle with UP#14h sync-hook
      retry.** _Mapping still present at error.rs:206._
- [x] **PR: stuck-redispatch-interim.** UP#5 — stuck-detector now re-dispatches
      via `tokio::spawn(dispatch_to_plugin(...))` with a
      `retry_count >=     max_dispatch_retries` guard, mirroring the lease/steal
      pattern; opens a fresh `submission_judgement` per re-dispatch (da3da409).
- [x] **PR: worker-max-concurrency-config.** UP#6 —
      `worker.max_concurrency:     u32` default 1 in `WorkerConfig` (547eec8b).
- [x] **PR: worker-heartbeat-plumb.** UP#7 — `max_concurrency` + `fairness_mode`
      populated in `WorkerHeartbeat` at `heartbeat.rs:30-32, 132-133`
      (547eec8b).
- [x] **PR: reflink-fast-path.** UP#9 — `link_or_copy` helper tries
      `std::fs::hard_link` first (unix), falls back to `tokio::fs::copy` on
      EXDEV/EEXIST/any failure; replaces all three
      `tokio::fs::copy(&cached,     dest)` sites in `fetch_to_path` (3acbca45).
- [x] **PR: cache-size-default.** UP#10 — `max_cache_size` default bumped to
      `4 * 1024 * 1024 * 1024` (887bd6ce).
- [x] **PR: cache-exists-shortcircuit.** UP#11 — `upload_from_path` now hashes
      the file locally first, probes `BlobStore::exists(&hash)`, and skips
      `put_stream` on a hit; falls open to streaming on probe error (3acbca45).
- [x] **PR: cache-metrics.** UP#12 — `blob_cache_hits_total`,
      `blob_cache_misses_total`, `blob_cache_size_bytes` (UpDownCounter),
      `blob_cache_evictions_total`, plus a sibling
      `blob_store_remote_hits_total` for the UP#11 short-circuit; threaded via
      `Option<Metrics>` into `BlobStoreFileCacher::new` (849ef6fb).
- [x] **PR: host-fn-metrics.** UP#13 — `host_fn_duration`, `host_fn_calls_total`
      wired around the `spawn_blocking` site in `plugin-core/src/traits.rs`;
      `host_fn_block_in_place_total` regression sentinel +
      `record_block_in_place_regression` helper in `host_funcs/mod.rs` (never
      invoked in shipping code; flips on if someone re-adds `block_in_place`)
      (7b612064).
- [x] **PR: verdict-cancelled-enum.** UP#14 — `Verdict::Cancelled` at
      `common/src/submission_status.rs:123`; predicates and string mappings
      complete.

### Phase B — lease/steal/sweeper (audit Phase 1b PRs A–I)

- [x] **PR-A: lease-schema.** UP#15 — `owner_server_id`, `lease_heartbeat_at`,
      `retry_count` on submission (entity:56-59) and code_run (311ae8b9).
- [x] **PR-B: server-heartbeat.** UP#16 — `broccoli:server:heartbeat:<id>` SET
      EX wired in `dispatcher/fleet_capacity.rs` (807d579c + 311ae8b9).
- [x] **PR-C: lease-refresher.** UP#17 — `dispatcher/refresher.rs` background
      task (311ae8b9).
- [x] **PR-D: steal-scanner.** UP#18 — `dispatcher/steal.rs` implements
      `FOR     UPDATE SKIP LOCKED` with retry_count enforcement (311ae8b9).
  - [ ] Integration test: kill api replica mid-judgement, verify recovery under
        75s. _Pending §0 harness sub-scenario._
- [ ] **PR-E: stuck-wide-net.** UP#19 — raise `stuck_job_timeout_secs` to 6h.
      _Default still 7200s (2h) at `common/src/config.rs:31`._ **Depends on
      UP#14f (`updated_at` fix) from Tranche 2 landing first.**
- [x] **PR-F: dedup-ttl-split.** UP#20 — `worker.dedup_ttl_secs` default 600s at
      `worker/src/config.rs:134`.
- [x] **PR-G: sweeper-scaffold.** UP#21 — `dispatcher/sweeper.rs` with
      `dead_since` debounce + `sweeper_dry_run` config knob (311ae8b9).
- [ ] **PR-H: sweeper-del.** UP#22 — flip dry-run flag. _`sweeper_dry_run`
      default is still `true` at `server/src/config.rs:144`._
  - [ ] Integration test: ghost queue lifecycle (api restart, queue populated,
        sweeper observes, debounces, DELs).
- [ ] **PR-I: lease-steal-flag-flip.** UP#23 — set
      `server.dispatcher.lease_steal_enabled = true` in
      `release/.env.server.example`. _No `lease_steal_enabled` mention in env
      example._

### Phase C — cancel primitive + admission + windowing

- [x] **PR-J: verdict-cancelled-wireup.** Subsumed by UP#14 above (UP#14
      ticked).
- [x] **PR-K: cancel-host-fns.** UP#24 — `cancel_evaluate_batch_fn` +
      `cancel_evaluate_test_cases_fn` with Lua bulk SET;
      `EvaluateBatchOpsRegistry` new (a14702cc + c81bb093).
- [x] **PR-L: worker-exists-fast-path.** UP#25 — pipelined EXISTS check before
      sandbox setup via `RedisCancelChecker`; emits `Cancelled` (bd53a195).
  - [ ] Criterion benchmark: <1ms p99 added to op-start. _Harness has p95 ≈
        940µs but it's against the read path in isolation, not full op-start._
- [x] **PR-M: sdk-cancel-wrapper.** UP#26 — `cancel_evaluate_test_cases` SDK
      wrapper (c81bb093).
- [x] **PR-S: dispatcher-semaphore.** UP#27 — `DispatcherSemaphore` wraps
      dispatcher spawns; `dispatcher_concurrency` + `max_queued_submissions`
      config knobs present (7c129b09 + 311ae8b9).
- [x] **PR-T: windowing-sdk.** UP#28 — `judge_epoch` threaded through evaluator
      run loop (`server-sdk/src/evaluator/run.rs:21`) (a2699fb6 + f35b5654).
- [x] **PR-V: fleet-capacity-monitor.** UP#29 — `dispatcher/fleet_capacity.rs`
      polls heartbeats and resizes `evaluator_slots`;
      `fleet_aware_admission_enabled` config knob.
- [x] **PR-W: icpc-cancelled-rewrite.** UP#30 — ICPC aggregate filter treats
      cancelled as non-judging (45b8da4d).
- [x] **PR-X: ioi-testcase-filter.** UP#31 — `compute_scoring_test_case_ids` in
      `plugins/ioi/src/subtasks.rs:114` excludes scoreless non-sample cases.
- [x] **PR-Y: ioi-subtask-shortcircuit.** UP#32 — `SubtaskShortCircuit` in
      `plugins/ioi/src/evaluate_batch.rs:59` with `cancellable_after` and
      `mark_cancelled` → `Skipped` emission.
  - [ ] Integration test: nested-subtask correctness (sibling-active check).
        _Unit test `compute_scoring_test_case_ids_keeps_nested_subtask_members`
        exists; an end-to-end fault-harness scenario is still missing._
- [x] **PR-DD: dedup-config-cleanup.** UP#20 already ticked as PR-F.

---

## Tranche 2 — New code for surfaced gaps (~5–7 days)

Items genuinely not in any worktree. Order matters: UP#14b first (unblocks Phase
G testing); UP#14f before Phase B PR-E.

### Phase A.5 — server-side hoist + stabilization

- [ ] **PR: server-fanout-semaphore.** UP#14b — bounded fan-out in
      `packages/server/src/services/evaluate_batch.rs:84-185`. New helper in
      `packages/server/src/dispatcher/fanout.rs`. Default
      `server.batch_evaluator_fanout_concurrency = 64` per server.
  - [ ] Integration test: 1000-submission burst observes
        `plugin_pool_contention_total` rate well below the no-semaphore
        baseline.
- [ ] **PR: publish-parallelism.** UP#14c — `try_join_all` in
      `packages/server/src/services/operation_batch.rs:70-136`. Default
      `server.operation_batch_publish_concurrency = 32`.
- [x] **PR: waiter-then-publish.** UP#14d — waiter insert at
      `operation_batch.rs:73` precedes publish at line 121.
  - [ ] Acceptance: `WaiterNotFound` log count zero under 1000-op burst. _Code
        ordering correct; assertion still needs harness coverage._
- [ ] **PR: healthz-off-runtime.** UP#14e — `/healthz` and `/metrics` on a
      dedicated tokio runtime + listener.
  - [ ] Acceptance: under deliberate api-runtime saturation, `curl /healthz`
        still returns 200 within 100ms.
- [ ] **PR: stuck-updated-at-fix.** UP#14f — switch `stuck.rs:54` from
      `created_at` to `updated_at`. **Must merge before Phase B PR-E.**
- [ ] **PR: regression-guard-block-in-place.** UP#14g — CI test asserts
      `host_fn_block_in_place_total == 0` after a 60s integration-test stress
      wave. Fail build with file/line offender hint on violation.
- [ ] **PR: sync-hook-retry.** UP#14h — copy retry-with-backoff helper from
      synchronous-evaluator path to hook path. **Pairs with Tranche 1 UP#4 (drop
      PoolTimeout→429).**
- [ ] **PR: silent-error-remediation.** UP#14a — replace each of the 10
      `let _ = mark_submission_dispatch_system_error(...)` sites in
      `submission_dispatch.rs` with explicit error-log + counter increment.
  - [ ] Acceptance: `submission_dispatch_failure_total` counter emitted with
        submission_id, error_code labels.

### Phase C — worker cache leader-election

- [ ] **PR: cache-leader-election.** UP#33 — Redis SETNX with TTL, leader
      heartbeat every 5s, followers poll `task_cache`. Lives in
      `packages/worker/src/models/operation/{handler,task_cache}.rs`.
  - [ ] Integration test: multi-worker concurrent-compile scenario; assert only
        one worker actually compiles, others wait and read the cached binary.
- [ ] **PR: bulk-insert-results.** UP#34 — convert single-element-slice callers
      to multi-row endpoint in `plugins/{icpc,ioi}/src/evaluate*.rs`.
  - [ ] Acceptance: per-submission INSERT count drops from ~20 to ~2–3.

---

## Tranche 3 — Durable accept + UX + observability (~7–10 days)

### Phase D — durable submission accept

- [ ] **PR: queued-status-enum.** UP#36 — add `Queued` variant; update
      `IN_PROGRESS_STATUSES`.
- [ ] **PR: insert-queued-on-post.** UP#37 — replace `tokio::spawn` with
      `INSERT ... status='Queued'` at the 9 dispatch sites.
- [ ] **PR: claim-fiber.** UP#38 — `SELECT ... FOR UPDATE SKIP LOCKED` polling
      every 1s; in-txn UPDATE to `Pending`, set `owner_server_id`, bump
      `retry_count`.
  - [ ] Integration test: kill api mid-POST, verify the submission still
        eventually judges (no silent loss).
- [ ] **PR: backpressure-on-post.** UP#39 — 503 + `Retry-After` when
      queued-count exceeds `max_queued_submissions` (default 5000).
- [ ] **PR: queue-wait-vs-exec-timeout.** UP#40 — plugin's `next_result` polling
      sees only execution time, not queue wait. Edit
      `packages/server/src/host_funcs/evaluate.rs:564` clock semantics.
- [ ] **PR: stuck-detector-reenqueue.** UP#41 — `retry_count > 5` →
      SystemError + DLQ; otherwise lease/steal handles recovery.

### Phase E — status semantics + score correctness

- [ ] **PR: redefine-pending-running.** UP#42 — `Running` set only after first
      exec result; new SDK helpers `host.submission.set_compiling()` /
      `set_running()`.
- [ ] **PR: per-state-stuck-timeouts.** UP#43 — `Queued` 5min, `Pending` 5min
      with `owner_server_id IS NULL`, `Compiling`/`Running` no time limit.
- [ ] **PR: ioi-compile-error-fill.** UP#44 — IOI fills remaining testcases with
      `Verdict::Skipped` on `CompileError` (matching ICPC's `is_known_failure`
      branch). Same fix for `code-run`.
- [ ] **PR: score-display-fix.** UP#45 — render submission_score as "—" / "in
      progress" when `status != Judged`.
- [ ] **PR: ioi-feedback-comment.** UP#46 — doc-only clarifying comment at
      `plugins/ioi/src/lib.rs:472`. Rides UP#44.

### Phase F — observability

- [ ] **PR: plugin-pool-metrics.** UP#47 —
      `plugin_evaluator_semaphore_wait_duration`,
      `plugin_pool_contention_total`, verify `plugin_instance_acquire_duration`
      on every pool.get path.
- [ ] **PR: submission-lifecycle-metrics.** UP#48 —
      `submission_state_transition_duration`, `submission_in_flight`,
      `submission_judge_queue_depth`, `submission_age_in_pending_seconds`.
- [ ] **PR: worker-metrics.** UP#49 — `worker_permits_*`, `sandbox_*`,
      `file_materialization_copy_seconds`,
      `worker_compile_cache_redundancy_total`, `operation_result_e2e_duration`.
- [ ] **PR: tracing-spans.** UP#50 — `info_span!("host_fn", ...)` on every
      host-fn `_fn` with explicit parent linkage (required because UP#1 moves
      callbacks to blocking-pool threads). `#[instrument]` on isolate +
      file-cacher functions.
- [ ] **PR: build-flags-for-profiling.** UP#51 — `debug = 1` (full DWARF),
      `RUSTFLAGS="-C force-frame-pointers=yes"`.
- [ ] **PR: dashboard-update.** UP#52 — Grafana panels for the new metrics. Edit
      `config/grafana/provisioning/dashboards/broccoli-overview.json`.

---

## Tranche 4 — Sketch D coalescing (~10–15 days)

Highest per-item complexity. UP#14b (Tranche 2) must land first so the
batch-evaluator host-fanout is bounded before introducing callback semantics.

- [ ] **PR: sketch-d-batch-evaluator.** UP#53 — replace
      `host.operations.get_next_operation_result` polling with host-driven
      callback-on-result. K=4 coalescing.
  - **File:** `packages/server/src/host_funcs/dispatch.rs`, batch-evaluator
    plugin.
  - [ ] Criterion benchmark: confirm ~100ms slot-time per testcase (vs 1–5s pin
        baseline). Target: 20–50× reduction.
- [ ] **PR: sketch-d-icpc.** UP#54 — replace `while next_result()` with
      `host.eval.run_batch(batch_id, on_K_results_handler)` in
      `plugins/icpc/src/evaluate.rs:120-174`. ICPC short-circuit preserved via
      immediate server-side callback on failing result.
  - [ ] Integration test: ICPC short-circuit on first WA verdict; verify no
        further testcases are executed (worker EXISTS check fires).

---

## Pre-release gates (final tick before merge to master)

- [ ] **All Tranche 1–4 PRs merged to integration branch with green CI.**
- [ ] **Full integration test suite green** including all fault-injection
      scenarios from §0.
- [ ] **Run-2 stress harness re-run** against the integration branch. Required
      telemetry recorded:
  - [ ] `host_fn_block_in_place_total == 0` for full 1h run.
  - [ ] `plugin_instance_acquire_duration` p99 < 5s under 1000-burst.
  - [ ] No `WaiterNotFound` logs.
  - [ ] `submission_dispatch_failure_total` rate flat (no silent failure
        regression).
  - [ ] Cancel storm: 1000 simultaneous cancels propagate to worker in <5s
        end-to-end.
  - [ ] Server restart with N=1000 in-flight: recovery in <75s, zero lost
        submissions.
- [ ] **Worktree archived.** `judging-reliability-phase1-recovered` worktree no
      longer needed; tag the final-stage commit and remove the worktree.
- [ ] **Runbook update.** `release/docs/operator-runbook.md` covers: pool sizing
      for checker hazard (UP#35), `lease_steal_enabled` flag,
      `max_queued_submissions` tuning, new metrics dashboard URL, 503-handling
      guidance for contestant-facing UI.

---

## Post-release validation (after master deploy)

Not part of the integrated release itself, but the checklist for verifying it
worked in production.

- [ ] **Run-3 profiling session** at Signpost-class load. Expect:
  - Zero deadlock events.
  - p99 evaluator acquisition < 5s.
  - Phase G effort justified by measured batch-evaluator pin durations
    pre-Phase-G (now historical baseline).
- [ ] **30-day SystemError rate audit.** Expected: zero SystemError caused by
      contention or `block_in_place` cascade.
- [ ] **30-day 429 rate audit.** Expected: zero 429 from
      `PluginError::PoolTimeout`; only from genuine rate-limit policy if any.
- [ ] **Decision point: Phase H items.** Based on run-3 telemetry, choose
      whether to schedule:
  - UP#55 — `perf_event_open` instruction counting (only if fairness complaints
    surface from `max_concurrency > 1` ambition).
  - UP#56 — async post-accept hooks (only if synchronous-hook latency remains
    the user-perceived bottleneck).
  - UP#57 — dispatch consumer to worker tier (only if api process still holds
    plugin pools under measured load).
  - UP#58 — cgroup pool (only if per-testcase sandbox setup is the measured
    bottleneck).
  - UP#59 — server-side compile (only if cache hit rate is still poor after
    UP#33).
  - UP#60 — checker-in-operation (only if the UP#35 hazard surfaces in
    production).
  - UP#61 — contest-plugin instance pool (only if Mutex contention is measured).
  - UP#62 — IOI resume-from-partial (only if steal-recovery wastes meaningful
    compute on partial judgements).

---

## Conventions for this checklist

- **PR naming:** prefix each PR title with the unified-plan item key (e.g.
  `[UP#14b] Server-side fan-out semaphore in evaluate_batch.rs`).
- **Ticking discipline:** `- [x]` only after code merged + integration tests
  green on the integration branch. `- [~]` if code merged but tests pending.
  `- [!]` if blocked (add blocker description on same line).
- **Adding items:** if a new item surfaces during implementation, add a row here
  _and_ add a row to `unified-plan.md` §C. Don't let this file drift ahead of
  the plan doc — agents reading the plan first should see every item ticked
  here.
- **Removing items:** never silently delete. Strike through (`~~text~~`) and add
  a note explaining why the item was dropped.
