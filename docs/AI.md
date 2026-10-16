# AI players

`gcg_sim.ai` provides the players that the benchmark runner pits against each other:

| Kind | What it does | Cost |
| --- | --- | --- |
| `random` | Uniformly random legal action from a seeded generator. | ~0 |
| `greedy` | One-ply look-ahead on one determinization, scored by the evaluation. | ~0.01 s per decision |
| `mcts` | Determinized information-set MCTS with rollouts through the opponent's reply turn. | see [Presets](#presets-and-measured-throughput) |

```python
from gcg_sim.ai import make_agent
from gcg_sim.ai.tuning import TUNING_DECKS
from gcg_sim.engine.game import apply, new_game

state = new_game((TUNING_DECKS[0], TUNING_DECKS[1]), 7)
agent = make_agent("mcts", preset="standard", seed=1234, log=True)
action = agent.choose(state, state.pending.player)  # one of state.pending.options
apply(state, action)
print(agent.decision_log())  # [{"turn", "player", "decision", "chosen", "label", "alternatives"}]
```

Every agent is a deterministic function of its construction seed and the sequence of
`choose` calls: decision number *n* draws its randomness from
`SplitMix64(derive_seed(seed, agent_name, "decision", n))`. Nothing reads the wall clock or a
global random generator.

## Information-set guarantees

A player knows both decklists (a modelling assumption, see `docs/ASSUMPTIONS.md`), the public
zones, its own hand, the number of
cards in hidden zones, and cards it has looked at. The agents respect that boundary:

- From the true state an agent reads only `st.pending` (its own legal options) and what it
  may know; every look-ahead runs on `gcg_sim.engine.observe.determinize(st, player, seed)`,
  which redraws every card hidden from the player from the known decklists and replaces the
  game RNG (future shuffles and draws).
- Search seeds come from the agent seed and the decision counter, never from the state.
- Move keys (below) identify hand, trash, and deck cards by definition only when the
  deciding player knows them; otherwise by instance number, which carries no identity
  (instance numbers are assigned after the shuffle).
- `tests/ai/test_information_set.py` takes 25 mid-game states from greedy self-play, shuffles
  the hidden identities with `permute_hidden` (three permutations each: a different true state
  inside the same information set), and checks that a fresh MCTS agent and a fresh greedy
  agent with the same seed return the same move **and** the same search statistics.
  `test_negative_control_a_search_on_the_true_state_is_not_invariant` shows that the check
  can fail: with `determinize` replaced by a copy of the true state, the search's decision
  or statistics change on 23 of the 25 states.

## Architecture of the MCTS player (`mcts.py`)

Single-observer information-set MCTS (SO-ISMCTS) over *moves*:

1. **Moves.** `actions.canonical` merges legal actions that are the same move: two copies of a
   card in hand, or the same play paid with a different number of EX Resources (the
   representative spends the fewest EX Resources, which are single-use). A move key is
   `(kind, card, target, play-variant)` with hand/trash/deck cards named by definition.
2. **Dominated moves.** Before searching, the root drops moves that can only be worse than
   another legal move (`actions.prune_dominated`): declining a free "【Burst】Add this card to
   your hand", and attacking with a Unit whose AP is 0 or less when no attack-triggered
   effect or action-step trick could make the attack matter (greedy uses the same filter).
   It also drops Command plays and ability activations without effect (`noop.without_effect`):
   in one determinization, the move and "end the main phase / pass" are each followed by the
   deterministic policy to the next turn, and the move goes if both lines end in the same
   position apart from the card and Resources it spent and own cards it rested as a cost.
3. **Root.** Each iteration picks a root move by UCB1 with a progressive prior:
   `mean + c·sqrt(ln(avail)/visits) + w·prior/(visits+1)` (`c = exploration`, 0.3 in the
   standard preset; `w = prior_weight = 0.5`). Moves not yet tried go first, highest prior
   first. Priors are the playout policy's scores through a softmax (temperature 1.5).
4. **Common random numbers.** The *n*-th visit of every root move is played in the *n*-th
   determinization with the *n*-th rollout seed, so root moves are compared on identical
   worlds (paired sampling). This alone raised the win rate against greedy from 0.61 to 0.68
   in 192-game mirrored tests.
5. **Tree.** Below the root the walk chooses among the moves legal in the current world;
   children are keyed by `(mover, move key)` and keep availability counts, so a move that
   exists only in some worlds (the opponent holding a certain card) is judged fairly.
   Decisions with one legal move are applied without a node. One node is added per
   iteration.
6. **Rollout.** The heuristic playout policy (softmax over its scores, temperature 0.5 in the
   standard preset) plays both sides until `horizon_turns = 2` turn boundaries have passed:
   the rest of the current turn and the whole reply turn, stopping at the start of the turn
   after that.
7. **Leaf value** (for the searching player): a finished game scores `1 - 0.01·turns elapsed`
   for a win and `0.01·turns elapsed` for a loss (win sooner, lose later; 0.5 for a draw);
   otherwise `0.05 + 0.9·σ(score / value_scale)` with `value_scale = 2`, so a certain result
   always outranks an estimate and lopsided positions keep a usable gradient.
8. **Budget and choice.** A decision with *n* distinct moves gets
   `clamp(iterations_per_option·n, min_iterations, max_iterations)` iterations; the search stops
   early once the most-visited move cannot be overtaken. The move with the most visits is
   played (ties: higher mean, then option order).

Pre-game choices (`setup.py`):

- **Play/draw** (`CHOOSE_FIRST`): policy-vs-policy games from both choices on the same
  determinizations (`first_player_games` pairs); the agent goes second only when that is better
  by more than two standard errors of the paired difference. In MCTS self-play on the tuning
  decks Player One won 436 of 720 games (60.6%), and the agent goes first with the test decks.
- **Mulligan** (`REDRAW`): `opening_score` plays the hand out greedily over the first four
  turns (resources = turn number, plus Player Two's EX Resource) and weights early board
  development most. The agent redraws when the hand scores below 90% of the average of
  `mulligan_samples` fresh five-card hands drawn from the rest of the deck (the returned hand
  goes to the bottom, rule 6-2-1-6-1, so it cannot come back).

## Playout policy (`policy.py`)

The policy scores every option of every decision kind from the current board, without
look-ahead; search uses it for rollouts and priors, and greedy uses it (deterministically) to
finish the turn.

| Decision | Heuristic |
| --- | --- |
| Main phase | Play the largest affordable Unit; pair Pilots, strongly preferring one that links (more so on a Unit deployed this turn, which can then attack); replace an EX Base; play Commands and abilities; spend EX Resources only when needed. |
| Attacks | Lethal on an exposed player first; with more Blockers than attackers, play removal first. Attack a rested Unit when it dies and the attacker survives; avoid attacks into a Blocker that wins the fight; small penalty when the attacker will be exposed to a stronger enemy Unit on the reply turn. |
| Blocks | Must-block when the player would lose; otherwise value the Shield or Unit saved plus the fight outcome. |
| Action steps | Tricks during a battle are tried; passing is preferred in the end phase. |
| Targets (`SELECT`) | Reads the resolving effect's program to classify the choice as helping or harming the chosen cards, then picks the most valuable enemy card to harm or friendly card to help (least valuable for costs); rest-substitution picks cheaper cards. |
| `EXCESS` / `DISCARD` | Trash or discard the least valuable card. |
| `ARRANGE` | Keep valuable cards on top of your own deck. |
| `BURST` / `YES_NO` | Yes. |
| `CHOOSE_FIRST` / `REDRAW` | Go first / keep (search agents use the pre-game procedures above). |

## Evaluation (`evaluation.py`)

A logistic model of the probability that `player` wins. Each feature is
`side(player) − side(opponent)` except `tempo`; the vector is antisymmetric, so the two players'
estimates add up to 1. The search evaluates determinized states only, so the opponent's hand
composition is a sample from the player's information set.

| Feature | Meaning (per side) | Hand-set | Tuned |
| --- | --- | ---: | ---: |
| `tempo` | +1 if `player` is the turn player, −1 otherwise | 0.00 | −0.20 |
| `life` | Shields + Base HP / 3 | 0.35 | 0.43 |
| `danger` | max(0, 3 − life) | −0.30 | −0.44 |
| `can_kill` | next attack step has enough attackers (minus Blockers not answered by castable removal in hand) to break every Shield and the Base and hit the player | 1.00 | 0.53 |
| `ap` | total AP of Units | 0.10 | 0.11 |
| `hp` | total remaining HP of Units | 0.10 | 0.16 |
| `linked` | Link Units | 0.20 | 0.21 |
| `ready` | active (unrested) Units | 0.20 | 0.53 |
| `hand_units` | Unit cards in hand | 0.30 | 0.72 |
| `hand_pilots` | Pilot cards in hand | 0.25 | 0.49 |
| `hand_commands` | Command cards in hand | 0.25 | 0.37 |
| `hand_bases` | Base cards in hand | 0.25 | 0.57 |
| `level` | cards in the resource area | 0.15 | 0.16 |
| `bursts` | Shields × fraction of 【Burst】 cards among the cards this player cannot see (from the known decklist) | 0.30 | 0.55 |

The negative `tempo` weight reflects when positions are recorded: at the turn player's first
decision, after its draw and resource placement, which the hand and level features already
count.

### Feature selection

Candidate features were compared by five-fold cross-validated log-loss (folds by game) on
11,792 turn-start positions from 720 MCTS self-play games. Splitting the hand by card type
lowered log-loss from 0.515 to 0.491; features that did not help (Unit count, Blocker count,
EX Resources, deck size, exposed flag, maximum AP, trash size, a continuous attack margin)
were dropped.

### Tuning procedure (`tuning.py`)

1. Self-play: two MCTS agents with the small `tuning` budget (16–48 iterations) and the
   current weights play `--games` games on the three tuning decks (Zeon green/red, Wing
   green/blue, Tekkadan purple), cycling through all nine ordered deck pairings. The tuning
   decks are different from the AI test decks.
2. At the turn player's first main-phase decision of every turn, record its feature vector;
   label it 1 if that player won the game (draws dropped).
3. Fit an L2-regularised (λ = 1) logistic regression by Newton's method on the even-indexed
   games of all rounds so far; report log-loss on the current round's odd-indexed games
   before (current weights) and after (fitted weights).
4. Repeat with the fitted weights; the final weights are fitted on every position of every
   round.
5. Validate: a mirrored head-to-head match between the tuned and hand-set weights.

Every seed derives from `--seed` (default 2026); the result does not depend on `--workers`.
The run that produced `TUNED_WEIGHTS` was
`python -m gcg_sim.ai.tuning --games 540 --rounds 2 --workers 13 --validate 216 --validate-preset standard`
(13 min on 13 cores):

| Round | Positions | Held-out log-loss before | after |
| --- | ---: | ---: | ---: |
| 0 (hand-set weights) | 8,605 | 0.5452 | 0.5187 |
| 1 (round-0 weights) | 7,957 | 0.5218 | 0.5188 |

Final fit on 16,562 positions. Validation, standard preset, 216 mirrored games: tuned 112,
hand-set 104 (51.9%, Wilson 95% [0.452, 0.584]) — the tuned model predicts outcomes better,
and in play it is at least as strong as the hand-set one. An earlier round with the first
feature set (whole-hand count) showed a clearer effect: tuned against hand-set weights won
128 of 192 games (66.7%, [0.597, 0.730]).

A small version of the procedure runs in the test suite
(`tests/ai/test_tuning.py`: three games, one round, run twice and compared bit for bit), and
from the command line:

```bash
uv run python -m gcg_sim.ai.tuning --games 2 --rounds 1
```

## Presets and measured throughput

| Preset | Iterations per decision | Other |
| --- | --- | --- |
| `fast` | clamp(8·n, 20, 80) | exploration 0.15, rollout temperature 0.3, 24 play/draw game pairs, 96 mulligan samples |
| `standard` (default) | clamp(32·n, 80, 320) | exploration 0.3, rollout temperature 0.5, 24 play/draw game pairs, 96 mulligan samples |
| `strong` | clamp(128·n, 320, 1280) | exploration 0.15, rollout temperature 0.3, 48 play/draw game pairs, 192 mulligan samples |
| `expert` | clamp(256·n, 640, 2560) | exploration 0.1, rollout temperature 0.2, 96 play/draw game pairs, 384 mulligan samples |

`fast` is for screening deck variants: it plays several times more games per core-minute
than `standard` and gives matchup estimates within a few points of it, but it is a weaker
player, so confirm a screening result with `standard` or stronger. `expert` doubles
`strong`'s search but was not measurably stronger than it; use it to check that a conclusion
does not change with more search. How the two were calibrated, and what they cost and win, is
in [Calibrating the presets](#calibrating-the-presets).

Measured on commit 6be0ea8 with `gcg_sim.ai.throughput` (tuning decks 0 and 1, MCTS against
itself, 48 games per run, seed 0, 8 worker processes on an otherwise idle 14-core
Apple-silicon machine; core-seconds = wall time × workers). `fast` and `standard` were run
twice:

| Preset | Core-seconds per game | Games per core-minute | Relative to `standard` | Mean turns |
| --- | ---: | ---: | ---: | ---: |
| `fast` | 2.8 | 21.27, 21.40 | 3.74× | 15.4 |
| `standard` | 10.5, 10.6 | 5.73, 5.68 | 1 | 15.4 |
| `strong` | 34.5 | 1.74 | 0.31× | 16.2 |
| `expert` | 59.9 | 1.00 | 0.18× | 16.1 |

So `fast` plays 3.7× as many games per core-minute as `standard`, `strong` costs 3.3× as much
per game as `standard`, and `expert` 1.7× as much as `strong`. On the example decks the
`fast` fidelity runs below were 3.7× faster per match than the `standard` runs on the same
400 deals (on a loaded machine, so only the ratio is meaningful).

Earlier measurements on the integrated engine (commit 013ea8f, same machine and workers):

| Measurement | Games | Mean turns | Wall time | Core-seconds per game | Games per core-minute |
| --- | ---: | ---: | ---: | ---: | ---: |
| `gcg_sim.ai.throughput --preset standard` (tuning decks, MCTS vs MCTS) | 24 | 14.3 | 38.0 s | 12.7 | 4.74 |
| `gcg_sim.ai.throughput --preset strong` (tuning decks, MCTS vs MCTS) | 16 | 15.0 | 121.8 s | 60.9 | 0.99 |
| `gcg-sim benchmark` red-green-zeon vs blue-white-federation, 16 BO3 matches, standard | 44 | 15.7 | 100.2 s | 18.2 | 3.29 |
| `gcg-sim benchmark` red-green-zeon mirror, 32 BO3 matches, standard | 81 | — | 107.9 s | 10.7 | 5.63 |

So 100 BO3 matches of the example decks (about 250–275 games) take about 6–11 minutes on 8
cores with `standard`, well inside the 30-minute target; `strong` is about 3–5× slower.
During development, on the AI branch's older engine and the AI test decks (Federation vs
SEED, 13 workers), `standard` measured 1.65 games per core-minute and `strong` 0.44.
About 90% of search time is spent inside the engine's `apply`, two thirds of it recomputing
derived characteristics (`engine/view.py:_compute`); the playout policy and evaluation take
under 10%.

Measure with (wall-clock is read only to report speed):

```bash
uv run python -m gcg_sim.ai.throughput --preset standard --games 1
```

## Strength

Acceptance test `tests/slow/test_ai_strength.py` (default `standard` preset, test decks
Federation blue/white and SEED red/white from `tests/ai/decks.py`, 400 games per opponent in
mirrored blocks of four so each deal is played with the MCTS agent on both decks and in both
seats). Wins are counted; draws count as non-wins, as in the benchmark reports. Run on the
integrated engine with dominated-move and no-effect pruning (commit 15ae2eb, 12 worker
processes):

| Opponent | MCTS wins | Draws | Win rate | Wilson 95% interval | Mean turns |
| --- | ---: | ---: | ---: | --- | ---: |
| random | 400 / 400 | 0 | 1.000 | [0.990, 1.000] | 11.5 |
| greedy | 289 / 400 | 0 | 0.723 | [0.677, 0.764] | 17.4 |

(Earlier runs of the same test against greedy: 281/400 and 287/400 with earlier versions of
the no-effect check,
275/400 with dominated-move pruning only, 276/400 before any pruning on commit 013ea8f, and
272/400 on the AI branch's older engine; the differences are within sampling noise.)

Greedy is a demanding baseline: it shares the tuned evaluation and the playout policy, and
it compares its moves on a single world (so its comparisons have no sampling noise).

`strong` against `standard` (measured during development on the older engine; test decks,
mirrored): 58 of 96 games (seed 7) and 103 of 192
(seed 8), together 161 of 288 = 0.559, Wilson 95% [0.501, 0.615]. Simply giving the standard
knobs 4× the iterations did not help (46 of 96, 0.479), nor did a three-turn horizon at 2×
(51 of 96); what helps a large budget is spending it on the best lines (exploration 0.15) with
less noisy rollouts (temperature 0.3). Those two settings at the standard budget were not
better than the standard ones (91 of 192, 0.474).

`fast` against greedy: 255 of 400 (0.638, Wilson 95% [0.589, 0.683]). `expert` against
`strong`: 605 of 1200 (0.504, [0.476, 0.532]), not measurably stronger. Both were measured on
held-out decks; see [Calibrating the presets](#calibrating-the-presets).

Tactical puzzles (`tests/ai/test_puzzles.py`, each solved by the standard preset with two
agent seeds against an MCTS defender): lethal on an exposed player; lethal by going face
instead of taking a free kill; rest the Blocker with removal before the lethal attack; pair
the linking Pilot so a Unit deployed this turn can attack for lethal; choose the Pilot and
Unit that link; order attacks around a known 【Burst】 (attack with the Unit the Burst could
rest first); hold removal for next turn's lethal instead of wasting it (the agent even casts
it in the opponent's end step so the Blocker stays rested); take a favourable trade; do not
attack into a bigger Blocker; block to survive; block when the Blocker wins the fight; use an
action-step trick (AP−3) to survive; aim removal at the most valuable Unit it can destroy;
trash the weakest Unit to the battle-area limit; take a free card from 【Burst】; mulligan a
hand of uncastable cards and keep a hand with early plays; go first.

## Calibrating the presets

`fast` and `expert` are built only from the `SearchConfig` knobs above: the search code is
the same for every preset, and `standard` and `strong` are unchanged (the same seeds give
identical games). Candidates were screened on the tuning decks
(`gcg_sim.ai.tuning.TUNING_DECKS`) and confirmed on held-out decks (the test decks in
`tests/ai/decks.py` and the three `examples/decks`) with fresh seeds, under a plan written
before the first confirmation game. The harness is `scripts/ai_calibrate.py`: `match` plays
mirrored blocks of four games between two agents (each deal with each agent on each deck and
in each seat), and `paired` compares two BO1 `gcg-sim benchmark` runs played on the same
deals. Every run's command and raw output is kept with the run; costs measured during
screening were taken on a loaded machine and only rank the candidates, while the throughput
table below was measured on an idle one.

### fast

Targets: at least 3× `standard`'s games per core-minute, a clear win over greedy, and matchup
estimates close to `standard`'s.

Screening on the tuning decks. "vs standard": 192 mirrored games (seed 3201). "Matchups": the
first deck's wins in 96 self-play games for each of the three tuning pairings (seed 3101), to
compare with `standard`'s 52, 51 and 39 on the same deals. The candidates used 8 play/draw
game pairs and 48 mulligan samples.

| Candidate | Iterations | Other knobs | vs standard (Wilson 95%) | Matchups | Core-s per game |
| --- | --- | --- | --- | --- | ---: |
| fast-c | clamp(6·n, 16, 64) | as standard | 76/192 = 0.396 [0.329, 0.466] | 62, 56, 46 | 2.4 |
| fast-a | clamp(8·n, 20, 80) | as standard | 84/192 = 0.438 [0.369, 0.508] | 54, 59, 46 | 3.1 |
| fast-b | clamp(10·n, 24, 96) | as standard | 87/192 = 0.453 [0.384, 0.524] | 57, 51, 37 | 3.7 |
| fast-e | clamp(8·n, 20, 80) | rollout temperature 0.3 | 87/192 = 0.453 [0.384, 0.524] | 53, 67, 43 | 3.1 |
| fast-d | clamp(8·n, 20, 80) | exploration 0.15, rollout temperature 0.3 | 96/192 = 0.500 [0.430, 0.570] | 53, 54, 49 | 2.8 |
| `standard` | clamp(32·n, 80, 320) | | | 52, 51, 39 | 12.3 |

`fast` is fast-d with `standard`'s pre-game sample counts (24 play/draw game pairs, 96
mulligan samples), and that is the configuration confirmed below. The mulligan takes 2–10 ms,
but the play/draw simulation takes about 0.4 s, about 11% of a `fast` game. With 8 pairs
`fast` would be about 8% cheaper, and the answer ("go first") was the same in every call
measured; that change is proposal 5 in
[`research/AI_IMPROVEMENT_PROPOSALS.md`](research/AI_IMPROVEMENT_PROPOSALS.md).

Confirmation (commit 988cd15, held-out decks, fresh seeds):

- Against greedy (test decks, 400 mirrored games, seed 7102): 255/400 = 0.638, Wilson 95%
  [0.589, 0.683]. `standard` won 289/400 in the acceptance test above.
- Matchup fidelity: `gcg-sim benchmark DUT BENCH --format bo1 --matches 400 --seed 8101`, once
  with `--ai-preset fast` and once with `standard`, then
  `scripts/ai_calibrate.py paired FAST_RUN STANDARD_RUN`. Δ is the change in the deck under
  test's game win rate, with a 95% interval from the per-match differences on the same deals:

  | Deck under test vs benchmark deck | `fast` | `standard` | Δ (95% interval) |
  | --- | ---: | ---: | --- |
  | blue-white-federation vs red-green-zeon | 0.463 | 0.500 | −0.038 [−0.088, +0.013] |
  | blue-white-federation vs purple-white-tekkadan | 0.513 | 0.523 | −0.010 [−0.063, +0.043] |
  | purple-white-tekkadan vs red-green-zeon | 0.305 | 0.343 | −0.038 [−0.086, +0.011] |

  Every interval contains 0 and no |Δ| exceeds 0.04. All three differences have the same
  sign, though: red-green-zeon gained under `fast` in both of its pairings, and the pooled
  difference over the 1200 paired games is −0.028 [−0.058, +0.001]. Treat a `fast` screening
  difference of a few points as a lead to confirm with `standard`, not as a result.

### expert

Targets: stronger than `strong` on held-out decks (required: a win rate above 0.50 over at
least 192 games; goal: a Wilson lower bound above 0.50), every puzzle solved, and at most 4×
`strong`'s cost per game.

Screening on the tuning decks: 96 mirrored games against `strong` per candidate (seed 4101),
all with 96 play/draw game pairs and 384 mulligan samples. The cost is that of an
expert-vs-strong game on a loaded machine.

| Candidate | Iterations | Other knobs | vs strong (Wilson 95%) | Core-s per game |
| --- | --- | --- | --- | ---: |
| expert-a | clamp(256·n, 640, 2560) | as strong | 50/96 = 0.521 [0.422, 0.618] | 66 |
| expert-b | clamp(256·n, 640, 2560) | exploration 0.1, rollout temperature 0.2 | 52/96 = 0.542 [0.442, 0.638] | 57 |
| expert-c | clamp(512·n, 1280, 5120) | as strong | 49/96 = 0.510 [0.412, 0.608] | 87 |

The three are within noise of each other; `expert` is expert-b, nominally the strongest and
the cheapest.

Confirmation (commit 6be0ea8, held-out decks, seed 9101, 300 mirrored games per pairing
against `strong`):

| Pairing | `expert` wins | Wilson 95% |
| --- | ---: | --- |
| test decks: Federation vs SEED | 149/300 = 0.497 | [0.441, 0.553] |
| blue-white-federation vs purple-white-tekkadan | 153/300 = 0.510 | [0.454, 0.566] |
| blue-white-federation vs red-green-zeon | 152/300 = 0.507 | [0.450, 0.563] |
| purple-white-tekkadan vs red-green-zeon | 151/300 = 0.503 | [0.447, 0.560] |
| all four | 605/1200 = 0.504 | [0.476, 0.532] |

`expert` meets the required bar but is **not measurably stronger** than `strong`: the
interval allows anything from 2.4 points worse to 3.2 points better. Two and four times
`strong`'s budget bought little on the tuning decks too. This matches the slow budget scaling
under [Known weaknesses](#known-weaknesses): above `strong`, the playout policy, the
evaluation and the two-turn horizon limit play more than the iteration count does, and no
`SearchConfig` setting screened here moved that limit. Use `strong` for confirmation runs, and
`expert` to check that a conclusion does not change with twice the search. `expert` solves
every puzzle in `tests/ai/test_puzzles.py`, which runs each puzzle with `standard` and with
`expert`.

### Ideas that did not help

- Fewer iterations than clamp(8·n, 20, 80) (fast-c): clearly weaker than `standard` and up to
  10 points off its matchup estimates.
- A lower rollout temperature alone at the `fast` budget (fast-e): it needs the lower
  exploration constant as well (fast-d), as `strong` does.
- Four times `strong`'s budget (expert-c): no better than twice (49/96 against 50/96), at
  about 1.3 times the cost.

### Reproducing the calibration

Each run's exact command is stored next to its output. The screening and confirmation runs
use `scripts/ai_calibrate.py match` with a challenger such as
`mcts:strong:name=expert-b,iterations_per_option=256,min_iterations=640,max_iterations=2560,first_player_games=96,mulligan_samples=384,exploration=0.1,rollout_temperature=0.2`
(a preset with fields replaced; the `name` seeds its decisions). The `expert` confirmation is
`scripts/ai_calibrate.py match --challenger mcts:expert --opponent mcts:strong --pairings test example --games 300 --seed 9101 --workers 12`,
and the `fast` fidelity check runs `gcg-sim benchmark` twice and compares the two runs with
`scripts/ai_calibrate.py paired`. A four-game smoke test of the harness:

```bash
uv run python scripts/ai_calibrate.py match --challenger mcts:fast --opponent greedy \
    --pairings test --games 4 --seed 1 --workers 2
```

## Known weaknesses


- **Horizon.** Rollouts stop at the start of the turn after the reply turn; plans that pay
  off later rely on the evaluation (for example `can_kill` counts castable removal in hand).
- **Policy bias.** Rollouts and priors come from a hand-written policy: it plays Commands
  somewhat eagerly, judges modal choices and activated abilities coarsely, and models the
  opponent as playing the same way.
- **Strategy fusion.** Determinized search can assume it will "know" hidden cards when a
  later decision is made; common random numbers at the root reduce, but do not remove, this.
- **Linear evaluation.** The model has no interaction terms (for example between life and
  the opponent's board) and is fitted to self-play of the tuning budget.
- **Budget scaling.** Against greedy the win rate grows slowly with iterations: before paired
  sampling, 4× the iterations moved it from 0.70 to 0.77, and with paired sampling 2.5× left
  it at 0.69–0.71 (192 games each). The standard preset therefore spends a moderate budget;
  the strong preset is for confirmation runs.
- **Target choice.** The root drops a Command play or an ability activation whose
  deterministic continuation reaches the next turn in the position of idling, apart from
  what it spent (see step 2); this removed the "AP-3 on a Unit that cannot battle", "rest a
  rested Unit" and "pay a cost for an effect that does nothing" plays the replay review
  found. Choosing the less useful of two legal targets for an effect remains possible when
  the search values are within sampling noise: target choices have no "do nothing" option
  to compare against.
- **Engine-bound speed.** In a profiled standard game, 91% of the time was inside the
  engine's `apply` and 69% in `engine/view.py:_compute` (338,330 recomputations for 92,691
  applied actions; 20.8 million `_host_abilities` calls, mostly for cards in hand and trash).
  Caching per-definition ability lists by zone in the engine would speed search up directly.
