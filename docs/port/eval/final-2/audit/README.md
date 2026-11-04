# W-17 step (3): blinded audit of the shipped `standard` preset

The audit of docs/port/WEAKNESSES.md W-17, step (3): auditors who did not generate the games
classify the implausible plays in blinded transcripts of a fixed sample, and the confirmed count
per game is reported with its interval.

## Sample

20 BO1 games per example pairing for each AI version, both seats the same version, dev-role
masters 22710601-22710603 (the pairings in the order of the 2026-10 audit):

- shipped strengthened `standard` (M15b, commit 8c89b5e binaries):
  `runs/search-standard/` (`gcg-sim benchmark ... --format bo1 --matches 20 --seed 2271060N
  --workers 2 --replays 20 --ai-preset standard`);
- reference `standard`: `docs/port/eval/GAP-A1/audit/runs/mcts-standard/` (the reference search
  is unchanged since, so those games are the same).

The rule-based candidates (`audit-report.json`): `gcg-eval weakness audit --runs <the six run
directories> --policy search:standard --worlds 4 --seed 22710700 --role dev --ledger
docs/port/eval/final-2/audit/seed-ledger.json --workers 2`.

## Blinding and packets

`gcg-eval transcripts OUT KEY --run runs/search-standard --run
docs/port/eval/GAP-A1/audit/runs/mcts-standard` wrote 120 transcripts without agent names,
versions or search statistics, with ids derived from the game alone; `key.json` maps the ids to
the replays. `prepare_audit.py OUT_TRANSCRIPTS key.json PACKETS` built five packets:

- three with full transcripts of a stratified sub-sample: per version and pairing, the
  lowest-numbered game the deck under test won and the lowest-numbered game it lost (12 games,
  four per auditor);
- two with excerpts (every decision of the flagged turn, naming the player to check, not the
  flag) of every other game the rule-based audit flagged (25 excerpts).

The packets were shuffled by SHA-256 of their file names. Each auditor was a fresh agent with
`INSTRUCTIONS.md` and its own packet only (no key, no assignment, no repository evaluation
files), allowed card text through `gcg_sim.tools.explain` and the Comprehensive Rules. Only
findings with the verdict `clear` (a competent player would agree, with the better alternative
named) count. `assignment.json` maps packets to games; `auditors/auditor-N.json` are the
classifications as written.

## Result

`uv run python docs/port/eval/final-2/audit/tally_audit.py` writes `audit-result.json`:

| | shipped `standard` (M15b) | reference `standard` |
| --- | --- | --- |
| confirmed implausible plays per fully read game (6 games) | 1.33 (SE 0.21) | 2.50 (SE 0.56) |
| fully read games with at least one confirmed play | 6/6 [0.610, 1.000] | 6/6 [0.610, 1.000] |
| by category (full reads) | passivity 5, bad target 2, other 1 | passivity 8, bad target 3, other 2, questionable attack 1, wasted effect 1 |
| rule-flagged turns confirmed | 3 of 10 | 8 of 16 |
| games with a confirmed missed lethal (60 games) | 0/60 [0.000, 0.060] | 2/60 [0.009, 0.114] |

The missed-lethal count covers the flagged turns of all 60 games and every turn of the fully
read ones; a missed lethal in an unflagged turn of an unread game is not counted.

The most frequent confirmed mistake of both versions is passivity: ending a main phase with
resources unused and an affordable unit in hand early in the game, or not attacking when the
attack is free. The shipped `standard` makes fewer of them than the reference, and no confirmed
missed lethal.

The frozen WK scenarios rescored with the M15b presets (`scenarios-report.json`, `gcg-eval
weakness scenarios --scenarios docs/port/eval/GAP-A1/weakness/scenarios.json --agents
ref:standard,search:fast,search:standard --seeds 16 --workers 3`): reference 266/352 (0.756),
shipped `fast` 336/352 (0.955), shipped `standard` 336/352 (0.955).

## Deviation from the plan

The plan's audit reads all 120 transcripts. They total about 10 MB, so the full reads cover a
stratified 12, and every rule-flagged turn of the 120 is checked. The sub-sample's intervals are
correspondingly wide.
