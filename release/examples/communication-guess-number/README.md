# Authoring an interactive (communication) problem

A **communication** problem runs the contestant's program against a problem-author
**manager** (a.k.a. interactor). The two talk over pipes; the manager decides the
verdict. Broccoli evaluates these with the `communication-evaluator` plugin
(`problem_type = "communication"`).

This directory is a complete, working example — a "guess the number" problem:

| File | Role |
|------|------|
| `manager.cpp`  | The interactor. Knows the secret, answers `HIGHER`/`LOWER`/`CORRECT`, scores the run. |
| `solution.cpp` | A correct contestant solution (binary search) → **Accepted**. |
| `wrong.cpp`    | An incorrect solution (always guesses the low bound) → **WrongAnswer**. |
| `sample.in`    | One test case: `lo hi secret` = `1 1000000 42`. |

## How the runtime wires things up

For `num_processes = 1` and `communication_mode = "redirect"` (the defaults), the
evaluator creates two FIFOs and starts two processes:

```
                 test-case input (stdin)
                          │
                          ▼
   ┌─────────────────────────────────┐
   │            manager              │   stdout ──► FIRST LINE = score (double)
   │  argv[1] = write-to-contestant  │   stderr ──► verdict message
   │  argv[2] = read-from-contestant │
   └───────┬──────────────────▲──────┘
           │ m_to_c0          │ c0_to_m
           ▼                  │
   ┌─────────────────────────────────┐
   │           contestant            │
   │  stdin  ◄── m_to_c0 (from mgr)   │
   │  stdout ──► c0_to_m (to manager) │
   │  argv[1] = process index (0)     │
   └─────────────────────────────────┘
```

### Manager contract
- **stdin** = the test-case input (the per-test-case `input` you upload).
- **argv[1]** = path of the FIFO to **write** to the contestant.
- **argv[2]** = path of the FIFO to **read** from the contestant.
  (For `num_processes = N`: contestant `i` uses `argv[1+2*i]` / `argv[2+2*i]`.)
- **stdout** — the **first line must be the score** as a `double`. It is clamped
  to `[0,1]`; `>= 1.0` ⇒ **Accepted**, otherwise **WrongAnswer**. Write nothing
  else to stdout before the score.
- **stderr** — free-form; shown to the author as the verdict message.
- **exit code must be 0.** A non-zero manager exit ⇒ **SystemError** (treated as a
  problem-setup bug, not a contestant failure).

### Contestant contract (`redirect` mode)
- **stdin** reads what the manager writes; **stdout** is read by the manager.
- **Flush after every write** (`fflush(stdout)` / `cout.flush()` / `sys.stdout.flush()`),
  or both sides deadlock.
- **argv[1]** is the 0-based process index (only meaningful when `num_processes > 1`).
- A contestant that crashes, TLEs, MLEs, or exits non-zero yields the corresponding
  verdict. Receiving `SIGPIPE` (manager closed the pipe first) is **not** counted
  against the contestant — the manager's score wins.

`communication_mode = "fifo_args"` is the alternative: the contestant's stdin/stdout
are **not** redirected; instead it receives the FIFO paths as
`argv[1]` (read), `argv[2]` (write), `argv[3]` (index) and opens them itself.

## communication config

Stored per-problem under plugin `communication-evaluator`, namespace `communication`:

| Field | Default | Meaning |
|-------|---------|---------|
| `num_processes` | `1` | Number of contestant processes (`>= 1`, `<= max_processes`). |
| `communication_mode` | `"redirect"` | `"redirect"` or `"fifo_args"` (see above). |
| `manager_language` | `"cpp"` | Language id of the manager sources (`c`, `cpp`, `python3`, …). |
| `manager_sources` | `[]` | `[{ "filename", "hash" }]` — uploaded manager blobs. **Required.** |
| `manager_time_limit_s` | `30.0` | Manager wall-clock limit. |
| `manager_memory_limit_kb` | `524288` | Manager memory limit. |

## Authoring via the API

Assuming `BASE=https://<host>/api/v1` and a `$TOKEN` with `problem:create`/`problem:edit`:

```bash
# 1) Upload the manager source -> get its content hash
HASH=$(curl -s -X POST "$BASE/config/upload" -H "Authorization: Bearer $TOKEN" \
  -F "file=@manager.cpp" | jq -r .content_hash)

# 2) Create the problem (problem_type = communication)
PID=$(curl -s -X POST "$BASE/problems" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{
    "title":"Guess the Number","content":"Interactive.","time_limit":2000,
    "memory_limit":262144,"problem_type":"communication","checker_format":"none",
    "default_contest_type":"icpc","submission_format":{"cpp":["solution.cpp"]}
  }' | jq -r .id)

# 3) Set the communication config (point it at the uploaded manager)
curl -s -X PUT "$BASE/problems/$PID/config/communication-evaluator/communication" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{
    "config":{
      "num_processes":1,"communication_mode":"redirect","manager_language":"cpp",
      "manager_sources":[{"filename":"manager.cpp","hash":"'"$HASH"'"}],
      "manager_time_limit_s":30.0,"manager_memory_limit_kb":524288
    },"enabled":true,"position":0
  }'

# 4) Add a test case (input = "lo hi secret")
curl -s -X POST "$BASE/problems/$PID/test-cases" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"input":"1 1000000 42\n","expected_output":"","score":100,"is_sample":false}'

# 5) Submit a solution (normal submission flow)
curl -s -X POST "$BASE/problems/$PID/submissions" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"language":"cpp","files":[{"filename":"solution.cpp","content":"...source..."}]}'
```

`expected_output` is unused for communication problems (the manager decides the
verdict) — leave it empty. The same steps are available in the web UI: create the
problem, open its **Config → communication-evaluator** panel to set the manager and
language, then add test cases.

## Notes for problem setters
- Always `fflush` after writing on **both** sides; unflushed buffers are the #1 cause
  of interactive deadlocks/TLEs.
- Decide your protocol up front and document it for contestants (what the manager
  sends first, message formats, query limits).
- Use `stderr` from the manager for diagnostics — it reaches the verdict message.
- A manager that always prints a finite score in `[0,1]` and exits `0` keeps every
  outcome a contestant verdict; reserve non-zero exit / NaN score for genuine
  setup errors (they become `SystemError`).
