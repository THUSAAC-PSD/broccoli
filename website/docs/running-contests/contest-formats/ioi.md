---
title: IOI format
sidebar_label: IOI
sidebar_position: 2
---

# IOI format

This format scores each problem out of a total built from subtasks, gives
partial credit, and can offer tokens and graded feedback.

The settings below split into two scopes, contest level under
`[config.contest]`, set once per contest, and task level under
`[config.task]`, set per contest problem so the same problem can carry
different subtasks in different contests.

## Subtasks

A problem is divided into subtasks under `[config.task]`. Each subtask has a
name, a scoring method, a max score, and the test labels it covers.

| Key              | Default | Meaning                                    |
| ----------------- | ------- | ------------------------------------------- |
| `name`            | Subtask | label shown to contestants                  |
| `scoring_method`  | group_min | group_min, sum, or group_mul               |
| `max_score`       | 100     | points the subtask is worth in full          |
| `test_cases`      | empty   | labels of the test cases the subtask covers  |

A test case's label is the one set on the test case, or its numeric ID when
it has none.

The three scoring methods behave differently.

- `group_min`, the subtask scores its full max score only when every test in
  it passes, otherwise it scores zero.
- `sum`, the subtask gives partial credit in proportion to the tests that
  pass, weighted by each test's own point value.
- `group_mul`, the subtask multiplies the per test fractions together, so one
  weak test drags the whole subtask down.

A problem's total is the sum of its subtask scores. A problem with no
subtask config gets one `sum` subtask named All Tests, covering every test
case that is not a sample and is worth more than zero points, with a max
score equal to the sum of those points.

## Scoring across submissions

`scoring_mode` is a contest setting that picks how a contestant's score for a
problem is chosen across all of their submissions, and it defaults to
`max_submission`.

- `max_submission`, the best whole submission, the highest total score seen
  across every submission.
- `sum_best_subtask`, the best score seen for each subtask across all
  submissions, then summed.
- `best_tokened_or_last`, the classic token rule, the higher of the best
  tokened submission's score and the last submission's score.

## Feedback

`feedback_level` is a contest setting that decides how much of a submission's
result a contestant sees before spending a token, and it defaults to `full`.

| Value             | Reveals                                                              |
| ------------------ | ---------------------------------------------------------------------- |
| `full`             | the verdict, the total score, and every test case's own verdict, score, time, and memory |
| `subtask_scores`   | the verdict, the total score, and the score of each subtask, with individual test case results held back |
| `total_only`       | the verdict and the total score only                                 |
| `none`             | nothing about the result                                             |

:::note[Where the subtask breakdown shows up]

The extra detail at `subtask_scores` shows up in the task's subtask list, the
scoreboard's per problem scores, and the submission's subtask score endpoint.
The submission view itself hides per test case results at every level below
`full`.

:::

## Tokens

A token unlocks full feedback on one submission regardless of
`feedback_level`. A contestant spends a token on their own submission while
the contest is running, and a submission can only be tokened once.

`tokens.mode` picks the token model, and it defaults to `none`.

- `none`, tokens are off, feedback always follows `feedback_level`.
- `fixed_budget`, a contestant starts with a fixed number of tokens that
  never grows.
- `regenerating`, a contestant starts with a token budget that grows over
  time, up to a cap.

| Key                          | Default | Meaning                                 |
| ------------------------------ | ------- | ------------------------------------------ |
| `tokens.initial`               | 2       | tokens a contestant starts the contest with |
| `tokens.max`                   | 5       | cap for regenerating mode                   |
| `tokens.regen_interval_min`    | 30      | minutes between each regenerated token      |

`tokens.max` plays no part in `fixed_budget` mode, the budget stays at
`tokens.initial` for the whole contest.

## The scoreboard

`scoreboard_visibility` decides who sees the full board while the contest is
running, and it defaults to `admins_only`. Once the contest ends, every
contestant sees the full board no matter this setting, and contest managers
always see it.

| Value                 | Meaning                                                        |
| ----------------------- | ----------------------------------------------------------------- |
| `admins_only`           | only contest managers see the full board while the contest runs, everyone else sees only their own row |
| `all_contest_viewers`   | every contest viewer sees the full board while the contest runs |

Each problem carries a score time, the elapsed time from contest start
associated with reaching the contestant's score on that problem.
`scoreboard_tiebreaker` decides how tied total scores are ranked using those
per problem times, and it defaults to `max_score_time`.

| Value             | Meaning                                                           |
| ------------------ | -------------------------------------------------------------------- |
| `equal_rank`       | tied contestants share the same rank                                 |
| `sum_score_time`   | ties are broken by the sum of each problem's score time, faster total first |
| `max_score_time`   | ties are broken by the largest single problem's score time, faster first |

## Where this differs from CMS and a classic IOI contest

- `group_min` matches CMS GroupMin and `sum` matches CMS Sum, but
  `group_mul` has no CMS equivalent.
- There is no GroupThreshold method.
- The default tiebreak is time based, not the classic equal rank.
- The token model is a subset of the CMS one.
- Partial credit only reaches the score from a checker that returns a
  fraction, for example a testlib checker that reports points. The built in
  comparators are all or nothing. Problem authoring, including how to choose
  a checker, is its own page.
