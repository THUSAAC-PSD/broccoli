# Broccoli Run-2 Follow-Up Plan

Synthesis of five parallel deep-dive investigations:

- reliability + host-fn bridge (`a0a912cb...`)
- worker performance + blob cache (`af317afc...`)
- single-binary-per-worker + server overhead + metrics (addenda)
- **cascade physics verification (`aaa05fbd...`) — corrects three claims about
  tokio + extism internals**
- **plugin pinning + lock-lifetime verification (`aa8bed1a...`) — identifies
  batch-evaluator (not ICPC) as the primary saturation point**
- **rule-out alternatives (`a9e5bfd9...`) — finds the operation_result consumer
  as an independent co-conspirator**

The verification round produced material corrections to the original narrative.
See §0 for the corrections; the rest of the document is updated accordingly.

Author goal: produce **one architecture** that satisfies every constraint the
user named:

1. **Never crash, never SystemError, never 429.** Every accepted submission
   eventually judges with a real verdict (AC / WA / TLE / MLE / RE / CE); only
   timeouts the user _actually_ hits — i.e. the testcase's wall-time limit —
   produce TLE. Plugin pool contention, runtime starvation, and worker
   saturation are _never_ user-visible.
2. **Workers run at single-core max speed.** Per testcase, everything except the
   user binary's own runtime should be minimized. Platform overhead measured and
   bounded.
3. **One contestant binary per worker by default.** Configurable upward but
   default `1`. Concurrency scales by adding worker machines, not by
   parallelizing within a worker.
4. **Observability that answers all the above next time.** Specific `broccoli_*`
   metric and tracing-span list grounded in code.

---

## 0. Verification-round corrections to this document

Three claims in §1 and §2 are wrong as originally stated. Corrected versions
inline below; this section names them up front so future readers don't trust the
un-corrected language elsewhere.

### Correction A — tokio `block_in_place` is bounded, not unbounded

The replacement worker tokio spawns when `block_in_place` is called is
dispatched through the **blocking pool**, counted against
`max_blocking_threads + worker_threads` (520 default on our 8-core droplet). It
is NOT unbounded. Source:
`tokio-1.50.0/src/runtime/scheduler/multi_thread/worker.rs:344-484`, in
particular the `runtime::spawn_blocking(move || run(worker))` at line 464.

The 866-thread count observed at deadlock decomposes as: ~520 active blocking
pool + ~200-300 threads in the 10-second `keep_alive` limbo (idle but not yet
exited) + a secondary OTLP exporter runtime sharing the default
`"tokio-rt-worker"` thread name.

### Correction B — the futex is on extism's pool mutex, not on `thread::sleep`

`std::thread::sleep(100ms)` uses `clock_nanosleep`, which shows up as
`hrtimer_nanosleep` in kernel stacks. The observed `futex_wait_queue` stack at
deadlock cannot be sleep. The threads are parked at
**`Arc<Mutex<PoolInner>>::lock()`** inside extism's `Pool::find_available`
(`extism-1.13.0/src/pool.rs:114, 125, 139`). Every busy-wait iteration takes
this mutex; with hundreds of threads pounding it, the futex behind this
`std::sync::Mutex` is the global serialization point.

### Correction C — batch-evaluator (not ICPC) is the primary saturated pool

The original narrative pinned the ICPC pool as the bottleneck. The actual
saturation order:

- ICPC pool load multiplier: **N_submissions** (one slot per active submission,
  pinned for full judgement ~30s).
- Batch-evaluator pool load multiplier: **N_submissions × N_testcases** (one
  slot per active testcase, pinned for per-testcase wallclock ~1-5s).

For Signpost (20 testcases) and 100 concurrent submissions: 2000 in-flight
batch-evaluator calls competing for a pool of 256 → **~8× over-subscription at
batch-evaluator** while ICPC sees 100/256 = no saturation. ICPC pool _appears_
saturated only as a downstream consequence: each ICPC instance pinned at
`next_result()` (a `crossbeam::recv_timeout` host fn) waiting for
batch-evaluator results that aren't coming.

The 47% SystemError verdicts in profile2 came from **batch-evaluator's**
`pool.get` timing out (caught in `services/evaluate_batch.rs:157-169`), not from
ICPC's. Steady-state throughput cap:

**Throughput = min(icpc_pool / icpc_pin_duration, batch_evaluator_pool /
(N_testcases × per_testcase_duration))**

With current settings: min(256/30s, 256/(20×2s)) = min(8.5, 6.4) = **~6.4
sub/s**. Matches observed ~2 sub/s after retry-loop overhead and
`operation_result` consumer return-path bottleneck.

### Correction D — the host-fn bridge fix is structurally simpler than described

Originally I claimed every `host_funcs/*.rs` file would need to drop its
`block_in_place(|| ...)` wrapper. **It doesn't.** Tokio's `block_in_place`
checks "am I on a multi-thread runtime worker?"; if no, it just runs the closure
directly (`worker.rs:418-422`). After the `spawn_blocking` fix at
`traits.rs:333`, plugin calls run on blocking-pool threads (not workers), so
every nested host-fn `block_in_place` becomes a **no-op automatically**.

Per-submission thread amplification factor drops from O(host_fn_calls) ≈ 30+
down to **1**. One line change kills the entire cascade. Caveats:
`tracing::Span::current()` is thread-local (explicit propagation needed), panic
semantics change from unwinding to `JoinError`.

### Correction E — operation_result return path is an independent co-conspirator

`packages/server/src/consumers/operation_result.rs:9-64` runs
`process_messages(.., None /* concurrency */, ..)` — strictly serial: pop one
result from Redis (ZPOPMIN, with 500ms sleep when empty), process, ack, loop.
Under load with 2000+ result messages/min, this caps return-path throughput at
~200 msg/s/replica regardless of how fast workers are.

**This is why workers stayed idle (`w1_load=0.05`) while the api saturated.**
The api couldn't drain the return path fast enough; upstream judging tasks
remained in-flight longer, holding their `block_in_place`-promoted worker
threads longer, feeding the cascade.

Fix is independent of the host-fn bridge: `concurrency = Some(8)` (handler at
lines 20-58 is concurrency-safe — DashMap waiter table + cheap oneshot send).
5-line change. Phase A item.

### Correction F — per-worker concurrency is already 1, not 2

I claimed worker effective concurrency was 2 (shared + private queues). Wrong:
`tokio::select!` over the two queues **cancels the loser branch each iteration**
(`packages/worker/src/runtime.rs:166-199`). Effective per-worker concurrency is
**1** today. The `worker.max_concurrent_operations` knob in §2.4 is mainly about
making this observable and explicit, not changing default behavior.

---

## 1. What actually broke under load (recap, with code refs)

**The smoking gun (api1-threads-223116.txt at deadlock):**

```
    847 tokio-rt-worker     S (sleeping)   futex_wait_queue
     18 broccoli-server     S (sleeping)   futex_wait_queue
      1 OpenTelemetry.T     S (sleeping)
```

The cascade has two stages:

### Stage 1 — worker-thread promotion via `block_in_place`

Every plugin host function in `packages/server/src/host_funcs/` uses:

```rust
tokio::task::block_in_place(|| Handle::current().block_on(async fn))
```

Per `tokio::task::block_in_place` semantics, the current tokio worker is marked
"blocking" and the runtime spawns a replacement worker to keep the reactor
responsive. Each plugin host call grows the worker pool by 1. The ICPC
server-side fan-out at `packages/server/src/services/evaluate_batch.rs:101`
issues `tokio::spawn` × 20 per submission, and each spawn eventually descends
into multiple host-fn calls (`storage:get`, `sql:db_query`,
`submission:insert_results`, etc.). Under ~100 concurrent submissions × 20-way
fan-out × N recursive host-fn calls per task, the worker pool blows past tokio's
512-thread default cap, then past OS limits.

### Stage 2 — futex storm on `extism::Pool`

`packages/plugin-core/src/traits.rs:299–410` calls `pool.get(timeout)` for each
`call_raw`. Inside the extism crate (`extism-1.13.0/src/pool.rs:131–158`),
`Pool::get` is a **`std::thread::sleep(100ms)` busy-wait loop** under an
`Arc<std::sync::Mutex<PoolInner>>` until either a slot frees or timeout fires.
So the 800+ threads aren't on `Mutex<Plugin>` literally — they're sleeping in
`Pool::get`'s 100 ms `thread::sleep`s, with synchronous OS-level mutex
acquire-release each iteration. Once the pool is full _and_ each ICPC contest
plugin instance is pinned for the full judgement (because of the
`while next_result` polling loop at `plugins/icpc/src/evaluate.rs:120-174`), no
slot frees and the storm self-sustains.

**The two reinforcing conditions:**

- Per-call worker-thread promotion (the `block_in_place` family).
- Per-plugin instance pinned for the entire judgement (the contest plugin's
  `while next_result()` loop holds one slot for ~tens of seconds of testcase
  fan-out).

The retry-loop fix shipped this run preserves the verdict correctness (no
SystemError) but does **not** untangle either condition. It just adds queueing
pressure to a runtime that's already past saturation.

---

## 2. The target architecture

Three independent moves, deliverable in three phases. None requires changing
Extism v1 (which is sync-only — confirmed in
`extism-1.13.0/src/function.rs:189`).

### 2.1 Durable submission accept (closes the silent-loss gap)

**Current state.** `packages/server/src/handlers/submission.rs:880` accepts the
request, runs the synchronous `before_submission` hook chain (which can 429 via
`PluginError::PoolTimeout → AppError::RateLimited`), inserts the submission row
at `submission.rs:945-957`, commits the txn, and then **fires
`tokio::spawn(dispatch_to_plugin)` at `submission.rs:970-974`**. Dispatch is
entirely in-process tokio state — no MQ enqueue, no DB job queue. If the api
process dies, panics, or the dispatch task aborts, the row stays `Pending`
forever; the stuck-job detector at `packages/server/src/dlq/stuck.rs:167-173`
then **writes `SystemError`**.

**Fix.** After `txn.commit()`, publish a
`JudgeJob { submission_id, judge_epoch }` to a durable MQ queue
(`submission_judge_queue`, modeled on the existing `operation_tasks` queue in
`packages/server/src/services/operation_batch.rs:108-129`). A pool of consumers
(initially in-process on the api, later movable to the worker tier) picks them
up and runs `dispatch_submission_to_plugin_with_judgement`. The contest plugin's
existing `judge_epoch` check at `plugins/icpc/src/evaluate.rs:99-108` already
gives at-least-once safety.

**Stuck-job detector becomes a re-enqueue, not a verdict-setter.** Replace
`mark_submission_system_error` (`dlq/stuck.rs:167`) with
`enqueue_for_redispatch`. The DLQ keeps a record for observability; the
submission row stays `Pending → Running → final` as designed.

**before_submission hooks.** The synchronous-hook path is the one place left
where contention can surface as a user-visible error. Two options:

- **Move blocking hooks to async post-accept.** Submission is recorded, hooks
  fire post-commit, plugin-rejected submissions become `Rejected` (new terminal
  status) with the plugin-provided reason. Pro: never any sync wait at HTTP
  time. Con: changes API contract — clients no longer get a synchronous
  rejection. Likely the right long-term shape.
- **Internal retry-with-backoff on `PoolTimeout` for sync hooks** (no longer
  mapping to 429). Same retry pattern as `services/evaluate_batch.rs:157-169`.
  Easier; preserves API contract; trades latency for reliability.

Either way, the `From<PluginError> for AppError` mapping for
`PoolTimeout → RateLimited` (`server/src/error.rs:206-209`) goes away.
Contention is an internal concern, never a 429.

### 2.2 Eliminate the worker-thread promotion (closes Stage 1 of the smoking gun)

**One change at `packages/plugin-core/src/traits.rs:333`:**

```rust
// Before:
let result = tokio::task::block_in_place(|| { ... pool.get(timeout) ... plugin.call(...) });

// After:
let result = tokio::task::spawn_blocking(move || {
    pool.get(timeout) ... plugin.call(...)
}).await??;
```

`spawn_blocking` puts the work on tokio's dedicated blocking pool (default 512,
configurable) and never promotes a core worker. The blocking pool is bounded —
once it's full, new `spawn_blocking` calls queue, but core workers stay
responsive. `/healthz`, `/metrics`, and all non-plugin HTTP routes survive
saturation.

**Inside the plugin call, host-fn callbacks are still sync** (Extism's hard
constraint). The callbacks themselves run on the now-blocking thread. The
pattern they use today — `block_in_place(Handle::current().block_on(async fn))`
— needs to change to plain `Handle::current().block_on(async fn)` because
`block_in_place` requires a multi-thread tokio runtime worker and the host fn is
no longer on one. Audit list (every host-fn file in
`packages/server/src/host_funcs/`):

| File:line                                       | Function                           |
| ----------------------------------------------- | ---------------------------------- |
| `host_funcs/checker.rs:57,85`                   | `checker:run` lookup + invoke      |
| `host_funcs/config.rs:147,212`                  | `config:get`, `config:set`         |
| `host_funcs/dispatch.rs:78`                     | `operations:start`                 |
| `host_funcs/evaluate.rs:73`                     | `evaluator:start_batch`            |
| `host_funcs/language.rs:51,81`                  | `language:resolve` lookup + invoke |
| `host_funcs/registry.rs:131,174,310`            | registry mutators                  |
| `host_funcs/sql.rs:157,188,215,251,286,311,338` | sql/txn API                        |
| `host_funcs/storage.rs:73,157,207,264,300`      | blob + KV store                    |

Mechanical change: drop the `block_in_place(|| ...)` wrapper, keep the
`Handle::current().block_on(async { ... })`.

### 2.3 Bounded contest-plugin dispatch + freeing the pinned instance (closes Stage 2)

**Pool exhaustion at the contest plugin.** ICPC's
`plugins/icpc/src/evaluate.rs:120-174` runs a
`while collected < N { host.eval.next_result(...) }` loop while inside one
`pm.call_raw("icpc", ...)`. The instance held for that call is pinned for the
_entire judgement_ — tens of seconds of testcase fan-out. With pool=256 and 256+
concurrent judgements, every instance is pinned and the 257th submission backs
up in `Pool::get`'s busy-wait.

**Two fixes, complementary:**

**(a) Bound contest-plugin dispatch with a Rust-side semaphore** _before_ it
reaches the pool's busy-wait. Add `Arc<Semaphore>` keyed by `plugin_id` (sized
to `pool_max_instances` for that plugin, e.g. 64), acquire in
`services/submission_dispatch.rs` before spawning the dispatch task. When the
semaphore is full, new MQ messages stay in the queue; consumers don't pull them.
**No 429, no SystemError, just backlog growing on the MQ** — the explicit
work-conservative property the user wants.

**(b) Eliminate the contest plugin's polling pin** (the bigger architectural
win, designed but landable in phase 2 below). Replace the WASM-side
`while next_result() { ... }` loop in `plugins/icpc/src/evaluate.rs:120` with a
single host call `host.eval.run_batch(batch_id, on_each_result_handler)`. The
host fn drives the loop in Rust, and the contest plugin's WASM instance is freed
during the testcase fan-out. The instance returns to the pool while the 20
testcases run; one more re-entry into WASM at the end to compute the final
verdict. Pool slots become judgement-step-granular instead of
full-judgement-granular.

### 2.4 Single-binary-per-worker (the production constraint)

**Current state.** `packages/worker/src/runtime.rs:166-199` subscribes to
shared + private queues via `tokio::select!`, each using
`mq::process_messages(queue, None /* concurrency */, None, handler)`.
`concurrency=None` in broccoli_queue's driver
(`broccoli_queue-0.4.6/src/queue.rs:836-862`) means strict serial
dequeue-process-ack per branch. Effectively **2 concurrent OperationTasks per
worker** (shared + private), not exposed as a knob.

**Fix.**

1. Add `worker.max_concurrent_operations: usize` (default `1`) to
   `packages/worker/src/config.rs`.
2. Wrap the `Worker::execute_task` call in
   `packages/worker/src/models/worker.rs:55` with a process-wide
   `tokio::sync::Semaphore::new(max_concurrent_operations)`. Acquire _before_
   the isolate-touching path (`executor.rs:177`). Single permit = single binary
   running at a time, even with both queues delivering messages.
3. Surface in `HeartbeatConfig.max_concurrency` (`runtime.rs:94`, the field
   already exists).

**Why a semaphore wrapper around `execute_task`, not a `process_messages`
concurrency cap.** Because broccoli_queue's `process_messages` with
`concurrency=Some(N)` pulls up to N messages eagerly. If we want **true**
work-stealing across workers, we must only pull a message when we have a permit
available — otherwise the worker hoards messages while idle workers in the
cluster sit empty. The semaphore wrapping `execute_task` doesn't solve that on
its own; the right shape is
`loop { permit = sem.acquire().await; let msg = pull_one(); spawn(process(msg, permit)) }`.
Audit whether broccoli_queue lets us drive the loop manually; if not, the loop
lives in worker/src and the consumer trait gets `pull_one`.

**Implication for server-side fan-out:** keep `services/evaluate_batch.rs:101`'s
20-way `tokio::spawn`. Each spawn awaits its OperationTask result via MQ; the
spawns themselves are cheap. Workers absorb them at single-binary pace. The
20-way fan-out becomes a way to **spread work across the worker fleet**, exactly
the right knob.

### 2.5 Worker self-sufficiency — already correct, codify it

Both deep-dives confirmed: **the worker has zero RPC back to the api server
during judging.**

- No `host_funcs/` directory under `packages/worker/src/`. No Extism plugins
  loaded on the worker.
- Worker connects directly to Postgres (`models/operation/executor.rs:96-114`,
  default `max_connections=3`) for the operation task-cache lookup, and directly
  to SeaweedFS (`executor.rs:118-133`) for blob fetch/put. No api-server proxy.
- Worker→server communication is purely MQ-mediated: `task_runner.rs:36`
  publishes `TaskResult` to `operation_results`; server consumer at
  `packages/server/src/consumers/operation_result.rs` dispatches to a
  `tokio::sync::oneshot` waiter.

**Codify as an invariant.** Add to `CLAUDE.md`: _"The worker has no host
functions and makes no synchronous calls to the API server. Any new worker
capability must be satisfied via either MQ messages or direct access to shared
infrastructure (Postgres, blob storage, Redis)."_ Without this, a careless
future change (e.g. an "auth-checked blob proxy") could re-introduce an RPC
hot-path.

**One last server-side path worth watching.** Per-testcase, after the worker
publishes its result, the server runs:

1. Redis consume + ack (`consumers/operation_result.rs:20-58`).
2. Oneshot send → wakes the spawned task in `services/evaluate_batch.rs:101`.
3. Spawned task deserializes `TestCaseVerdict`, ships it through a
   `crossbeam::channel` to the batch's result_rx.
4. ICPC plugin's `next_result()` (`evaluate.rs:120`) wakes via
   `result_rx.recv_timeout`; **one Extism call boundary, one
   `pool.get(timeout)`, one `Mutex<Plugin>` acquire-release**.
5. Plugin calls `insert_results`
   (`packages/server-sdk/src/sdk/submissions.rs:120-167`) → server-side host fn
   → `block_in_place(block_on(...))` → **one Postgres INSERT** per testcase.

So **20 testcases = 20 Extism contest-plugin re-entries on the verdict path**,
each holding the contest plugin's mutex briefly. Plus 20 single-row INSERTs (the
multi-row INSERT path at `submissions.rs:158-164` is supported but the call site
passes a single-element slice from `plugins/icpc/src/evaluate.rs:303`). Two
fixes:

- **Batch the verdict insertion**: accumulate verdicts in the plugin and call
  `insert_results` every K verdicts or every T ms. Cuts 20 INSERTs → 2-3 INSERTs
  per submission.
- **Combine with 2.3(b)**: once the contest plugin no longer pins its instance
  for the full judgement, the per-testcase verdict re-entry doesn't have
  contention pressure either.

---

## 3. Worker performance — single-core max speed

Even with all the reliability fixes above, the user wants the worker to actually
be fast.

### 3.1 Per-testcase wallclock budget (Signpost C++ AC, hot cache, warm pool)

| Phase                                               | Time                              | Code                                                  |
| --------------------------------------------------- | --------------------------------- | ----------------------------------------------------- |
| MQ consume + dedup claim                            | ~1–3 ms                           | `consumer.rs:80-145`                                  |
| Sandbox env-init (`isolate --init --cg`)            | **~30–80 ms**                     | `isolate.rs:271-307`                                  |
| Materialize input files from cache                  | **~150–300 ms for 27 MB**         | `handler.rs:413-470`, `file_cacher.rs:208-220`        |
| Compile (cache hit)                                 | ~5–20 ms (cache restore copy)     | `handler.rs:702-779`                                  |
| Run user binary (the budgeted cost)                 | the actual user cost, e.g. 200 ms | sandbox `execute`                                     |
| Run checker (standard-checkers)                     | ~5–50 ms                          | `host_funcs/checker.rs:85`                            |
| Collect output → SeaweedFS + cache                  | ~3–10 ms per file                 | `handler.rs:1052-1075`                                |
| Sandbox cleanup (`isolate --cleanup --cg`)          | **~20–50 ms**                     | `isolate.rs:309-332`                                  |
| MQ publish result                                   | ~1–3 ms                           | `task_runner.rs:36`                                   |
| Server result-consumer → contest plugin → DB INSERT | ~5–20 ms                          | `consumers/operation_result.rs`, `submissions.rs:120` |

For a 200 ms user binary, non-user overhead is **~220–540 ms** — i.e. **half to
three-quarters of wallclock is platform**. The biggest two: input-file copy (27
MB at ~80–150 MB/s tokio::fs::copy is 200ms+) and sandbox init/cleanup (combined
~50–130 ms).

### 3.2 Fixes ranked by impact

**A. Reflink/hardlink for cache→sandbox file materialization.**
`file_cacher.rs:213-220` does `tokio::fs::copy(cached, dest)` for every cache
hit. For 27 MB inputs this is a 27 MB read+write per testcase. Use
`std::os::unix::fs::link` (hardlink) when source and dest are on the same FS, or
a reflink ioctl (`FICLONE`) on btrfs/xfs. Cuts 200 ms → <1 ms per cache hit.
_This is the single biggest worker-side win for Signpost._

**B. Reuse sandbox box across testcases of one submission.** Today each testcase
is a separate `OperationTask` → separate `--init` and `--cleanup`
(`handler.rs:236-242`). If the server-side fan-out batches K testcases into one
OperationTask DAG (with K separate `exec` steps that share the compile-step
output), one init+cleanup pays for K testcases. The handler already supports
multi-step DAGs (`handler.rs:503-564`). Saves ~50–130 ms × (K-1) per submission.
Requires a server-side change to `services/evaluate_batch.rs` to group testcases
per worker.

**C. Eliminate cross-worker compile redundancy.** With single-binary-per-worker
and 20 testcases spread across N workers, each worker does its own compile (~2–5
s for C++ -O2) → N × compile_cost. Two options:

- **Compile-once-on-server** before fan-out: the evaluator does the compile step
  centrally, populates the task_cache, then fans out 20 exec-only operations.
- **Distributed compile lock**: Postgres advisory lock keyed by compile
  cache_key inside `try_cache_hit` (`handler.rs:702-779`). Only one worker
  compiles per cache key; others wait for the row to land. Cheaper to implement,
  slightly slower in the cold-cache case.

**D. Bulk insert verdicts.** Already covered in §2.5.

**E. Populate `toolchain_fingerprint`.** `executor.rs:27` is
`let fingerprint = String::new();`. So the compile cache key has no
compiler-version component. Probe `g++ --version` (and other relevant
toolchains) at worker boot and hash into the fingerprint. Today this is a
**correctness bug** (compiler upgrade serves stale cached binaries), not just a
perf miss.

**F. Parallel `load_environment_files`.** `handler.rs:419-468` fetches input
files in a serial `for` loop. Use `join_all`. Minor for Signpost (1 big input);
meaningful for IOI subtask DAGs.

**G. `exists()` short-circuit in `upload_from_path`.** `file_cacher.rs:278-314`
always streams the file to SeaweedFS even if its hash already exists. Cheap HEAD
probe first. Saves SeaweedFS bandwidth.

### 3.3 Blob cache audit

- **Content-hash addressed**, **LRU by size**, **no TTL**, **single-flight per
  hash**. (`file_cacher.rs:82-204`).
- **Default `max_cache_size = 512 MB`** (`config.rs:80-81`).
- **Signpost working set is 419 MB** (inputs only). With 20 expected-outputs the
  total can be **500–800 MB → cache thrashes**. Bump default to e.g. 4 GiB
  (worker droplets are 16 GB) or expose as a contest-sized knob. The cost is
  purely disk, which is plentiful.
- **No cache metrics exposed today.** Listed in §4 below.

### 3.4 Plugin call hot path

ICPC plugin doesn't actually fan out parallel — its WASM loop is
`while next_result()`. The fan-out lives in `services/evaluate_batch.rs:101`.
Per testcase verdict, the contest plugin is re-entered once (briefly). Per
testcase, the standard-checkers plugin runs once (also briefly, ~5–50 ms). So
plugin overhead per submission is bounded; the _bottleneck_ is the contest
plugin's mutex contention, which §2.3 addresses.

---

## 4. Observability for next profiling run

The single biggest gap last run: when api `/metrics` got starved, we lost
visibility for the most diagnostic 25 min. Three categorical fixes:

### 4.1 Move `/healthz` and `/metrics` off the user-traffic tokio runtime

Today they share a runtime with all request handling. When the runtime
saturates, both die. Cheapest fix: a separate axum sub-router on a different
`tokio::spawn`'d listener, with the metrics-collection task on its own runtime
if needed. Or use prometheus's pull endpoint via a dedicated thread+`tiny_http`.

### 4.2 New `broccoli_*` metrics to add (concrete list)

In `packages/common/src/metrics.rs`:

**Plugin call hot path:**

- `host_fn_duration` (Histogram<f64>): seconds per host-fn call. Labels:
  `host_fn.name`, `plugin.id`, `outcome={ok,err}`.
- `host_fn_calls_total` (Counter<u64>): same labels. QPS per host fn.
- `host_fn_block_in_place_total` (Counter<u64>): incremented only inside fns
  still using `block_in_place`. After §2.2 this should be zero; useful as a
  regression guard.
- `plugin_evaluator_semaphore_wait_duration` (Histogram<f64>): wait at
  `services/evaluate_batch.rs:103`'s `evaluator_slots.acquire_owned()`. Labels:
  `plugin.id`.
- `plugin_instance_acquire_duration` (already exists at `metrics.rs:28`): verify
  emitted on every `pool.get(timeout)` path including the retry loop at
  `services/evaluate_batch.rs:138-141`.
- `plugin_pool_contention_total` (Counter<u64>): increment when an acquire
  takes > N ms. Labels: `plugin.id`, `bucket={50ms,200ms,1s,5s}`.

**Submission lifecycle:**

- `submission_state_transition_duration` (Histogram<f64>): seconds between
  transitions. Labels: `from`, `to`, `problem_type`.
- `submission_in_flight` (UpDownCounter<i64>) by `state`: total in each state.
  Sample from DB periodically, or maintain in-process.
- `submission_judge_queue_depth` (Gauge): Redis LLEN of the new MQ queue.
- `submission_age_in_pending_seconds` (Histogram<f64>): `now - created_at` for
  `Pending` rows.

**Worker side:**

- `worker_permits_in_flight` (UpDownCounter<i64>): running count of acquired
  permits (§2.4).
- `worker_permits_max` (Gauge): configured cap.
- `blob_cache_hits_total` / `blob_cache_misses_total` (Counter<u64>): cache
  hit/miss in `file_cacher.rs:213`.
- `blob_cache_size_bytes` (Gauge): current cache size.
- `blob_cache_evictions_total` (Counter<u64>): LRU evictions at
  `file_cacher.rs:177-189`.
- `blob_cache_fetch_seconds` (Histogram<f64>): labels `outcome={hit,miss}` —
  separates the 1 ms reflink from the 200 ms SeaweedFS RTT.
- `sandbox_init_duration` / `sandbox_cleanup_duration` (Histogram<f64>): around
  `isolate.rs:271-307` / `:309-332`. Labels: `enable_cgroups`.
- `file_materialization_copy_seconds` (Histogram<f64>) with
  `path_kind={input,output,source}` label — separates 27 MB input copy from 1 KB
  source copy.
- `worker_compile_cache_redundancy_total` (Counter<u64>): increment when a
  worker observes a `task_cache` miss for a key that already has a row by the
  time we want to write. Detects cross-worker compile redundancy.

**End-to-end:**

- `operation_result_e2e_duration` (Histogram<f64>): `task.enqueued_at_unix_ms` →
  moment delivered to awaiting plugin via `next_operation_result`. Labels:
  `task_type`, `operation`, `worker.id`, `outcome`.
- `evaluate_batch_total_duration` (Histogram<f64>): `start_batch` returns → last
  `pending_count == 0`. Labels: `problem.type`, `evaluator.plugin.id`,
  `test_case_count_bucket`.

### 4.3 Tracing spans to add or tighten

`packages/common/src/observability.rs` already has OTLP. Audit:

- Every host-fn `_fn` callback in `packages/server/src/host_funcs/` opens a
  child span. Most use `tracing::instrument` on the inner async fn but the outer
  extism callback (which is the synchronous bridge) does not. Add
  `let _span = tracing::info_span!("host_fn", name = ..., plugin_id = ...).entered();`
  at the top of each `_fn`. After §2.2 these run on `spawn_blocking` threads, so
  the span needs explicit linking to the parent via
  `Span::current().or_current()`.
- `isolate_init`, `isolate_cleanup`, `file_cacher_fetch`, `file_cacher_upload` —
  add `#[instrument]` to these so Jaeger shows the platform-overhead breakdown
  per testcase.
- Per-submission span at the server (`services/submission_dispatch.rs`) with
  attributes `submission.id`, `worker.id` (when known), `judge_epoch`. Lets
  Jaeger visualize cross-worker fairness as span-span variance.

### 4.4 Build flags for the next profiling run

In `Cargo.toml`:

```toml
[profile.release]
debug = 1   # full DWARF, not line-tables-only
```

and at build time: `RUSTFLAGS="-C force-frame-pointers=yes"`.

This converts the next perf flame graph from "kernel-time-dominated, every Rust
frame collapses to `[broccoli-server]`" into actionable Rust source maps. Costs
~5–10% steady-state CPU. Worth it.

### 4.5 Dashboard

Extend `config/grafana/provisioning/dashboards/broccoli-overview.json`:

- Host-fn duration P50/P95/P99 (one panel per fn name).
- Plugin instance acquire P99.
- Plugin pool contention rate.
- Worker permits in-flight vs max.
- Submission state transition durations by transition.
- Operation-result e2e duration.
- Blob cache hit ratio + size.

That's the minimum set to answer "where is time spent" without re-running cargo
flamegraph.

---

## 5. Phased delivery

### Phase A — quick wins (1 day) — REVISED post-verification

These are mechanical and ship the same day:

1. **`traits.rs:333`: `block_in_place` → `spawn_blocking`.** ONE LINE change
   closes the entire cascade (see Correction D). Add span propagation:
   `let span = Span::current(); spawn_blocking(move || { let _g = span.enter(); ... })`.
   Add panic handling: explicit `.await` with `JoinError::is_panic()` check.
2. **Raise `max_blocking_threads` to 1024** (default 512) via explicit
   `tokio::runtime::Builder` in `packages/server/src/main.rs`. Move from
   `#[tokio::main]` to an explicit runtime builder so we control the cap.
3. **`operation_result` consumer: `concurrency = Some(8)`** at
   `packages/server/src/consumers/operation_result.rs:9`. Independent of #1,
   attacks Correction E.
4. Remove the `PluginError::PoolTimeout → AppError::RateLimited` mapping at
   `error.rs:206-209`. Contention is an internal concern.
5. Replace `stuck.rs:167-173`'s `mark_submission_system_error` with
   `tokio::spawn(dispatch_submission_to_plugin(...))` re-dispatch. Still
   in-process, but no SystemError verdict.
6. Worker config `max_concurrent_operations = 1` default, semaphore wrapping
   `execute_task` (§2.4). Per Correction F, this is
   observability/configurability — current default behavior already matches.
7. `toolchain_fingerprint` from `g++ --version` etc. (§3.2 E). **Correctness
   fix.**
8. Reflink/hardlink fast-path in `file_cacher.rs:213-220` (§3.2 A).
9. `exists()` short-circuit in `upload_from_path` (§3.2 G).
10. Bump `max_cache_size` default to 4 GiB (§3.3).
11. Cache hit/miss + size + eviction metrics (§4.2 worker section).
12. `host_fn_duration` + `host_fn_calls_total` + `host_fn_block_in_place_total`
    (regression guard) histograms (§4.2 plugin call hot path).

**Removed from prior Phase A**: the "drop `block_in_place(|| ...)` wrapper in
every `host_funcs/*.rs` file" step. Per Correction D, inner `block_in_place`
calls auto-collapse to no-ops after #1.

### Phase B — proper architecture (1 week) — REVISED post-verification

12. Durable submission accept via new MQ queue (§2.1). Add consumer in
    `runtime.rs`.
13. **Bounded contest-plugin dispatch** via per-plugin-id semaphore (§2.3 a).
14. **Bounded batch-evaluator dispatch** via the same semaphore mechanism. **THE
    NEW PHASE B ITEM** — per Correction C, this is the actually-saturated pool.
    Add a per-plugin-id semaphore around `services/evaluate_batch.rs:101`'s
    spawn (before `pm.call_raw("batch-evaluator", ...)`).
15. Codify worker-self-sufficiency invariant in `CLAUDE.md` (§2.5).
16. **[Rejected as written]** Sandbox-reuse across testcases of one submission
    via batched OperationTasks (§3.2 B). _Reason_: cross-testcase contamination
    within one submission is a fairness/correctness problem — user code can leak
    state in `/tmp` and `/sandbox/scratch` between testcases, leak
    fds/processes, or leave the sandbox in a bad state after a crash
    mid-testcase. ICPC and most contest systems traditionally guarantee
    per-testcase isolation explicitly; operators will reasonably push back on
    losing that. The ~50-130 ms × (K-1) saving is real but not worth the
    contract change. _Replacement_: pre-allocate a pool of cgroup hierarchies at
    worker boot. `isolate --init` claims one from the pool; `--cleanup` returns
    it (after wiping). Most of the per-init cost is cgroup setup; pool
    eliminates that on the hot path. Filesystem-namespace setup is also
    amortizable. Smaller win per testcase (~30-80 ms vs ~50-130 ms) but
    preserves the per-testcase-fresh contract. Alternative: just accept the
    overhead and prioritize higher-leverage fixes — on a 1-2 s user binary, 80
    ms × 20 testcases = 1.6 s per submission, which is meaningful but not
    dominant.

17. **Cross-worker compute coordination via leader-election in the cache
    subsystem.** _(REVISED — the original plan had this as a Postgres advisory
    lock keyed by cache_key, and considered moving compile to the server. Both
    are wrong, see below.)_ _Original PG advisory lock issue_: lock is
    session-scoped, holds a worker DB connection for the duration of the compute
    (2-5s for compile). Worker sea-orm `max_connections` defaults to 3; waiting
    workers consume a connection from their pool the whole time they wait.
    Over-subscription cascades. _"Compile on server" issue_: leaks plugin-domain
    knowledge into the server. The server's `OperationTask` abstraction is
    supposed to be an opaque step-DAG; making the server pre-issue "the compile
    step" requires the server to know which step is the compile, how to extract
    it from a `[compile, exec]` DAG, what its cache_key looks like, etc. Bad
    layering. _Correct design_: leader-election becomes a generic property of
    the worker's cache subsystem. The host doesn't know about compile; the
    plugin still builds the same `[compile, exec]` DAGs it builds today; **only
    the worker changes**.

    Concretely, when a worker's `try_cache_hit` returns miss for a step with
    `cache: Some(...)` (see
    `packages/worker/src/models/operation/handler.rs:702-779` and
    `task_cache.rs:125-157`):

    ```
    fn execute_step(step):
        if step.cache.is_some():
            if try_cache_hit(cache_key): return cached_outputs
            match try_acquire_compute_lock(cache_key):
                Acquired(lock) =>
                    spawn_heartbeat(lock, every=5s)
                    outputs = run_step()
                    write_cache(cache_key, outputs)
                    release_lock(lock)
                    return outputs
                HeldByOther =>
                    wait_for_cache_hit(cache_key, timeout=lock_ttl)
                    return cached_outputs
        else:
            return run_step()
    ```

    Lock implementation: Redis SETNX with TTL
    (`SET compute_lock:<cache_key> <worker_id> NX EX 30`), heartbeated every 5s
    by the leader (single Redis `EXPIRE`). Followers poll `task_cache` for the
    row; on TTL expiry without a row, attempt to acquire as a new leader.
    Implementation lives entirely in
    `packages/worker/src/models/operation/{handler.rs, task_cache.rs}` — no
    server, plugin, or `OperationTask`-format changes.

    Properties:
    - Host stays generic (doesn't know "compile").
    - Plugins stay unchanged (batch-evaluator keeps building `[compile, exec]`
      DAGs).
    - Generalizes to any other heavyweight cacheable step a future plugin might
      emit (build-interactive-judge, pre-process-large-fixture, ...).
    - TTL handles leader-crash automatically.
    - No serial front-latency (workers still pull from MQ in parallel; followers
      just wait briefly for the cache row).

18. **Bulk `insert_results`** at `plugins/icpc/src/evaluate.rs:303` — currently
    calls with single-element slices despite multi-row support at
    `server-sdk/src/sdk/submissions.rs:158-164`. Buffer K verdicts or T ms.
19. Move `/healthz` and `/metrics` off the user-traffic runtime (§4.1).
20. Submission lifecycle metrics + worker permits gauge + sandbox init/cleanup
    histograms (§4.2).
21. Tracing-span audit per §4.3.

### Phase C — bigger architectural changes (2-3 weeks) — REVISED post-verification

22. **Sketch D with coalescing**, targeting BATCH-EVALUATOR FIRST then ICPC. Per
    Correction C, batch-evaluator is the actually-saturated pool. The polling
    pin elimination matters more there.
    - **Batch-evaluator**: replace `host.operations.get_next_operation_result`
      polling with host-driven callback-on-result. K=4 coalescing: 5 callbacks ×
      ~20ms each = **100ms slot-time per testcase**, vs 1-5s pin. **20-50×
      win.**
    - **ICPC**: replace `while next_result()` with
      `host.eval.run_batch(batch_id, on_K_results_handler)`. K=4 coalescing: 5
      callbacks × ~20ms each = **100ms slot-time per submission**, vs 30s pin.
      **300× win.**
    - Per-call WASM startup is real (3-15ms) but tiny vs the pin. Coalescing
      balances per-call overhead vs pool churn.
    - Preserves ICPC short-circuit semantics: server can fire immediate callback
      when it sees a failing result.
23. Move blocking hooks to async post-accept with a new `Rejected` terminal
    status (§2.1 alt). Changes API contract for sync hook rejections — needs
    operator/contestant communication.
24. Move dispatch consumer out of the api process and into the worker tier so
    the api becomes a pure HTTP front-end (§2.1 stretch). The api process never
    holds plugin pools again.

---

## 6. Open questions to answer in code before phase B

1. **ICPC's `judge_epoch` replay semantics.** Does the plugin's persist layer
   (`plugins/icpc/src/persist.rs`) accept a _new_ call with the same epoch?
   Reliability deep-dive flagged this — needed for at-least-once safety.
2. **`broccoli_queue` durability semantics.** Is Redis configured for AOF/RDB
   persistence? If not, MQ messages survive only as long as Redis is up. May
   need to flip to Postgres-backed durable queue, or to a more durable Redis
   config.
3. **`target_worker_id` setting at the evaluator level.** Who sets
   `OnSubmissionInput.target_worker_id` (`plugins/icpc/src/evaluate.rs:58`)? If
   always `None`, cross-worker compile redundancy is a real concern. If
   sometimes set, sandbox-reuse becomes free.
4. **Per-instance plugin memory baseline.** Profile3 ran 7.8 GB RSS with
   `pool_max_instances=256`. Expected by code reading: 256 × 16 MiB × 8 plugins
   ≈ 5–6 GiB. Confirm with a smaller pool (e.g. 64) whether memory drops
   proportionally.
5. **Operator-level concurrency knobs.** Settled split:
   `worker.max_concurrent_operations` per-worker (§2.4);
   `plugin.evaluator_parallelism` server-side (already exists, controls how many
   evaluator-plugin calls run concurrently in-process); new per-plugin-id
   dispatch semaphore (§2.3 a). Three knobs; documented separately.

---

## 7. What the next profiling run should answer

After Phase A + B ship, run-3 with frame-pointers-enabled binaries. Specific
questions:

- **Throughput floor:** sustained ops/s on 2-api + 2-worker cluster with no
  SystemError, no 429.
- **Per-testcase wallclock decomposition** (from the new metrics):
  %{binary_exec, isolate_init+cleanup, file_materialization, plugin_call,
  mq_overhead, db_writeback}.
- **Compile cache hit rate.** With `toolchain_fingerprint` fixed and
  cross-worker coordination in place: should be ≥95% steady-state for repeated
  submissions.
- **Cross-worker fairness:** variance of `task_process_duration` for testcases
  of the same submission across workers.
- **Submission queue depth under burst.** Burst phase should grow the MQ depth
  but not drop throughput; once burst ends, queue should drain in
  `queue_depth / steady_throughput` seconds.
- **5000-contestant projection:** with single-binary-per-worker and 167 ops/s
  steady target, required worker fleet =
  `167 ops/s × avg_testcases_per_op / per_worker_ops_per_sec`. Use the measured
  per-worker rate from run-3 to size production cluster.
