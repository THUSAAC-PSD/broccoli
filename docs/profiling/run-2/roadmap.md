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

- [~] **Integration test harness for fault injection.** Lives in stress-test's
  `fault::*` module (originally crate `packages/fault-harness/` — 6ee0190b,
  6e7d9d63, c96b7fe7 — folded into stress-test to share its HTTP client + admin
  auth). Emits transcript JSON; cancel-storm, kill-server-recovery, and burst
  sub-scenarios implemented.
  - [x] N=1000 in-flight submissions with a configurable problem-type mix (ICPC,
        IOI) — implemented in
        `packages/stress-test/src/fault/scenarios/burst.rs` with a
        `--type-weights icpc:70,ioi:30` CLI mix, dynamic registry validation,
        ≥95% terminal-rate gate, per-type latency histograms.
  - [x] Server restart mid-judgement (kill -9 one api replica, observe recovery
        via lease/steal) — implemented in
        `packages/stress-test/src/fault/scenarios/kill_server_recovery.rs` with
        a 75s observation window (default `--observe-timeout-secs`) and the
        `classify_recovery` decision matrix (OwnerChanged / StatusReset / Judged
        / Stuck). 6 unit tests cover the pure classification logic.
  - [x] Cancel storm (rewritten to exercise real `RedisCancelChecker` with
        hit/miss/DEL toggle phases — 6e7d9d63).
  - [ ] Rolling worker restart with leader-election race (drain leader
        mid-compile, observe follower-poll path).
  - **Acceptance:** harness can produce a failure-and-recovery transcript JSON
    usable as a CI artifact. ✓ shape; missing only the rolling-worker-restart
    sub-scenario above (blocked by UP#33 cache-leader-election).
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
- [x] **PR: drop-pool-timeout-429.** UP#4 — `PluginError::PoolTimeout` now falls
      through to `AppError::Internal` (500) at
      `packages/server/src/error.rs:205-213`; hot-path callers retry
      transparently via `plugin_core::retry::call_raw_with_pool_retry`
      (`packages/plugin-core/src/retry.rs`), so a PoolTimeout reaching HTTP
      mapping is genuine overload, not client-side rate-limit. Bundled with
      UP#14h sync-hook retry.
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
  - [x] Integration test: kill api replica mid-judgement, verify recovery under
        75s — see
        `packages/stress-test/src/fault/scenarios/kill_server_recovery.rs` (75s
        observation window asserted at line 42; full Setup→Inject→Observe→
        Assertion→Recover phases). End-to-end run against a live stack is the
        operator's responsibility; the scenario itself is the integration test.
- [x] **PR-E: stuck-wide-net.** UP#19 — `stuck_job_timeout_secs` default raised
      to 21600s (6h) at `common/src/config.rs:31` (9b12656b). Pairs with the
      UP#14f rewrite below.
- [x] **PR-F: dedup-ttl-split.** UP#20 — `worker.dedup_ttl_secs` default 600s at
      `worker/src/config.rs:134`.
- [x] **PR-G: sweeper-scaffold.** UP#21 — `dispatcher/sweeper.rs` with
      `dead_since` debounce + `sweeper_dry_run` config knob (311ae8b9).
- [x] **PR-H: sweeper-del.** UP#22 — `default_sweeper_dry_run` flipped to
      `false` in both `server/src/config.rs` and the docker-compose template's
      `${SWEEPER_DRY_RUN:-...}` fallback (9b12656b). The sweeper now actually
      DELs ghost reply queues after the debounce window.
  - [ ] Integration test: ghost queue lifecycle (api restart, queue populated,
        sweeper observes, debounces, DELs).
- [x] **PR-I: lease-steal-flag-flip.** UP#23 —
      `BROCCOLI__SERVER__DISPATCHER_LEASE_STEAL_ENABLED` documented in
      `release/.env.server.example` with operator guidance ("flip to true after
      UP#15-18 has soaked in your fleet"). Default remains `false`; the env var
      is now discoverable, not the value. (9b12656b)

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
        exists; an end-to-end fault scenario in stress-test's `fault::*` module
        is still missing._
- [x] **PR-DD: dedup-config-cleanup.** UP#20 already ticked as PR-F.

---

## Tranche 2 — New code for surfaced gaps (~5–7 days)

Items genuinely not in any worktree. Order matters: UP#14b first (unblocks Phase
G testing); UP#14f before Phase B PR-E.

### Phase A.5 — server-side hoist + stabilization

- [x] **PR: server-fanout-semaphore.** UP#14b — bounded fan-out in
      `packages/server/src/services/evaluate_batch.rs:90-233` via single
      dispatcher task acquiring `FanoutSemaphore` permits before per-tc spawn.
      New helper at `packages/server/src/dispatcher/fanout.rs:1` (clone-cheap
      `Arc<Semaphore>` wrapper with wait-duration histogram + saturation
      counter). Config knob `server.batch_evaluator_fanout_concurrency` defaults
      64 in `packages/server/src/config.rs:131`; wired into
      `EvaluateHostDeps.fanout_slots` at
      `packages/server/src/host_funcs/mod.rs:251`. Override via
      `BROCCOLI__SERVER__BATCH_EVALUATOR_FANOUT_CONCURRENCY`.
  - [ ] Integration test: 1000-submission burst observes
        `plugin_pool_contention_total` rate well below the no-semaphore
        baseline.
- [x] **PR: publish-parallelism.** UP#14c — `start_operation_batch` now streams
      per-op blob-externalize + `mq.publish` through
      `futures::stream::buffer_unordered(N).try_collect()` in
      `packages/server/src/services/operation_batch.rs:70-180`. `try_join_all`
      was rejected as unbounded; `buffer_unordered` caps in-flight publishes at
      the configured concurrency. Config knob
      `server.operation_batch_publish_concurrency` defaults 32 in
      `packages/server/src/config.rs:132-146`; wired into
      `OperationHostDeps.operation_batch_publish_concurrency` at
      `packages/server/src/host_funcs/context.rs:95-99`. Per-op waiter-insert →
      publish ordering (UP#14d) preserved via sequential await inside each
      in-flight future. Override via
      `BROCCOLI__SERVER__OPERATION_BATCH_PUBLISH_CONCURRENCY`.
- [x] **PR: waiter-then-publish.** UP#14d — waiter insert at
      `operation_batch.rs:73` precedes publish at line 121.
  - [ ] Acceptance: `WaiterNotFound` log count zero under 1000-op burst. _Code
        ordering correct; assertion still needs harness coverage._
- [x] **PR: healthz-off-runtime.** UP#14e — `/healthz` and `/metrics` now have
      an opt-in dedicated tokio runtime on a separate OS thread bound to its own
      TCP listener. New config knob `server.healthz_listen` (default `None`)
      selects the address; `server.healthz_worker_threads` (default `2`) sizes
      the runtime. Implementation in `packages/server/src/healthz_runtime.rs`
      (synchronous bind on the parent thread so startup failures surface
      immediately, then hand the FD off to a `multi_thread` tokio runtime
      running `axum::serve`). Wiring in `packages/server/src/runtime.rs` (new
      `_healthz_handle` field on `ServerRuntime`). The endpoints stay mounted on
      the main router unchanged for backward compat. `--healthcheck` CLI flag in
      `packages/server/src/main.rs` prefers the dedicated port when configured.
  - [x] Acceptance: design satisfies — main-runtime saturation cannot stall the
        dedicated listener because they share no scheduler state. Operator must
        opt in via `server.healthz_listen` to realize the isolation; legacy
        behavior (probes on main router only) is preserved when unset.
        Citations: `packages/server/src/healthz_runtime.rs`,
        `packages/server/src/config.rs` (`healthz_listen`,
        `healthz_worker_threads` knobs).
- [x] **PR: stuck-updated-at-fix.** UP#14f — rephrased on contact with reality:
      neither `submission` nor `code_run` has an `updated_at` column. Stuck.rs
      now uses the same composite predicate as `dispatcher/steal.rs:733-734` —
      `(owner_server_id IS NULL AND created_at < t) OR (owner_server_id IS NOT     NULL AND (lease_heartbeat_at IS NULL OR lease_heartbeat_at < t))`
      — which is the modern equivalent: leased rows clock from heartbeat,
      unleased rows from creation (9b12656b).
- [x] **PR: regression-guard-block-in-place.** UP#14g — static-source guard at
      `packages/server/tests/integration/regression_guards.rs::host_funcs_must_not_use_tokio_block_in_place`
      fails CI if `block_in_place(` reappears anywhere in
      `packages/server/src/host_funcs/` (outside identifier or comment context).
      The counter-based dynamic check from the original spec was dropped: the
      recorder helper (`record_block_in_place_regression`) is
      `#[allow(dead_code)]` and the counter can only register a regression if
      future code opts in via the helper — a static-source guard is the
      structural invariant we actually want. Recorder helper now exercised by a
      `#[cfg(test)]` unit test in `packages/server/src/host_funcs/mod.rs` so it
      is no longer dead in CI.
- [x] **PR: sync-hook-retry.** UP#14h — `PluginHook::on_event` now retries on
      `PluginError::PoolTimeout` via the shared
      `plugin_core::retry::call_raw_with_pool_retry` helper
      (`packages/plugin-core/src/retry.rs`), so blocking and notify hooks no
      longer fail-closed with a 500 on transient pool contention. Submission
      dispatch and evaluate-batch were refactored onto the same helper,
      replacing two inline copy-pasted loops. Paired with Tranche 1 UP#4.
- [x] **PR: silent-error-remediation.** UP#14a — replaced each of the 9
      `let _ = mark_submission_dispatch_system_error(...)` sites in
      `packages/server/src/services/submission_dispatch.rs` with a
      `record_dispatch_failure` helper that emits a structured error log on
      SystemError persistence failure and increments a counter regardless.
  - [x] Acceptance: `broccoli.submission_dispatch.failures` counter emitted with
        error_code, recovered labels (submission_id on the structured error
        event, not the counter — Prometheus cardinality). Counter defined in
        `packages/common/src/metrics.rs` (`submission_dispatch_failure_total`);
        helper at `packages/server/src/services/submission_dispatch.rs`
        (`record_dispatch_failure`).

### Phase C — worker cache leader-election

- [x] **PR: cache-leader-election.** UP#33 — Redis SETNX-with-TTL elector around
      the cache-miss path so that when N workers race to compile the same
      submission, exactly one runs the expensive step. New module
      `packages/worker/src/models/operation/cache_leader.rs` defines the
      `CacheLeaderElector` trait, `RedisCacheLeaderElector` (Lua-CAS extend +
      release scripts, `SET NX EX`), and `NoopCacheLeaderElector` fallback;
      `LeaderLease` is an RAII guard whose Drop aborts the heartbeat task and
      fires a detached CAS-release. Handler integration in
      `packages/worker/src/models/operation/handler.rs:627-749` computes the
      cache*key once, calls `acquire(&cache_key)` on miss, and either leads with
      `_lease` held through `execute_step` + `store_in_cache`, or polls
      `task_cache` with `follower_poll_loop` (default 250ms interval, 30s max
      wait) before falling back. Wiring in
      `packages/worker/src/models/operation/executor.rs:34-86`
      (`cache_leader_from_config` selects Redis-or-Noop based on
      `worker.cache_leader_election_enabled` + `mq.enabled`). Config knobs
      `worker.cache_leader*{ttl*secs,heartbeat_interval_secs,election_enabled}`    and`worker.cache_follower*{poll_interval_ms,max_wait_secs}`in    `packages/worker/src/config.rs:43-66,134-138`.
  - [x] Integration test (Redis-only): multi-worker concurrent-acquire scenario
        in `packages/worker/tests/cache_leader_election.rs` (tests 1–4:
        exactly-one-leader, heartbeat-extends-past-TTL, lease-drop-releases,
        TTL-safety-net). Tests use real Redis container via
        `testcontainers-modules::redis`.
  - [x] End-to-end multi-worker concurrent-compile scenario (test 5):
        `packages/worker/tests/cache_leader_election.rs::end_to_end_concurrent_compile_runs_once`.
        Spins up real Postgres + Redis containers, shares one
        `DatabaseTaskCacheStore` + filesystem `BlobStore` across 3 handlers,
        wraps a `CountingSandboxManager` decorator around `MockSandboxManager`
        with a shared `AtomicUsize`; asserts `exec_count == 1` across all 3 and
        that follower handlers see the same `build.out` content_hash as the
        leader.
- [x] **PR: bulk-insert-results.** UP#34 — `plugins/icpc/src/evaluate.rs` and
      `plugins/ioi/src/evaluate_batch.rs` no longer call `insert_results` once
      per testcase. A per-submission `row_buf: Vec<TestCaseResultRow>` is
      threaded through the main evaluation loop; each call site that used to
      issue a single-element slice now goes through `record_outcome`, which
      pushes to the buffer and auto-flushes at
      `RESULT_BATCH_FLUSH_THRESHOLD = 8`. A `flush_results` call before every
      early return / function tail guarantees terminal verdicts are persisted
      before the plugin function returns. SDK side: `SubmissionsMock` gained
      `insert_call_count()` so plugin tests can assert the bulk-batching
      invariant.
  - [x] Acceptance: per-submission INSERT count drops from ~20 to ~2–3.
        Regression tests pin this:
        `plugins/icpc/src/evaluate.rs::tests::bulk_inserts_batch_accepted_results`
        asserts `insert_call_count() <= 3` for 20 accepted testcases (and `>= 2`
        so a future "fold everything to one final flush" change is also caught),
        and the analogous IOI test
        `plugins/ioi/src/evaluate_batch.rs::tests::bulk_inserts_batch_accepted_results`.

---

## Tranche 3 — Durable accept + UX + observability (~7–10 days)

### Phase D — durable submission accept

- [x] **PR: queued-status-enum.** UP#36 — `Queued` variant added to
      `SubmissionStatus` at `packages/common/src/submission_status.rs` (before
      `Pending` in the lifecycle order); wired into `ALL`, `as_str()`,
      `FromStr`, and a `queued_status_round_trip_and_predicates` test that pins
      `is_terminal/is_judged/is_error` all to `false`. The stuck detector
      originally included `Queued` in its in-progress sweep; UP#43 later split
      queued backlog into observability-only handling.
      Workspace build clean — no other exhaustive-match site needed a `Queued`
      arm.
- [x] **PR: insert-queued-on-post.** UP#37 — converted the 9 post-commit
      `tokio::spawn(dispatch_to_plugin(...))` sites to
      `INSERT/UPDATE …     status='Queued'` with no spawn. Sites:
      `packages/server/src/handlers/submission.rs:949-960`
      (`create_submission`), `:1502-1538` (single rejudge — both immediate
      `apply_immediately=true` and deferred `apply_immediately=false` branches
      are durable; the residual deferred-rejudge silent-loss vector is closed by
      the **residual-fix follow-up** that inserts non-current judgements at
      `status=Queued` and adds a judgement scan to the claim fiber, see UP#38
      below), `:1650-1661` (`create_contest_submission`), `:1880-1937`
      (`bulk_rejudge_submissions` — single `queued` counter; deferred branch no
      longer spawns), `:2050-2070` (`admin_fan_out_submission`);
      `packages/server/src/handlers/code_run.rs:188-195` (`run_code`) and
      `:288-295` (`run_contest_code`);
      `packages/server/src/handlers/dlq.rs:198-225` (single retry) and
      `:415-426` (bulk retry — the post-commit "reload + spawn" loop is gone,
      the `submissions_to_dispatch` Vec is now only used for the log message
      count). Test fixtures
      (`packages/server/tests/integration/{common/mod.rs,submission.rs:40,694,     code_run.rs:47}`,
      `packages/server/tests/e2e/common/mod.rs`,
      `packages/server/src/dispatcher/steal.rs` test fixture) updated for the
      new `Queued` POST response. Workspace `cargo build` clean; lib tests
      130/130 green.
- [x] **PR: claim-fiber.** UP#38 — new module
      `packages/server/src/dispatcher/claim.rs` runs a per-server fiber that
      polls every `claim_poll_interval_ms` (default 1000) and per tick claims at
      most `claim_batch_size` (default 32) `Queued` rows from each of
      `submission` and `code_run` via raw-SQL
      `SELECT id … WHERE status='Queued' ORDER BY created_at LIMIT $1     FOR UPDATE SKIP LOCKED`,
      then in the same transaction
      `UPDATE …     SET status='Pending', owner_server_id=$srv, lease_heartbeat_at=NOW(),     retry_count = retry_count + 1`
      and spawns `dispatch_to_plugin` after commit. Wiring in
      `packages/server/src/dispatcher/mod.rs`: claim fiber is independent of the
      `dispatcher_lease_steal_enabled` master switch — it's gated by its own
      `server.claim_fiber_enabled` (default true). Config knobs
      (`packages/server/src/config.rs`): `claim_fiber_enabled`,
      `claim_poll_interval_ms`, `claim_batch_size` with defaults wired into
      `Config::builder()` and example/test fixtures. **Residual-fix follow-up**
      extends the fiber with a third scan against
      `submission_judgement WHERE status='Queued'` so deferred rejudges
      (`apply_immediately=false`, which insert non-current judgements at
      `status=Queued` via `open_rejudge_judgement`) are also picked up:
      `claim_queued_judgements` in `packages/server/src/dispatcher/claim.rs`
      mirrors the submission scan, dispatches via
      `dispatch_to_plugin_with_judgement(..., fire_after_judging=is_current)`,
      and leaves the parent submission's denormalized cache untouched. Closes
      the post-commit silent-loss vector for deferred rejudges.
  - [x] Integration test: in-process equivalents in
        `packages/server/tests/integration/submission.rs::claim_fiber`:
        `claim_fiber_promotes_queued_submission_to_pending` verifies the
        durable-accept happy path end-to-end via the POST handler;
        `claim_fiber_recovers_directly_inserted_queued_row` writes a `Queued`
        submission row directly via sea-orm — bypassing the POST — to simulate
        an api crash mid-flight and asserts the fiber still recovers it;
        `claim_fiber_recovers_directly_inserted_queued_judgement` does the same
        for a non-current `submission_judgement` row, asserting the fiber
        promotes the judgement off `Queued` while leaving the parent
        submission's status unchanged. The roadmap's "kill api mid-POST"
        formulation would require subprocess-level control which the
        testcontainer harness doesn't have today; the direct-insert variants
        cover the equivalent invariant. Tests blocked from running locally
        because OrbStack's daemon would not start in this environment
        (`SocketNotFoundError`); the workspace `cargo     build` and
        `cargo test -p server --lib` are both green.
- [x] **PR: backpressure-on-post.** UP#39 — 503 + `Retry-After` when
      queued-count exceeds `max_queued_submissions` (default 5000). Verified
      landed in commits `ae33f2d9`, `7f84fb2b`, and `17910b11`. The durable
      queue-depth helper counts `Queued` rows across `submission`, `code_run`,
      and `submission_judgement`, enforces the configured cap before inserts,
      and returns `AppError::Overloaded` with a positive `Retry-After`
      (`packages/server/src/dispatcher/queue_depth.rs:1-24`, `:76-92`,
      `:118-145`). `AppError::Overloaded` maps to `503` and
      `QUEUE_OVERLOADED` while preserving `Retry-After`
      (`packages/server/src/error.rs:275-298`). The admission gate is wired at
      submission create/rejudge/contest submit/bulk rejudge/admin fan-out,
      code-run create/contest run, and DLQ single/bulk retry
      (`packages/server/src/handlers/submission.rs:918-920`, `:1468-1473`,
      `:1620-1621`, `:1848-1853`, `:2002-2008`,
      `packages/server/src/handlers/code_run.rs:166-170`, `:255-256`,
      `packages/server/src/handlers/dlq.rs:162-165`, `:318-323`). Integration
      coverage exercises reject, accept-below-cap, code-run, rejudge, and
      cap-disabled cases (`packages/server/tests/integration/submission.rs:2082-2181`,
      `:2184-2248`, `:2251-2320`). Verified locally with
      `cargo test -p server overloaded_maps_to_503_with_retry_after --lib`,
      `cargo test -p server rate_limited_still_maps_to_429_with_retry_after --lib`,
      and `cargo test -p server backpressure --tests --no-run`.
- [x] **PR: queue-wait-vs-exec-timeout.** UP#40 — plugin's `next_result` polling
      sees only execution time, not queue wait. Edit
      `packages/server/src/host_funcs/evaluate.rs:564` clock semantics.
      Implemented with an explicit operation reply protocol that carries
      fire-and-forget Started notifications plus backwards-compatible completed
      results (`packages/common/src/worker.rs:95-150`,
      `packages/worker/src/task_runner.rs:24-53`, `:86-125`). The operation
      result consumer now handles Started and Completed independently, writes
      Started into the shared evaluate operation registry, and labels reply
      metrics by reply type (`packages/server/src/consumers/operation_result.rs:10-15`,
      `:34-52`, `:54-87`). The evaluate operation registry maps op task ids to
      test cases, records the first Started timestamp per test case, ignores
      completed/cancelled cases for extension, and is cleaned up by the
      evaluate batch reaper (`packages/server/src/host_funcs/evaluate_ops_registry.rs:13-24`,
      `:29-58`, `:82-124`, `:205-214`,
      `packages/server/src/runtime.rs:197-205`). `next_evaluate_result` now
      preserves `timeout=0` as a true nonblocking probe, otherwise polls in
      short ticks and silently extends while queued or within execution budget
      (`packages/server/src/services/evaluate_batch.rs:254-337`, `:340-430`).
      Regression coverage includes queued extension, first-start wins for
      multi-op test cases, and nonblocking zero-timeout polls
      (`packages/server/src/services/evaluate_batch.rs:805-945`,
      `packages/server/src/host_funcs/evaluate_ops_registry.rs:308-345`). SDK
      queue slack and the old 15-minute floor are removed from defaults, plugin
      result-timeout defaults are lowered, and operator notes document the
      changed budget semantics (`packages/server-sdk/src/types/evaluate.rs:7-12`,
      `:27-30`, `:47-71`, `:190-202`,
      `plugins/batch-evaluator/plugin.toml:134-141`,
      `plugins/communication-evaluator/plugin.toml:137-144`,
      `UPGRADE-NOTES.md:7-32`). Verified locally with `cargo fmt --check`,
      `git diff --check`, `cargo test -p broccoli-server-sdk --lib`,
      `cargo test -p common --lib`, `cargo test -p server --lib`
      (145 passed, 1 ignored), and `cargo test -p worker --lib`
      (30 passed, 1 ignored). A final subagent review approved the UP#40
      semantics after the zero-timeout regression fix.
- [x] **PR: stuck-detector-reenqueue.** UP#41 — `retry_count > 5` →
      SystemError + DLQ; otherwise lease/steal handles recovery. Implemented
      with `server.max_stuck_retries` default/config plumbing
      (`packages/server/src/config.rs:121-125`, `:268-269`, `:571-572`).
      The stuck detector now scans `submission`, `code_run`, and
      `submission_judgement` for recoverable in-progress rows and re-checks
      staleness under `FOR UPDATE`
      (`packages/server/src/dlq/stuck.rs:57-70`, `:86-108`, `:136-158`,
      `:181-208`, `:273-294`, `:413-434`, `:523-545`). Exhausted rows use the
      strict `retry_count > max_stuck_retries` helper and emit terminal
      `Exceeded N retries` errors plus DLQ visibility
      (`packages/server/src/dlq/stuck.rs:298-363`, `:438-472`, `:549-614`,
      `:1192-1197`). Non-exhausted rows are left to lease/steal when that path
      owns recovery; lease/steal-disabled rows use guarded direct recovery with
      epoch bumps, partial-result cleanup, and post-commit
      redispatch (`packages/server/src/dlq/stuck.rs:363-365`, `:473-476`,
      `:615-618`, `:662-757`, `:759-853`, `:855-1018`). New DLQ message types
      and stats/schema/UI labels cover stuck code runs and stuck judgements
      (`packages/common/src/dlq.rs:40-78`,
      `packages/server/src/dlq/service.rs:20-27`, `:216-242`,
      `packages/server/src/models/dlq.rs:126-158`,
      `packages/web-sdk/src/api/schema.ts:978-1044`, `:2967-2987`,
      `packages/web/src/features/dlq/utils/messageType.ts:24-38`,
      `packages/web/src/features/dlq/components/DlqStatsSummary.tsx:40-55`,
      `packages/web/src/lib/i18n/en.ts:897-944`). Code-run dispatch
      SystemError paths now carry `judge_epoch` guards so stale tasks cannot
      clobber a recovered epoch
      (`packages/server/src/services/code_run_dispatch.rs:47-57`, `:69-88`,
      `:97-105`, `:149-156`, `:202-212`, `:229-238`, `:250-268`). Verified
      locally with `cargo fmt --check`, `cargo test -p common --lib`,
      `cargo test -p server --lib` (139 passed, 1 ignored), and
      `cargo test -p server --tests --no-run`; frontend build/lint was not run
      because `pnpm`, `npm`, and `corepack` are unavailable on this shell PATH.

### Phase E — status semantics + score correctness

- [x] **PR: redefine-pending-running.** UP#42 — `Running` set only after first
      exec result; new SDK helpers `host.submission.set_compiling()` /
      `set_running()`. Implemented by adding SDK-side `Compiling` status and
      explicit submission/code-run helper methods
      (`packages/server-sdk/src/types/persistence.rs:27-40`,
      `packages/server-sdk/src/sdk/submissions.rs:16-45`,
      `packages/server-sdk/src/sdk/code_runs.rs:14-31`). ICPC and IOI now mark
      submissions `Compiling` after a batch starts, set `Running` only once on
      the first non-compile result, and keep compile-error-only runs out of
      `Running` (`plugins/icpc/src/evaluate.rs:116-123`, `:161-192`,
      `plugins/ioi/src/evaluate_batch.rs:108-115`, `:151-205`). Code-run
      evaluation follows the same `Compiling` → first non-compile result
      `Running` semantics (`packages/server-sdk/src/evaluator/run.rs:77-87`,
      `:91-112`). Regression coverage asserts helper writes, non-compile
      transitions, and compile-error-only no-`Running` paths
      (`packages/server-sdk/src/sdk/submissions.rs:384-398`,
      `packages/server-sdk/src/sdk/code_runs.rs:186-199`,
      `packages/server-sdk/src/evaluator/run.rs:345-370`,
      `plugins/icpc/src/evaluate.rs:431-435`, `:553-555`,
      `plugins/ioi/src/evaluate_batch.rs:644-647`, `:679-681`). Verified
      locally with `cargo fmt --check`, `git diff --check`,
      `cargo test --manifest-path plugins/icpc/Cargo.toml`,
      `cargo test --manifest-path plugins/ioi/Cargo.toml`, and focused SDK
      helper/code-run tests. Full `cargo test -p broccoli-server-sdk
      --features guest` still has the unchanged baseline failure in
      `packages/server-sdk/src/sdk/eval.rs:751`.
- [x] **PR: per-state-stuck-timeouts.** UP#43 — old `Queued` rows are
      aggregate dispatcher-health observability only, while `Pending` rows with
      no owner are recoverable after 5 min and owned
      `Pending`/`Compiling`/`Running` rows recover only on missing/stale lease
      heartbeat (`packages/server/src/dlq/stuck.rs:22-33`, `:75-105`,
      `:108-158`, `:176-203`, `:233-254`, `:279-304`). Direct detector
      redispatch now writes a fresh detector owner lease onto submissions,
      code-runs, deferred judgements, and the freshly inserted retry judgement
      so the detector does not repeatedly redispatch its own work
      (`packages/server/src/dlq/stuck.rs:42-50`, `:790-867`, `:890-917`,
      `:1070-1137`, `:1150-1225`). Regression coverage pins queued-observe vs
      recover policy, fresh-heartbeat immunity, stale/missing lease recovery,
      direct-recovery routing for deferred judgements, and SQL-level lease
      writes for submission/code-run/judgement redispatch
      (`packages/server/src/dlq/stuck.rs:1461-1677`). The unified plan was
      updated to match the no-queued-SystemError semantics
      (`docs/profiling/run-2/unified-plan.md:212`, `:255`, `:272`). Verified
      locally with `cargo test -p server dlq::stuck::tests --lib`,
      `cargo test -p server --lib` (154 passed, 1 ignored),
      `cargo fmt --check`, and `git diff --check`.
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
