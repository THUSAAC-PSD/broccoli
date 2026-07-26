# Dedup backlog

Deferred extractions from the July 2026 duplication audit. Each entry is a
known, _aware_ copy pair — the second author knew about the first — left in
place because extraction was judged non-trivial at the time. When you touch one
of these areas, prefer doing the extraction over adding a third copy.

Done already (for reference on the pattern):

- `services/windowed_session.rs` — shared windowed-session engine extracted from
  `operation_batch.rs` / `evaluate_batch` (commit e618f98); a later
  drain-on-refill-failure fix then landed once instead of twice.
- `entity/judgement_reset.rs` — single source for the judged-output column clear
  list used by `dispatcher/steal.rs`, `dlq/stuck/recovery.rs`, and
  `services/submission_dispatch.rs` (whose pre-extraction judgement reset was
  missing `CompileOutput`).
- `Verdict::severity` — canonical table now lives only in
  `broccoli_types::types::Verdict`; `common::Verdict` delegates and carries a
  drift-pin test.

## Open items

1. **Server test-support crate** — biggest single duplication.
   `packages/server/tests/e2e/common/mod.rs` (~1,291 lines) is a fork of
   `packages/server/tests/integration/common/mod.rs` (~1,265 lines):
   testcontainers Postgres/Redis singletons, admin-DB runtime workaround,
   AppConfig/AppState builders, DB-per-test locking, user/role seeding. The e2e
   copy has since gained Redis + worker wiring the integration copy lacks;
   harness fixes must be made twice. Fix: a dev-dependency `test-support` crate
   (cargo's per-tree test binaries are why the fork happened; a shared crate is
   the sanctioned way around that).

2. **Move `push_judge_sets` / `push_double_opt[_str]` into `broccoli-types`**.
   `packages/server-sdk/src/sdk/shared.rs:49-108` is mirrored verbatim in
   `packages/server/src/host_funcs/submissions.rs:69-128` (the host copy's doc
   comment cites the guest source). `broccoli-types` exists precisely to hold
   shared host/guest code; the SET-list semantics belong there once.

3. **Repoint `plugin-core/src/http.rs` at `broccoli-types`**. `PluginHttpAuth` /
   `PluginHttpRequest` are redefined verbatim
   (`packages/broccoli-types/src/types/http.rs:4-66` vs
   `packages/plugin-core/src/http.rs:2-38`) even though plugin-core already
   depends on broccoli-types. Serde shapes are kept in sync by hand today.

4. **Extism host-fn prologue macro**. The deserialize-input → lock UserData →
   open-span prologue is hand-rolled ~18 times across
   `packages/server/src/host_funcs/*.rs`. A generic wrapper or macro collapses
   hundreds of lines and makes new host fns harder to get wrong.

5. **Generic problem-file upload handler**.
   `packages/server/src/handlers/additional_file.rs` is a near-verbatim copy of
   `handlers/attachment.rs` (same multipart loop, filename/path validation,
   txn + re-check; only a `language` field differs). Low-level helpers are
   already shared; extract the handler-level orchestration.

6. **Parametrized sandbox test harness**.
   `packages/worker/tests/worker_mock_sandbox.rs` (1,539 lines) and
   `worker_isolate_sandbox.rs` (898 lines) were cloned from each other on the
   same day and have drifted in coverage (newer cases exist only in the mock
   suite). A harness generic over `SandboxManager` restores parity.

7. **Sync/async result-wait loop skeletons**. `services/operation_batch.rs`
   (`next_operation_result[_async]`) vs `services/evaluate_batch/result_wait.rs`
   (`next_evaluate_result[_async]`) remain parallel ~300-line skeletons after
   the windowed-session extraction, differing only in extension policy and
   channel types. Both already carry a matching drain-delivered-result fix —
   next shared fix should trigger the generic unification that was deferred.

8. **Shared API-client auth/retry protocol** (defensible split, lowest
   priority). `packages/cli-core/src/client.rs` (sync ureq) vs
   `packages/stress-test/src/client.rs` (async reqwest) both maintain login +
   token storage + retry-on-401 against the same server API.
