# Authoring a special judge with testlib

Some problems accept **more than one correct output** — any valid factorisation,
any shortest path, any point on a line. An exact / `tokens` / `lines` checker
can't grade those, because it only knows how to compare against one fixed answer.
For these you write a **special judge**: a small C++ program that inspects the
input and the contestant's output and decides the verdict itself.

Broccoli runs special judges written against [**testlib**](https://github.com/MikeMirzayanov/testlib)
(Mike Mirzayanov's standard checker library, used by Codeforces/Polygon). Set the
problem's `checker_format = "testlib"` and upload your `checker.cpp` together with
`testlib.h`.

This directory is a complete, working example — a **"factor pair"** problem:

| File | Role |
|------|------|
| `checker.cpp`      | The special judge. Accepts any `a b` with `a,b >= 2` and `a*b == n`. |
| `solution.cpp`     | A correct solution → prints `17 23` for `n = 391` → **Accepted**. |
| `solution_alt.cpp` | An equally-correct solution → prints `23 17` (swapped) → **Accepted**. |
| `wrong.cpp`        | Prints `2 391` (`2*391 != 391`) → **WrongAnswer**. |
| `sample.in`        | One test case input: `391`. |
| `sample.ans`       | A jury answer: `17 23` (this checker recomputes, so it is only illustrative). |
| `fetch-testlib.sh` | Downloads the canonical `testlib.h` next to `checker.cpp`. |

`solution_alt.cpp` is the whole point: `23 17` would be **WrongAnswer** under an
exact/tokens checker, but the special judge **accepts** it. That non-uniqueness is
why a special judge is needed.

> **testlib.h is not vendored here** (it is third-party). Run `./fetch-testlib.sh`
> first, or drop your own `testlib.h` into this directory.

## How broccoli invokes the checker

testlib's `registerTestlibCmd` expects the checker to be called as:

```
checker <input-file> <output-file> <answer-file>
```

Broccoli compiles `checker.cpp` once in a dedicated checker sandbox and runs it
per test case with exactly those three files:

| testlib stream | File | Contents |
|----------------|------|----------|
| `inf` | `<input-file>`  | the test-case `input` you uploaded |
| `ouf` | `<output-file>` | the **contestant's** stdout |
| `ans` | `<answer-file>` | the test-case `expected_output` (the jury answer) |

Read the input with `inf.read*`, the contestant output with `ouf.read*`, and (if
you need it) the jury answer with `ans.read*`. This checker ignores `ans` because
it recomputes correctness from `n`.

### Verdict = exit code

You end a testlib checker with `quitf(verdict, fmt, ...)`. Broccoli maps the exit
code to a submission verdict:

| `quitf` call | exit | Broccoli verdict |
|--------------|------|------------------|
| `quitf(_ok, …)`     | 0 | **Accepted** |
| `quitf(_wa, …)`     | 1 | **WrongAnswer** |
| `quitf(_pe, …)`     | 2 | **WrongAnswer** (message prefixed *Presentation error*) |
| `quitf(_fail, …)`   | 3 | **SystemError** — a *judge bug*, not a contestant failure |
| `quitf(_points, s, …)` | 7 | **Accepted** if `s >= 1.0`, else **WrongAnswer** (partial score `s`) |
| anything else       | * | **SystemError** |

**Reserve `_fail` (and any non-standard exit) for genuine setup errors** — a
corrupt input file, an impossible jury answer. They surface as `SystemError`,
which on a contest means "the problem is broken", not "the contestant is wrong".
Everything that is the contestant's fault must be `_wa` / `_pe` / `_points`.

### Gotcha: use `seekEof()`, not `readEof()`

`checker.cpp` finishes the output with:

```cpp
if (!ouf.seekEof()) quitf(_pe, "extra tokens after a b");
```

`ouf.readEof()` is **strict**: the trailing `\n` that virtually every solution
prints counts as leftover input, so `readEof()` would mark a *correct* answer as a
presentation error. `seekEof()` skips trailing whitespace first — it tolerates the
newline but still rejects real extra tokens (e.g. `17 23 99`). Verified on this
box: `17 23\n` → AC, `17 23 99` → PE, `2 391` → WA.

Prefer testlib's bounded readers (`ouf.readLong(2, n, "a")`) — they auto-produce a
clean `_pe`/`_wa` on non-integer or out-of-range tokens, so you never read garbage.

## Authoring via the API

With `BASE=https://<host>/api/v1` and a `$TOKEN` holding `problem:edit`:

```bash
./fetch-testlib.sh   # puts testlib.h next to checker.cpp

# 1) Create the problem with checker_format = testlib
PID=$(curl -s -X POST "$BASE/problems" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{
    "title":"Factor Pair","content":"Print a b with a,b>=2 and a*b=n.",
    "time_limit":2000,"memory_limit":262144,"problem_type":"batch",
    "checker_format":"testlib","default_contest_type":"icpc",
    "submission_format":{"cpp":["solution.cpp"]}
  }' | jq -r .id)

# 2) Upload the checker source: checker.cpp + testlib.h together (PUT)
curl -s -X PUT "$BASE/problems/$PID/checker-source" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d "$(jq -n \
    --rawfile c checker.cpp --rawfile t testlib.h \
    '{files:[{filename:"checker.cpp",content:$c},{filename:"testlib.h",content:$t}]}')"

# 3) Add a test case (input + jury answer)
curl -s -X POST "$BASE/problems/$PID/test-cases" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"input":"391\n","expected_output":"17 23\n","score":100,"is_sample":false}'

# 4) Submit a solution (normal flow)
curl -s -X POST "$BASE/problems/$PID/submissions" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"language":"cpp","files":[{"filename":"solution.cpp","content":"...source..."}]}'
```

The same steps exist in the web UI: create the problem, open its **Checker
Source** panel, paste `checker.cpp` + `testlib.h`, then add test cases.

## Optional: checker resource limits

The checker's own compile/run limits live in the global `standard-checkers`
plugin config (namespace `testlib`), independent of the problem's `time_limit`:

| Field | Default |
|-------|---------|
| `compile_time_limit_s` | `10.0` |
| `compile_memory_limit_kb` | `524288` (512 MB) |
| `run_time_limit_s` | `5.0` |
| `run_memory_limit_kb` | `262144` (256 MB) |

```bash
curl -s -X PUT "$BASE/plugins/standard-checkers/config/testlib" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{
    "config":{"compile_time_limit_s":10.0,"run_time_limit_s":5.0,
              "compile_memory_limit_kb":524288,"run_memory_limit_kb":262144},
    "enabled":true,"position":0}'
```

Bump these only if a heavy checker needs it — a checker that itself TLEs/MLEs exits
with an unexpected code and becomes a `SystemError`.

## Notes for problem setters
- Decide what counts as correct and grade exactly that. Don't compare against one
  fixed answer when several are valid.
- Keep the checker fast and deterministic; it runs once per test case.
- Use `_ok` / `_wa` / `_pe` / `_points` for contestant outcomes; keep `_fail` for
  real jury/setup bugs only.
- Read with bounded readers and finish with `seekEof()` — lenient on trailing
  whitespace, strict on extra tokens.
