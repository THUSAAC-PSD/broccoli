---
title: ICPC format
sidebar_label: ICPC
sidebar_position: 1
---

# ICPC format

This format ranks teams by problems solved, breaking ties by time penalty, and
it judges every test as all or nothing.

## Settings

All five settings below are set per contest.

| Key                   | Default | Meaning                                                              |
| ---------------------- | ------- | --------------------------------------------------------------------- |
| `penalty_minutes`      | 20      | minutes added per rejected attempt made before the accepted one       |
| `count_compile_error`  | false   | whether a compile error counts as a rejected attempt                  |
| `show_test_details`    | false   | advisory only, not enforced by the plugin                             |
| `public_standings`     | false   | whether contestants see every team's rows during the contest, instead of only their own row until it ends |
| `freeze_minutes`       | 0       | minutes before the end when the standings stop updating publicly     |

## How submissions are judged

A submission is Accepted only when every test passes. Judging stops at the
first failing test, so a submission that fails an early test never runs the
tests after it.

A problem with no tests is a System Error, not a free solve. A misconfigured
problem can never be credited as solved for every team by accident.

## How the standings rank

A problem counts as solved once a submission earns an Accepted verdict. Its
penalty in minutes is `floor(solve_time_ms / 60000) + attempts_before_solve * penalty_minutes`, and it is zero for an unsolved problem.

Teams rank by problems solved, high to low, then by penalty, low to high. The
team that solves a problem first is highlighted on the board.

## Freezing the standings

With `freeze_minutes` set, the standings a contestant sees stop updating that
many minutes before the contest ends. A submission made during that window
shows as pending rather than its real verdict, until the board is revealed.
Organizers always see the real board, frozen or not.

Freezing only matters once `public_standings` is on, since a contestant with
public standings off already sees only their own row, which never freezes.

:::note[Reveal after the freeze]

Once the freeze window opens, an organizer can reveal the board from the
standings page. Revealing plays every frozen submission back to its real
verdict and cannot be undone.

:::

## Where this differs from a classic ICPC contest

- The final tiebreak is team name order, not the time of the last accepted
  submission.
- `show_test_details` is advisory and the plugin does not enforce it.
- A solve under one minute counts as zero penalty minutes for time.
- There are no balloons, and there are no subtasks or partial credit.
