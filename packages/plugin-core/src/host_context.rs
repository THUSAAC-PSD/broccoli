use std::cell::{Cell, RefCell};

thread_local! {
    static HOST_CONTEXT: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
    /// Bytes a host function streamed *into* the guest during the in-flight
    /// plugin call (e.g. `blob_read_range` slices). A plugin call's `input`
    /// length only counts the call's marshalled argument - for the checker that
    /// is a few bytes of refs, while the real 30 MB of test data is pulled in
    /// later via host-function slices. Counting those here lets the pool weigh
    /// how much data each instance has churned through, which is what grows its
    /// WASM linear memory. Reset at call start, summed during the call, read at
    /// call end - all on the same blocking thread, so a `Cell` is sufficient.
    static CALL_STREAM_BYTES: Cell<u64> = const { Cell::new(0) };
}

pub fn current() -> Option<serde_json::Value> {
    HOST_CONTEXT.with(|cell| cell.borrow().clone())
}

pub fn with_context<T>(context: Option<serde_json::Value>, f: impl FnOnce() -> T) -> T {
    HOST_CONTEXT.with(|cell| {
        let previous = cell.replace(context);
        let result = f();
        cell.replace(previous);
        result
    })
}

/// Reset the per-call streamed-byte counter. Call immediately before invoking a
/// plugin function on the current (blocking) thread.
pub fn reset_stream_bytes() {
    CALL_STREAM_BYTES.with(|c| c.set(0));
}

/// Add to the per-call streamed-byte counter. Called by host functions (e.g.
/// `blob_read_range`) that pull data into the guest during a plugin call.
pub fn add_stream_bytes(n: u64) {
    CALL_STREAM_BYTES.with(|c| c.set(c.get().saturating_add(n)));
}

/// Read the bytes streamed into the guest since the last [`reset_stream_bytes`].
/// Call immediately after a plugin function returns, on the same thread.
pub fn stream_bytes() -> u64 {
    CALL_STREAM_BYTES.with(|c| c.get())
}
