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
  game RNG (future shuffles and draws). A determinization is a copy: it never writes into the
  true game (`tests/ai/test_search_side_effects.py` checks that the true state's digest is
  unchanged by `determinize`, `permute_hidden` and a whole MCTS decision).
- Search seeds come from the agent seed and the decision counter, never from the state.
- Move keys (below) identify hand, trash, and deck cards by definition only when the
  deciding player knows them; otherwise by instance number, which carries no identity
  (instance numbers are assigned after the shuffle).
- In the tree, an opponent's action-step pass keeps the walk in the same node: the engine
  passes silently for a player without options, so the searcher's later nodes would otherwise
  depend on whether the opponent's hidden hand gave it an option (`observation_keys`).
- `tests/ai/test_information_set.py` takes 25 mid-game states from greedy self-play, shuffles
  the hidden identities with `permute_hidden` (three permutations each: a different true state
  inside the same information set), and checks that a fresh MCTS agent and a fresh greedy
  agent with the same seed return the same move **and** the same search statistics.
  `test_negative_control_a_search_on_the_true_state_is_not_invariant` shows that the check
  can fail: with `determinize` replaced by a copy of the true state, the search's decision
  or statistics change on most of the states. The same procedure runs with each group of
  new settings switched on separately (`test_root_moves.py`, `test_information_set_policy.py`,
  `test_search_options.py`).

## Architecture of the MCTS player (`mcts.py`)

Single-observer information-set MCTS (SO-ISMCTS) over *moves*:

1. **Moves.** `actions.canonical` merges legal actions that are the same move: two copies of a
   card in hand, or the same play paid with a different number of EX Resources (the
   representative spends the fewest EX Resources, which are single-use). A move key is
   `(kind, card, target, play-variant)` with hand/trash/deck cards named by definition. At the
   root, attacks, blocks, choices and pairings that differ only in which of two
   interchangeable field cards they use become one move, so the search does not split its visits
   between them. Two cards are interchangeable when their public fingerprints match:
   definition, state, derived stats, and the same this-turn history, once-per-turn records and
   lasting effects, with the card itself written as "self"; a card that a battle, a resolving
   effect or a waiting trigger names is never merged (`merge_equivalent_moves`).
2. **Dominated moves.** Before searching, the root drops moves that can only be worse than
   another legal move (`actions.prune_dominated`):
   - declining a free "【Burst】Add this card to your hand";
   - attacking with a Unit whose AP is 0 or less, unless declaring the attack (in one
     determinization) queues a trigger the player controls, judged by the engine's own
     trigger tests, or opens an Action Command or ability the player can afford and aim in
     that battle (`strict_attack_prune`; greedy keeps the older, looser test);
   - choosing an already-rested enemy Unit for an effect whose only use of the choice is to
     rest it, unless something reacts to a Unit being rested, a delayed effect is armed, an
     "If you do" step follows, or a once-per-turn effect has not acted yet
     (`prune_noop_rest_targets`).

   It also drops Command plays and ability activations without effect
   (`noop.without_effect`): in one determinization, the move and "end the main phase / pass"
   are each followed by the deterministic policy to the next turn, and the move goes if both
   lines end in the same position apart from the card and Resources it spent and own cards it
   rested as a cost. Two refinements keep useful moves: the first choice inside the move's
   resolution is tried with every option, and the move stays if one of them reaches a
   position the evaluation scores above idling (`noop_branch_choices`; the policy alone would
   aim a Rest at an already-rested Unit), and Resources placed, set active or removed beyond
   the move's own payment count as an effect (`noop_resource_aware`).
3. **Root.** Each iteration picks a root move by UCB1 with a progressive prior:
   `mean + c·sqrt(ln(avail)/visits) + w·prior/(visits+1)` (`c = exploration`, 0.3 in the
   standard preset; `w = prior_weight = 0.5`). Moves not yet tried go first, highest prior
   first. Priors are the playout policy's scores through a softmax (temperature 1.5).
4. **Common random numbers.** The *n*-th visit of every root move is played in the *n*-th
   determinization with the *n*-th rollout seed, so root moves are compared on identical
   worlds (paired sampling). This alone raised the win rate against greedy from 0.61 to 0.68
   in 192-game mirrored tests. Each world is determinized once per search and copied for
   every visit that plays in it.
5. **Tree.** Below the root the walk chooses among the moves legal in the current world;
   children are keyed by `(mover, decision kind, move key)` and keep availability counts, so
   a move that exists only in some worlds (the opponent holding a certain card) is judged
   fairly, and a card's main-phase and action-step plays stay apart. Decisions with one legal
   move are applied without a node. One node is added per iteration.
6. **Rollout.** The heuristic playout policy (softmax over its scores, temperature 0.5 in the
   standard preset) plays both sides until `horizon_turns = 2` turn boundaries have passed:
   the rest of the current turn and the whole reply turn, stopping at the start of the turn
   after that. Every non-terminal leaf is the turn player's first main-phase decision of
   that turn, the point at which the evaluation is fitted (measured on 250,680 leaves).
7. **Leaf value** (for the searching player): a finished game scores `1 - 0.01·turns elapsed`
   for a win and `0.01·turns elapsed` for a loss (win sooner, lose later; 0.5 for a draw);
   otherwise `0.05 + 0.9·σ(score / value_scale)`, so a certain result always outranks an
   estimate (`SearchConfig` rejects a horizon at which it would not). The presets use
   `value_scale = 1`, the calibrated log-odds; the earlier 2 halved them, so positions that
   win 95% of the time were valued about 0.76.
8. **Budget and choice.** A decision with *n* distinct moves gets
   `clamp(iterations_per_option·n, min_iterations, max_iterations)` iterations; the search stops
   early once the most-visited move cannot be overtaken. For an effect target choice
   (cards to choose, trash, discard or order), when the budget runs out while the move to
   play is within two paired standard errors of another (the visits two moves share are
   pairs, by step 4), the search goes on one budget at a time up to three budgets
   (`target_extension`, `target_extension_z`). The move with the most visits is played
   (ties: higher mean, then option order).

Pre-game choices (`setup.py`):

- **Play/draw** (`CHOOSE_FIRST`): policy-vs-policy games from both choices on the same
  determinizations (`first_player_games` pairs); the agent goes second only when that is better
  by more than two standard errors of the paired difference. In MCTS self-play on the tuning
  decks Player One won 436 of 720 games (60.6%), and the agent goes first with the test decks.
- **Mulligan** (`REDRAW`): `opening_score` plays the hand out greedily over the first four
  turns (resources = turn number, plus Player Two's EX Resource) and weights early board
  development most. The agent redraws when the hand scores below 90% of the average of
  `mulligan_samples` fresh five-card hands drawn from the rest of the deck. The returned hand
  goes to the bottom and the deck is shuffled after the new five are drawn (rule 6-2-1-6-1),
  so a returned card is never in the new hand but can come back later (in the Shields about
  half the time); sampling the new hand from the rest of the deck is the right model.

## Playout policy (`policy.py`)

The policy scores every option of every decision kind from the current board, without
look-ahead; search uses it for rollouts and priors, and greedy uses it (deterministically) to
finish the turn. A search configuration can change some of its rules (`PolicyKnobs`, set by
the `policy_*` fields of `SearchConfig`); greedy always uses the original rules.

| Decision | Heuristic |
| --- | --- |
| Main phase | Play the largest affordable Unit; pair Pilots, strongly preferring one that links (more so on a Unit deployed this turn, which can then attack); replace an EX Base; play Commands and abilities; spend EX Resources only when needed. |
| Attacks | Lethal on an exposed player first; with more Blockers than attackers, play removal first. Attack a rested Unit when it dies and the attacker survives; avoid attacks into a Blocker that wins the fight; small penalty when the attacker will be exposed to a stronger enemy Unit on the reply turn. With `policy_zero_ap_face`, a face attack by a Unit that can neither deal damage nor matter through a trigger or a battle trick scores below ending the main phase, and such a Unit does not count as an attacker against the Blockers. |
| Blocks | Must-block when the player would lose; otherwise value the Shield or Unit saved plus the fight outcome. |
| Action steps | Tricks during a battle are tried; passing is preferred in the end phase. |
| Targets (`SELECT`) | Reads the resolving effect's program to classify the choice as helping or harming the chosen cards, then picks the most valuable enemy card to harm or friendly card to help (least valuable for costs); rest-substitution picks cheaper cards. With `policy_rule_polarity_table`, a granted rule is signed by its kind (protection such as "can't receive damage" helps, locks such as "stays rested in the start phase" harm; the old rule called every "can't" harmful) and a rule granted to your own Units for this battle goes to the Unit in the battle. |
| `EXCESS` / `DISCARD` | Trash or discard the least valuable card. |
| `ARRANGE` | Keep valuable cards on top of your own deck. |
| `BURST` | Yes, with score `policy_burst_yes_score` against 0 for no: at the presets' 4.0 a rollout at temperature 0.5 declines a Burst 0.03% of the time instead of 12% (no Burst in the example or test decks should be declined; the root decision is searched either way). |
| `YES_NO` | Yes (score 1 against 0, so a rollout at temperature 0.5 says no about 12% of the time). |
| `CHOOSE_FIRST` / `REDRAW` | Go first / keep (search agents use the pre-game procedures above). |

## Evaluation (`evaluation.py`)

A logistic model of the probability that `player` wins. The search evaluates determinized
states only, so the opponent's hand composition is a sample from the player's information
set. `Weights` carry a feature set (`evaluation.FEATURE_SETS`):

- **`roles`** (the default, `DEFAULT_WEIGHTS = ROLE_WEIGHTS`, 17 features): computed from the
  turn player's point of view, then negated for the other player. It keeps 12 of the base
  differences below (all but `tempo` and `can_kill`), uses an intercept for the turn player,
  and keeps the two sides' threats apart instead of forcing them to be mirror images:
  `can_kill_turn_player`, `can_kill_other`, and `inv_race_turn_player`, `inv_race_other`, the
  inverse of the number of turns that side needs to win if both boards stayed as they are
  (every Unit with AP attacks the player each turn, each of the defender's Blockers stops the
  largest blockable attack, the Base absorbs hits at the connecting attacks' mean AP, then
  every Shield and one more hit; capped at 6 turns). Weights of note: can_kill turn player
  +1.27, can_kill other −0.42, 1/race turn player +3.25, 1/race other −0.85, intercept −0.75.
- **`base`** (14 features, `TUNED_WEIGHTS`, the pre-2026-10 default kept for
  `baseline-standard`): each feature is `side(player) − side(opponent)` except `tempo`, so the
  two players' estimates add up to 1.

| Feature (base) | Meaning (per side) | Hand-set | Tuned |
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

Candidate features were compared by five-fold cross-validated log-loss (folds by game).
- For the base set, on 11,792 turn-start positions from 720 MCTS self-play games: splitting
  the hand by card type lowered log-loss from 0.515 to 0.491; features that did not help (Unit
  count, Blocker count, EX Resources, deck size, exposed flag, maximum AP, trash size, a
  continuous attack margin) were dropped.
- For the role set (2026-10), on 7,941 positions from 540 standard-preset self-play games on
  the tuning decks: −0.0090 nats against `TUNED_WEIGHTS` out of fold (fold SE about 0.0035),
  −0.0074 on positions from test-deck games (never in the fit; cluster SE 0.017) and +0.0030
  (SE 0.003, slightly worse) on example-deck games; at the tuning budget a role-feature agent
  scored 384/800 = 0.480 [0.446, 0.515] against a base refit, so the game-level evidence is the
  bundle measurement under "Strength"; a refit of the 14 base
  weights on the same data was worse out of fold (+0.0025) and did not ship. Interaction terms
  (life × enemy threat), a sharper lethal estimate, a held-trick feature and dropping
  `bursts` were measured and gained nothing (within ±0.001).
- `value_scale`: against outcomes, the leaf mapping at `value_scale = 2` is underconfident
  (leaves valued 0.65–0.8 win 91% of the time); among 1, 1.5 and 2, 1 has the lowest log-loss
  on every source (−0.026 to −0.040 nats against 2).

### Tuning procedure (`tuning.py`)

1. Self-play: two MCTS agents with the small `tuning` budget (16–48 iterations), or a preset's
   search settings with `--preset`, and the current weights play `--games` games on the three
   tuning decks (Zeon green/red, Wing green/blue, Tekkadan purple), cycling through all nine
   ordered deck pairings. The tuning decks are different from the AI test decks.
2. At the turn player's first main-phase decision of every turn, record its feature vector
   (`--feature-set base|roles`); label it 1 if that player won the game (draws dropped).
3. Fit an L2-regularised (λ = 1) logistic regression by Newton's method on the even-indexed
   games of all rounds so far; report log-loss on the current round's odd-indexed games
   before (current weights) and after (fitted weights).
4. Repeat with the fitted weights; the final weights are fitted on every position of every
   round.
5. Validate: a mirrored head-to-head match between the tuned and hand-set weights.

Every seed derives from `--seed` (default 2026); the result does not depend on `--workers`.
`--start hand|tuned` chooses the weights round 0 plays with; the defaults reproduce the
original procedure byte for byte. The base weights `TUNED_WEIGHTS` came from
`python -m gcg_sim.ai.tuning --games 540 --rounds 2 --workers 13 --validate 216 --validate-preset standard`
(13 min on 13 cores):

| Round | Positions | Held-out log-loss before | after |
| --- | ---: | ---: | ---: |
| 0 (hand-set weights) | 8,605 | 0.5452 | 0.5187 |
| 1 (round-0 weights) | 7,957 | 0.5218 | 0.5188 |

Final fit on 16,562 positions. Validation, standard preset, 216 mirrored games: tuned 112,
hand-set 104 (51.9%, Wilson 95% [0.452, 0.584]). `ROLE_WEIGHTS` came from
`python -m gcg_sim.ai.tuning --preset baseline-standard --start tuned --feature-set roles --games 540 --rounds 1 --seed 35001`
(self-play with the pre-change standard search and `TUNED_WEIGHTS`; `--preset` takes the
presets, `tuning`, and the pre-change `baseline-<preset>`).

A small version of the procedure runs in the test suite
(`tests/ai/test_tuning.py`: three games, one round, run twice and compared bit for bit), and
from the command line:

```bash
uv run python -m gcg_sim.ai.tuning --games 2 --rounds 1
```

## Presets and measured throughput

| Preset | Use | Iterations per decision | Other |
| --- | --- | --- | --- |
| `fast` | screening deck variants | clamp(7·n, 16, 64) | exploration 0.15, rollout temperature 0.3, 8 play/draw game pairs, 96 mulligan samples |
| `standard` (default) | benchmarks | clamp(64·n, 160, 640) | exploration 0.3, rollout temperature 0.5, 24 play/draw game pairs, 96 mulligan samples |
| `strong` | confirmation runs | clamp(128·n, 320, 1280) | exploration 0.15, rollout temperature 0.3, 48 play/draw game pairs, 192 mulligan samples |

All three use the same settings otherwise: the root pruning and no-effect refinements, the
policy settings (Burst score 4.0, 0-AP face attacks, the rule table), close target extension
(3 budgets, 2 paired standard errors), observation keys, `value_scale = 1` and the role
evaluation. The pre-change presets stay available as `gcg_sim.ai.baseline.BASELINE_PRESETS`
(`baseline-standard`, `baseline-strong`) with `BASELINE_WEIGHTS`.

Measured on commit fc29d5f, 8 worker processes on an otherwise idle 14-core Apple M4 Pro, two
alternating rounds that agreed within 1% (core-seconds = wall time × workers;
`gcg_sim.ai.throughput --games 48 --workers 8 --seed 11`, 96 games for `fast`, 16 for
`strong`; tuning decks, MCTS vs MCTS):

| Measurement | Games per core-minute | Core-seconds per game | Mean turns |
| --- | ---: | ---: | ---: |
| `fast` | 28.2 | 2.1 | 15.3 |
| `standard` | 2.90 | 20.7 | 15.9 |
| `strong` | 1.82 | 33.0 | 14.2 |
| pre-change `standard` (`main`, bc38ab4), same machine and protocol | 4.72 | 12.7 | 16.2 |

`fast` plays 9.7× the games per core-minute of `standard`. 100 BO3 matches of
red-green-zeon against blue-white-federation (`gcg-sim benchmark … --matches 100 --seed 1
--workers 8`) took 13.0 minutes (251 games) with `standard`, against 7.4 minutes (260 games)
for the pre-change AI, inside the 30-minute target. The engine speedups of 2026-10
(output-identical: same games, decision logs and results) made the old AI 1.26× faster on
the same throughput measurement; `standard` spends that and more on twice the iterations.
In a profiled game on the pre-change engine, 80–83% of search time was inside the engine's
`apply`; the playout policy and the evaluation take most of the rest.

Measure with (wall-clock is read only to report speed):

```bash
uv run python -m gcg_sim.ai.throughput --preset fast --games 2
```

## Comparing AI versions

`python -m gcg_sim.ai.compare CHALLENGER OPPONENT` plays mirrored blocks of four (each deal
with the challenger on both decks and in both seats) and prints the challenger's wins with a
Wilson 95% interval; draws count as non-wins. An agent is `random`, `greedy`, a preset, or a
baseline preset (`baseline-standard`: the AI as released before the 2026-10 changes), with
`:field=value,...` to change `SearchConfig` fields or `weights=default|hand|baseline|roles`.
Decks: `tests` (the AI test decks, from the repository root), `tuning`, `examples`, files, or
`module:attr`. `--out` saves every game for paired analyses.

```bash
uv run python -m gcg_sim.ai.compare fast random --games 12 --seed 1 --decks tuning
```

`tests/ai/test_baseline.py` replays one game recorded on bc38ab4 with `baseline-standard`,
moves and every logged search statistic included, so the baseline cannot drift.

## Strength

All results count wins; draws count as non-wins, as in the benchmark reports. "Mirrored"
means blocks of four games per deal (each agent on each deck in each seat).

**New `standard` against the pre-change `standard`** (`compare standard baseline-standard`,
test decks, held-out seed 90001, run once): **925 of 1,600 = 0.578, Wilson 95% [0.554,
0.602]**. On the example decks (240 games, three pairings, seed 90005):
132 of 240 = 0.550 [0.487, 0.612] (80 games per pairing: blue-white-federation and purple-white-tekkadan 45, blue-white-federation and red-green-zeon 46, purple-white-tekkadan and red-green-zeon 41).

Where the gain comes from (development seed 36005, 800 mirrored games each against
`baseline-standard`, same deals):

| Agent | Wins | Rate | Wilson 95% |
| --- | ---: | ---: | --- |
| new settings at twice the iterations (the new `standard`) | 472 / 800 | 0.590 | [0.556, 0.624] |
| new settings at the old iterations | 435 / 800 | 0.544 | [0.509, 0.578] |
| old settings at twice the iterations | 433 / 800 | 0.541 | [0.507, 0.576] |

So better search and more iterations contribute about equally (about 4 points each) and
roughly add up. After these measurements the final review found that the no-op Rest-target
prune ignored "while this Unit is rested" effects; the fix (5a6f226) adds a check that never
fired in 32 games on the test and example decks, which hold no such card, so it does not change
the games behind these numbers. Settings that were measured and not adopted are listed under
[Known weaknesses](#known-weaknesses).

Acceptance test `tests/slow/test_ai_strength.py` (default preset, test decks Federation
blue/white and SEED red/white from `tests/ai/decks.py`, 400 mirrored games per opponent):

| Opponent | MCTS wins | Win rate | Wilson 95% interval | Mean turns |
| --- | ---: | ---: | --- | ---: |
| random | 400 / 400 | 1.000 | [0.990, 1.000] | 11.7 |
| greedy | 292 / 400 | 0.730 | [0.684, 0.771] | 18.4 |

Before the change the same test gave 400/400 against random and 289/400 (0.723, [0.677,
0.764]) against greedy. Greedy uses the default evaluation, so it now plays with the role
weights too; these results are floors, not the measure of improvement.

`strong` against `standard` (400 mirrored games, test decks, seed 8401): 209 of 400 = 0.523
[0.474, 0.571]. On the same deals the pre-change `strong` beat the pre-change `standard`
230 of 400 = 0.575 [0.526, 0.623]: `standard` gained more than `strong`, which keeps only
twice `standard`'s iterations. Earlier measurements on the older engine: simply giving the
old standard knobs 4× the iterations did not help (46 of 96), nor did a three-turn horizon at
2× (51 of 96).

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
hand of uncastable cards and keep a hand with early plays; go first. Puzzles added with the
2026-10 settings, each failing with the pre-change configuration: rest the Blocker while a
bigger enemy Unit is already rested (the no-effect check used to prune the removal; 0/16 →
16/16 agent seeds), do not rest an already-rested Unit (5/16 and 13/16 → 16/16), aim a ping
at a Unit that keeps the damage rather than one whose <Repair> heals it and at the active
rather than the rested Unit with hidden hands (close target extension: 11/24 → 18/24 and
12/16 → 16/16), and protect the attacked Unit with a battle-scoped protection (0/8 → 8/8).

Blinded replay audit (30 games: 15 from the pre-change AI generated on bc38ab4 and 15 from
the new `standard`, the same seeds 90031–90033, five BO1 games per example pairing; ten
auditors read anonymized transcripts with boards and every option but no search statistics,
and a second agent checked every flagged play): confirmed implausible plays 22 for the old AI
and 19 for the new one. Wasted effects fell from 6 to 4 and bad targets from 5 to 2;
passivity (5 → 6), questionable attacks (3 → 4) and two missed lethals in the new games show
what remains.

## The `fast` preset

`fast` is for screening many deck variants, sub-variants and hypothetical builds; confirm the
survivors with `standard` on different seeds and never compare or pool rates across presets
(see the gcg-benchmark skill). Its budget is the cheapest the investigation screened (with the pre-change
search settings) whose decisions still agreed with the pre-change `strong` on 173 confident
positions about as often as a clamp(8·n, 20, 80) budget (0.744 against 0.733; the pre-change
`standard` 0.808); clamp(6·n, 16, 64) agreed less (0.721) and its lower bound against greedy
fell below 0.5.

| Check (held-out seeds, each run once) | Result |
| --- | --- |
| Games per core-minute against `standard` (same machine, idle) | 28.2 vs 2.90: 9.7× |
| vs random, 400 seat-swapped games (seed 90002) | 399 / 400 = 0.998 [0.986, 1.000] |
| vs greedy, 400 games (seed 90003) | 251 / 400 = 0.628 [0.579, 0.674] |
| vs `standard`, 400 games (seed 90004) | 156 / 400 = 0.390 [0.344, 0.439] |
| Tactical puzzles of `tests/ai/test_puzzles.py` with `fast` agents on both sides | all 18, with both agent seeds (36/36) |

Fidelity: each example pairing played 400 BO1 matches under `fast` and under `standard` on
the same seed (the same deals), deck under test listed first:

| Pairing (seed) | `fast` | `standard` | Paired difference (fast − standard) |
| --- | --- | --- | --- |
| red-green-zeon vs blue-white-federation (90011) | 234 / 400 = 0.585 [0.536, 0.632] | 207 / 400 = 0.518 [0.469, 0.566] | +0.068 [+0.011, +0.124] |
| blue-white-federation vs purple-white-tekkadan (90012) | 222 / 400 = 0.555 [0.506, 0.603] | 226 / 400 = 0.565 [0.516, 0.613] | −0.010 [−0.066, +0.046] |
| purple-white-tekkadan vs red-green-zeon (90013) | 147 / 400 = 0.368 [0.322, 0.416] | 161 / 400 = 0.403 [0.356, 0.451] | −0.035 [−0.085, +0.015] |

Where `standard`'s interval excludes 50% (the second and third pairings), `fast` favours the
same deck. In the first pairing `standard` does not resolve the matchup and `fast` rates the
deck under test about 7 points higher (a significant paired difference): `fast` rates are not
`standard` rates, which is why the screening funnel confirms survivors with `standard`.

A deliberately weakened legal variant of red-green-zeon (`tests/data/decks/
red-green-zeon-weakened.txt`: four Gelgoog, two Schuzrum-Galluss and three Close Combat
replaced by weak same-colour cards), 200 BO1 matches against the original (seed 90021):

| Preset | Original deck wins | Rate | Wilson 95% |
| --- | ---: | ---: | --- |
| `fast` | 141 / 200 | 0.705 | [0.638, 0.764] |
| `standard` | 139 / 200 | 0.695 | [0.628, 0.755] |

## Known weaknesses

- **Horizon.** Rollouts stop at the start of the turn after the reply turn; plans that pay
  off later rely on the evaluation. The leaf now prices the next turn's lethal better
  (calibrated `value_scale` and the role features' separate can-kill and race terms; part of
  the measured gain above). A lethal extension that played one more turn when the horizon
  position threatened lethal, with a face-first push and a Blocker guard, was built and
  measured: it fixed an audited defence position, but with the root, search and evaluation
  settings at twice the budget (before the policy settings were merged) it won 449/800 against
  448/800 without it on the same deals, and on 100 decisions where it changed the move
  the old move did better in play-outs (36 to 23), so it was removed. A three-turn horizon
  costs about 43% more per iteration and is deferred for `strong`.
- **Policy bias.** Rollouts and priors come from a hand-written policy. Measured: rollouts
  cast Commands in 33% of turns where search casts them in 45% (they are under-cast, not
  eager), and activate abilities more than search does (55% against 22%); modal choices are
  scored flat; the opponent is modelled with the same policy. The Burst, 0-AP face attack and
  rule polarity fixes shipped. Kill-aware damage targeting and damage-payoff targeting made
  the policy's own picks clearly better (regret per Damage decision −0.020) but made no
  difference to the search's choices (+0.00025 ± 0.00032 against a reseed placebo), so they
  were removed; effect-aware Command scoring and priors distilled from search statistics made
  decisions worse in the investigation and were not built.
- **Strategy fusion.** Determinized search can assume it will "know" hidden cards when a
  later decision is made. Measured: the searcher's in-tree nodes took hidden information
  almost only through the opponent's automatic action-step pass (276 of 288 splits), which
  `observation_keys` removes (389 splits in 24 of 183 positions → 0; 12 splits of other
  causes remain). Opponent nodes pool statistics across worlds where the opponent holds
  different cards (0.84–0.89 distinct hands per visit), so a multiple-observer search would
  have almost no visits per opponent node at these budgets; the measured fusion effect at
  opponent nodes is small (+0.021 at a large budget). Known limit of the observation keys: when
  the opponent passes in a battle's action step and again in the end phase with no searcher
  decision between them, both passes map to one child, which that iteration then counts twice
  (9 of 11,402 measured iterations).
- **Linear evaluation.** The model is still linear. Interaction terms gained nothing
  (±0.001 nats); the role features replace the forced antisymmetry, which was the measured
  misspecification. The weights are fitted to self-play of the pre-change standard search;
  refitting to the new search's own games is a follow-up.
- **Budget scaling.** Decision regret against a much larger search falls from 0.0035 per
  decision at half the standard iterations to 0.0019 at the old budget, 0.0013 at twice and
  0.0006 at four times; past about twice the old budget the limit is the model (rollouts,
  horizon, evaluation), not the root allocation: every root rule tried (sequential halving,
  progressive widening, near-tie extensions, visit minimums, budget-dependent exploration)
  was null within ±0.0004 at equal iterations. `standard` therefore doubled its iterations;
  `strong` keeps twice `standard`'s and is only slightly stronger (0.523 above).
- **Target choice.** Close target choices now get up to three budgets (regret −0.00090 ±
  0.00023 per target decision on 683 decisions, 32 changed choices better and 6 worse), and
  no-op Rest targets are pruned. Choosing the less useful of two targets is still possible
  when their values stay within noise after the extension. One target puzzle (ping a Unit that
  keeps the damage rather than one whose <Repair> heals it) is solved by the extension in the
  pre-change configuration (11/24 → 18/24 agent seeds on its test) but only about 7 of 24 times
  by the shipped `standard`: with the calibrated leaf value the two targets score closer (with
  `value_scale` 2 the same preset solves it about 20 of 24 times). Not adopted: a heuristic
  tie-break toward the policy's preferred target (worse), a fixed target budget multiplier at
  the same cost (−0.00036 ± 0.00019 against −0.00090 ± 0.00023 for the extension), and a
  separate root prior weight for target decisions (z −1.4 and −1.0).
- **Engine-bound speed.** Most search time is inside the engine's `apply` (80–83% before the
  2026-10 speedups). The output-identical speedups cut function calls per game by about a third (derived-view
  recomputations −40%, listened-events fast path, rules-management skips) and the world cache
  cut determinizations by 73%; `standard` spent the savings on iterations. Invalidating the
  derived view by mutation kind (74% of recomputations produce an identical view) needs a
  dependency analysis of every condition and is deferred.

### Deferred proposals

- Calibrate matchup estimates against real tournament results (needs data from the user).
- Refit the evaluation on games of the new search, and test whether `strong` and `fast`
  benefit from their own fits.
- A three-turn horizon for `strong`; a larger `strong` budget relative to the new `standard`.
- Invalidate the derived view by mutation kind; a stable, process-independent
  `Lasting.effect_key` so saved states reload faithfully in another process.
- A multiple-observer search, or opponent models other than the shared policy, if a larger
  budget makes opponent nodes worth splitting.
- Decision-level regret tooling (the oracle replay used in the investigation) as a committed
  tool for future search changes.
- Paired analysis of same-seed variant runs in the benchmark skill (fewer games for the same
  precision).
