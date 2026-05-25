# broccoli-server-sdk

SDK for building [Broccoli](https://github.com/THUSAAC-PSD/broccoli) online
judge WASM plugins (backend).

## Feature flags

| Feature | Default | Description                                                                                                       |
| ------- | ------- | ----------------------------------------------------------------------------------------------------------------- |
| `guest` | off     | Enables host function wrappers (`db`, `evaluator`, `host`) and the `WasmHost` runtime for use inside WASM plugins |

Without `guest`, only shared types, traits, and error definitions are available
(useful for host-side code that needs the same type definitions).

## Usage

Add the dependency with the `guest` feature in your plugin's `Cargo.toml`:

```toml
[dependencies]
broccoli-server-sdk = { version = "0.1", features = ["guest"] }
```

Then import the prelude:

```rust
use broccoli_server_sdk::prelude::*;
```

### Evaluation cancellation

Evaluation plugins can cancel outstanding work through a `WindowedEvalSession`:

- `cancel_test_cases(&[test_case_id])` on `WindowedEvalSession` cancels specific
  active testcases and drops matching queued testcases. It is intended for
  scoring short-circuits such as IOI `GroupMin` and `GroupMul`.
- `cancel_all()` cancels active evaluate batches and clears queued testcases.

Detached eval and operation flows cancel outstanding work by returning the
shared callback output action `cancel`, or by listing specific pending/active
items in the callback output. Synchronous operation `collect(...)` cancels its
active operation batches internally on timeout or host error.

### Detached windowed callbacks

Long-running plugin exports should avoid polling worker results while holding a
WASM plugin instance. The detached windowed APIs start bounded work on the host,
return a session id immediately, and invoke a named plugin export for each
result, timeout, or exhausted session event:

```rust
host.eval
    .windowed(&batch)
    .concurrency(8)
    .fire_after_judging_for_submission(&submission)
    .start_detached("on_eval_result", serde_json::json!({}))?;

host.operations
    .windowed(&operations)
    .concurrency(4)
    .start_detached("on_operation_result", serde_json::json!({}))?;
```

Callback inputs and outputs are shared SDK types:
`DetachedEvaluateCallbackInput`, `DetachedEvaluateCallbackOutput`,
`DetachedOperationCallbackInput`, and `DetachedOperationCallbackOutput`.
Callbacks return updated opaque `state`, an action (`continue`, `finish`, or
`cancel`), a `refill` decision, and optional active/pending work to cancel. Eval
defaults its detached result timeout from the testcase time limits; operation
defaults to `DEFAULT_OPERATION_RESULT_TIMEOUT_MS`. Callers can still override
with `.result_timeout_ms(...)`. Submission-judging eval flows should call
`.fire_after_judging_for_submission(...)` with the submission request; the
server then emits the normal `after_judging` hook after the terminal detached
callback persists results, subject to the original dispatch's current-judgement
hook policy.

Detached refill policy lives in the callback output because the host continues
the window after the original plugin export has returned:

```rust
let output = DetachedEvaluateCallbackOutput::continue_with(next_state)
    .refill_while(&input, |result| result.verdict == Verdict::Accepted);
```

For synchronous operation exports that must return a value immediately, use the
same builder and collect the batch result explicitly:

```rust
let results = host
    .operations
    .windowed(&operations)
    .concurrency(4)
    .collect(result_timeout_ms)?;
```

This compatibility path still blocks the current plugin export while worker
operations run, so detached callbacks should be preferred for long-running
flows.

Synchronous eval sessions use the same builder naming, but intentionally do not
offer a `collect` shortcut:

```rust
let mut session = host.eval.windowed(&batch).concurrency(4).start()?;
while let Some(result) = session.next_result(result_timeout_ms)? {
    // persist or score result
}
```

## License

MIT
