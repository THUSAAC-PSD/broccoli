# Worktree Review Synthesis — Tranche 1

**Source:** four parallel review agents over
`judging-reliability-phase1-recovered` worktree (branch
`feature/judging-reliability-phase1`, ~4500 +/-672 lines across 79 files + 6 new
files). **Reviewed against:** master @ 96421d7. **Companion to:** `roadmap.md`,
`unified-plan.md`.

This is the single source of truth for what to do with each worktree hunk before
PR-staging. Each section maps reviewer findings to concrete merge actions.

---

## 1. Implementation status by item

| UP#   | Item                                       | Status                          | Action                                                       |
| ----- | ------------------------------------------ | ------------------------------- | ------------------------------------------------------------ |
| UP#1  | spawn_blocking cascade fix                 | **Absent**                      | Greenfield. Implement from scratch.                          |
| UP#2  | Runtime builder, max_blocking_threads=1024 | **Absent**                      | Greenfield.                                                  |
| UP#3  | Result consumer concurrency=8              | **Absent + regression**         | Worktree file strips master's metrics. Rebase, then change.  |
| UP#4  | Drop PoolTimeout -> 429                    | Present                         | PR ready (clean).                                            |
| UP#5  | Stuck redispatch via tokio::spawn          | **Absent**                      | Greenfield.                                                  |
| UP#6  | worker.max_concurrency config              | Present                         | PR ready.                                                    |
| UP#7  | Heartbeat fields + SystemInfo              | Present                         | PR ready.                                                    |
| UP#8  | Toolchain fingerprint                      | Absent (§0 prereq)              | Greenfield, ships first.                                     |
| UP#9  | Reflink fast-path                          | Absent                          | Greenfield.                                                  |
| UP#10 | max_cache_size 4 GiB default               | **Absent**                      | One-line change, bundle with UP#6.                           |
| UP#11 | HEAD probe in upload_from_path             | Absent                          | Greenfield.                                                  |
| UP#12 | Blob cache metrics                         | **Absent + regression**         | Worktree metrics.rs is stripped. Rebase, then add.           |
| UP#13 | Host-fn metrics                            | **Absent + regression**         | Same as UP#12.                                               |
| UP#14 | Verdict::Cancelled enum                    | Present                         | PR ready. UI + stress-test mirror complete.                  |
| UP#15 | Lease columns                              | Present                         | PR ready modulo C1 (is_current).                             |
| UP#16 | Server heartbeat                           | Present                         | PR ready.                                                    |
| UP#17 | Lease refresher                            | Present                         | PR ready.                                                    |
| UP#18 | Steal scanner                              | Present                         | **PR blocked on C1: is_current=FALSE inverted**              |
| UP#19 | Wide-net 6h timeout                        | **Absent**                      | Greenfield (config-default change).                          |
| UP#20 | dedup_ttl_secs split                       | Present                         | PR ready.                                                    |
| UP#21 | Sweeper scaffold                           | Present                         | PR ready.                                                    |
| UP#22 | Sweeper deletion (flip flag)               | Present (gated by dry_run=true) | PR ready.                                                    |
| UP#23 | release env flag flip                      | **Absent**                      | Greenfield (env file edits).                                 |
| UP#24 | Cancel host fns + Lua                      | Present                         | **PR blocked on C-Phase-C1: phantom registry entries**       |
| UP#25 | Worker EXISTS fast-path                    | Present                         | PR ready.                                                    |
| UP#26 | SDK cancel wrappers                        | Present                         | **PR blocked on C-Phase-C2: partial-cancel on first error**  |
| UP#27 | DispatcherSemaphore                        | Present                         | PR ready. All 9 sites guarded.                               |
| UP#28 | start_windowed                             | Present (partial)               | communication-evaluator not opted in; minor wait_cursor bug. |
| UP#29 | Fleet capacity monitor                     | Present                         | PR ready. Cross-tranche split needed.                        |
| UP#30 | ICPC Cancelled rewrite                     | Present                         | PR ready.                                                    |
| UP#31 | IOI testcase filter                        | Present                         | PR ready.                                                    |
| UP#32 | IOI subtask short-circuit                  | Present                         | PR ready. Sibling-active check verified.                     |
| UP#35 | Pool sizing config + runbook               | **Absent (§0 prereq)**          | Greenfield, ships first.                                     |

**Tally:** Present 18 / Absent 12 / Regression 3 / Bugged 4.

---

## 2. Critical bugs to fix before any merge

Each entry: file, line, fix, owning PR.

### P-CRIT-1: `is_current = FALSE` inverted in steal path

- File: `packages/server/src/dispatcher/steal.rs:757-759` and `lease.rs:84`
- Fix: change to `is_current = TRUE` for both queries. The active row is the one
  being judged; `is_current = FALSE` rows are superseded history.
- Owns: PR-D (UP#18). Block merge until fixed + regression test added.

### P-CRIT-2: `mark_cancelled` creates phantom registry entries

- File: `packages/server/src/host_funcs/evaluate_ops_registry.rs:65-79`
- Fix: replace `entry(...).or_insert_with(...)` with `get(...)`. Phantom
  `EvaluateBatchOps::default()` entries leak DashMap memory permanently when a
  plugin cancels an already-removed batch.
- Owns: PR-K (UP#24). Block merge until fixed.

### P-CRIT-3: `cancel_test_cases` early-returns on first error

- File: `packages/server-sdk/src/sdk/eval.rs:156-163`
- Fix: collect errors, attempt all batches, return first error after the loop.
  IOI subtask short-circuit (UP#32) depends on this to stop sibling-batch worker
  compute.
- Owns: PR-M (UP#26). Block merge until fixed.

### P-CRIT-4: `bulk_retry_dlq` holds 10k FOR UPDATE locks in one txn

- File: `packages/server/src/handlers/dlq.rs:332-453`
- Fix: process in batches of <=100 per transaction; commit between batches. Drop
  FOR UPDATE on inner messages or keep but cap batch size.
- Owns: PR-H sibling (Phase B handlers). Block merge until fixed.

### P-CRIT-5: `claim_submissions` re-reads after txn.commit (TOCTOU)

- File: `packages/server/src/dispatcher/steal.rs:315-323`
- Fix: load models inside the transaction before commit, or use update result
  directly.
- Owns: PR-D (UP#18).

### P-CRIT-6: host_context.rs TLS incompatible with spawn_blocking

- File: `packages/plugin-core/src/host_context.rs:11-18`
- Fix: capture context into closure, not TLS. When UP#1 lands, the host_fn
  wrapper must do `with_context(ctx, || ...)` _inside_ the spawn_blocking
  closure, after the thread hop.
- Owns: PR-cascade-fix (UP#1). Greenfield work.

### P-CRIT-7: worktree consumers/metrics files are regressions vs master

- Files: `packages/server/src/consumers/operation_result.rs`,
  `packages/common/src/metrics.rs`
- Fix: do NOT merge worktree versions as-is. For each, start from master, apply
  only the intended UP#3/UP#12/UP#13 hunks. Two cherry-pick passes, one per
  file.
- Owns: PR-result-consumer-concurrency (UP#3), PR-cache-metrics (UP#12),
  PR-host-fn-metrics (UP#13).

---

## 3. Important issues (fix during PR-staging)

| ID      | File:line                                                   | Issue                                                                                                                                           | Owning PR                  |
| ------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| P-IMP-1 | `host_funcs/cancel.rs:1`                                    | `CANCEL_KEY_TTL_SECS = 21_600` hard-coded; once UP#19 raises stuck timeout, invariant breaks silently. Add doc assertion or derive from config. | PR-K                       |
| P-IMP-2 | `host_funcs/dispatch.rs:191-204, 270-293, 330-361, 418-432` | `block_in_place(Handle::block_on)` at 4 sites. Auto-collapses to no-ops after UP#1 lands; document dependency.                                  | PR-K (note), gated by UP#1 |
| P-IMP-3 | `server-sdk/src/sdk/eval.rs:315-318`                        | `wait_cursor` modulo skips position after batch removal. Minor fairness; fix: `self.wait_cursor = index + 1`.                                   | PR-T                       |
| P-IMP-4 | `dispatcher/fleet_capacity.rs:71-79`                        | `forget_permits` TOCTOU on shrink — can drain pool to zero under shrink+release race.                                                           | PR-V                       |
| P-IMP-5 | `dispatcher/sweeper.rs:121-128`                             | `DEAD_SINCE` TTL 24h vs grace 1h; set to `GHOST_QUEUE_GRACE_SECS * 3` for sanity.                                                               | PR-G                       |
| P-IMP-6 | `dispatcher/steal.rs:37`                                    | `retry_count` exhaustion off-by-one vs roadmap spec. Code exhausts at `==5`, plan says `>5`. Reconcile.                                         | PR-D                       |
| P-IMP-7 | `handlers/submission.rs:779`                                | `IsCurrent.eq(false)` filter on lease claim is fragile; only safe given the current call-site invariant. Document or restructure.               | PR-S                       |
| P-IMP-8 | Multiple sites in `submission.rs`, `code_run.rs`            | Unreachable-after-SELECT error paths missing `// Unreachable in tests:` annotations per CLAUDE.md convention. ~8 sites.                         | Bundle with each owning PR |
| P-IMP-9 | `release/docs/operator-runbook.md:127-128, 209-215`         | Premature documentation of UP#28/UP#29 features that aren't merged yet. Reword to qualify "when this lands" or strip.                           | PR-runbook (§0 prereq)     |

---

## 4. AI-like comments and release-era labels to strip

### "Phase 1b" / "Phase 2" labels in production code (must strip)

- `packages/server/src/config.rs:68` — comment `"Phase 1b master switch"`
- `packages/server/src/config.rs:70` — `"Phase 2 admission-control switch"`
- `packages/server/src/config.rs:98` — `"Phase 2 master switch"`
- `packages/server/src/config.rs:102` — `"Phase 2 admission switch"`
- `packages/server/src/dispatcher/mod.rs:29` — log:
  `"Phase 1b lease/steal disabled by config"`
- `packages/server/src/dispatcher/mod.rs:69-79` — log:
  `"Phase 1b dispatcher foundation started"`

Replace each with a plain description of the function, not the
implementation-era phase.

### General AI-comment scan: clean

- Phase A surface: clean.
- Phase B: minor narrative blocks in `entity/submission_judgement.rs:5-12` and
  `config.rs:56-65, 92-96` — acceptable as doc comments, no em dashes.
- Phase C: minor multi-line doc comments on `dispatch.rs:54-58` and
  `icpc/src/evaluate.rs:27-29, 208-215` — acceptable.
- Cross-cutting: 2 em-dash uses, both idiomatic.
- No `NOTE:` / `IMPORTANT:` / `Critical:` filler comments anywhere.

**Verdict:** The "Phase N" labels are the only systematic AI-pattern issue. Em
dashes and narrative blocks are within acceptable bounds.

---

## 5. PR boundary splits needed

These worktree files mix code from multiple roadmap items and must be split
during PR-staging:

### `handlers/submission.rs`

- Phase B (lease claim logic, `claim_submission_judgement_dispatch_lease`,
  `reset_submission_for_retry`)
- Phase C (`dispatcher_permits.reserve()` calls at all 9 dispatch sites, 503 on
  overflow)
- **Action:** PR-A's submission.rs hunks for lease + PR-S's hunks for permits
  must be separable. Use `git add -p` to split.

### `handlers/code_run.rs`

- Same Phase B/C mix.
- **Action:** Same split treatment.

### `host_funcs/evaluate.rs`

- UP#24 (`cancel_evaluate_test_cases_fn`) — Phase C cancel
- UP#28 windowing host-side dispatch
- **Action:** Land both in PR-K (UP#24) since the file is unitary; PR-T (UP#28)
  becomes SDK-only.

### `server/src/dispatcher/`

- Phase B: `lease.rs`, `steal.rs`, `sweeper.rs`, `mod.rs`
- Phase C: `permits.rs` (UP#27), `fleet_capacity.rs` (UP#29)
- **Action:** Phase B PRs land the lease/steal/sweeper files with
  `dispatcher_semaphore_enabled = false` default. Phase C PR-S adds permits.rs
  and flips the flag. Phase C PR-V adds fleet_capacity.rs.

### `state.rs`, `lib.rs`, `main.rs`

- Touched by every Phase B and Phase C PR.
- **Action:** Sequenced merges; rebase each PR on the previous one. No way to
  fully atomize without N-way conflicts.

---

## 6. Worktree gaps requiring greenfield work

Tasks not present in worktree, must be implemented before Tranche 1 closes:

1. **UP#1 cascade fix** (`plugin-core/traits.rs:336`) — plus redesign
   `host_context.rs` to capture context into closure.
2. **UP#2 runtime builder** (`server/main.rs`) — replace `#[tokio::main]`.
3. **UP#3 result-consumer concurrency** — rebase consumers/operation_result.rs
   onto master, then set `Some(8)`.
4. **UP#5 stuck redispatch** (`dlq/stuck.rs:170`) — replace
   `mark_submission_system_error` with
   `tokio::spawn(dispatch_submission_to_plugin(...))`.
5. **UP#10 max_cache_size 4 GiB** — two-site config default change.
6. **UP#12/UP#13 metrics** — rebase metrics.rs onto master, add `blob_cache_*` +
   `host_fn_*` fields, instrument call sites.
7. **UP#9 reflink fast-path** — `file_cacher.rs` `std::os::unix::fs::link`
   before `tokio::fs::copy`.
8. **UP#11 HEAD probe** — `upload_from_path` short-circuit.
9. **UP#19 wide-net 6h timeout** — config default change.
10. **UP#23 release env flag flip** — `.env.server.example` edits.
11. **UP#8 toolchain fingerprint** (§0 prereq) — worker boot hash computation.
12. **UP#35 pool sizing config + runbook section** (§0 prereq).
13. **Integration test harness** (§0 prereq).
14. **`tests/integration/dlq.rs`** — PR-H acceptance: ghost-queue lifecycle
    test.

---

## 7. Pre-merge schema gaps

- **Missing index `(status, lease_heartbeat_at)`** on `submission` and
  `code_run` tables. PR-A spec requires this. Not visible in entities or in
  `database.rs` ADD COLUMN block; verify `seed::ensure_indexes`.
- **`judge_epoch` on `submission_judgement`** added via SeaORM schema-sync but
  not in `database.rs` manual ADD COLUMN list. Verify schema-sync applies
  `DEFAULT 0` for existing rows.
- **`judgement_id` backfill** on `test_case_result` — relies on
  `seed::backfill_submission_judgements` running before any read filters on
  `judgement_id`. Verify startup order.

---

## 8. Merge sequencing

Updated from `roadmap.md` Tranche 1 ordering based on review findings.

### Block 0 — Hard prerequisites (greenfield, no worktree content)

1. UP#8 toolchain fingerprint
2. UP#35 pool sizing config + runbook
3. Integration test harness scaffold

### Block 1 — Phase A worktree-clean PRs (no bugs, ready)

4. UP#14 Verdict::Cancelled enum + UI + stress-test mirror
5. UP#4 drop PoolTimeout->429
6. UP#6 worker.max_concurrency config (bundle UP#10 cache size here)
7. UP#7 heartbeat fields + SystemInfo

### Block 2 — Phase A greenfield (cascade fix path)

8. UP#1 spawn_blocking + host_context.rs redesign
9. UP#2 runtime builder
10. UP#3 result-consumer concurrency (rebased on master)
11. UP#5 stuck redispatch interim
12. UP#9 reflink fast-path
13. UP#11 HEAD probe
14. UP#12 blob cache metrics (rebased)
15. UP#13 host-fn metrics (rebased)

### Block 3 — Phase B worktree-clean PRs

16. PR-A UP#15 lease schema (+ verify index)
17. PR-B UP#16 server heartbeat
18. PR-C UP#17 lease refresher (fix is_current first)
19. PR-D UP#18 steal scanner (fix P-CRIT-1, P-CRIT-5, P-IMP-6)
20. PR-F UP#20 dedup ttl split
21. PR-G UP#21 sweeper scaffold (fix P-IMP-5)
22. PR-H UP#22 sweeper deletion (config flip)
23. PR-H' bulk_retry_dlq batching fix (P-CRIT-4)

### Block 4 — Phase B greenfield

24. UP#19 wide-net 6h timeout (depends on Phase A.5 UP#14f first)
25. UP#23 release env flag flip

### Block 5 — Phase C worktree-clean PRs

26. PR-S UP#27 DispatcherSemaphore (extract from handlers)
27. PR-K UP#24 cancel host fns (fix P-CRIT-2, P-IMP-1, P-IMP-2)
28. PR-L UP#25 worker EXISTS fast-path
29. PR-M UP#26 SDK cancel wrappers (fix P-CRIT-3)
30. PR-T UP#28 start_windowed (fix P-IMP-3, opt in communication-evaluator)
31. PR-V UP#29 fleet capacity monitor (fix P-IMP-4)
32. PR-W UP#30 ICPC Cancelled rewrite
33. PR-X UP#31 IOI testcase filter
34. PR-Y UP#32 IOI subtask short-circuit
35. PR-DD UP#20 cleanup tick (already merged as PR-F above)

### Block 6 — Phase A.5 (Tranche 2, but UP#14f gates Phase B PR-E above)

36. UP#14f stuck.rs created_at->updated_at fix
37. ...remainder of Phase A.5 per roadmap

---

## 9. Comments-to-strip explicit list

Strip these specific lines/blocks before staging the owning PR:

```
packages/server/src/config.rs:68     "Phase 1b master switch"
packages/server/src/config.rs:70     "Phase 2 admission-control switch"
packages/server/src/config.rs:98     "Phase 2 master switch"
packages/server/src/config.rs:102    "Phase 2 admission switch"
packages/server/src/dispatcher/mod.rs:29     info! string
packages/server/src/dispatcher/mod.rs:69-79  info! string
```

Replace each with a plain functional description.

---

## 10. Outstanding gaps for Tranche 1 close

- Stress-test scenarios: no `expected_verdict = Cancelled` scenario. Add after
  UP#24/UP#25 merge.
- `install.sh` does not propagate `fairness.env` automatically. Either
  source-include or print a post-install reminder.
- Worktree does not contain `tests/integration/dlq.rs`. Required as PR-H
  acceptance test.

---

**End of synthesis.** Use this doc, not the individual review transcripts, as
the implementation reference.
