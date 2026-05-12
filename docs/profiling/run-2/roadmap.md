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

- [ ] **Integration test harness for fault injection.** New crate
      `packages/integration-harness/` (or extend `packages/server/tests/`) that
      can drive:
  - [ ] N=1000 in-flight submissions with a configurable problem-type mix (ICPC,
        IOI).
  - [ ] Server restart mid-judgement (kill -9 one api replica, observe recovery
        via lease/steal).
  - [ ] Cancel storm (1000 simultaneous batch cancels, observe worker cancel-key
        EXISTS fast-path).
  - [ ] Rolling worker restart with leader-election race (drain leader
        mid-compile, observe follower-poll path).
  - **Acceptance:** harness can produce a failure-and-recovery transcript JSON
    usable as a CI artifact.
- [ ] **Toolchain-fingerprint correctness fix lands first** (UP#8). This is not
      a performance item — a compiler upgrade today silently serves stale cached
      binaries, and no other Tranche-1 work depends on cache being populated, so
      we ship the fix before _anything_ else touches the compile path.
  - **Acceptance:** `worker_compile_fingerprint` metric reports a non-empty
    string at worker boot, hash visible in `task_cache` keys.
- [ ] **Worker-pool sizing for recursive-checker hazard** (UP#35). Edit
      `release/.env.server.example` to set `checker_pool ≥ contest_pool`;
      document the constraint in `release/docs/operator-runbook.md`. The
      move-to-worker refactor stays deferred.
  - **Acceptance:** env example committed, runbook section visible at
    `release/docs/operator-runbook.md#plugin-pool-sizing`.

---

## Tranche 1 — PR-stage the existing worktree (~5–7 days)

The `judging-reliability-phase1-recovered` worktree holds ~86 uncommitted files.
**No greenfield work in this tranche** — it's review, test, and merge of code
that already exists. Decompose into atomic PRs matching the PR-A through PR-Y
identifiers in the source impl plans.

### Phase A — runtime-cascade fix + mechanical wins

- [ ] **PR: cascade-fix.** UP#1 — replace
      `block_in_place(Handle::current().block_on(...))` with `spawn_blocking` at
      `packages/plugin-core/src/traits.rs:333`. Propagate `Span::current()`; map
      `JoinError::is_panic()` to `PluginError::ExecutionFailed`.
  - [ ] Integration test: 60s stress wave, assert no thread-count blow-up.
  - [ ] Verify no `block_in_place` regressions in `host_funcs/*.rs`
        auto-collapsed (zero-line edit required; just confirm).
- [ ] **PR: runtime-builder.** UP#2 — explicit `tokio::runtime::Builder` in
      `packages/server/src/main.rs`; `max_blocking_threads=1024`.
- [ ] **PR: result-consumer-concurrency.** UP#3 —
      `process_messages(.., Some(8), ..)` in
      `packages/server/src/consumers/operation_result.rs:9`.
- [ ] **PR: drop-pool-timeout-429.** UP#4 — remove
      `PluginError::PoolTimeout → AppError::RateLimited` mapping at
      `packages/server/src/error.rs:206-209`. **Bundle with UP#14h sync-hook
      retry.**
- [ ] **PR: stuck-redispatch-interim.** UP#5 —
      `tokio::spawn(dispatch_submission_to_plugin(...))` replaces
      `mark_submission_system_error` at `stuck.rs:167-173`.
- [ ] **PR: worker-max-concurrency-config.** UP#6 — add
      `worker.max_concurrency: usize` (default `1`) to `WorkerConfig`.
- [ ] **PR: worker-heartbeat-plumb.** UP#7 — populate `max_concurrency`,
      `current_in_flight`, `fairness_mode` in the worker heartbeat.
- [ ] **PR: reflink-fast-path.** UP#9 — try `std::os::unix::fs::link` before
      `tokio::fs::copy` in `file_cacher.rs:213`.
- [ ] **PR: cache-size-default.** UP#10 — bump `max_cache_size` default to 4
      GiB.
- [ ] **PR: cache-exists-shortcircuit.** UP#11 — HEAD probe before stream in
      `upload_from_path`.
- [ ] **PR: cache-metrics.** UP#12 — `blob_cache_hits_total`,
      `blob_cache_misses_total`, `blob_cache_size_bytes`,
      `blob_cache_evictions_total`.
- [ ] **PR: host-fn-metrics.** UP#13 — `host_fn_duration`,
      `host_fn_calls_total`, `host_fn_block_in_place_total`.
- [ ] **PR: verdict-cancelled-enum.** UP#14 — add `Verdict::Cancelled` variant +
      stress-test mirror gap fix. Land additively so downstream PRs can
      `match Verdict::Cancelled` without compile errors.

### Phase B — lease/steal/sweeper (audit Phase 1b PRs A–I)

- [ ] **PR-A: lease-schema.** UP#15 — add `owner_server_id`,
      `lease_heartbeat_at`, `retry_count` columns to `submission` and
      `code_run`. Index on `(status, lease_heartbeat_at)`.
- [ ] **PR-B: server-heartbeat.** UP#16 — `broccoli:server:heartbeat:<id>` SET
      EX 30s, refresh every 10s.
- [ ] **PR-C: lease-refresher.** UP#17 — background task, every 10s, one batch
      UPDATE per server.
- [ ] **PR-D: steal-scanner.** UP#18 —
      `SELECT ... FOR UPDATE SKIP LOCKED LIMIT 8`, in-txn
      `DELETE FROM test_case_result`, `retry_count++`, re-dispatch.
      `retry_count > 5` → SystemError + DLQ.
  - [ ] Integration test: kill api replica mid-judgement, verify recovery under
        75s.
- [ ] **PR-E: stuck-wide-net.** UP#19 — raise `stuck_job_timeout_secs` to 6h.
      **Depends on UP#14f (`updated_at` fix) from Tranche 2 landing first.**
- [ ] **PR-F: dedup-ttl-split.** UP#20 — add `worker.dedup_ttl_secs` (default
      600s), decouple from `stuck_job_timeout_secs`.
- [ ] **PR-G: sweeper-scaffold.** UP#21 — SCAN MATCH `operation_results.*`,
      two-stage `dead_since` debounce, dry-run mode.
- [ ] **PR-H: sweeper-del.** UP#22 — flip dry-run flag.
  - [ ] Integration test: ghost queue lifecycle (api restart, queue populated,
        sweeper observes, debounces, DELs).
- [ ] **PR-I: lease-steal-flag-flip.** UP#23 — set
      `server.dispatcher.lease_steal_enabled = true` in
      `release/.env.server.example`.

### Phase C — cancel primitive + admission + windowing

- [ ] **PR-J: verdict-cancelled-wireup.** Subsumed by UP#14 above; tick when
      Tranche 1 Phase A PR-verdict-cancelled-enum lands.
- [ ] **PR-K: cancel-host-fns.** UP#24 — `cancel_evaluate_batch_fn` writes Redis
      key, `cancel_evaluate_test_cases_fn` via Lua EVAL.
      `EvaluateBatchOpsRegistry` module new.
- [ ] **PR-L: worker-exists-fast-path.** UP#25 — pipeline EXISTS check before
      sandbox setup; emit `Cancelled` verdict.
  - [ ] Criterion benchmark: <1ms p99 added to op-start.
- [ ] **PR-M: sdk-cancel-wrapper.** UP#26 —
      `host.eval.cancel_evaluate_test_cases(batch_id, task_ids)`.
- [ ] **PR-S: dispatcher-semaphore.** UP#27 — wrap all 9 outer
      `tokio::spawn(dispatch_to_plugin*)` sites with
      `DispatcherSemaphore::acquire_owned()`. Add
      `server.dispatcher_concurrency` (default 16),
      `server.max_queued_submissions` (default 100×n_machines). Return 503 with
      `Retry-After` on overflow.
- [ ] **PR-T: windowing-sdk.** UP#28 — `host.eval.start_windowed(input, W)`.
      ICPC opts in with W=1; IOI with W=4. **Pairs with Tranche 2 UP#14b
      server-side fanout semaphore.**
- [ ] **PR-V: fleet-capacity-monitor.** UP#29 — poll
      `broccoli:worker:heartbeat:*` every 5s, atomically resize evaluator slots.
- [ ] **PR-W: icpc-cancelled-rewrite.** UP#30 — treat `Verdict::Cancelled` like
      `Verdict::Skipped` in the short-circuit fill loop.
- [ ] **PR-X: ioi-testcase-filter.** UP#31 — `compute_scoring_test_case_ids`
      excludes scoreless non-sample testcases.
- [ ] **PR-Y: ioi-subtask-shortcircuit.** UP#32 — per-subtask running tally; on
      threshold hit, build cancellable task_ids and emit `Skipped` rows.
  - [ ] Integration test: nested-subtask correctness (sibling-active check).
- [ ] **PR-DD: dedup-config-cleanup.** UP#20 already merged above as PR-F; this
      is the Phase 1b inheritance cleanup portion. Tick when PR-F lands.

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
- [ ] **PR: waiter-then-publish.** UP#14d — insert waiter into DashMap registry
      _before_ publishing op to MQ.
  - [ ] Acceptance: `WaiterNotFound` log count zero under 1000-op burst.
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
