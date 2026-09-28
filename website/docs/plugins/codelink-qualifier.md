---
title: Codelink qualifier
sidebar_label: Codelink qualifier
sidebar_position: 2
---

# Codelink qualifier

The `codelink-qualifier` contest type runs the Codelink morning round.
Contestants pick any problems they like, and each problem gives scoring slots
to the first few contestants who solve it. Earn slots on enough different
problems and you qualify for the afternoon
[Codelink bracket](./codelink-bracket.md).

## Enable the plugins

The Codelink qualifier and bracket plugins ship with every release bundle but
stay off until you enable them. On each server node, run this from the bundle
directory, then select **Reload All** on the admin **Plugins** page.

```bash
./enable-plugin.sh codelink-qualifier codelink-bracket
```

To enable them while installing instead, set `BROCCOLI_PLUGINS` for the
installer. This works the same way for `install.sh` and `install-native.sh`.

```bash
BROCCOLI_PLUGINS="codelink-qualifier codelink-bracket" ./install.sh server
```

In a development checkout, build and install them from the repository.

```bash
pnpm --filter @broccoli/web-sdk build
just build-plugin plugins/codelink-qualifier --install
just build-plugin plugins/codelink-bracket --install
```

## Create the contest

1. In the contest editor, choose `codelink-qualifier` as the contest type and
   set the start and end of the morning round.
2. Add the problems. Each one needs test cases and its usual evaluator,
   checker, and language plugins.
3. Enroll every contestant. The standings list enrolled contestants only.

## Configure the rules

Open the contest's **Configure** dialog in the admin area and choose the
`codelink-qualifier` plugin's `contest` settings.

| Setting | Default | Effect |
| --- | --- | --- |
| **Expected Problem Count** (`expected_problem_count`) | 16 | Shows a notice when the contest has a different number of problems. 0 turns the check off. |
| **Scoring Slots per Problem** (`slots_per_problem`) | 2 | How many different contestants can earn a slot on each problem. |
| **Credited Problems to Qualify** (`solves_to_qualify`) | 2 | How many different problems a contestant needs a slot on to qualify. |
| **Scoreboard Refresh Interval** (`scoreboard_refresh_seconds`) | 5 | Seconds between automatic refreshes. 0 means manual refresh only. |

The defaults describe the usual morning round of 16 problems, two slots per
problem, and two credited problems to qualify. The two scoring limits take
whole numbers from 1 to 1000. The expected problem count takes 0 to 1000 and
only drives the setup notice, since the rules always count the problems the
contest actually has. The refresh interval takes 0 to 3600.

Set these before the round starts. A saved change takes effect on the next
refresh and recalculates the standings, and it needs no rebuild.

## How contestants qualify

Slots go to accepted submissions in the order they were submitted.

1. Accepted submissions are sorted by submission time. On an exact tie, the
   smaller submission id comes first. The order in which the judge happens to
   finish does not matter.
2. Only a contestant's first accepted submission on each problem counts.
3. That submission takes a free slot on the problem, unless the contestant has
   already qualified.
4. A contestant qualifies once they hold slots on the required number of
   problems. They keep those slots. Their later accepted submissions still show
   on the board but take no slots and do not change their qualification time.

An accepted submission that got no slot does not count toward qualifying.
Because qualified contestants are skipped, a slot they would have taken goes to
the next contestant instead. With the default settings this plays out as
follows.

| Event | Result |
| --- | --- |
| Alice earns slots on A and B | Alice qualifies and keeps both slots |
| Alice later solves C | Her AC shows, and both C slots stay free |
| Bob and Carol then solve C | Each takes one C slot |
| Dave also solves C | His AC shows without a slot |

Only submissions made during the morning round count. A submission made before
the end still counts if its judging finishes afterwards. Practice submissions
and submissions outside the round take no slots. A failed evaluation, or a
problem with no test cases, never produces an AC.

## During the contest

Contestants see the rules, with your configured numbers, on the **Contest
Homepage**. The **Rankings** page shows each problem's slots and who holds
them, then one row per contestant with their credited problems and status.

| Status | Meaning |
| --- | --- |
| **Qualified** | The contestant qualifies however the remaining judging turns out |
| **Awaiting judging** | Unfinished judging could still decide whether the contestant qualifies |

Everyone else has no status yet and can follow their progress in the
**Credited** column. The qualifier count and the qualification times include
**Qualified** contestants only.

The board refreshes at the configured interval, and keeps refreshing after the
round ends while queued judging finishes. **Auto refresh** pauses it and
**Refresh** updates it right away. With an interval of 0 the page shows
**Manual refresh only**.

## When judging is slow or fails

A contestant shows **Awaiting judging** while their own submission is still
being judged, or while an earlier submission by someone else could still take a
slot they are counting on. Submissions still waiting in the queue count as
unfinished too. Submissions that cannot change anything do not hold anyone
back. That covers attempts on a problem whose slots are certainly full, repeat
attempts on a problem already solved, and attempts by contestants who have
already qualified.

**Qualified** appears as soon as the result is certain, even while other
judging is still running. The slots and times shown next to it can still move
until that judging finishes.

Rejudges and rule changes recalculate the board once applied. A rejudge that
has not been applied yet does not change the official result.

## After the contest

Wait until no submission is still being judged and every rejudge has been
reviewed. The contestants marked **Qualified** are the qualifiers. The plugin
does not enroll them anywhere, so enroll them in the
[Codelink bracket](./codelink-bracket.md) contest yourself.

The score on an individual submission only says whether that solution passed.
Qualification comes from this board alone.
