# Stress Test — Plugin Strategy Decision

**Question:** How should the stress test obtain valid `contest_type` /
`problem_type` values for the problems it creates?

## Findings

### Plugin registration mechanism

Plugins call host functions to register types:

- `register_contest_type` —
  `{ "type": String, "submission_handler": String, "code_run_handler": String }`.
  Input struct at `packages/server/src/host_funcs/registry.rs:11-18`; host fn at
  `:164-207`.
- `register_evaluator` — `{ "type": String, "handler": String }`. Input struct
  at `:20-26`; host fn at `:209-232`.

Handlers land in `RwLock<HashMap<...>>` registries: `ContestTypeRegistry` at
`packages/server/src/registry.rs:42-43`, `EvaluatorRegistry` at `:45-46`.
Manifests must declare `"plugin:register"` in `[server].permissions`. Plugins
call via the guest SDK, e.g.
`host.registry.register_contest_type("icpc", "handle_icpc_submission", "handle_icpc_code_run")?`
at `plugins/icpc/src/lib.rs:28-32`.

### Existing plugin landscape

Repo root `plugins/` (the active directory; `packages/plugins/` does not exist):

| Dir                       | Role                          | LOC (`src/*.rs`)         |
| ------------------------- | ----------------------------- | ------------------------ |
| `batch-evaluator/`        | Registers evaluator `batch`   | 657 (lib 93 + batch 564) |
| `icpc/`                   | Registers contest type `icpc` | 1289                     |
| `ioi/`                    | Registers contest type `ioi`  | (not counted)            |
| `standard-checkers/`      | Registers 9 checker formats   | 189                      |
| `standard-languages/`     | Registers language resolvers  | (not counted)            |
| `cooldown/`               | Submission-cooldown hook      | 199                      |
| `submission-limit/`       | Submission-count hook         | (not counted)            |
| `communication-evaluator` | Evaluator (interactive)       | (not counted)            |
| `config-ui-test/`         | UI test fixture               | (not counted)            |
| `broccoli-zh-cn/`         | Translations submodule        | n/a                      |

### Cost of a minimal fixture plugin

A minimal contest-type+evaluator plugin needs `plugin.toml` (~10 lines,
declaring `[server] entry` and permissions including `plugin:register`,
`operations:dispatch`, `checker:run`), `src/lib.rs` `init()` (~10 lines), a
contest `handle_submission` (`OnSubmissionInput` shape at
`packages/server-sdk/src/types/submission.rs:14-30`), and an evaluator. The
evaluator is the load-bearing piece: `plugins/batch-evaluator/src/batch.rs` (564
LOC) compiles, runs, and checks via `host.operations.start_batch`/`next_result`
— there is no shorter path since the runtime ships no built-in evaluator.

**Realistic estimate:** ~700 LOC if we duplicate `batch-evaluator`'s pipeline;
~50 LOC if the contest plugin delegates each test case to the already-loaded
`batch` evaluator via `host.evaluate.dispatch_single`. Either way: separate
Cargo crate, `wasm32-wasip1` toolchain in CI, server-sdk dependency that breaks
on SDK churn, .wasm bundled via `include_bytes!`.

### Runtime discovery viability

**Yes — fully exposed and unauthenticated.**

`GET /api/v1/plugins/registries` (handler
`packages/server/src/handlers/plugin.rs:21-76`, route mounted at
`packages/server/src/routes/v1.rs:15,98`). Returns `RegistriesResponse`
(`packages/server/src/models/plugin.rs:27-43`):

```json
{
  "problem_types": ["batch", "interactive"],
  "checker_formats": ["exact", "tokens", ...],
  "contest_types": ["icpc", "ioi"],
  "languages": [...]
}
```

No `auth_user` extractor on the handler; no `require_permission` call — public
read.

There is also `GET /api/v1/admin/plugins` (`handlers/admin.rs:39-54`) which
lists all plugins and their manifests, but it requires `plugin:manage`
permission.

### Server boot guarantees

`server::main` calls `sync_plugins(&state)` at
`packages/server/src/main.rs:209`. `sync_plugins`
(`packages/server/src/utils/plugin.rs:117-154`) calls `discover_plugins()` to
scan the `plugins_dir` directory (default `./plugins` per
`config/config.example.toml:54`) and activates every plugin whose DB row has
`is_enabled = true`.

**No types are self-registered by the server.** All contest types, evaluators,
and checker formats come from plugins — no plugins, no types. The "default to
first registered" logic in `models/problem.rs:436-445` returns
`String::default()` (empty string) when the registry is empty, and
`validate_contest_type`/`validate_problem_type` will then fail with a
`VALIDATION_ERROR` saying `must be one of: <empty>`. So problem creation is
impossible on a fresh server with no plugins loaded.

In practice, every deployed Broccoli server has at minimum `batch-evaluator` +
one contest plugin loaded (otherwise the system can't judge anything). The
default test setup (`docker compose`) ships them under `./plugins`.

## Options reconsidered with concrete data

### Option A: Build minimal fixture plugin

- Cost: ~700 LOC duplicating batch-evaluator, or ~50 LOC delegating to the
  already-loaded `batch` evaluator. Plus a separate Cargo crate, `wasm32-wasip1`
  toolchain in CI, server-sdk dependency that breaks on API churn (already
  happened: `register_contest_type` gained a `code_run_handler` param).
- Benefits: deterministic plugin IDs; unaffected by what the operator has
  loaded.
- Drawbacks: bundling .wasm requires either committing a binary (mirrors
  `tests/fixtures/echo-plugin/`) or building it during stress-test compile,
  which adds a hard dep on `wasm32-wasip1` for anyone running `cargo build`.
  SDK-churn maintenance burden.

### Option B: Discover at runtime

- Viability: **yes** — one HTTP call to `GET /api/v1/plugins/registries` at
  startup, no auth required.
- Drawbacks: types differ across deployments. Contest plugins' scoring policies
  differ (`icpc` short-circuits, `ioi` aggregates), so verdict assertions must
  stay verdict-level not score-level — already documented in the design doc.
- Failure mode: zero plugins loaded → empty arrays → stress-test exits with a
  clear "no contest types registered" error.

### Option C: Admin specifies via flags

- UX cost: admin must `curl /api/v1/plugins/registries | jq` to know what's
  loaded.
- Failure mode: typo → server returns 400 `VALIDATION_ERROR` with the valid list
  embedded in the message (`models/problem.rs:489-491`). Self-documenting.
- Useful for repro ("test specifically against `ioi`"), but redundant
  standalone.

## Recommendation

**Option B with Option C as override.** At startup, fetch
`GET /api/v1/plugins/registries`, pick the first sorted entry from
`contest_types` and `problem_types`, log the choice, proceed. CLI flags
`--contest-type` / `--problem-type` override; empty arrays fail fast with a
message naming a fix.

Rationale: the registries endpoint exists, is public, and returns exactly the
needed data. Building a fixture plugin to dodge a one-line HTTP call is
overengineering — bundling a .wasm taxes the stress-test with server-sdk API
churn and a `wasm32-wasip1` toolchain dependency for a tool whose value is being
trivial to run. Verdict-determinism is already addressed by the design's choice
to assert verdicts (not scores) and use AC solutions that pass under any
reasonable contest type.

## Implications for Task 7

**Task 7 is dropped.** No fixture plugin. Task 8 absorbs its scope:

- `cli.rs` keeps the design's optional `--contest-type` / `--problem-type`
  flags.
- HTTP client (Task 4) gains `list_registries() -> RegistriesResponse`,
  mirroring the DTO at `packages/server/src/models/plugin.rs:27-43`.
- `Runner::bootstrap()` calls `list_registries()` once. Resolution: CLI flag →
  first sorted entry → error if either array is empty (with message naming
  `batch-evaluator` + a contest plugin as the canonical fix).
- The chosen pair is logged at INFO at startup.

Zero new crates, zero `wasm32-wasip1` builds, zero plugin SDK surface in the
stress-test binary.
