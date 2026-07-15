# Vendored crate patches

We vendor third-party crates under `third_party/` only when an upstream release
has a bug we cannot work around from the outside, and wire them in via
`[patch.crates-io]` in the root `Cargo.toml`. Each patch is documented here and
marked in-source with a `BROCCOLI VENDOR PATCH` comment so it survives a
re-vendor.

## `broccoli_queue` 0.4.6 (`third_party/broccoli_queue`)

Two message-loss bugs in the Redis broker path. Both silently drop a message
that the queue is supposed to retry or dead-letter, and neither is reachable
from our own code (they live inside `reject` / `process_messages`), so they can
only be fixed in the crate itself.

### 1. `reject` loses a message on its first retry

`src/brokers/redis/broker.rs`, `RedisBroker::reject`.

`reject` first `LREM`s the message out of the `<queue>_processing` list, then —
for a message that still has retry budget (`attempts < 3`) — tries to re-publish
it, reading the publish priority from `message.metadata["priority"]`. But
`InternalBrokerMessage::metadata` is `#[serde(skip)]`, so after a message has
been consumed (`HGETALL`) that metadata is **always** `None`. Upstream then
`?`-returns `"Missing priority"` — _after_ the `LREM` — so the re-publish never
happens and the message is **silently lost on the very first rejection**. This
hits every consumer's error path (e.g. a transient DB failure while persisting a
DLQ envelope drops that dead-letter record).

Patch: when the priority metadata is absent, fall back to the lowest priority
(`5`) instead of erroring, so a rejected message is always re-queued for retry.

### 2. `process_messages` leaks an undeserializable message

`src/queue.rs`, the `Some(concurrency)` branch of `process_messages` (and the
identical block in `process_messages_with_handlers`).

When a consumed message fails to deserialize into the handler's type
(`into_message()` errors — e.g. a schema drift / version skew), upstream logs
and `continue`s **without acking or rejecting it**. The message was already
popped off the main queue and pushed onto `<queue>_processing` at consume time,
so it leaks there forever (with an orphaned payload hash) and its operation
never completes — recovered only by a downstream waiter timeout.

Patch: on a deserialize failure, `acknowledge` the message (drop the poison
message, matching how a poison message would otherwise be dead-lettered) and log
loudly, instead of leaking a processing-queue entry. Both the
`Some(concurrency)` and the `None` (single-threaded) branches of
`process_messages` are patched (the `None` branch previously `?`-propagated the
deser error, killing the consumer loop). `process_messages_with_handlers` has
the same latent gap but is **not called anywhere in broccoli** (only
`process_messages` is used), so it is left unpatched rather than modifying dead
code; if it ever gets used, apply the same fix to its two branches.

### Re-vendoring

To pull a new upstream version: replace `third_party/broccoli_queue`, bump the
version here and in `[patch.crates-io]`, and re-apply both patches (grep for
`BROCCOLI VENDOR PATCH`). If upstream fixes these, drop the vendor and the
`[patch.crates-io]` entry.
