# Config, Language Resolver, and Fair Scheduling Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to
> implement this plan task-by-task.

**Goal:** Rename `submission_format` to `required_files`, add ordering + binary
support to additional files, design the language resolver plugin API, and
implement fair scheduling for the evaluator semaphore.

**Architecture:** Four independent workstreams that can be executed in any
order. The `required_files` rename is a mechanical refactor touching ~25 files
across backend and frontend. Additional files gets a `position` column and
binary blob support. The language resolver replaces template-based
`resolve_language()` with a plugin-based `ResolveLanguageOutput` API. Fair
scheduling replaces the FIFO `evaluator_slots` semaphore with a dispatcher that
distributes permits across submissions equitably.

**Tech Stack:** Rust (SeaORM 2.0, axum, Extism WASM), TypeScript/React,
PostgreSQL, Redis MQ

---

## Context for the Implementer

### Why these changes?

1. **`submission_format` -> `required_files`**: The field name doesn't convey
   its purpose. The key set of `required_files` also doubles as
   `allowed_languages` (if a language isn't a key, it's not allowed). No
   separate `allowed_languages` field needed.

2. **Additional file ordering + binary support**: Additional files
   (judge-provided stubs like `grader.cpp`) currently have no guaranteed
   ordering and are forced through `String::from_utf8()`. We need deterministic
   ordering for reproducible builds and binary support for pre-compiled
   libraries.

3. **Language resolver plugin API**: The current `resolve_language()` in
   `common/src/language.rs` uses template expansion (`{source}`, `{binary}`,
   `{basename}`). This can't handle cases where the entry point changes based on
   whether a grader is present (Python: `python3 grader.py` vs
   `python3 solution.py`), or where compiler arguments differ from
   cache-relevant files (C++: `.h` files affect cache but aren't compiler args).
   A plugin-based resolver gives full control.

4. **Fair scheduling**: The `evaluator_slots` semaphore (sized to CPU count, ~8)
   is FIFO. When 50 submissions compete for 8 slots, the first submission's 100
   evaluator tasks fill the FIFO queue first, starving later submissions. The
   semaphore doesn't limit throughput (workers are the bottleneck), but its FIFO
   ordering causes unfairness. A fair dispatcher ensures each submission gets
   equitable access.

### Key files you'll be working with

| File                                            | Purpose                                                                  |
| ----------------------------------------------- | ------------------------------------------------------------------------ |
| `packages/server/src/entity/problem.rs`         | Problem entity with `submission_format` field (line 44)                  |
| `packages/server/src/entity/additional_file.rs` | Additional file entity (no `position` column yet)                        |
| `packages/server/src/models/problem.rs`         | DTOs: Create/Update/Response + `validate_submission_format()` (line 597) |
| `packages/server/src/handlers/problem.rs`       | Problem CRUD handlers                                                    |
| `packages/server/src/handlers/submission.rs`    | Submission handlers calling `get_submission_format()` (lines 715, 1057)  |
| `packages/server/src/utils/judging.rs`          | `validate_submission_contract()` (line 60)                               |
| `packages/server/src/main.rs`                   | Server startup, schema_sync at line 33-35                                |
| `packages/server/src/host_funcs/evaluate.rs`    | `start_evaluate_batch_fn` with `evaluator_slots` semaphore (line 364)    |
| `packages/server/src/host_funcs/mod.rs`         | `evaluator_slots` creation (line 221), host function registration        |
| `packages/server/src/host_funcs/language.rs`    | `create_language_function` host function                                 |
| `packages/common/src/language.rs`               | `LanguageDefinition`, `ResolvedLanguage`, `resolve_language()`           |
| `packages/server-sdk/src/types/evaluate.rs`     | SDK-side `ResolvedLanguage` type                                         |
| `packages/server-sdk/src/sdk/language.rs`       | SDK-side `host.language.get_config()` wrapper                            |
| `packages/plugin-core/src/registry.rs`          | `PluginRegistry` type                                                    |
| `plugins/batch-evaluator/src/lib.rs`            | Primary-source selection + `build_operation()` consumer                  |
| `plugins/communication-evaluator/src/lib.rs`    | Dual-program language resolution                                         |

### How to run tests

```bash
cargo test -p server                     # Integration + unit tests (~27s)
cargo test -p common                     # Common crate tests
cargo test -p plugin-core                # Plugin core tests
pnpm --filter @broccoli/web build        # Frontend build check
pnpm lint                                # ESLint
```

### Database

Schema-sync auto-creates/updates tables on server start. For the
`submission_format` -> `required_files` rename, we add a pre-sync migration
because schema-sync would drop the old column and create a new one (losing
data). For new columns (like `additional_file.position`), schema-sync handles it
automatically.

---

## Task 1: Rename `submission_format` to `required_files` (Backend)

This is a mechanical rename across ~15 backend files. The DB column rename
requires a pre-sync SQL migration.

**Files:**

- Create: `packages/server/src/migration.rs`
- Modify: `packages/server/src/main.rs:33-35`
- Modify: `packages/server/src/lib.rs` (add `pub mod migration;`)
- Modify: `packages/server/src/entity/problem.rs:42-75`
- Modify: `packages/server/src/models/problem.rs:53-124,597-643`
- Modify: `packages/server/src/handlers/problem.rs` (create + update handlers)
- Modify: `packages/server/src/handlers/submission.rs:715,1057`
- Modify: `packages/server/src/utils/judging.rs:60-104`
- Test: `packages/server/tests/integration/submission.rs`

### Step 1: Create the pre-sync migration module

Create `packages/server/src/migration.rs`:

```rust
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Statement};

/// Run migrations that must execute before schema-sync.
///
/// Schema-sync renames columns by dropping the old and creating a new one,
/// which loses data. These migrations run ALTER TABLE RENAME COLUMN instead.
pub async fn run_pre_sync_migrations(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Rename submission_format -> required_files (2026-04-03)
    db.execute(Statement::from_string(
        db.get_database_backend(),
        r#"
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'problem' AND column_name = 'submission_format'
            ) THEN
                ALTER TABLE problem RENAME COLUMN submission_format TO required_files;
            END IF;
        END $$;
        "#
        .to_string(),
    ))
    .await?;

    Ok(())
}
```

### Step 2: Wire migration into server startup

In `packages/server/src/lib.rs`, add `pub mod migration;`.

In `packages/server/src/main.rs`, add the migration call after `ensure_indexes`
and before anything else:

```rust
// After line 35 (ensure_indexes):
server::migration::run_pre_sync_migrations(&db).await?;
```

### Step 3: Rename entity field and helper method

In `packages/server/src/entity/problem.rs`:

- Line 42-45: Rename `submission_format` to `required_files` (field name and doc
  comment)
- Lines 70-75: Rename `get_submission_format` to `get_required_files`

```rust
// Line 42-45: was submission_format
/// Expected submission file names per language (e.g. {"cpp": ["solution.cpp"]}).
/// Null means all languages allowed, any filenames accepted.
#[sea_orm(column_type = "JsonBinary", nullable)]
pub required_files: Option<serde_json::Value>,
```

```rust
// Lines 70-75: was get_submission_format
pub fn get_required_files(&self) -> Option<HashMap<String, Vec<String>>> {
    self.required_files
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}
```

### Step 4: Rename all DTO fields

In `packages/server/src/models/problem.rs`:

**CreateProblemRequest** (line 53-57): Rename field and doc comment.

```rust
/// Expected submission file names per language.
/// Keys are language ids (e.g. "cpp", "java"), values are arrays of filenames.
/// If set, its key set determines which languages are allowed.
/// Null or omitted means all languages allowed with any filenames.
#[schema(example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
pub required_files: Option<std::collections::HashMap<String, Vec<String>>>,
```

**UpdateProblemRequest** (line 87-91): Rename field.

```rust
/// Expected submission file names per language.
/// Set to a value to update, set to null to clear, or omit to leave unchanged.
#[serde(default, deserialize_with = "double_option")]
#[schema(value_type = Option<std::collections::HashMap<String, Vec<String>>>, example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
pub required_files: Option<Option<std::collections::HashMap<String, Vec<String>>>>,
```

**ProblemResponse** (line 121-124): Rename field.

```rust
/// Expected submission file names per language.
/// Null means all languages allowed with any filenames.
#[schema(example = json!({"cpp": ["solution.cpp"], "java": ["Main.java"]}))]
pub required_files: Option<std::collections::HashMap<String, Vec<String>>>,
```

**`From<Model> for ProblemResponse`**: Update the field assignment from
`submission_format` to `required_files`. The deserialization logic stays the
same, just rename the binding.

### Step 5: Rename validation function

In `packages/server/src/models/problem.rs` lines 597-643:

- Rename function: `validate_submission_format` -> `validate_required_files`
- Rename parameter: `submission_format` -> `required_files`
- Update all 5 error message strings from `"submission_format ..."` to
  `"required_files ..."`

```rust
pub fn validate_required_files(
    required_files: Option<&HashMap<String, Vec<String>>>,
    valid_languages: &HashMap<String, LanguageDefinition>,
) -> Result<(), AppError> {
    let Some(required_files) = required_files else {
        return Ok(());
    };
    // ... same logic, replace "submission_format" in error strings with "required_files"
}
```

### Step 6: Update handler field accesses

In `packages/server/src/handlers/problem.rs`:

**Create handler**: Update `payload.submission_format` ->
`payload.required_files` in:

- The `validate_required_files()` call
- The `serde_json::to_value()` serialization
- The `ActiveModel` field assignment (`required_files: Set(...)`)

**Update handler**: Same pattern for the three-state match on
`payload.required_files`.

In `packages/server/src/handlers/submission.rs`:

- Line 715: `problem.get_submission_format()` -> `problem.get_required_files()`
- Line 1057: Same rename

### Step 7: Update validate_submission_contract

In `packages/server/src/utils/judging.rs` lines 60-104:

- Rename parameter `submission_format` -> `required_files`
- Rename local bindings accordingly
- Update the unit tests at lines 143-251 (rename local variables)

```rust
pub fn validate_submission_contract(
    files: &[SubmissionFileDto],
    language: &str,
    required_files: Option<HashMap<String, Vec<String>>>,
    languages: &HashMap<String, LanguageDefinition>,
) -> Result<(), AppError> {
    // ... same logic with renamed bindings
}
```

### Step 8: Update integration test

In `packages/server/tests/integration/submission.rs`:

- Line 524: Rename test function `rejects_submission_format_filename_mismatch`
  -> `rejects_required_files_filename_mismatch`
- Line 539: Change JSON key from `"submission_format"` to `"required_files"` in
  the create-problem request body

### Step 9: Run backend tests

```bash
cargo test -p server
```

Expected: All tests pass. The migration is idempotent (IF EXISTS check), so
tests using the template database will work fine since the column either doesn't
exist yet (fresh template) or has already been renamed.

### Step 10: Commit

```bash
git add packages/server/src/migration.rs packages/server/src/lib.rs \
  packages/server/src/main.rs packages/server/src/entity/problem.rs \
  packages/server/src/models/problem.rs packages/server/src/handlers/problem.rs \
  packages/server/src/handlers/submission.rs packages/server/src/utils/judging.rs \
  packages/server/tests/integration/submission.rs
git commit -m "refactor: rename submission_format to required_files

The key set of required_files doubles as allowed_languages - if a
language isn't a key, it's not allowed for this problem. No separate
allowed_languages field needed.

Includes a pre-sync SQL migration to ALTER TABLE RENAME COLUMN,
avoiding data loss from schema-sync's drop-and-recreate behavior."
```

---

## Task 2: Rename `submission_format` to `required_files` (Frontend)

**Files:**

- Modify: `packages/web/src/components/CodeEditor.tsx`
- Modify: `packages/web/src/features/problem/components/ProblemEditForm.tsx`
- Modify: `packages/web/src/features/admin/components/AdminProblemsTab.tsx`
- Modify: `packages/web/src/features/admin/components/ProblemForm.tsx`
- Modify: `packages/web/src/features/problem/components/ProblemView.tsx`
- Modify: `packages/web/src/features/problem/components/ProblemCodingTab.tsx`
- Modify:
  `packages/web/src/features/problem/deprecated/dock/ProblemDockContext.tsx`
- Modify: `packages/web/src/features/problem/deprecated/CodeEditorPanel.tsx`
- Modify: `packages/web/src/lib/i18n/en.ts`
- Modify: `plugins/broccoli-zh-cn/locales/zh-CN.toml`
- Modify:
  `plugins/communication-evaluator/frontend/src/ManagerLanguageSelector.tsx`

### Step 1: Regenerate OpenAPI schema types

After the backend changes from Task 1, regenerate the SDK types:

```bash
# Start the server temporarily to serve the OpenAPI spec
cargo run -p server &
SERVER_PID=$!
sleep 3

# Regenerate types from spec
cd packages/web-sdk
pnpm run generate  # or whatever the generation command is

kill $SERVER_PID
```

If there's no generation script, manually update
`packages/web-sdk/src/api/schema.ts`: find-and-replace `submission_format` ->
`required_files` in the 3 occurrences (CreateProblemRequest, ProblemResponse,
UpdateProblemRequest).

### Step 2: Rename props and state in frontend components

This is a bulk find-and-replace across all frontend files. The pattern is:

| Find                | Replace          | Scope                                                               |
| ------------------- | ---------------- | ------------------------------------------------------------------- |
| `submissionFormat`  | `requiredFiles`  | All TS/TSX props, state, destructuring                              |
| `submission_format` | `required_files` | All API read/write (JSON keys in fetch bodies, API response access) |

Apply across all files listed above. Key transformations:

**CodeEditor.tsx**: Rename prop `submissionFormat` -> `requiredFiles` in
`CodeEditorProps` interface and all ~20 internal usages.

**ProblemForm.tsx**: Rename `submissionFormat` field in `ProblemFormData`
interface (line 41). Rename all internal usage in the submission format editor
UI (~30 occurrences).

**ProblemEditForm.tsx**: Rename state initialization (`requiredFiles: {}`), API
read (`data.required_files`), API write (`required_files: ...`).

**AdminProblemsTab.tsx**: Rename `useState` variable, API read/write.

**ProblemView.tsx**: Rename prop assignment
`requiredFiles={problem?.required_files}`.

**ProblemCodingTab.tsx**: Rename prop in interface and destructuring.

**Deprecated files**: Same pattern for `ProblemDockContext.tsx` and
`CodeEditorPanel.tsx`.

### Step 3: Rename i18n keys

In `packages/web/src/lib/i18n/en.ts` (lines 482-490):

```typescript
// Before:
'admin.field.submissionFormat': 'Submission Format',
'admin.submissionFormat.language': 'Select language',
'admin.submissionFormat.addLanguage': 'Add Language',
'admin.submissionFormat.filenamePlaceholder': 'e.g. solution.cpp',
'admin.submissionFormat.addFile': 'Add File',
'admin.submissionFormat.empty': 'No language configured yet.',

// After:
'admin.field.requiredFiles': 'Required Files',
'admin.requiredFiles.language': 'Select language',
'admin.requiredFiles.addLanguage': 'Add Language',
'admin.requiredFiles.filenamePlaceholder': 'e.g. solution.cpp',
'admin.requiredFiles.addFile': 'Add File',
'admin.requiredFiles.empty': 'No language configured yet.',
```

In `plugins/broccoli-zh-cn/locales/zh-CN.toml` (lines 329-337): Same key
renames.

In `plugins/communication-evaluator/frontend/src/ManagerLanguageSelector.tsx`
(line 68): Update i18n key reference from `admin.submissionFormat.language` to
`admin.requiredFiles.language`.

### Step 4: Update all i18n key references in components

In `ProblemForm.tsx`: Update all `t('admin.submissionFormat.*')` calls to
`t('admin.requiredFiles.*')` and `t('admin.field.submissionFormat')` to
`t('admin.field.requiredFiles')`.

### Step 5: Build and lint

```bash
pnpm --filter @broccoli/web build
pnpm lint
```

Expected: Clean build, no lint errors.

### Step 6: Commit

```bash
git add packages/web/ packages/web-sdk/ plugins/broccoli-zh-cn/ plugins/communication-evaluator/
git commit -m "refactor(web): rename submissionFormat to requiredFiles in frontend

Matches the backend rename of submission_format -> required_files.
Updates all components, props, state, API keys, and i18n strings."
```

---

## Task 3: Additional Files - Add Position Column and Binary Support

**Files:**

- Modify: `packages/server/src/entity/additional_file.rs`
- Modify: `packages/server/src/handlers/additional_file.rs` (create + list +
  reorder handlers)
- Modify: `packages/server/src/host_funcs/evaluate.rs:212-246`
- Modify: `packages/server-sdk/src/types/evaluate.rs` (SourceFile type)
- Test: `packages/server/tests/integration/` (additional file tests)

### Step 1: Add `position` column to entity

In `packages/server/src/entity/additional_file.rs`, add after `size`:

```rust
/// Ordering position within (problem_id, language). Lower = first.
#[sea_orm(default_value = 0)]
pub position: i32,
```

Schema-sync will add this column automatically with default 0.

### Step 2: Update list handler to order by position

In the additional file list handler, add
`.order_by_asc(additional_file::Column::Position)` to the query.

### Step 3: Update create handler for auto-position

In the create handler, before inserting, compute the next position:

```rust
// Inside the transaction, after FOR UPDATE on problem:
let max_position = additional_file::Entity::find()
    .filter(additional_file::Column::ProblemId.eq(problem_id))
    .filter(additional_file::Column::Language.eq(&payload.language))
    .select_only()
    .column_as(additional_file::Column::Position.max(), "max_pos")
    .into_tuple::<Option<i32>>()
    .one(&txn)
    .await?
    .flatten()
    .unwrap_or(-1);

let next_position = max_position.checked_add(1).ok_or_else(|| {
    AppError::Internal("Position overflow".into())
})?;

// Set on ActiveModel:
active_model.position = Set(next_position);
```

### Step 4: Add reorder endpoint

Add `PUT /api/v1/problems/{id}/additional-files/reorder` using the same pattern
as test case reorder:

- Accept `{ "ids": [uuid1, uuid2, ...] }` body
- Validate exact set match (no missing, no extra, no duplicates) via
  `validate_reorder_ids()`
- Assign positions 0, 1, 2, ... by array index
- Scope to `(problem_id, language)` — require a `language` query param
- Return 204 No Content

### Step 5: Remove UTF-8 restriction in evaluate host function

In `packages/server/src/host_funcs/evaluate.rs` lines 221-246, the current code
fetches blob content and converts to `String::from_utf8()`. Change this to pass
blob hashes instead of inline content:

```rust
// Before (lines 221-246):
let mut additional_source_files: Vec<SourceFile> = Vec::new();
for r in af_models {
    let hash = ContentHash::from_hex(&r.content_hash).map_err(|e| { ... })?;
    let content_bytes = blob_store.get(&hash).await.map_err(|e| { ... })?;
    let content = String::from_utf8(content_bytes).map_err(|e| { ... })?;
    additional_source_files.push(SourceFile {
        filename: r.path,
        content,
    });
}

// After:
let mut additional_source_files: Vec<SourceFile> = Vec::new();
for r in af_models {
    additional_source_files.push(SourceFile {
        filename: r.path,
        blob_hash: Some(r.content_hash.clone()),
        content: None,
    });
}
```

This requires updating the `SourceFile` type in
`packages/server-sdk/src/types/evaluate.rs` to support blob references:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceFile {
    pub filename: String,
    /// Inline UTF-8 content (for contestant-submitted source code).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Blob store hash (for additional files that may be binary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_hash: Option<String>,
}
```

The evaluator plugins need to handle both: if `blob_hash` is set, use
`SessionFile::Blob { hash }` in the OperationTask environment. If `content` is
set, use `SessionFile::Content`. Update `batch-evaluator` and
`communication-evaluator` accordingly.

### Step 6: Update additional file query to order by position

In the evaluate host function (`evaluate.rs` line 212-215), add ordering:

```rust
let af_models = additional_file::Entity::find()
    .filter(additional_file::Column::ProblemId.eq(problem_id))
    .filter(additional_file::Column::Language.eq(solution_language.as_str()))
    .order_by_asc(additional_file::Column::Position)
    .all(&db)
    .await
    // ...
```

### Step 7: Run tests

```bash
cargo test -p server
```

### Step 8: Commit

```bash
git add packages/server/src/entity/additional_file.rs \
  packages/server/src/handlers/additional_file.rs \
  packages/server/src/host_funcs/evaluate.rs \
  packages/server-sdk/src/types/evaluate.rs \
  plugins/batch-evaluator/ plugins/communication-evaluator/
git commit -m "feat: add position ordering and binary support to additional files

Additional files now have a position column for deterministic ordering.
Files are passed as blob references instead of inline UTF-8 content,
enabling binary files (pre-compiled libraries, data files)."
```

---

## Task 4: Language Resolver Plugin API - Types and Registration

This task defines the new types. The actual plugin implementation comes in
Task 5.

**Files:**

- Modify: `packages/server-sdk/src/types/evaluate.rs` (new types)
- Modify: `packages/plugin-core/src/registry.rs` (language resolver
  registration)
- Modify: `packages/server/src/host_funcs/mod.rs` (registration host function)

### Step 1: Define the resolver types in the SDK

In `packages/server-sdk/src/types/evaluate.rs`, add:

```rust
/// Input to a language resolver plugin function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageInput {
    pub language_id: String,
    /// Filenames submitted by the contestant (e.g. ["solution.cpp"]).
    pub submitted_files: Vec<String>,
    /// Filenames provided by the judge as additional files (e.g. ["grader.cpp", "grader.h"]).
    pub additional_files: Vec<String>,
    /// Per-problem language configuration from plugin config system. Null if not set.
    pub problem_config: Option<serde_json::Value>,
}

/// Output from a language resolver plugin function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveLanguageOutput {
    /// Compilation specification. None for interpreted languages.
    pub compile: Option<CompileSpec>,
    /// Execution specification.
    pub run: RunSpec,
}

/// How to compile the source files.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompileSpec {
    /// Fully resolved compile command, e.g. ["g++", "-O2", "solution.cpp", "grader.cpp", "-o", "solution"].
    pub command: Vec<String>,
    /// ALL files whose contents influence the compilation output (for cache key computation).
    /// Includes headers and other files not on the command line.
    /// If empty, the evaluator should default to all files in the sandbox.
    pub cache_inputs: Vec<String>,
    /// Expected output artifacts from compilation.
    /// Can be exact filenames ("solution") or glob patterns ("*.class").
    /// Globs are resolved relative to the sandbox working directory only.
    pub outputs: Vec<OutputSpec>,
}

/// A compilation output specification - either an exact filename or a glob pattern.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "pattern")]
pub enum OutputSpec {
    /// Exact filename, e.g. "solution".
    File(String),
    /// Glob pattern resolved relative to sandbox workdir, e.g. "*.class".
    /// Validated: must not contain ".." or start with "/".
    Glob(String),
}

/// How to run the compiled/interpreted program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunSpec {
    /// Fully resolved run command, e.g. ["./solution"] or ["python3", "grader.py"].
    pub command: Vec<String>,
    /// Files needed in the execution environment beyond compilation outputs.
    /// For interpreted languages: all source files (e.g. ["grader.py", "solution.py"]).
    /// For compiled languages: typically empty (binary is a compile output).
    pub extra_files: Vec<String>,
}
```

### Step 2: Add language resolver registration to PluginRegistry

The `PluginRegistry` is a `HashMap<String, PluginEntry>` keyed by plugin ID.
Language resolver registrations should go on a separate shared registry (similar
to how contest types, evaluators, and checker formats are registered).

In `packages/server/src/host_funcs/mod.rs`, add a `LanguageResolverRegistry`:

```rust
/// Maps language_id -> (plugin_id, function_name) for language resolver plugins.
pub type LanguageResolverRegistry = Arc<RwLock<HashMap<String, (String, String)>>>;
```

Add a `register_language_resolver` host function that plugins call during
`init()`:

```rust
// Input from WASM:
#[derive(Deserialize)]
struct RegisterLanguageResolverInput {
    /// Language ID this resolver handles (e.g. "cpp", "python3").
    language_id: String,
    /// Function name in this plugin that implements resolution.
    function_name: String,
}
```

Register it under the existing `"plugin:register"` permission alongside
`register_contest_type`, `register_evaluator`, and `register_checker_format`.

### Step 3: Run tests

```bash
cargo test -p server
cargo test -p plugin-core
```

### Step 4: Commit

```bash
git add packages/server-sdk/src/types/evaluate.rs \
  packages/plugin-core/src/registry.rs \
  packages/server/src/host_funcs/
git commit -m "feat: define language resolver plugin API types and registration

New types: ResolveLanguageInput, ResolveLanguageOutput, CompileSpec,
RunSpec, OutputSpec. Plugins register language resolvers via
register_language_resolver() during init. OutputSpec supports both
exact filenames and glob patterns (scoped to sandbox workdir)."
```

---

## Task 5: Language Resolver - Host Function and Dispatch

Replace the current `get_language_config` host function with a new
`resolve_language` that dispatches to registered language resolver plugins,
falling back to the existing template-based resolution if no plugin is
registered for that language.

**Files:**

- Modify: `packages/server/src/host_funcs/language.rs`
- Modify: `packages/server/src/host_funcs/mod.rs` (pass new dependencies)
- Modify: `packages/server-sdk/src/sdk/language.rs` (new SDK method)
- Modify: `packages/server-sdk/src/host/raw.rs` (new extern declaration)
- Modify: `packages/common/src/language.rs` (add `to_resolve_output` conversion)

### Step 1: Add conversion from old ResolvedLanguage to new ResolveLanguageOutput

In `packages/common/src/language.rs`, add a method that converts the
template-based `ResolvedLanguage` into the new `ResolveLanguageOutput` format.
This is the fallback path for languages without a plugin resolver:

```rust
impl ResolvedLanguage {
    /// Convert to the plugin API output format.
    /// Used as fallback when no language resolver plugin is registered.
    pub fn to_resolve_output(&self, all_files: &[String]) -> ResolveLanguageOutput {
        ResolveLanguageOutput {
            compile: self.compile_cmd.as_ref().map(|cmd| CompileSpec {
                command: cmd.clone(),
                cache_inputs: all_files.to_vec(),
                outputs: vec![OutputSpec::File(self.binary_name.clone())],
            }),
            run: RunSpec {
                command: self.run_cmd.clone(),
                extra_files: if self.compile_cmd.is_none() {
                    // Interpreted: all source files needed at runtime
                    all_files.to_vec()
                } else {
                    // Compiled: binary is the compile output, nothing extra needed
                    vec![]
                },
            },
        }
    }
}
```

Note: Import the new types from `server-sdk` or define them in `common`
(whichever is the appropriate dependency direction -- if `common` can't depend
on `server-sdk`, define the types in `common` and re-export from `server-sdk`).

### Step 2: Update the host function to dispatch to plugin or fallback

In `packages/server/src/host_funcs/language.rs`, update
`create_language_function` to:

1. Accept the `LanguageResolverRegistry` and `PluginManager` as additional
   parameters
2. On call: check if a plugin resolver is registered for the requested
   `language_id`
3. If yes: call the plugin's resolver function with `ResolveLanguageInput`,
   return its `ResolveLanguageOutput`
4. If no: fall back to `resolve_language()` from `common`, convert via
   `to_resolve_output()`

```rust
pub fn create_language_function(
    plugin_id: String,
    languages: HashMap<String, LanguageDefinition>,
    resolver_registry: LanguageResolverRegistry,
    plugin_manager: Arc<ServerManager>,
) -> Function {
    Function::new(
        "resolve_language",
        [ValType::I64],
        [ValType::I64],
        UserData::new(LanguageUserData {
            plugin_id,
            languages,
            resolver_registry,
            plugin_manager,
        }),
        |plugin, inputs, outputs, user_data| {
            let data = user_data.get()?;
            let input: ResolveLanguageInput = /* deserialize from WASM memory */;

            // Check for plugin resolver
            let resolver = data.resolver_registry.read().unwrap()
                .get(&input.language_id).cloned();

            let result = if let Some((resolver_plugin_id, resolver_fn)) = resolver {
                // Dispatch to plugin resolver (avoid self-call deadlock)
                if resolver_plugin_id == data.plugin_id {
                    return Err(extism::Error::msg(
                        "Language resolver cannot call itself"
                    ));
                }
                let input_bytes = serde_json::to_vec(&input)?;
                let output_bytes = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        data.plugin_manager.call_raw(
                            &resolver_plugin_id, &resolver_fn, &input_bytes
                        ).await
                    })
                })?;
                serde_json::from_slice::<ResolveLanguageOutput>(&output_bytes)?
            } else {
                // Fallback to template-based resolution
                let all_files: Vec<String> = input.submitted_files.iter()
                    .chain(input.additional_files.iter())
                    .cloned()
                    .collect();
                let primary = input.submitted_files.first()
                    .map(|s| s.as_str()).unwrap_or_default();
                let extras: Vec<String> = all_files[1..].to_vec();

                let resolved = resolve_language(
                    &input.language_id, primary, &data.languages, &extras
                ).map_err(extism::Error::msg)?;

                resolved.to_resolve_output(&all_files)
            };

            /* serialize result back to WASM memory */
            Ok(())
        },
    )
}
```

### Step 3: Update SDK facade

In `packages/server-sdk/src/sdk/language.rs`, add a new method `resolve()` that
calls the new `resolve_language` host function and returns
`ResolveLanguageOutput`. Keep the old `get_config()` method for backwards
compatibility during the transition, but mark it deprecated.

In `packages/server-sdk/src/host/raw.rs`, add the extern declaration for
`resolve_language`.

### Step 4: Run tests

```bash
cargo test -p server
cargo test -p common
```

### Step 5: Commit

```bash
git add packages/common/src/language.rs \
  packages/server/src/host_funcs/language.rs \
  packages/server/src/host_funcs/mod.rs \
  packages/server-sdk/src/sdk/language.rs \
  packages/server-sdk/src/host/raw.rs
git commit -m "feat: language resolver dispatch with template fallback

The resolve_language host function now checks the LanguageResolverRegistry
for a plugin resolver. If found, dispatches to the plugin. If not, falls
back to the existing template-based resolve_language() from common.

This allows standard languages to keep working with TOML config while
enabling plugins to handle custom cases (Python graders, Java entry
points, etc.)."
```

---

## Task 6: Update Evaluator Plugins to Use New Resolver API

**Files:**

- Modify: `plugins/batch-evaluator/src/lib.rs`
- Modify: `plugins/batch-evaluator/src/batch.rs`
- Modify: `plugins/communication-evaluator/src/lib.rs`
- Modify: `plugins/communication-evaluator/src/operation.rs`

### Step 1: Update batch-evaluator to use `host.language.resolve()`

In `plugins/batch-evaluator/src/lib.rs`, replace the two-pass `get_config`
pattern (lines 35-58) with a single `resolve()` call:

```rust
let resolved = host.language.resolve(&ResolveLanguageInput {
    language_id: req.solution_language.clone(),
    submitted_files: req.solution_source.iter().map(|f| f.filename.clone()).collect(),
    additional_files: req.additional_files.iter().map(|f| f.filename.clone()).collect(),
    problem_config: None, // or fetch from plugin config if needed
})?;
```

### Step 2: Update `build_operation()` in batch-evaluator

In `plugins/batch-evaluator/src/batch.rs`, use `CompileSpec` and `RunSpec` from
the resolved output:

- If `resolved.compile` is Some: create compile step with `compile.command`, set
  `StepCacheConfig.key_inputs = compile.cache_inputs`,
  `outputs = compile.outputs` (handle OutputSpec::File and OutputSpec::Glob)
- If `resolved.compile` is None: skip compile step
- Create exec step with `resolved.run.command`
- For file loading: put `compile.cache_inputs` files in the sandbox for compiled
  languages, put `run.extra_files` in the sandbox for interpreted languages

### Step 3: Update communication-evaluator similarly

Same pattern, but for both contestant and manager programs.

### Step 4: Build evaluator plugins

```bash
cd plugins/batch-evaluator && cargo build --target wasm32-wasip1
cd plugins/communication-evaluator && cargo build --target wasm32-wasip1
```

### Step 5: Run integration tests

```bash
cargo test -p server  # integration tests use the echo-plugin, not evaluators
```

### Step 6: Commit

```bash
git add plugins/batch-evaluator/ plugins/communication-evaluator/
git commit -m "feat: update evaluator plugins to use new language resolver API

Replaces the two-pass get_config pattern with a single resolve() call.
Uses CompileSpec.cache_inputs for cache keys (includes headers),
CompileSpec.outputs with OutputSpec for compilation artifacts,
and RunSpec.extra_files for interpreted language runtime files."
```

---

## Task 7: Fair Scheduling for Evaluator Slots

Replace the FIFO `evaluator_slots` semaphore with a fair dispatcher that
distributes permits equitably across submissions.

**Files:**

- Create: `packages/server/src/fair_scheduler.rs`
- Modify: `packages/server/src/lib.rs` (add `pub mod fair_scheduler;`)
- Modify: `packages/server/src/host_funcs/mod.rs:217-221` (replace semaphore
  with FairScheduler)
- Modify: `packages/server/src/host_funcs/evaluate.rs:358-380` (use
  FairScheduler)
- Test: `packages/server/src/fair_scheduler.rs` (unit tests)

### Step 1: Write failing tests for FairScheduler

Create `packages/server/src/fair_scheduler.rs` with test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn single_group_gets_full_capacity() {
        let scheduler = FairScheduler::new(4);
        let p1 = scheduler.acquire("sub-1").await;
        let p2 = scheduler.acquire("sub-1").await;
        let p3 = scheduler.acquire("sub-1").await;
        let p4 = scheduler.acquire("sub-1").await;
        // All 4 permits acquired by one group
        assert_eq!(scheduler.active_groups(), 1);
        drop(p1);
        drop(p2);
        drop(p3);
        drop(p4);
    }

    #[tokio::test]
    async fn two_groups_share_fairly() {
        let scheduler = FairScheduler::new(4);

        // Group A gets 2 permits
        let _a1 = scheduler.acquire("sub-a").await;
        let _a2 = scheduler.acquire("sub-a").await;

        // Group B should also be able to get permits (not starved)
        let b1 = tokio::time::timeout(
            Duration::from_millis(100),
            scheduler.acquire("sub-b"),
        ).await;
        assert!(b1.is_ok(), "Group B should not be starved");
    }

    #[tokio::test]
    async fn group_cleanup_on_completion() {
        let scheduler = FairScheduler::new(4);
        {
            let _p = scheduler.acquire("sub-1").await;
            assert_eq!(scheduler.active_groups(), 1);
        }
        // After permit dropped, group should be cleaned up
        // (may need a small yield for async cleanup)
        tokio::task::yield_now().await;
        assert_eq!(scheduler.active_groups(), 0);
    }
}
```

### Step 2: Run tests to verify they fail

```bash
cargo test -p server --lib fair_scheduler
```

Expected: Compilation error (module doesn't exist yet).

### Step 3: Implement FairScheduler

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{Notify, Semaphore, OwnedSemaphorePermit};

/// A fair semaphore that distributes permits equitably across groups.
///
/// When multiple groups (submissions) compete for permits, the dispatcher
/// preferentially grants permits to groups with fewer in-flight tasks.
/// A lone group gets full capacity (no artificial throttling).
pub struct FairScheduler {
    total_permits: usize,
    available: Arc<Semaphore>,
    groups: DashMap<String, GroupState>,
    notify: Arc<Notify>,
}

struct GroupState {
    in_flight: AtomicUsize,
    waiting: AtomicUsize,
}

pub struct FairPermit {
    _permit: OwnedSemaphorePermit,
    scheduler: Arc<FairScheduler>,
    group_id: String,
}

impl Drop for FairPermit {
    fn drop(&mut self) {
        if let Some(group) = self.scheduler.groups.get(&self.group_id) {
            let remaining = group.in_flight.fetch_sub(1, Ordering::SeqCst);
            if remaining == 1 && group.waiting.load(Ordering::SeqCst) == 0 {
                drop(group);
                self.scheduler.groups.remove(&self.group_id);
            }
        }
        // OwnedSemaphorePermit is dropped here, returning it to `available`
        // Then wake the dispatcher to reallocate
        self.scheduler.notify.notify_waiters();
    }
}

impl FairScheduler {
    pub fn new(total_permits: usize) -> Arc<Self> {
        Arc::new(Self {
            total_permits,
            available: Arc::new(Semaphore::new(total_permits)),
            groups: DashMap::new(),
            notify: Arc::new(Notify::new()),
        })
    }

    pub async fn acquire(self: &Arc<Self>, group_id: &str) -> FairPermit {
        // Register as waiting
        let group = self.groups
            .entry(group_id.to_string())
            .or_insert_with(|| GroupState {
                in_flight: AtomicUsize::new(0),
                waiting: AtomicUsize::new(0),
            });
        group.waiting.fetch_add(1, Ordering::SeqCst);
        drop(group);

        loop {
            // Compute fair share: ceil(total / active_groups), minimum 1
            let active_groups = self.groups.iter()
                .filter(|g| {
                    g.waiting.load(Ordering::SeqCst) > 0
                        || g.in_flight.load(Ordering::SeqCst) > 0
                })
                .count()
                .max(1);
            let fair_share = (self.total_permits + active_groups - 1) / active_groups;

            let my_in_flight = self.groups.get(group_id)
                .map(|g| g.in_flight.load(Ordering::SeqCst))
                .unwrap_or(0);

            if my_in_flight < fair_share {
                // Under fair share — try to get a global permit
                match self.available.clone().try_acquire_owned() {
                    Ok(permit) => {
                        if let Some(group) = self.groups.get(group_id) {
                            group.waiting.fetch_sub(1, Ordering::SeqCst);
                            group.in_flight.fetch_add(1, Ordering::SeqCst);
                        }
                        return FairPermit {
                            _permit: permit,
                            scheduler: Arc::clone(self),
                            group_id: group_id.to_string(),
                        };
                    }
                    Err(_) => {
                        // No permits available, wait for one to be released
                        self.notify.notified().await;
                    }
                }
            } else {
                // At or over fair share — yield to other groups
                self.notify.notified().await;
            }
        }
    }

    pub fn active_groups(&self) -> usize {
        self.groups.iter()
            .filter(|g| {
                g.waiting.load(Ordering::SeqCst) > 0
                    || g.in_flight.load(Ordering::SeqCst) > 0
            })
            .count()
    }
}
```

### Step 4: Run tests to verify they pass

```bash
cargo test -p server --lib fair_scheduler
```

Expected: All 3 tests pass.

### Step 5: Wire FairScheduler into evaluate host function

In `packages/server/src/host_funcs/mod.rs` (lines 217-221), replace:

```rust
// Before:
let evaluator_parallelism = std::thread::available_parallelism()
    .map(|parallelism| parallelism.get())
    .unwrap_or(1)
    .max(1);
let evaluator_slots = Arc::new(Semaphore::new(evaluator_parallelism));

// After:
let evaluator_parallelism = std::thread::available_parallelism()
    .map(|parallelism| parallelism.get())
    .unwrap_or(1)
    .max(1);
let fair_scheduler = FairScheduler::new(evaluator_parallelism);
```

In `packages/server/src/host_funcs/evaluate.rs`, update the spawned evaluator
task (lines 358-380):

```rust
// Before (line 364):
let _permit = match evaluator_slots.acquire_owned().await {

// After:
// submission_id comes from the StartEvaluateBatchInput — it identifies
// the submission this evaluation belongs to, used for fair scheduling.
let _permit = match fair_scheduler.acquire(&submission_id).await {
```

The `StartEvaluateBatchInput` type (in server-sdk) needs a
`submission_id: String` field so the scheduler can group evaluations by
submission. This is the correlation key.

If `StartEvaluateBatchInput` doesn't have a submission ID, use the
`caller_plugin_id + batch_id` as the group key, or add a submission_id field.

### Step 6: Run full test suite

```bash
cargo test -p server
```

### Step 7: Commit

```bash
git add packages/server/src/fair_scheduler.rs \
  packages/server/src/lib.rs \
  packages/server/src/host_funcs/mod.rs \
  packages/server/src/host_funcs/evaluate.rs
git commit -m "feat: fair scheduling for evaluator slots

Replaces FIFO semaphore with FairScheduler that distributes permits
equitably across submissions. Each submission gets ceil(total/active)
slots. A lone submission gets full capacity. Prevents starvation
during contest peaks with many concurrent submissions."
```

---

## Dependency Graph

```
Task 1 (Backend rename) ──> Task 2 (Frontend rename)

Task 3 (Additional files) ──> Task 6 (Update evaluators)
Task 4 (Resolver types)  ──> Task 5 (Resolver dispatch) ──> Task 6

Task 7 (Fair scheduling) is independent of all others.
```

Tasks 1, 3, 4, and 7 can all start in parallel. Task 2 depends on Task 1. Task 5
depends on Task 4. Task 6 depends on Tasks 3 and 5.

## Verification Checklist

- [ ] `cargo test -p server` passes (integration + unit tests)
- [ ] `cargo test -p common` passes
- [ ] `cargo test -p plugin-core` passes
- [ ] `cargo clippy --workspace` clean
- [ ] `pnpm --filter @broccoli/web build` succeeds
- [ ] `pnpm lint` clean
- [ ] Pre-sync migration is idempotent (can run multiple times safely)
- [ ] `required_files` JSON key appears in API responses (not
      `submission_format`)
- [ ] Additional files respect `position` ordering in list and evaluate queries
- [ ] Binary additional files don't cause UTF-8 errors
- [ ] Language resolution falls back to template when no plugin resolver
      registered
- [ ] Fair scheduler gives full capacity to a single submission
- [ ] Fair scheduler prevents starvation with multiple concurrent submissions
