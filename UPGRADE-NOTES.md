# Upgrade Notes

Operator-facing notes for changes that alter the meaning or default of an
existing knob, or that change observable HTTP behavior. New entries go at the
top.

## 2026-05 — UP#40: evaluator result timeouts no longer include queue wait

**Affected versions:** deployments using `result_timeout_ms` in evaluator plugin
configuration, or custom plugins that compute `EvaluationTimeoutBudget`
manually.

**Change:** evaluator `next_result` polling now measures sandbox execution time
instead of charging worker queue wait against the plugin's timeout. Workers emit
a best-effort `Started` reply when an operation begins execution; while a test
case is still queued or has remaining execution budget, the host silently
extends the wait.

The SDK compatibility field `queue_slack_s` remains present, but its default is
now `0.0`, and the old 15-minute minimum timeout floor is removed. Built-in
evaluator plugin defaults were lowered from 900000ms to 240000ms because queue
wait is no longer part of this budget.

**Action required:**

- Review any custom `result_timeout_ms` or `EvaluationTimeoutBudget`
  configuration that depended on the old 300s queue slack or 15-minute minimum
  floor.
- Treat `result_timeout_ms` as execution-result wait budget plus compile/checker
  slack, not as a queue-depth safety valve.
- Keep queue capacity/load tuning on admission and worker scaling knobs rather
  than padding evaluator result timeouts.

## 2026-05 — UP#39: `max_queued_submissions` semantic change + 503 backpressure

**Affected versions:** any deployment that previously set
`server.max_queued_submissions` (or `BROCCOLI__SERVER__MAX_QUEUED_SUBMISSIONS`)
in config.

**Change:** Prior to UP#39, `server.max_queued_submissions` capped the
per-server in-process semaphore queue length (default 100). After UP#39, it caps
the **durable DB-row count** of submissions, code-runs, and judgements with
`status='Queued'` across the cluster (default 5000); the old in-process knob has
been renamed to `server.dispatcher_admission_queue_max` (still default 100).

POST endpoints that would insert a new `Queued` row (submission create, contest
submission, run code, run contest code, rejudge, bulk rejudge, admin fan-out,
DLQ retry, DLQ bulk retry) now return `503 Service Unavailable` with code
`QUEUE_OVERLOADED` and a `Retry-After: <seconds>` header when the cap is
reached. RFC 7231 §6.6.4 makes 503 the correct status for "server is currently
unable to handle the request due to a temporary overload" — distinct from 429
(`RATE_LIMITED`), which signals client-side throttling.

**Action required:**

- If you had `max_queued_submissions = 100` (the old default), expect to hit
  503 + `Retry-After` errors on POSTs once 100 durable queue rows accumulate.
  Raise to 5000 (new default) or higher for production load.
- If you tuned `max_queued_submissions` for in-process queue behavior, transfer
  that value to `dispatcher_admission_queue_max` and pick a new DB-cap value for
  `max_queued_submissions` based on observed `Queued` p95 depth during load
  tests.
- Update API clients that previously branched on `429` / `RATE_LIMITED` from
  submission POSTs to also handle `503` / `QUEUE_OVERLOADED`. Both responses
  populate `Retry-After`, so a generic "back off for the indicated duration on
  either status" path is the simplest migration.

See `config/config.example.toml` for the documented defaults of both fields.
