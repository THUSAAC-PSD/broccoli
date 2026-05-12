use std::cell::RefCell;

thread_local! {
    static HOST_CONTEXT: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
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
