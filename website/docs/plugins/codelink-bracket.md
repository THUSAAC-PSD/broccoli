---
title: Codelink bracket
sidebar_label: Codelink bracket
sidebar_position: 3
---

# Codelink bracket

The `codelink-bracket` contest type runs the Codelink afternoon round. Sixteen
players, usually the qualifiers from the
[Codelink qualifier](./codelink-qualifier.md), meet in a single elimination
bracket of four rounds. A match is three games, each won by whoever solves
their problem first, plus tiebreak games if the score is level.

## Install the plugin

Run these commands from the Broccoli repository after installing its
development dependencies.

```bash
pnpm --filter @broccoli/web-sdk build
just build-plugin plugins/codelink-bracket --install
```

The build writes `plugins/codelink-bracket/codelink_bracket.wasm` and the
frontend bundle under `plugins/codelink-bracket/frontend/dist`. The server
finds plugins in its plugins directory, `./plugins` by default. Restart the
server, or select **Reload All** on the admin **Plugins** page.

## Create the contest

1. In the contest editor, choose `codelink-bracket` as the contest type and
   set the start and end of the afternoon round. Leave enough time for four
   rounds, since no match can be played after the end.
2. Add every problem the bracket uses. Each round needs seven or more, as the
   next section explains.
3. Enroll the sixteen players.

## Configure the rules

Open the contest's **Rankings** page. Until the bracket exists, staff see the
setup screen there.

1. **Players and seeding.** The first sixteen enrolled players are listed in
   registration order. Drag them into seed order, or use **Shuffle**. Seed 1
   plays seed 2 in round 1, seed 3 plays seed 4, and so on. Remove a player to
   bring in someone from the other enrolled players.
2. **Problems per round.** Every round needs three problems for the first
   player of each match, three for the second, and at least one tiebreak
   problem. The slots start out filled in the contest's problem order. A
   problem can appear only once in the whole bracket.
3. **Timing.** Set the length of a game, the break each player gets between
   their own matches, and how long a game waits for a stuck judge before staff
   have to decide the match. The defaults are 30 minutes, 10 minutes, and 120
   seconds.

**Create bracket** checks all of this and lists anything missing. It also turns
on the plugin's `before_submission` check for the contest, which rejects
submissions to any problem a player is not currently playing. Seeding and
problems cannot be changed once the bracket exists.

To script the setup instead, send the same data to the setup route as a user
with `contest:manage`.

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  "$BROCCOLI/api/v1/p/codelink-bracket/api/plugins/codelink-bracket/contests/$CONTEST/setup" \
  -d '{
    "seeds": [11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26],
    "rounds": [
      { "group_a": [101, 102, 103], "group_b": [104, 105, 106], "tiebreak": [107, 108] },
      { "group_a": [111, 112, 113], "group_b": [114, 115, 116], "tiebreak": [117] },
      { "group_a": [121, 122, 123], "group_b": [124, 125, 126], "tiebreak": [127] },
      { "group_a": [131, 132, 133], "group_b": [134, 135, 136], "tiebreak": [137] }
    ],
    "xiaoju_seconds": 1800,
    "round_intermission_seconds": 600,
    "escalation_grace_seconds": 120
  }'
```

| Field | Effect |
| --- | --- |
| `seeds` | Sixteen distinct user ids in seed order. The first plays the second in round 1, the third plays the fourth, and so on. |
| `rounds` | Exactly four rounds. In every match of a round, the first player gets `group_a` and the second gets `group_b`. |
| `xiaoju_seconds` | Length of each game. Must be more than 0. |
| `round_intermission_seconds` | Break each player gets after their own match before their next one can start. |
| `escalation_grace_seconds` | How long a game waits for a stuck judge before staff must decide. Defaults to 120. |

A scripted setup does not turn on the `before_submission` check. Turn it on in
the contest's **Configure** dialog.

## How a match is won

1. **Ranking.** Each player drags the opponent's three problems into the order
   the opponent must solve them in. A ranking can be changed until the match
   starts.
2. **Start.** The match starts by itself once both players have ranked, the
   contest has started, and each player has had their break since their
   previous match. There is no ranking deadline, so a match waits until both
   rankings are in.
3. **Games.** In each of the three games, both players work on their next
   problem at the same time. The first accepted submission wins the game.
   Submission time decides, not the order in which the judge finishes, and the
   smaller submission id breaks an exact tie. If neither player solves their
   problem before the game ends, nobody scores. The next game opens as soon as
   one is decided.
4. **Result.** After three games the player with more wins advances. On a
   level score, including when nobody scored, both players get the round's
   first tiebreak problem. A scoreless tiebreak moves on to the next tiebreak
   problem.

The winner moves into the next round straight away. Matches do not wait for
the rest of their round, so a quick winner can start the next round while other
matches are still running. Every match in a round uses the same problems, so a
pair that starts later faces problems earlier pairs have already seen. Run the
round in a supervised room.

## During the contest

Players work from the **Contest Homepage**. It opens with their own match,
showing where they are, the ranking list when it is their turn to rank, and
their current problem with its clock once a game is live. A player sees only
the problems they have reached. The **Rankings** page shows them a short link
back to their match.

The **Rankings** page shows the whole bracket as a tree. Each card shows the
seeds, one dot per game, and the clock of the live game. A waiting match shows
when it will start. **Present** puts the bracket full screen for an audience
and hides the staff counts. Select a match to open its panel with the score,
each game's problems and winner, both players' submissions with links to their
judging, and the staff controls.

Staff can act from the match panel. Every action asks for confirmation first.

| Control | When it appears | What it does |
| --- | --- | --- |
| **Start now** | The match is waiting for rankings | Starts the match at once. A missing ranking becomes the listed order and any remaining break is skipped. |
| **Force expiry** | A game is live or waiting on judging | Checks the current game again right away, for example after a rejudge. It never ends a game before its time is up. |
| **Award to** | A game is live, waiting on judging, or the match needs a staff decision | Gives the match to that player, who moves into the next round. This cannot be undone. |

## When judging is slow or fails

A game is not decided while an earlier submission is still being judged, even
after its time is up. The match shows **Waiting on a pending submission**. When
the result arrives, the game is decided as usual.

If judging has not finished after `escalation_grace_seconds`, the match changes
to **Needs staff decision**, and the summary above the bracket counts it. The
match panel says why, links to the stuck submission, and can rejudge it. Check
the result, then use **Award to**. A match also needs a staff decision when its
tiebreak problems run out without a winner.

## After the contest

No match starts at or after the contest's end time, whether on its own or with
**Start now**. A game still running at the end cannot be won any more, because
nobody can submit. It is not scored as a draw. The match goes to **Needs staff
decision** and the panel says the contest ended first. A submission still being
judged at that moment is waited for, as it would be at any game's end.

The summary above the bracket warns staff before this happens, counting the
matches whose live game or planned start falls after the end. Once the end has
passed, it counts the matches still unfinished. Award each one to finish the
bracket.
