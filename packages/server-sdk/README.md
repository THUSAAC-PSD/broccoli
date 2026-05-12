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

Evaluation plugins can cancel outstanding work through `host.eval`:

- `cancel_batch(batch_id)` cancels every pending testcase in an evaluate batch.
- `cancel_test_cases(&[test_case_id])` on `WindowedEvalSession` cancels specific
  active testcases and drops matching queued testcases. It is intended for
  scoring short-circuits such as IOI `GroupMin` and `GroupMul`.

Operation plugins can cancel outstanding work through `host.operations`:

- `cancel_batch(batch_id)` cancels every pending task in an operation batch.

## License

MIT
