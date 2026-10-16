# AI improvement proposals

A study of gcg-sim's MCTS player and of the benchmark statistics, written while the `fast` and
`expert` presets were calibrated (October 2026). It proposes changes; none of them is
implemented here. Six analysis lenses (search, playout policy and evaluation, pre-game and
special decisions, engine speed, benchmark statistics, play review) and two rounds of
follow-up studies chosen by a completeness critic (AI-level bias, evaluation quality,
strategy fusion, rollout horizon; seed dispersion, `fast` decision quality and speed,
decklist knowledge) produced the findings. Two independent skeptics re-checked each finding
of the six lenses and of the second follow-up round, and each first-round finding that has a
skeptic verdict says so (the first round's evaluation and strategy-fusion findings were not
re-checked; they are listed separately). Code references are to commit 6be0ea8; the engine,
search, policy and runner files cited are unchanged since 5c90b83, where the experiments ran.
Evidence paths point into `build/ai-presets/phase1/`, which is local and untracked; each
folder holds the scripts and a `commands.txt` with the exact commands.

Labels: **MEASURED** means a command was run and its output is quoted. **HYPOTHESIZED** means
it was not measured. Scores are the skeptics' averaged impact × confidence / effort, on 1–5
scales. Merged items take the score of their recommended variant.

## Summary

- **One correctness bug.** `determinize` writes sampled card identities into the true game's
  history, through `HistoryEvent` objects that `clone()` shares. Play, winners and hidden
  information are unaffected. But `final_state_sha256` then depends on the agents' private
  samples, so `gcg-sim replay` rejects genuine games: 5.75% of `standard` and 3.5% of `fast`
  benchmark games, and 4 of the 36 replay files the benchmarks in this study saved. Fix it
  first. The fix is a few lines, and digest-based identity checks of speed work depend on it.
- **Speed without changing play.** Four engine and search changes leave every game
  bit-identical. Together they cut CPU per game by about 16–19% at `standard`. At `fast` they
  give about +18–25% games per core-minute. The listened-events fix alone is worth 5–7%.
  Separately, `fast` spends 11% of each game on the play/draw simulation, and the answer
  never changed on any deck pair tested. Setting `first_player_games=8` gives +8% at `fast`
  and keeps the games identical whenever the answer is the same.
- **Statistics.** Better analysis buys more precision per CPU-second than any search change
  measured here:
  - analyse same-seed variant runs as pairs (about 20–30% fewer games);
  - replace the undefined "intervals overlap heavily" rule with an explicit test;
  - remove the play/draw "tuning hypothesis", which reports a property of the matchup as a
    property of the deck and fires in most reports of the recommended size.
- **Decision quality.** The policy fixes are real at the policy level: kill-aware damage
  targeting, Bursts always accepted in rollouts, the polarity table, and 【Attack】 boosts in
  attack scoring. Their effect on MCTS strength and matchup bias is unmeasured and probably
  under 1–2 points. Search allocation and knob changes at `standard` showed no measurable
  strength gain. Batch the policy fixes behind one re-calibration of all presets.
- **More search buys little above `strong`.** In the `expert` calibration
  (`docs/AI.md`, "Calibrating the presets"), twice `strong`'s budget won 605/1200 = 0.504
  [0.476, 0.532] against `strong` on held-out decks, and four times its budget won 49/96 on
  the tuning decks. More strength has to come from the policy and evaluation (items 11, 16,
  25, 26 and B–E), whose effect on MCTS play is unmeasured. Whether `standard`'s matchup
  estimates hold under stronger play is also unmeasured (item A).

## Ranked proposals

| Rank | Id (merged) | Proposal | Category | Score | Status |
| ---: | --- | --- | --- | ---: | --- |
| 1 | CB-1 (search-1, engine-speed-2, seed-dispersion-2) | Stop `determinize` writing into the true game's history | correctness-bug | 10.0 | holds |
| 2 | stats-3 | Paired comparison of same-seed deck-variant runs | variance-reduction | 12.0 | downgraded |
| 3 | engine-speed-1 (fast-speed-1) | Re-enable the listened-events fast path; land the bit-identical engine bundle | engine-speed | 10.0 | holds |
| 4 | stats-2 | Remove the play/draw tuning hypothesis | benchmark-design | 10.0 | holds |
| 5 | pregame-1 (fast-speed-2, -3, -4) | Cut the play/draw simulation: `fast` at 8 pairs | search-efficiency | 9.0 | holds |
| 6 | engine-speed-7 | Cross-build play-identity harness and paranoid derived-cache mode | benchmark-design | 9.0 | downgraded |
| 7 | stats-1 | Explicit difference test in the benchmark skill | benchmark-design | 9.0 | downgraded |
| 8 | engine-speed-3 | Rules management: printed-lethal pre-check and unchanged-state memo | engine-speed | 8.0 | holds |
| 9 | pregame-3 | Information-set tests for pre-game and special decisions | benchmark-design | 8.0 | downgraded |
| 10 | seed-dispersion-1 | No over-dispersion caveat on Wilson intervals | benchmark-design | 8.0 | downgraded |
| 11 | policy-1 (fast-quality-2) | Kill-aware damage targeting and always-yes Burst in the policy | decision-quality | 6.7 | downgraded |
| 12 | engine-speed-5 | Memoize `link_satisfied`; alias `view.reg` | engine-speed | 6.0 | holds |
| 13 | engine-speed-4 | Hoist unknown pools; cache each search world once | engine-speed | 6.0 | downgraded |
| 14 | pregame-2 | Document the mulligan measurement; no simulation mulligan | decision-quality | 6.0 | downgraded |
| 15 | fast-quality-1 (play-review-2) | `prior_weight`: none at standard; optional 0.125 for `fast` | decision-quality | 5.3 | downgraded |
| 16 | policy-4 | Fix targeting polarity for `cant_*` grants and start-phase locks | decision-quality | 4.5 | downgraded |
| 17 | fast-quality-5 | Decision-log audit: ship a replay audit script | benchmark-design | 4.5 | downgraded |
| 18 | stats-4 | Report a derived BO3 match rate | benchmark-design | 4.0 | downgraded |
| 19 | stats-5 | Group-sequential stopping for paired variant comparisons | benchmark-design | 4.0 | downgraded |
| 20 | stats-6 | Per-card "drawn" hypotheses: exclude `shield_to_hand` | benchmark-design | 4.0 | downgraded |
| 21 | search-3 | Opt-in paired early-stop rule | engine-speed | 4.0 | downgraded |
| 22 | policy-3 | Policy-duel harness as a sanity screen | benchmark-design | 4.0 | downgraded |
| 23 | dk-2 | Decision-level screen with a placebo arm | benchmark-design | 4.0 | downgraded |
| 24 | search-6 | Exploration at standard: no change now | decision-quality | 4.0 | downgraded |
| 25 | play-review-4 | 【Attack】 self-boosts in MAIN-phase attack scoring | decision-quality | 4.0 | downgraded |
| 26 | play-review-1 | Self-damage targeting for the purple damaged-payoff cards | decision-quality | 3.5 | downgraded |
| 27 | stats-7 | Align variant deals with the baseline (opt-in, inside `compare`) | variance-reduction | 3.2 | downgraded |
| 28 | dk-1 | Decklist knowledge: record the null; flag only if needed | benchmark-design | 3.2 | downgraded |
| 29 | fast-quality-3 | Add `fast` to the puzzle presets plus one discriminating puzzle | benchmark-design | 3.0 | downgraded |
| 30 | fast-quality-4 | Record the negative results for wide decisions at `fast` | decision-quality | 3.0 | downgraded |
| 31 | policy-5 | Rollout temperature 0.3 at standard: no change | variance-reduction | 3.0 | downgraded |
| 32 | engine-speed-6 | Invalidate the derived view by mutation kind | engine-speed | 2.3 | downgraded |
| 33 | search-4 | Keyed rollout noise: do not pursue | variance-reduction | 2.0 | downgraded |
| 34 | pregame-4 (play-review-3) | Indifference and proven-outcome stops: do not pursue now | search-efficiency | 2.0 | downgraded |

The correctness bug is ranked first regardless of score. Ties are broken by confidence, then
impact.

### 1. CB-1: `determinize` writes into the true game's history (correctness bug)

- **Problem.** `GameState.clone()` copies the history list but shares its event objects
  (`engine/state.py:556`). `HistoryEvent` is a mutable slots dataclass (`state.py:414-423`).
  `determinize` (`engine/observe.py:78-80`) and `permute_hidden` (`observe.py:116-118`)
  assign `h.def_id` in place. So every look-ahead (greedy, MCTS, the no-effect check, the
  play/draw simulation) overwrites the true game's per-turn history with the searcher's
  sampled identities for cards hidden from it. The runner's guard checks only `st.version`
  and the identity of `st.pending` (`runner/game.py:92`), so it cannot see this. Nothing
  reads `HistoryEvent.def_id` (`view.py:888`, `wp_gd04_b.py:56`, `wp_gd05_b.py:118` and
  `runner/instrument.py:84` read other fields), so actions, winners and card statistics are
  unchanged and no hidden information leaks. But history is serialized into
  `final_state_sha256` (`runner/game.py:162`). `check_replay` (`runner/replayfile.py:185-195`)
  and `gcg-sim replay` then exit 1 on a genuine game. README promises the saved replays are
  replayable.
- **Evidence (MEASURED).**
  - Agent-free replays of the calibration's benchmark records fail the digest check in 69/1200
    `standard` games (5.75%) and 42/1200 `fast` games (3.5%). Winner, end reason and turns
    match in all of them. Changing one history `def_id` reproduces every recorded digest
    exactly (111/111). The changed events are 79 draw-step `draw` and 32 `shield_to_hand`.
  - Greedy runs fail in 33/2118 games (1.6%). Smaller tuning-budget MCTS runs showed 0/90:
    the rate depends on budget and decks.
  - 4 of the 36 replay files saved by benchmarks in this study fail `gcg-sim replay`, for
    example `build/ai-presets/phase2/fast_confirm/fid-purple-white-tekkadan-vs-red-green-zeon-standard/replays/loss-m0005-g1.json`.
  - With the fix applied in-process, greedy failures go from 1/36 to 0/36 and the
    action-stream hash is unchanged.
  - The existing guard test `tests/runner/test_instrument.py:124`
    (`test_search_copies_do_not_leak_into_the_records`) misses the bug: 0 failures in 60
    matches, 1 in 120. Asserting that `state_digest(st)` is unchanged around every `choose`
    call fails on 22 of 388 calls in that test's own 4 matches, in 1.7 CPU-s.
  - Evidence: `verify/search-1-evidence/`, `verify/seed-dispersion-2/`,
    `verify/engine-speed-2-skeptic-evidence/`.
- **Change.** In `determinize` and `permute_hidden`, build a new event wherever `def_id`
  changes instead of assigning in place. Copy-on-write costs about 0.1 µs; a determinize
  costs 65–92 µs. Keep `clone()` shallow. Freezing `HistoryEvent` is optional: construction
  goes from 72 ns to about 410–490 ns, roughly 0.5% of a `standard` decision. Add three tests:
  - a unit test that `state_digest` is unchanged by `determinize` and `permute_hidden` for
    both observers, on positions with a hidden `draw` and a hidden `shield_to_hand`;
  - a per-call digest assertion in `test_search_copies_do_not_leak_into_the_records`;
  - a check that `MctsAgent.choose` leaves the digest unchanged on the cached mid-game states.
  Do not rely on a seed-pinned failing greedy match: any engine change moves it.
- **Expected impact.** Recorded digests become a pure function of seed and actions, and
  replay verification passes for every game. No change to decisions, win rates or speed.
  This is a prerequisite for digest-based identity checks of engine-speed work: the world
  cache (item 13) changes how often `determinize` runs.
- **Effort.** S.
- **Determinism and reproducibility risk.** None for play. Digests change only for the
  affected games, and become their true values. Goldens hold compiled card behaviour and are
  unaffected.
- **Validation.** The new tests fail before the fix and pass after it. On a fresh 400-game
  `standard` run, the agent-free audit (`verify/search-1-evidence/replay_saved_runs.py`)
  should find 0 mismatches, with actions and winners identical to the pre-fix run.
- **Side note.** `tests/ai/test_information_set.py` never covered history: 0 of its 25 states
  contain a history entry hidden from the decider. See item 9.

### 2. stats-3: paired comparison of same-seed variant runs

- **Problem.** Game and agent seeds depend only on (master, match, game, seat)
  (`runner/config.py:76-81`). The deck under test sits in seat `m % 2`. `new_game` draws a
  number of random values that does not depend on deck contents. So a variant run with the
  baseline's `--seed` shares the benchmark deck's library and the die roll in 1000/1000
  deals, and almost always who goes first. The skill still compares unpaired intervals, and
  `scripts/ai_calibrate.py paired` refuses runs whose decks differ (`:227-229`).
- **Evidence (MEASURED).** Outcome correlation (phi) between paired matches:
  - greedy, one-card swap: 0.38 (250 pairs);
  - MCTS one-card swaps, pooled: 0.27 [0.13, 0.39] (micro budget n=60 and n=70, `fast`
    n=80);
  - entirely different decks under test at `standard`: 0.14 [0.05, 0.24] (400 pairs), which
    is a floor;
  - the same deck, `fast` vs `standard`: 0.42–0.48.
- **Change.** Add a `compare` command, or relax `ai_calibrate.py paired`:
  - require the same master seed, benchmark deck, preset, format and caps; allow the deck
    under test to differ; pair over the common prefix of match indices;
  - report Δ, a paired 95% interval, an exact McNemar p, phi and n;
  - for BO3, pair per match on the match result, or require BO1;
  - in the skill, add a paired sample-size table, n = (z_α+z_β)²·2p(1−p)(1−ρ)/δ². The skill
    already says to reuse the seed. Keep the fresh-seed confirmation run.
- **Expected impact.** About 20–30% fewer games for the same power: about 287 pairs instead of
  393 for a 10-point difference at ρ ≈ 0.27. The paired analysis is never materially worse
  than the unpaired one on the same data. Point estimates are unchanged.
- **Effort.** S–M.
- **Risk.** None: it only reads output files.
- **Validation.** Cheap greedy A/A replicates on disjoint seeds: the 95% interval should
  cover 0 about 95% of the time. Optionally, measure ρ at `fast` on a one-card swap with
  256 pairs (about 30 core-minutes).

### 3. engine-speed-1: listened-events fast path, and the bit-identical engine bundle

- **Problem.** `core.emit` returns early only when no card in the game listens for the event
  (`engine/core.py:327`). `_index_events` adds all 33 events to a registry-wide `always` set
  whenever any card has an `AbilityGrant` node (`effects/registry.py:384-385`). Four cards
  in the pool have one (GD04-115, GD05-002, GD05-104, ST12-014), so every game listens for
  everything.
- **Evidence (MEASURED).**
  - In the profiled game, 228,056 emits, of which only 9.5% have a candidate trigger host,
    and 62% are for events no decklist card listens to.
  - All five `AbilityGrant` nodes grant their own card's ability, so those events are already
    in each card's own entry. With the fix, `always` holds 2 events and each game listens to
    3–8.
  - CPU over 8 paired games: −3.7% to −8.8%, typically −5% to −7%. `_compute` calls fall by
    6–14%. Fingerprints (decision trace, every alternative's value and visits, final state)
    are identical.
  - Soundness: 60 games stuffed with granter cards give identical traces. 3231 card, rule
    and effect tests pass. In a negative control where the granters listen to nothing, 8
    GD04-115/GD05-002 tests fail, so the card tests catch a dropped granted trigger.
  - Evidence: `verify/engine-speed-1/`, `verify/engine-speed-1-impact/`.
- **Change.** Drop the catch-all `always.update(d.Ev)` for `AbilityGrant`, and add the granted
  card's trigger events to the granting card's entry (a superset, for future cross-card
  grants). Make `hp_reduction_possible` per game in the same way: it is also registry-wide
  (`registry.py:386-387`), so one future HP-lowering card would silently disable item 8's
  pre-check.
- **Bundle (fast-speed-1 merged).** The prototype bundle `p_all` combines this item, the
  rules-management pre-check (item 8), the world cache (item 13), the micro-optimizations
  (item 12), a host-level emit filter (P2) and a history-free view key (P4). Measured:
  - `standard`: −16% to −19% CPU per game (single-seed pairs);
  - `fast`, tuning decks: 0.82–0.85 of baseline CPU per game, +18% to +22% games per
    core-minute; the ratio depends on machine load;
  - `fast`, 9 example-deck games: 0.80, +25%;
  - identity: 30/30 `fast` games (traces, full decision logs, final digests), 9/9 on example
    decks, 4 `standard` games, and the earlier 186 games (of which only 6 were MCTS).
  - Most of the gain comes from the emit, view and rules items; the world cache adds only a
    few points. P2 and P4 gave nothing on their own: keep them only if they measure positive
    inside the bundle.
- **Expected impact.** This item alone: about 1.06× games per core-minute at every preset.
  The bundle: about 1.2×, which narrows Wilson intervals by about 9% at fixed CPU. No change
  to strength or bias.
- **Effort.** S for this item; the bundle is M (247-line prototype diff in four files,
  `engine-speed/proto/p_all2_vs_wt.diff`).
- **Risk.** Soundness only: a dropped trigger would be deterministic but wrong. The card tests
  and item 6 cover it.
- **Validation.** The full gate (pytest, `fuzz --games 500`). Item 6's A/B identity harness on
  the example decks and on decks containing the granters. Speed from the deterministic
  `_compute`/emit counters plus one idle-machine `gcg_sim.ai.throughput` run by the lead.

### 4. stats-2: remove the play/draw tuning hypothesis

- **Problem.** `reports/hypotheses.py:135-148` compares the deck under test's on-play and
  on-draw win rates with a test against 0. It concludes "it is built for going first". Leaving
  draws aside, that gap equals P(P1 wins | DUT is P1) + P(P1 wins | bench is P1) − 1, and the
  benchmark deck's own gap is exactly the same number. It measures the matchup's
  first-player advantage, not the deck. `.claude/skills/gcg-benchmark/SKILL.md:61` repeats
  the claim ("Large play/draw gaps point to tempo problems").
- **Evidence (MEASURED).**
  - The two decks' gaps are equal in every BO1 run of this study checked (5–7 runs).
  - Exact firing probability of the shipped test with an even matchup:

    | Player One win rate | 100 games | 200 games | 250 games | 400 games |
    | ---: | ---: | ---: | ---: | ---: |
    | 0.606 | 0.58 | 0.86 | 0.92 | 0.99 |
    | 0.58 | 0.37 | 0.63 | 0.72 | 0.89 |

    At 0.55 it fires about half the time at any n.
  - It fired in 4 of the 5 BO1 runs of this study; 3 of those were correlated arms on the same
    deals.
- **Change.** Remove the `play_draw_split` kind, or reword it as descriptive ("Player One won
  X% [CI] of games; the gap is shared by both decks"). Keep the enum value in
  `reports/schema/results.v1.schema.json:505` so old files still validate. Update
  `tests/reports/test_hypotheses.py:69-76`, the hypotheses text in `docs/REPORTS.md`, and
  `SKILL.md:61`. No golden regeneration (goldens hold card behaviour). The Splits table
  already shows the rates.
- **Expected impact.** Removes a misattributed tuning signal from most reports of 200 or more
  games. No effect on win rates.
- **Effort.** S.
- **Risk.** None.
- **Validation.** A unit test on synthetic records asserting that the deck under test's gap
  equals the benchmark deck's. A mirror run produces no play/draw hypothesis.

### 5. pregame-1 (fast-speed-2, fast-speed-3, fast-speed-4): cut the play/draw simulation

- **Problem.** `MctsAgent` answers `CHOOSE_FIRST` with `first_player_choice`
  (`ai/setup.py:85-106`), playing `first_player_games` pairs of policy games every game. At
  that point nothing has been dealt, so the answer depends only on the decklists and who
  chooses. `fast` keeps `standard`'s 24 pairs.
- **Evidence (MEASURED).**
  - Cost: about 0.40 CPU-s per game (sd 0.04). That is 11.5% of a `fast` game on the tuning
    decks (30 games), 9.4% on the example decks, about 2.7% (2–4%) of `standard`, 1.3% of
    `strong`, and about 1% of `expert` (96 pairs, about 1.6 s).
  - It never chose to go second: 0/80 calls at `standard`, 0/9 at `strong`, 0/30 at `fast`,
    0/75 stability calls (15 ordered pairings × 5 seeds, at 8 and at 24 pairs), and 0/40 on
    the test decks. Pooled first-minus-second is +0.238 (SE 0.013), with z ≥ 3.16 in every
    pairing.
  - `first_player_games=8`: CPU ratio 0.925 [0.913, 0.934], +8.1% games per core-minute. 30/30
    games identical apart from the play/draw log entry: the 8 pairs are the first 8 of the 24.
  - `first_player_games=0`: +12.3%, and the games are bit-identical (9/9).
  - A bootstrap puts the chance of a wrong "go second" at about 0.25% per call with 8 pairs
    (0.01% with 24): a matchup bias under 0.1 point.
  - A memo per run (fast-speed-4) gives +12.3% in steady state, but only about +8% at 96
    games with a per-worker build. It is M effort and makes one answer shared by the whole
    run.
  - Evidence: `fast-speed/`, `verify/fast-speed-2-*`, `verify/pregame-1*`.
- **Change.**
  - In `PRESETS['fast']` set `first_player_games=8` (fast-d was screened with 8). `0` is the
    alternative if giving up the go-second answer is acceptable.
  - Update the presets table in `docs/AI.md`.
  - Leave `standard`, `strong` and `expert` as they are, or apply `0` there for 1–3% if their
    published runs are re-labelled.
  - Do not build the memo unless a deck pair that favours going second turns up.
- **Expected impact.** `fast`: +7% to +8% games per core-minute (about +11% to +12% at `0`),
  narrowing intervals by about 4% at fixed CPU. No measured change in strength or bias.
- **Effort.** S.
- **Risk.** None for reproducibility. The play/draw decision-log entry changes. About 0.25% of
  play/draw calls can flip, so the 1600 confirmation games of commit 988cd15 are expected to
  reproduce except for about 4. Re-label or re-run them.
  `tests/ai/test_presets.py::test_pre_game_samples_do_not_shrink_with_strength` still passes
  (8 ≤ 24).
- **Validation.** A fixed-seed `fast` benchmark before and after the change: actions and
  winners identical except in flipped calls. Measure the `CHOOSE_FIRST` process time directly
  rather than whole-game wall time.

### 6. engine-speed-7: cross-build play identity and a paranoid derived cache

- **Problem.** `tools/golden.py:22-32` serializes compiled card scripts and plays no games.
  The AI determinism tests compare a build with itself. Nothing checks that play is unchanged
  between an old build and a new one.
- **Evidence (MEASURED).**
  - The prototype harness `engine-speed/identity_check.py` passes on the bundle (186/186 rows
    byte-identical). It detects a tie-break flip (6/6 MCTS traces change).
  - `state_digest` includes `GameState.version` (`state.py:607`), so the harness would
    wrongly fail a speed-up that skips redundant `touch()` calls.
  - A real skipped-`touch()` bug (`interp.py:709`, `:726`) left all 186 traces identical. A
    paranoid `derived()` check caught it in 4 card tests (8 stale hits, about 2.5 s over
    `tests/cards`), and the card suite alone caught it in 2.
  - The harness decks reach 89 of 956 cards.
- **Change.**
  - An identity tool or slow test with an A/B mode (base and head on the same machine). It
    compares action traces, winners and decision logs, plus a digest that excludes `version`
    (and masks history `def_id` until CB-1 lands). It should include MCTS games at the
    `standard` budget, the example decks, and fuzz-style all-card decks with random agents.
  - A test fixture that monkeypatches `view.derived` to recompute on every cache hit and
    assert `same_values`, run over `tests/cards` and a small fuzz. This keeps the hot path
    untouched.
- **Expected impact.** A safety net for items 3, 8, 12, 13 and 32. No direct effect on speed or
  strength.
- **Effort.** S–M.
- **Risk.** None: test-only.
- **Validation.** Passes on the bundle prototypes. Fails on two negative controls: the
  tie-break flip and the skipped `touch()`.

### 7. stats-1: an explicit difference test in the skill

- **Problem.** Step 3 of the skill compares variants with "if the intervals overlap heavily"
  (`SKILL.md:98`). The phrase is undefined, while the sample-size table (`SKILL.md:80-85`,
  `references/statistics.md:23`) is sized for a two-proportion z-test.
- **Evidence (MEASURED, exact binomial enumeration).** At 400 games per arm and a 10-point
  gap:

  | Reading of "overlap heavily" | Power | Alpha |
  | --- | ---: | ---: |
  | any overlap means no difference | 0.515 | 0.005 |
  | overlap of more than half the mean margin (Cumming–Finch) | 0.772 | 0.037 |
  | overlap of more than half an interval | 0.927 | 0.168 |
  | two-proportion z-test | 0.813 | 0.052 |

  The strictest reading needs about 650 games per arm for 80% power. On the BO3 match rate,
  ±5 points needs 385 matches (about 1,000 games); a match-rate point is worth about 1.5
  game-rate points.
- **Change.** Decide on Δ and its 95% interval: paired (item 2) when the runs share a seed,
  Newcombe or z otherwise. `reports/stats.py:50` already has `two_proportion_p`, and the
  report's hypotheses already use z plus Holm. Add a match-level row to the sample-size table
  with the 1.5× note.
- **Expected impact.** Defined error rates. A strict reader gains up to 1.65× effective sample
  size.
- **Effort.** S, text only.
- **Risk.** None.
- **Validation.** A doc review that one explicit rule is stated and its sizing matches the
  table. The power figures are exact, so no simulation is needed.

### 8. engine-speed-3: rules management

- **Problem.** `rules_management` runs 556,024 times in the profiled game: from `game.advance`
  497,521 times, `interp.run_top_frame` 51,342 and `interp.resume` 7,161. It builds the
  derived view whenever any card is damaged (`engine/core.py:860-862`), and 66% of calls see
  the same state as the previous call.
- **Evidence (MEASURED).**
  - Two prototypes. P3, a lower-bound check against printed HP: it is valid while
    `hp_reduction_possible` is False, which is true for all 1,153 definitions. P7, a memo
    keyed on (version, history length, battles length, winner).
  - P3+P7 against the baseline: −3.8% CPU (3 paired runs); P3 alone about −1%. On top of the
    rest of the bundle: −9%.
  - Shadow checks found 0 violations over 460 random and greedy games (1.6M calls) and
    MCTS games. 1,807 targeted tests pass, and fingerprints are identical.
- **Change.** Both prototypes. Key the memo on `V._view_key(st)` as well, which adds the active
  player, turn and last battle, or add a debug assertion, so a future mutation that skips
  `touch()` cannot silently skip a destroy. Fold the pre-check into the existing damaged-card
  scan.
- **Expected impact.** About −4% CPU alone and about −9% inside the bundle.
- **Effort.** S.
- **Risk.** The memo relies on `touch()` discipline, which holds today. A dual-path debug
  assertion guards it.
- **Validation.** A dual-path assertion (fast path vs full path) over pytest and fuzz. The
  identity harness. `_compute` counts as the primary speed metric.

### 9. pregame-3: information-set tests for pre-game and special decisions

- **Problem.** `tests/ai/test_information_set.py:27-43` samples 25 states from turn 3 on:
  22 MAIN, 2 ACTION_STEP and 1 DISCARD. The pre-game decisions and the SELECT, BURST, YES_NO,
  BLOCK, ORDER_TRIGGER, EXCESS and ARRANGE decisions are unguarded at the agent level.
- **Evidence (MEASURED).**
  - No leak found:
    - CHOOSE_FIRST invariant 12/12, and 9/9 through `MctsAgent.choose`;
    - REDRAW 36/36 and 18/18;
    - 32 special states 32/32, with both MCTS and greedy.
  - The negative controls work. A search on the true state changes the result in 6/6
    first-player cases and in 29/32 special states. A leaky mulligan is detected 16/18 and
    11/12.
  - `Scenario` defaults make such tests vacuous: filler decks, and hands known to both
    players (`testkit.py:31, 61, 70-72, 120-125`).
  - A key-identity leak in `actions._ident` is caught by none of the existing 25 states and
    none of the 32 special ones, because self-play never puts an unseen card into a root
    option.
- **Change.**
  - (a) Run the invariance test on `tests/ai/situations.py` `choose_first()` and `redraw()`,
    plus a Player-Two-after-Player-One-redrew state. Add a first-player negative control and
    a leaky-mulligan negative control.
  - (b) Collect special decision kinds from multi-deck greedy self-play, and keep a state only
    where a cheating search differs.
  - (c) Build a Scenario SELECT or ARRANGE state with varied decks, an opponent hand added
    with `known=False`, and root options that name cards the decider has not seen. Give it
    its own negative control.
- **Expected impact.** Guards the pre-game procedures and key handling against future leaks.
  This matters if the mulligan ever gains a determinization. No runtime effect.
- **Effort.** S (about 25 s of serial test time).
- **Risk.** None.
- **Validation.** The tests pass on current code and fail under a true-state search, a leaky
  mulligan, and a seeded `_ident` leak.

### 10. seed-dispersion-1: no over-dispersion caveat

- **Problem.** Early runs of this study suggested that win rates vary across master seeds more than
  binomial noise allows (3 runs p=0.0006; seeds 7001–7006 p=0.007). If true, Wilson
  intervals would be too narrow.
- **Evidence (MEASURED).**
  - The master seed reaches play only through SHA-256 `derive_seed`
    (`runner/config.py:76-81`). An audit of 2,118 records found 0 seeding problems; its 33
    digest mismatches are CB-1.
  - Over 28 seeds fixed before their games were played: 1268/2000 = 0.634, χ² = 37.78 on
    27 df, p = 0.08, dispersion index 1.40 [0.87, 2.59]. Fisher's combination of the three
    pre-registered batches gives p = 0.03, and the new pre-registered batch 7101–7112 gives
    p = 0.50.
  - The low run s20261017 was chosen after the fact for looking low. It began with a 13-loss
    streak and won 71/120 afterwards.
  - Results are identical with `--workers` 1 and 2, and replays in reversed order reproduce.
- **Change.** No change to `docs/AI.md`, which has no over-dispersion text. Record the
  conclusion where seed heterogeneity is discussed. It rests on the mechanism, and the
  Fisher p of 0.03 should be reported next to the pooled p of 0.08. Test future heterogeneity
  claims only on seeds fixed before running.
- **Expected impact.** Avoids an unjustified 1.45× widening of every interval and a doubling
  of sample-size targets.
- **Effort.** S.
- **Risk.** None.
- **Validation.** If more assurance is wanted, mechanism checks at the MCTS presets
  (workers identity, reversed-order replays). More greedy seed batches are underpowered
  (power 0.21 at the point estimate).

### 11. policy-1 (fast-quality-2): kill-aware damage targeting and Burst in rollouts

- **Problem.** `policy._select` (`ai/policy.py:303-325`) ranks harmful targets by
  `pile_value`, so a damage effect goes to the most valuable enemy even when it survives.
  `_yes` (`policy.py:263-264`) gives Burst a margin of 1 in the softmax, so rollouts decline
  1/(1+e^(1/T)) of Bursts: 11.9% at T=0.5, 3.4% at T=0.3, 0.7% at T=0.2. The policy drives
  rollouts, priors, greedy's turn completion and the no-effect check. The dominated-move
  prune (`actions.py:117-118`) applies only at the root.
- **Evidence (MEASURED).**
  - Policy level:
    - the policy picks a survivor in 196/272 damage choices where a kill was possible; Unit
      and Pilot triggers account for 56% of these;
    - policy-only duels: kill-aware targeting +2.6 to +2.9 points, plus always-yes Burst
      +3.6 to +3.9 (fresh seeds), Burst alone +0.47.
  - MCTS level:
    - standard's root already kills in 90% of contested cases (94% with the fix);
    - the fix changes 18% of root moves, the same rate as reseeding alone;
    - root values shift +0.0034 (SE 0.0013) toward Zeon;
    - with both seats on the new policy, only Federation vs Zeon moves (−2.5 ± 0.8 points
      at the policy level).
  - `fast` (fast-quality-2), where the policy's argmax target survives (56 cases × 4 seeds):
    - kill rates: `fast` 0.73, `standard` 0.84;
    - costing about 0.003 value per game, a bias under 0.3 point;
    - `prior_weight` 0.125 alone or smart targeting alone reduces `fast`'s loss; the two
      combined do not.
- **Change.**
  - In `_select`, when the waiting Choose's first use is integer `Damage` on enemy cards,
    score killable targets high (worth + 2 if the damage destroys the target, else 0.25 ×
    worth).
  - Make Burst always yes in the policy.
  - Optionally put the change behind a policy-version switch, so the presets keep
    reproducing until they are re-calibrated.
- **Expected impact.** Better rollouts and opponent model. The effect on MCTS strength and bias
  is HYPOTHESIZED at 0–2 points and unmeasured. It changes every preset, greedy, the no-effect
  check and the play/draw simulation, so the strength and calibration tables need re-running.
- **Effort.** S for the code, M for re-validation.
- **Risk.** Deterministic. It is a behaviour change.
- **Validation.**
  - Policy duel on a fresh seed (item 22).
  - Regret on 100+ damage SELECTs against a deep (expert-budget) reference.
  - For the bias claim: paired benchmarks with both seats on the old policy against both on
    the new one, 2000+ paired games per pairing.
  - The puzzles.

### 12. engine-speed-5: memoize `link_satisfied`, alias `view.reg`

- **Problem.** `link_satisfied` is a pure function of two definitions, yet it is recomputed in
  every view computation (292,157 calls). `view.reg()` wraps the cached `get_registry()` and
  is called 6.2M times per game.
- **Evidence (MEASURED, one game seed).**
  - Retired instructions fall 3.0%, from 98.79G to 95.89G, of which startup is 2.43G.
  - CPU falls 3.6% to 3.9% over 8 concurrent ABBA slots.
  - Split: aliasing `reg` saves 1.33%, the link memo 1.78% (75 entries).
  - Fingerprints identical in 18/18 games.
- **Change.** Apply `engine-speed/proto/p6_micro.diff`. Clear the memo in `testkit.card_text`
  next to `core.clear_listen_cache()`.
- **Expected impact.** About −3% to −4% CPU.
- **Effort.** S.
- **Risk.** None: a pure memo keyed by integers.
- **Validation.** The link and pairing tests, `tests/cards/test_wp_gd05_a.py`, and the identity
  harness.

### 13. engine-speed-4: hoist unknown pools, then cache each world once

- **Problem.** `mcts.py:103` rebuilds world k once for every root move that reaches its k-th
  visit: 6,773 determinize calls for 2,550 distinct worlds (62% redundant). It also
  recomputes `unknown_instances` and `unknown_pools`, which do not depend on the seed.
- **Evidence (MEASURED).**
  - A cached clone equals a fresh determinize in 2,550/2,550 cases.
  - World construction falls from 0.554 s to 0.327 s, 2.7% of a `standard` game.
  - Hoisting the pools adds 0.35–0.9% on top of the cache, or about 1.5% without the cache.
  - Redundancy falls as the budget grows, and the cache's gain with it: `strong` about
    1–1.65%, `expert` about 0.6%.
  - Memory is about 19 KB per cached world: worst cases 6 MB (`standard`), 25 MB (`strong`)
    and 49 MB (`expert`).
- **Change.** First hoist the two pool computations into a per-search helper (all presets).
  Then add the world cache for `fast` and `standard`, optionally evicting world k once every
  root child has more than k visits. Requires CB-1 first, because cached worlds share
  history events.
- **Expected impact.** About 2–3.5% at `fast` and `standard`, about 1% at `expert`.
- **Effort.** S.
- **Risk.** No new randomness. Identity must be checked with MCTS games at all budgets.
- **Validation.** The identity harness at `standard`, `strong` and `expert` budgets. At least 5
  interleaved timing pairs, or deterministic counts.

### 14. pregame-2: document the mulligan; no simulation mulligan

- **Problem.** Is `opening_score` with `KEEP_RATIO` 0.9 (`ai/setup.py:22, 29-82`) sound, and
  would a simulation mulligan be better?
- **Evidence (MEASURED in policy-vs-policy play, as a proxy).**
  - The rule gains +3.5 points (SE 0.6) over always keeping (192 hands, 40 paired rollouts
    each); a replication on 128 hands gives +2.3.
  - A 20-pair simulation mulligan beats the rule by only +0.3 to +0.6, not significant, and
    would cost about 4.5–6% of a `standard` game.
  - A de-biased perfect oracle is worth about +5, so the rule captures 50–70%.
  - `KEEP_RATIO` 0.9–1.1 are indistinguishable (1.0 minus 0.9: +0.65, z 1.95 pooled).
  - Per-deck regret heterogeneity has p of about 0.06 per dataset and 0.013 pooled, so deck
    differences of about 2 points are not ruled out.
  - The mulligan costs 2.0 ms per decision at `standard` and 3.8 ms at `strong`.
- **Change.** A documentation paragraph with these numbers, stated as policy-play results.
  Fix `docs/AI.md:101-103` ("so it cannot come back"): `_redraw` shuffles the deck afterwards
  (`engine/game.py`), so the returned cards can be drawn later. Leave the `strong` and
  `expert` sample counts alone.
- **Expected impact.** Steers effort away from a simulation mulligan.
- **Effort.** S.
- **Risk.** None.
- **Validation.** None needed for documentation. An MCTS-level check would need about 1,600
  paired games.

### 15. fast-quality-1 (play-review-2): `prior_weight`

- **Problem.** The root bonus w·prior/(visits+1), with w = 0.5 in every preset
  (`mcts.py:180-188`, `config.py:25`), weighs about 4× more against the value estimate at
  `fast`'s budget.
- **Evidence (MEASURED).**
  - The played move is not the best logged mean in 105/534 `fast` decisions against 51/564
    at `standard`. The prior bonus exceeds the value gap in 80/534 against 44/564.
  - At `standard` these are ties:
    - paired t ≥ 2 in 0/49 cases;
    - independent references favour the best-mean move in only 24–28 of 51;
    - `prior_weight` 0.2 changes 17–19% of decisions for a net +0.0001 to +0.0014 per
      decision, not significant.
  - At `fast`, 0.125:
    - +0.0017 ± 0.0007 on 265 selected positions;
    - +0.0006 ± 0.0003 over all decisions of the original games;
    - +0.0003 ± 0.0003 over 1094 decisions in 16 fresh games, which fails the pre-registered
      2-SE criterion;
    - pooled +0.0004 ± 0.0002, concentrated in contested 2–4-move decisions;
    - +0.6–0.9% iterations.
- **Change.** None at `standard`. For `fast`, an optional knob candidate (0.125, or the
  untested 0.25), only after a larger pre-registered position study.
- **Expected impact.** Under 1–2 points against greedy; a match cannot detect it.
- **Effort.** S knob, plus 1–1.5 core-hours to re-confirm `fast`.
- **Risk.** None: config only.
- **Validation.** A position study of 3,000+ decisions against a reference. The fidelity re-run
  only if the bias claim matters.

### 16. policy-4: targeting polarity for `cant_*` grants and start-phase locks

- **Problem.** `targeting._effect_polarity` (`ai/targeting.py:92-93`) treats a `RuleGrant` as
  harmful exactly when its kind starts with `cant_`.
- **Evidence (MEASURED).**
  - 12 Choose sites on 12 cards are signed wrong:
    - friendly protections scored as harm (`cant_receive_damage`): EB01-079, GD02-040,
      GD02-105, GD03-115, GD04-119, ST03-014, ST06-013;
    - ST11-014 (`cant_be_attacked`), also scored as harm;
    - enemy `stays_rested_in_start_phase` locks scored as help: GD02-004, GD04-102,
      GD05-073, ST08-009.
  - GD04-107 is ambiguous. None of these cards is in a studied deck.
  - Blue Giant puzzle, valuable Unit attacked: the no-effect check prunes the Command 5/5
    and MCTS passes 3/3.
  - The table alone regresses the mirror case (cheap Unit attacked: pruned 5/5). It also
    makes Moment of Rest count as removal (`evaluation.py:133-138`) and earn `CLEAR_THE_WAY`
    (`policy.py:139`).
  - In a built deck, the bug matters about once per 30 games (104 vs 103 wins of 200).
- **Change.**
  - An explicit RuleKind-to-polarity table.
  - For battle-scoped protections, prefer the friendly Unit in the current battle, or have
    the no-effect check try every legal target.
  - Keep start-phase locks out of `command_harms_enemy_units`.
- **Expected impact.** Correct heuristics for user decks with these cards; under 1 point.
- **Effort.** S–M.
- **Risk.** None.
- **Validation.**
  - Unit tests of `_polarity_at` for the 12 cards.
  - Puzzles A, B and B' at seeds 1–3.
  - A count of correctly targeted protections in instrumented games.

### 17. fast-quality-5: decision-log audit

- **Problem.** `log_entry` keeps the top 5 alternatives (`ai/base.py:15, 84-99`) and drops
  iterations, the number of root moves and the priors.
- **Evidence (MEASURED).** Capturing them changes no play (4/4 games). Five extra fields grow
  logs by 15–16%; the full root capture by 129–140%. But the number of root moves, the budget
  and the exact priors can already be rebuilt from `games.ndjson` by a replay without search:
  534/534 decisions in 0.7 CPU-s (`verify/fast-quality-5-evidence/exact_priors.py`).
- **Change.** Ship the replay audit as a script. Optionally log the number of root moves, the
  budget and the iterations. Do not add a "prior-decided share" metric; it is unvalidated.
- **Expected impact.** Auditing convenience only.
- **Effort.** S.
- **Risk.** None.
- **Validation.** The decision-log tests (superset key checks), and identical results with and
  without `--decision-log`.

### 18. stats-4: derived BO3 match rate

- **Problem.** Within a BO3 match, games are independent given who goes first: agents are
  rebuilt per game (`runner/game.py:126`), seeds are per game, and only the chooser carries
  over (`runner/match.py:38`). So the match rate is a function of the play and draw rates and
  each side's chance of going first, and counting match results throws information away.
  The game-level Wilson interval in BO3 is conservative (design effect 0.83–1.00).
- **Evidence (MEASURED).**
  - The exact model's plug-in/direct variance ratio per game is 0.76–0.90, i.e. 10–24% fewer
    games.
  - Greedy data: game 1 vs games 2–3 agree within play/draw cells (χ² 0.84 on 2 df,
    p = 0.66). The derived rate 0.684 ± 0.051 matches the direct 0.693 [0.639, 0.743].
  - 20/20 games 2–3 replay identically standalone.
  - The chooser went first in 1252/1252 `standard` games and 1362/1364 `fast` games.
- **Change.** Report a derived match rate with a delta-method interval:
  - estimate the go-first probability separately for each deck;
  - score simultaneous defeats with BO3 match points and model turn-limit draws;
  - keep the direct rate next to it;
  - note in `statistics.md` that match-rate n is matches.
  The skill change the finding proposed is already done at HEAD.
- **Expected impact.** 1.1–1.3× precision per CPU for the match-rate headline only.
- **Effort.** M (aggregate, schema field, summary, docs).
- **Risk.** None with an analytic interval.
- **Validation.** Monte Carlo coverage on the exact model. A conditional-independence test on
  BO3 games. A standalone-replay regression test.

### 19. stats-5: group-sequential stopping for paired comparisons

- **Problem.** Comparisons run a fixed n. The skill's "run more games" (`SKILL.md:98-99`) is
  uncontrolled optional stopping.
- **Evidence (MEASURED, exact integration and Monte Carlo under the normal model).**
  - Design: 4 looks, O'Brien–Fleming bounds 4.048 / 2.862 / 2.337 / 2.024, futility when
    |z| < 0.25 / 0.75 / 1.25, and a maximum of 1.2 × the fixed n.
  - Alpha 0.039–0.041, power 0.79.
  - Expected sample size against the fixed paired design: −30% at no difference, −16% at the
    target, −31% at 1.5× the target; worst case +20%. These percentages hold at every ρ.
  - A stopped run is an exact prefix of a longer run (BO1, BO3, workers 1 and 2).
- **Change.** A skill procedure plus a pooling script, with looks at seed-block boundaries.
  No runner change is needed. Depends on item 2.
- **Expected impact.** 1.2–1.4× expected precision per CPU for variant comparisons, plus
  controlled alpha. The total saving is small when a baseline is played to the maximum for one
  variant only.
- **Effort.** M.
- **Risk.** Low, provided looks are taken only at fixed match-index boundaries.
- **Validation.** The Monte Carlo is the validation, since matches are i.i.d. given their seeds.
  Add many cheap greedy A/A runs.

### 20. stats-6: per-card "drawn" hypotheses

- **Problem.** `reports/hypotheses.py:77-100` tests win rate when a card was drawn against when
  it was not. "Drawn" includes `shield_to_hand` (`runner/instrument.py:24`).
- **Evidence (MEASURED, placebo cards, no new games).**
  - Ordinary cards: the placebo null has mean −0.01 ± 0.06 and near-nominal false-positive
    rates. Game length barely matters: the deck under test draws only 0.06–0.14 more cards
    in losses.
  - Burst add-to-hand cards: the deck under test loses about 6 Shields in a loss against 2.4
    in a win. A placebo with that Burst gets Δ of −0.07 to −0.11 and a false-positive rate
    10–17× nominal.
  - Observed drawn-minus-intention-to-treat gaps are about 0 for 15 ordinary cards and −0.07
    to −0.12 for the 3 Burst Pilots.
- **Change.** Exclude `shield_to_hand` (and search) entries from the tested exposure, or test
  the recorded `initial_hand`. Keep "drawn" as a descriptive figure.
- **Expected impact.** Valid card hypotheses for Burst add-to-hand cards. Reports only.
- **Effort.** S.
- **Risk.** None.
- **Validation.** Re-run `verify/stats-6/placebo.py` on the new exposure.

### 21. search-3: opt-in paired early stop

- **Problem.** The existing stop (`mcts.py:207-215`) saves 7.9% of iterations.
- **Evidence (MEASURED offline, 383 recorded searches).**
  - The rule as measured saves 11.7% and changes 3 choices. One was a premature stop on a
    lethal turn: 40 of 243 iterations, with values bimodal at sd 0.47 against the assumed
    0.083.
  - Safe variants save 7.8–9.1% of iterations with 1 change in 383. One is paired-only; the
    other floors the unpaired sd at max(running sd, about 0.08). That is roughly 5–8% of game
    CPU.
  - The regret comparison was vacuous: 0 of 58 reference decisions changed.
- **Change.** A paired-only rule behind an opt-in `stop_z: float | None = None`.
- **Expected impact.** About 1.05–1.1× games per CPU.
- **Effort.** S for the code; M to re-validate a preset.
- **Risk.** Low.
- **Validation.** An offline reference on 200+ decisions where the rule stops, oversampling
  bimodal end-game decisions, compared with a uniform budget cut at matched savings.

### 22. policy-3: policy-duel harness

- **Problem.** Testing a policy change through MCTS matches costs 10.7–18.2 core-s per game.
- **Evidence (MEASURED).**
  - 8000 policy games cost about 80 CPU-s.
  - For narrow changes the paired SE is 0.12–0.27 points; for broad ones pairing gains
    nothing.
  - Results are identical across worker counts.
  - The duel rates temperature 0.3 as stronger, but the MCTS data do not. How duel results
    relate to MCTS strength is untested.
- **Change.** Add a `policy` subcommand with a variant registry and a frozen base scorer.
  Report paired and unpaired estimates. Use it as a sanity and bug screen, not a gate.
- **Effort.** S–M.
- **Risk.** None.
- **Validation.** A dose-response check: about 4 variants, including degradations, at about 800
  mirrored `fast` games each.

### 23. dk-2: decision-level screen with a placebo arm

- **Problem.** The raw same-seed change rate is dominated by search chaos: at the micro budget,
  two seeds of the same agent disagree on 24–28% of decisions.
- **Evidence (MEASURED).**
  - The shift statistic d_BO − (d_OO + d_BB)/2 is unbiased. Stealth swap: 0.0006
    [−0.0012, 0.003]; Aces swap: 0.003 [−0.0013, 0.0069].
  - A transposition placebo shows the stealth change rate is all chaos and Aces about 80%.
  - Aligning the pool layout cuts chaos 1.7–3.7×.
  - A knob variant that is not stronger still shifts strongly (0.033), so the screen cannot
    rank knob variants.
  - Bootstraps over 8 games are unreliable.
- **Change.** Use it only for null-type questions (belief or information changes). Include a
  matched placebo arm, 30+ game clusters and cluster-robust intervals. Consolidate with
  `phase1/gap-2/decision_impact.py`.
- **Effort.** S–M.
- **Risk.** None.

### 24. search-6: exploration at standard

- **Problem.** c = 0.3 is about 2.5× the variance-matched value (0.12) for a per-visit sd of
  0.084.
- **Evidence (MEASURED).**
  - Offline at `standard` (58 decisions), against c = 0.3:

    | c | Regret change | Iterations used |
    | ---: | --- | ---: |
    | 0.2 | −0.0004 ± 0.0003 | 0.97 |
    | 0.15 | −0.0007 ± 0.0004 | 0.94 |
    | 0.1 | −0.0002 ± 0.0004 | 0.89 |

  - At the `fast` budget, c 0.15 minus c 0.3 is +0.047 ± 0.029 (paired).
- **Change.** None now. If `standard` is re-calibrated, c of 0.15–0.2 is a speed-only option
  (3–6% fewer iterations).
- **Effort.** S plus re-validation.
- **Risk.** None.
- **Validation.** Offline screen. A strength claim would need about 4,900 mirrored games.

### 25. play-review-4: 【Attack】 self-boosts in attack scoring

- **Problem.** `values.fight` (`ai/values.py:65-71`) judges a fight on current AP. `_attack`
  (`policy.py:201`) and `_block_risk` (`:183`) therefore ignore 【Attack】 self-boosts.
  `_block` is unaffected, because the trigger resolves before the block step.
- **Evidence (MEASURED).**
  - Zaku II example: score −2.20, ranked 6/6; with the boost, 3.65, ranked 3/6.
  - 8 cards have such boosts, several conditional and 3 hosted on Pilots.
  - Over 400 `standard` games, 1.2% of MAIN decisions are affected.
  - Root moves change 26% of the time (reseeding alone: 28%). Roughly one extra kill per 50
    games.
- **Change.** A condition-aware boost helper at the MAIN-phase call sites only, not inside
  `fight()`. It must evaluate If conditions, link gates and the paired Pilot's triggers.
- **Expected impact.** Under 0.5 point.
- **Effort.** S.
- **Risk.** None (a behaviour change for all presets).
- **Validation.** Unit tests (Zaku II against a 3-HP Unit; Zee Zulu gets no boost against a
  Unit) and a puzzle.

### 26. play-review-1: self-damage targeting (purple)

- **Problem.** A friendly Choose whose first use is Damage is classed as harm
  (`ai/targeting.py:18, 59-73`), so the policy damages its least valuable Unit and misses the
  damaged-payoff Barbatos synergy.
- **Evidence (MEASURED).**
  - 13 self-damage definitions, all purple.
  - Root regret on friendly-damage SELECTs: current policy 0.032–0.038, prototype
    0.012–0.022, the search itself 0.003.
  - Rollout shift +0.021 (clustered SE 0.007–0.010).
  - The prototype's flat bonus picks the worst target at run2 m1 #58.
- **Change.** Add the destroy penalty, which is supported. Add a payoff bonus only with a
  DSL-anchored payoff test that weighs the gain against the HP cost.
- **Expected impact.** HYPOTHESIZED 0–2 points on Tekkadan matchups.
- **Effort.** M.
- **Risk.** None, but it changes play on the tuning and example Tekkadan decks.
- **Validation.** Regret against a deep reference on 100+ positions.

### 27. stats-7: deal alignment (opt-in)

- **Problem.** `build_deck` sorts the main deck (`deck/parse.py:104`), so a swap shifts the
  positions of unrelated cards.
- **Evidence (MEASURED).**
  - Identical top-21 positions: 13.5 sorted against 20.2 aligned. Same opening five: 10.8%
    against 82.3%.
  - Outcome variance-ratio gap: 0.038 (SE 0.083) with greedy, 0.133 (SE 0.157) at micro.
  - MCTS samplers reshuffle sorted pools, so aligned games never replay exactly (0/60).
- **Change.** If item 2 lands, offer alignment as an opt-in inside `compare`. Record it in
  `results.json` and in replays.
- **Expected impact.** Unproven, 0–25% fewer games.
- **Effort.** M.
- **Risk.** A replay-format change.
- **Validation.** A sign test on discordant pairs, 600+ pairs at `fast`.

### 28. dk-1: decklist knowledge

- **Problem.** Both agents know both exact decklists (`docs/AI.md:30`,
  `docs/ASSUMPTIONS.md:62`), so swap deltas may include an adaptation term.
- **Evidence (MEASURED).**
  - The prototype belief agent reproduces stock games with a null belief, is
    permute-invariant, and is worker-independent.
  - Micro-budget shifts: stealth 0.0006, Aces 0.003 (main phase 0.0049 [0.001, 0.0084]).
  - Paired Aces match: −0.017 [−0.103, +0.070], n = 120.
  - A `standard` probe of Aces gives 0.0075 ± 0.010.
  - Neither swap changes the Action-trick density the benchmark agent believes in.
- **Change.** Record the null result and its bounds in `docs/ASSUMPTIONS.md`. Ship
  `--dut-belief` only if a paired `standard` run on a trick-heavy swap shows a bias of at
  least 0.03. It needs a leak-free displacement rule and a decklist-invariance test.
- **Effort.** M.
- **Risk.** Low.

### 29. fast-quality-3: puzzles for `fast`

- **Problem.** The puzzles test `standard` and `expert` only (`tests/ai/test_puzzles.py:23-24`),
  and `fast` solves all of them: 216/216 on seeds 1–12 and 5820/5820 on seeds 13–400.
- **Evidence (MEASURED).**
  - The policy prior alone solves 12 of 14 searched puzzles.
  - Only the turn-2 Hawk of Endymion position discriminates. Ending the main phase is
    better by 0.032–0.037 (z > 13). `standard` solves it 200/200, `expert` 8/8, `fast`
    126/200.
  - Moment of Rest is a tie (0.0013–0.0020) and an artefact of item 16.
  - The 18-move candidate has a three-way tie.
- **Change.**
  - Add `fast` to `PRESETS_UNDER_TEST` (about +1.7 s).
  - Add one compact Hawk-style puzzle with a documented `fast` floor (about 0.5) and a
    `standard` threshold of at least 7/8, checked over 160+ seeds.
  - Exclude `expert` from the new puzzle or give it its own floor.
- **Effort.** S.
- **Risk.** None.

### 30. fast-quality-4: wide decisions at `fast` (record negative results)

- **Evidence (MEASURED).**
  - 62/534 `fast` decisions have 10 or more root moves. There `standard` is better by
    +0.0072 ± 0.0029, a third of the total gap.
  - `fast`'s knobs at `standard`'s budget recover the gap (+0.0057 ± 0.0024); `standard`'s
    knobs at `fast`'s budget do not.
  - A minimum of 2 root visits changes 0/62 moves.
  - Sequential halving at `fast`'s budget is null: +0.0002 ± 0.0015.
- **Change.** A docs note: `fast`'s error on wide turns closes only with budget. Do not add a
  root visit minimum.
- **Effort.** S.
- **Risk.** None.

### 31. policy-5: rollout temperature 0.3 at `standard`

- **Evidence (MEASURED).**
  - Paired variance falls to 0.80×, but decision stability does not improve: seed agreement
    +0.0006 [−0.030, +0.031] over 120 positions.
  - Temperature 0.3 alone was already screened at the `fast` budget as fast-e: +0.016
    against fast-a, and it shifted a self-play matchup by +0.167 [+0.076, +0.257].
- **Change.** None.
- **Risk.** It would change `standard`.

### 32. engine-speed-6: invalidate the derived view by mutation kind

- **Problem.** `touch()` drops the cached view on every mutation (`engine/state.py:527-529`).
- **Evidence (MEASURED).**
  - 83% of touch-triggered recomputes reproduce identical values.
  - Actually skipping the rest, damage and off-field touches removes 23% of recomputes on the
    baseline. On the bundle it removes 15–41% depending on the decks, with identical
    fingerprints and no stale views. That is about 5–9% CPU.
  - A whole-ability static mask removes only 13%: it counts use-time reads.
- **Change.**
  - A compute-time read mask (Constant conditions, scopes, StatMod values, gate filters,
    lasting effects), with `touch(kind)` at the 34–36 mutation sites.
  - Consider a dependency-aware view key first: history length and battle force most
    recomputes on the example decks.
- **Expected impact.** About 1.06–1.10× games per CPU.
- **Effort.** L.
- **Risk.** A missed dependency gives a stale view: reproducible but wrong. Do item 6's paranoid
  mode first.

### 33. search-4: keyed rollout noise (do not pursue)

- **Evidence (MEASURED).**
  - Out of sample, the proposed key cuts paired variance by 8–10% at T = 0.5 and by 1–4% at
    T = 0.3.
  - Without a per-decision counter it biases the rollout policy: the pass share rises from
    0.40 to 0.51.
  - With the counter the cut is 5–7%, while hashing costs 4–8% CPU, so the net gain is zero.
  - Tuning self-play would change too.
  - The dropped policy-2 measured the in-tree effect at 0.89–0.97× variance for +12–30% CPU.
- **Change.** None.

### 34. pregame-4 (play-review-3): indifference and proven-outcome stops

- **Evidence (MEASURED, exact replays of 8 games, 564 searches and 82,567 iterations; the log
  undercounts them as 66,833).**
  - Truly indifferent decisions use 1.3% of CPU.
  - The proposed m = 8 rule fires falsely in near-lost positions and changes 3–6 moves.
  - The safe stops save about 1.4–2.1% of CPU.
  - Breaking ties toward the passive move affects 1 decision in 564, in a won position.
- **Change.** None now. If pursued, use a structural test for ORDER_TRIGGER only: identical
  states after each order.

## Findings from the first follow-up round

These came from the first follow-up round. The workflow that ranked the table above was
resumed after a pause, its critic then chose a different second round, and these findings
never reached the ranking. Item A was re-checked by two skeptics; items B–E were not, and
the lead checked only the code facts they cite (weights, keys, line numbers).

### A. ai-level-bias-3 (+ ai-level-bias-1, -2, -4): measure matchup fidelity above `standard`

Score 6.0 (impact 3, confidence 4, effort 2), downgraded; it would rank next to items 12–13.

- **Problem.** The calibration measured only whether `fast` reproduces `standard`'s matchup
  estimates. Whether `standard`'s estimates move when both seats play at `strong` or `expert`
  is unmeasured, so a user cannot tell how much of a reported rate is the AI level.
- **Evidence (MEASURED, skeptic-checked).**
  - The level does matter between a one-ply player and search. On the deals of seed 8101,
    red-green-zeon (benchmark) beats blue-white-federation in 263/400 = 0.658 of games under
    greedy, 54/80 under a 16-iteration "micro" search, 215/400 = 0.538 under `fast` and
    200/400 = 0.500 under `standard`; the paired differences from the low levels to `fast`
    and `standard` are −0.12 to −0.17 (p ≤ 0.016). Between `fast` and `standard` it is
    −0.0375 [−0.088, +0.013].
  - The finding's original claim (a +0.09 to +0.16 shift toward red-green-zeon with level) was
    refuted: it compared different deal sets, and on the 130 deals greedy and micro share they
    agree (68 vs 66 wins, Δ +0.015 [−0.082, +0.113]).
  - Results at different levels on the same deals are paired: the discordance q is 0.26 for
    `fast` vs `standard` and up to 0.44 between other levels, against about 0.5 for
    independent deals, so pairing saves up to about half the games.
  - The calibration's fidelity rule (fail when the interval excludes 0 and |Δ| > 0.05) has
    about 45% power at a true |Δ| of 0.05 and 81% at 0.077 (exact trinomial, q = 0.30).
  - Mechanism, from fast to standard on the same deals: both decks move 【Main】/【Action】
    Commands from the main phase to the action step (Federation −0.14 main-phase and +0.36
    action-step casts per game; Zeon −0.15 and +0.16); Base activations do not change.
  - Evidence: `gap-1/`, `verify/ai-level-bias-*`.
- **Change.** A paired study of `standard` against `strong` (and `strong` against `expert`) on
  the three example pairings: BO1, the same seed, the deck under test's game win rate, and
  the paired interval from `scripts/ai_calibrate.py paired`. Stage 1 reuses the 400-match
  `standard` arms of the calibration (seed 8101) and adds a `strong` arm per pairing: a 95%
  half-width of about 0.05, detecting |Δ| ≥ 0.075 with 80% power. Stage 2, to show
  |Δ| ≤ 0.05 with a 95% interval, needs about 1,100–1,260 pairs per pairing at q = 0.26–0.30,
  and a small script to pool per-match differences across master seeds. State the result in
  `docs/AI.md`, and add to `docs/REPORTS.md` that `ci95` covers sampling error only.
- **Expected impact.** Tells users whether a `standard` result transfers to stronger play.
  No change to any result.
- **Effort.** S for the tooling, but stage 1 costs about 17–25 core-hours of `strong` games and
  stage 2 about 25 core-hours per pairing.
- **Risk.** None.
- **Validation.** The study is the validation; pre-register n and the decision rule.

### B. eval-2: the `bursts` evaluation feature (not re-checked)

- **Problem.** `bursts` is the Burst density of the unseen deck cards times the player's own
  Shields (`ai/evaluation.py:89-90, 178`), weighted 0.5532 in `TUNED_WEIGHTS`. Within one
  deck pool it is almost a constant multiple of Shields, so its weight mostly encodes deck
  composition: a Federation Shield (density 0.34) is worth about 0.05 logit more than a Zeon
  Shield (0.25).
- **Evidence (MEASURED by the analyst).** No predictive signal within any deck pool, and
  dropping it improves out-of-sample calibration on unseen decks. Evidence: `gap-2/`.
- **Change.** Drop the feature and refit, as part of one evaluation retune (with item C).
- **Expected impact.** Robustness on unseen decks; strength effect about zero. All presets
  change and need re-calibration.
- **Effort.** S, plus the re-calibration.
- **Validation.** Held-out log-loss on decks outside the tuning pool; the strength tables.

### C. eval-3: fit and check a retune at the budget it serves (not re-checked)

- **Problem.** How well a turn-start position predicts the winner depends on the play that
  follows. `tuning.py` fits on 16–48-iteration self-play and reports one held-out log-loss.
- **Evidence (MEASURED by the analyst).** The calibration slope of `TUNED_WEIGHTS` rises from
  0.74 on greedy games to 1.10 on `standard` games; refits on weaker play look better in
  pooled cross-validation but are not better at `standard`. Evidence: `gap-2/`.
- **Change.** Report held-out log-loss and calibration per preset in `tuning.py`, and validate
  any retune at `standard` (and at `fast` and `expert` if they are to use it).
- **Effort.** S. **Risk.** None.

### D. fusion-3: one tree node can mix main-phase and action-step decisions (not re-checked)

- **Problem.** A player with no action-step option is passed automatically, without a
  decision (`engine/game.py:347-351`), so whether the opponent gets an action-step decision
  depends on hidden cards. Children are keyed by (mover, move key) without the decision kind
  (`ai/mcts.py:32, 170-172, 204`), so one node can hold a `MAIN` decision in some worlds and
  an `ACTION_STEP` decision in others, and `play_command X` pools its statistics across both
  timings.
- **Change.** Add the decision kind to the child key.
- **Expected impact.** HYPOTHESIZED small; it also explains why a naive matched-path test
  overstated strategy fusion (+0.06 raw, +0.021 controlled).
- **Effort.** S. **Risk.** Changes play at every preset; re-calibrate.
- **Validation.** The puzzles, then a paired strength test at `standard`.

### E. fusion-2: a held action-step trick is valued like any Command in hand (not re-checked)

- **Problem.** `TUNED_WEIGHTS` gives `hand_commands` 0.3727 and `hand_units` 0.7157, so a trick
  held with resources open is worth about half a Unit at the leaf, and its value must be
  realised inside the two-turn horizon by a rollout policy that casts Commands at the first
  chance (`ai/policy.py:246-256`).
- **Evidence (MEASURED by the analyst).** Holding a trick instead of a same-cost Unit lowers
  the searched root value by 0.046–0.054 at every budget, about two thirds of it from the
  static evaluation. Evidence: `gap-3/`.
- **Change.** A feature for castable action-step Commands held with enough resources, fitted
  by the tuning procedure (with items B and C).
- **Effort.** M. **Risk.** Changes every preset; re-calibrate.

## Corrections to `docs/AI.md` found along the way

All MEASURED; line numbers refer to `docs/AI.md` at commit 6be0ea8.

- **Profile claims (`:218-220`, `:292-295`).** "About 90% inside `apply`, two thirds in
  `_compute`", and "20.8M `_host_abilities` calls, mostly hand and trash", describe the AI
  branch's older engine. The current engine measures 80–83% in `apply`, about 30% in
  `_compute`, and 1.23M `_host_abilities` calls with none for hand or trash. The suggested
  per-zone ability cache already exists (`CardEntry.field_entries` and friends, commit
  d3f8dd6).
- **`BURST / YES_NO: Yes` (`:119`).** Rollouts at T = 0.5 decline 11.9% of Bursts (item 11).
- **"So it cannot come back" (`:101-103`).** The deck is shuffled after the redraw (item 14).
- **"Every look-ahead runs on determinize" (`:34-36`).** True, but until CB-1 is fixed the
  copy is not side-effect-free.
- **Paired sampling (`:70-73`).** World pairing does the work. The shared rollout seed adds
  little over independent seeds: paired variance ratio 1.11.

## Ideas already tested

From `docs/AI.md`:

- 4× `standard` iterations (46/96 against `standard`);
- a three-turn horizon at 2× (51/96);
- `strong`'s knobs at the `standard` budget (91/192);
- the dropped evaluation features;
- paired sampling (0.61 to 0.68 against greedy);
- dominated-move and no-effect pruning;
- budget scaling against greedy (2.5× stayed at 0.69–0.71);
- the `fast` screening candidates, including a larger budget (fast-b) and rollout
  temperature 0.3 alone (fast-e);
- the `expert` candidates: two and four times `strong`'s budget, and twice with exploration
  0.1 and rollout temperature 0.2 (the shipped `expert`, 605/1200 against `strong`).

Dropped or found null in this study (MEASURED):

- **Sequential halving with common random numbers at the root (search-2).** At 0.5–0.6× it is
  worse than UCB at 1× by +0.0008 regret; at equal budget it is not better. At `fast`'s budget
  it is also null (item 30).
- **A higher `max_iterations` for wide decisions (search-5).** No regret change on 11–16 fresh
  decisions with 11 or more root moves; it costs +10% iterations.
- **Keyed Gumbel rollout noise inside the tree (policy-2).** 0.89–0.97× variance for +12–30%
  CPU (see also item 33).
- **Refitting evaluation weights on determinized features (policy-6).** Not a defect: the
  world-averaged prediction the search uses gets slightly worse (+0.001 to +0.003 nats). At
  most add a docs note that the held-out log-loss is measured on true-state features by
  design.
- **Root visit minimum.** 0/62 wide decisions changed.
- **A simulation mulligan, and `KEEP_RATIO` 0.9–1.1.** No significant gain (item 14).
- **`prior_weight` 0.2 at `standard`.** Neutral (item 15).
- **Tree reuse between decisions.** The reusable visits are about 18% of the budget, worth at
  most about 1.2× the budget; not prioritized.
- **Mirrored deals (playing a deal both ways) for single-run precision.** No help: the shared
  opening hand correlates the two results positively.

- **A one-turn rollout horizon (horizon-1).** At `fast`'s budget, `horizon_turns=1` lost
  36/96 = 0.375 [0.285, 0.475] to the two-turn horizon at equal iterations, and 42/96 at
  twice the iterations while costing about 1.4× the CPU. Not a speed lever.
- **A remedy for strategy fusion (fusion-1).** Fusion at opponent nodes is real but small: a
  controlled excess of +0.021 [−0.000, +0.047] at the `expert` budget, not growing per node
  with the budget, and it moves neither root choices nor the searched value of holding a
  trick. Do not build a multiple-observer search on this evidence; items D and E are the
  related channels that did show up.
- **Richer evaluation model forms (eval-1, not re-checked).** Interaction terms or gradient
  boosting on the same features gain about 0.01 log-loss, and swapping evaluations changes
  root choices about as often as re-seeding the search.
- **An apparent AI-level effect between greedy and micro (ai-level-bias-2).** A 0.18 gap came
  from comparing different deals; on the 130 deals they share the two agree.

Open, HYPOTHESIZED only:

- **`Lasting.effect_key` depends on first-use order within a process.** It is serialized in
  the state. Reversed-order replays reproduce the digests, so no effect has been seen.

## Recommended order of work

1. **CB-1.** Fix the history aliasing and add the per-`choose` digest tests.
2. **Item 6.** The identity harness (A/B, version-free digest) and the paranoid derived-cache
   fixture.
3. **Engine bundle.**
   - Land items 3, 12 and 8, then the pool hoist and world cache from item 13, each checked
     with the harness.
   - Item 5: `fast` `first_player_games=8`, and update the presets table.
   - Then re-measure the throughput table once, on an idle machine.
4. **Report and skill statistics.**
   - Item 4 (remove the play/draw hypothesis), item 7 (explicit test) and item 20 (cheap
     exposure fix).
   - Then item 2 (`compare`, paired), and on top of it items 19, 18 and 27.
   - Item A: the `standard` vs `strong` fidelity study, stage 1, with the paired tooling.
5. **Tests.** Item 9 (information-set coverage) and item 29 (`fast` in the puzzles plus the
   Hawk puzzle).
6. **Docs.** Items 10 and 14, item 30, the `docs/AI.md` corrections above, and item 28's null
   in `docs/ASSUMPTIONS.md`.
7. **One policy and evaluation batch.** Items 11, 16, 25 and 26, screened with item 22's duel
   harness, plus the evaluation retune of items B, C and E and the child key of item D.
   Then one re-calibration of all four presets, because any of these changes alters all of
   them, greedy and the no-effect check.
8. **Optional, only with new evidence.**
   - Item 21 (opt-in stop) and item 15 (`fast` `prior_weight`).
   - Items 17 and 23 (tooling).
   - Item 32 (L effort, after the bundle and item 6).
   - Items 24, 31, 33 and 34: not recommended.

## Knob-only follow-ups for `fast` and `expert`

- **`fast`, `first_player_games=8`.** Screen and adopt: +8.1% [7.0, 9.5] games per core-minute
  on the tuning decks, about +7% on the example decks; identical games when the answer is
  unchanged (item 5).
- **`fast`, `first_player_games=0`.** The alternative: +12.3%, but it can never choose to go
  second (none of 200+ calls did).
- **`fast`, `prior_weight=0.125` (or 0.25).** Weak evidence: +0.0004 ± 0.0002 per decision out
  of sample. It needs a 3,000+ decision position study before adoption (item 15).
- **`fast`, keep exploration 0.15.** fast-d minus fast-e is +0.047 ± 0.029. Do not switch to
  temperature 0.3 without c 0.15: fast-e shifted a self-play matchup by +0.167.
- **`fast`, do not raise `max_iterations` or add a visit minimum.** fast-b was already tested,
  and the wide-turn gap closes only at about 4× the budget (item 30).
- **`expert`, leave `first_player_games=96` and `mulligan_samples=384`.** They cost about 1%
  and 0.01% of a game; lowering them buys almost nothing and changes the just-calibrated
  preset's logs.
- **`expert`, no larger budget.** Four times `strong`'s budget was no better than twice on the
  tuning decks (49/96 against 50/96), and twice is within ±3 points of `strong` on held-out
  decks.
- **`expert`, `prior_weight` scaled with the budget.** Untested (HYPOTHESIZED). The prior
  weighs about 8× less per visit share than at `standard`; screen only with a position study.

## Raw evidence

Everything is under `build/ai-presets/phase1/` (local, untracked):

- **Lens outputs:** `search/`, `policy-eval/`, `pregame/`, `engine-speed/`,
  `benchmark-stats/`, `play-review/`.
- **Follow-up studies:** first round `gap-1/` (AI-level bias), `gap-2/` (evaluation),
  `gap-3/` (strategy fusion), `gap-4/` (horizon); second round `seed-dispersion/`,
  `fast-quality/`, `fast-speed/`, `decklist-knowledge/`.
- **Synthesis draft:** `proposals_draft.md`, which this document edits.
- **Skeptic re-checks:** `verify/<finding-id>*/`. Each folder holds a `commands.txt` with the
  exact commands.
