---
title: Judging
sidebar_label: Judging
sidebar_position: 1
---

# Judging

A submission resolves an evaluator by the problem's `problem_type` through the
evaluator registry (`dispatch.rs:15-20`), runs the prepared operations, and
produces a per test verdict plus score. The contest format plugin (ICPC, IOI)
aggregates those per test results into a submission verdict and a contest
score.

## Verdict and score are separate

A `TestCaseVerdict` carries a `verdict` field and a `score` field
(`evaluate.rs:516-527`), and neither substitutes for the other. For the batch
evaluator, the score is produced by the resolved checker through
`interpret_fused_result`: the checker's `interpret` call returns a score, and
that score lands on the verdict as is (`interpret.rs:22-143`). The no checker
path (`checker_format == "none"`, the custom "run code" case) scores a flat
`1.0` pass, with no comparison performed at all. For the batch evaluator, the
checker score is the only path partial credit reaches the aggregator.

The communication evaluator, used by interactive problems, computes its own
fractional score directly from the interactor's stdout, independent of
`interpret_fused_result` and the checker abstraction entirely. It parses the
manager's first stdout line as an `f64`, clamps it to `[0.0, 1.0]`, and writes
it straight onto `TestCaseVerdict.score`. A capped score below `1.0` still
yields `Verdict::WrongAnswer`, not a partial Accepted
(`communication-evaluator/src/interpret.rs:325-357`).

## Scoring is decoupled from task type

The evaluator is chosen by `problem_type` through the registry
(`dispatch.rs:15-20`), and that choice is orthogonal to the contest format.
The contest format plugin only decides how the per test verdicts and scores
it receives become a contest score: ICPC folds them into all or nothing, IOI
folds them into subtask totals. Interactive problems route to their own
evaluator (`communication`); output only problems reuse the batch evaluator
(`batch`) rather than getting one of their own. Either way, the evaluator
produces the same `TestCaseVerdict` shape, and ICPC or IOI aggregates it the
same way no matter which evaluator produced it.

## Contest format fidelity

| Area | ICPC | IOI |
| --- | --- | --- |
| Partial credit | none, all or nothing per test | yes, via subtasks and fractional checker scores |
| Subtasks | none | `group_min` (= CMS GroupMin), `sum` (= CMS Sum), `group_mul` (bespoke, no CMS equivalent) |
| Missing CMS feature | not applicable | no GroupThreshold |
| Cross submission scoring | earliest accepted submission wins, not the last | `max_submission`, `sum_best_subtask` (CMS IOI 2017 plus), `best_tokened_or_last` |
| Tiebreak | team name order, not last accepted time | default `max_score_time`, time based, not classic equal rank |
| Built in checker credit | binary | binary, fractional credit only via a testlib points checker |

## Where this can surprise you

- Sub minute ICPC solves round to zero penalty minutes.
- `show_test_details` in ICPC is advisory and unenforced.
- The IOI token model is a subset of the CMS one.
