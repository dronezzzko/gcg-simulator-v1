# Architecture

gcg-sim is a deterministic rules engine for the Gundam Card Game (Comprehensive Rules Ver. 1.9.0),
a typed effect DSL with a text compiler and per-card bindings, search-based AI players, a
multi-process match runner, and a report writer. Everything runs offline from packaged data.

```
             gcg-api snapshot (pinned) + official deck/B&R/BO3 rules + rules markdown
                                   │  (src/gcg_sim/data, provenance in data/SOURCES.lock.json)
                                   ▼
 cards.db (CardDB, overrides) ──► effects (text → DSL compiler, bindings) ──► registry (programs)
                                                                                 │
 deck (parse/validate) ──► runner (seeds, BO3, processes) ──► engine (state, rules, triggers)
                                   │                      ▲        │
                                   │                      └── ai (determinized MCTS, greedy, random)
                                   ▼
                              reports (results.json + schema, summary.md, games.ndjson, replays)
```

## Package map

| Module | Responsibility |
| --- | --- |
| `gcg_sim.rng` | `SplitMix64` (one 64-bit integer of state; cheap to clone) and `derive_seed(master, *path)`. |
| `gcg_sim.cards` | `CardDef` (normalized, override-applied), `CardDB` (by card number / product id), link and token parsing. |
| `gcg_sim.effects.dsl` | The typed effect DSL: abilities, triggers, conditions, costs, selectors, steps, continuous effects, durations. |
| `gcg_sim.effects.text` | Normalization (rule 2-11-4 reminder stripping) and ability splitting. |
| `gcg_sim.effects.compiler` | Regex grammar: marker chains, connectors ("Then"/"If you do"), conditions, triggers, costs, selectors, core step templates. |
| `gcg_sim.effects.bindings` | Per-set binding modules (auto-discovered) for text the compiler cannot handle; custom hooks. |
| `gcg_sim.effects.registry` | Deterministic registry: card scripts → abilities → flat programs; per-zone ability lists; event index. |
| `gcg_sim.engine.state` | `GameState`, `CardInstance`, `Frame`, `Lasting`, `TriggerInst`, `Action`, `Decision` — plain data, `clone()`, JSON. |
| `gcg_sim.engine.view` | Derived characteristics (constant + lasting effects fixpoint) and DSL expression evaluation. |
| `gcg_sim.engine.core` | Zone movement (new-card semantics), events and trigger collection, damage, destruction, rules management. |
| `gcg_sim.engine.interp` | Instruction interpreter for effect programs (resumable decisions). |
| `gcg_sim.engine.battle` | Attack, block, damage (First Strike, Suppression, Breach), battle end, damage-only battles. |
| `gcg_sim.engine.game` | Setup, turn structure, legal actions, `new_game`, `apply`, `advance`. |
| `gcg_sim.engine.observe` | Information sets: `determinize`, `permute_hidden`, `information_set_key`. |
| `gcg_sim.engine.invariants` | Card conservation and legal-state checks used by the robustness suite. |
| `gcg_sim.testkit` | Scenario builder and decision helpers for rule, card and AI tests. |
| `gcg_sim.tools.*` | `explain`, `golden`, `fuzz`, `traceability`, `sources`, `refresh`. |
| `gcg_sim.sources.conflicts` | Conflict detection (divergent printings, errata, rulings vs text, rules xrefs, B&R ambiguities). |
| `gcg_sim.rules.index` | Rules index keyed by rule number with the 1v1 testable/N-A split. |

## Engine model

**State.** A `GameState` holds every card instance (`uid`, `def_id`, owner, zone, rested,
damage, pairing, `zone_seq`, knowledge bits), per-player zone lists (deck and shields ordered
top first), turn/phase/step, the pending decision, the battle stack, lasting effects, delayed
triggers, pending triggers and trigger batches, the effect frame stack, once-per-turn usage,
per-turn history, and the RNG. Cloning copies ~120 small objects (≈15 µs); the derived-view
cache is shared until the next mutation.

**Decisions.** The engine runs until a player must choose (`st.pending`), exposing every legal
option as a small `Action(kind, a, b, c)`. `apply` validates the action, resumes the procedure or
the waiting effect, and advances again. Multi-card choices are sequential single picks with a
DONE option, and picks are ordered so each subset is reachable exactly once.

**Procedure.** Fine-grained steps (`types.Step`) implement setup (6-2), the start/draw/resource/
main/end phases (7), battles (8), and action steps (9). Between steps the loop resolves effect
frames and triggered effects first (7-1-3, 7-2-2, 7-6-2).

**Triggers (10-1-6).** Events (`dsl.Ev`) are emitted as the state changes. Matching triggered
abilities (own text, Pilot text gained by Units, granted abilities, delayed triggers, keyword
effects like <Repair>/<Breach>) queue as `TriggerInst`. Events in one atomic change share a
group id, so an ability triggers once per group (10-1-6-3). Queued triggers become a batch that
resolves after the current effect; newer batches go first (10-1-6-7). Within a batch Bursts go
first (10-1-6-8), then the active player's in the order they choose, then the standby player's
(10-1-6-5/6). Leaving-play events use last-known abilities (13-2-8-2, Q436).

**Effects.** Each ability's step tree is flattened into a linear program with jumps
(`effects.program`). A resolving effect is a `Frame(program_id, pc, vars, did, ...)`, so effects
suspend for decisions and clone cheaply. `did` implements "If you do" (5-20-1); `Then` just
continues (5-20-2). Commands and activated abilities cannot be used without a legal target
for their mandatory choices (10-1-8-1-1, 10-2-2).

**Continuous effects (10-1-5).** `view.derived(st)` computes AP/HP, keywords (stacking <Repair>/
<Breach>/<Support>), traits, rule modifications (restrictions, permissions, damage prevention
and reduction, redirection), cost modifiers, link status, and the active abilities of every
host, from constant abilities plus lasting effects, iterating to a fixpoint. "Can't" rules win
(10-1-5-6). Lasting effects bind to `(uid, zone_seq)`, so they end when a card changes location
(4-1-5).

**Rules management (11).** After every instruction and procedure step: deck-out defeat (11-2),
then simultaneous destruction of cards whose damage reached their HP (11-3), repeated until
stable. Battle damage to a player with an empty shield area ends the game immediately (8-5-2-2).
Excess Units/Bases go to the trash without being destroyed (11-4, 11-5).

**Information sets.** Card uids are assigned after the seeded shuffle and cut, so they carry
no identity. Each card has `known` bits per player. `observe.determinize` resamples every
identity hidden from a player from the open decklists and replaces the game RNG, so search depends
only on that player's information set.

## Effect implementation pipeline

1. `effects.text.normalize` strips reminder parentheticals (rule 2-11-4) and splits abilities.
2. `effects.compiler` parses markers and sentences into DSL nodes; unrecognized text raises
   `CompileError` (the card is then *unimplemented* unless a binding exists).
3. `effects.bindings.<package>` modules define full scripts for irregular cards (they may reuse
   compiled lines via `compile_parts`) and register deterministic custom hooks.
4. `effects.registry` resolves each card (binding > compiler), flattens programs, and indexes
   abilities by zone and event.
5. Golden files (`tests/effects/golden/<prefix>.json`) freeze each card's compiled behaviour
   with its normalized-text hash; `tests/effects/test_card_coverage.py` requires every card to be
   implemented and every non-vanilla card to have a `@pytest.mark.card` behaviour test.

## Determinism

All randomness goes through `SplitMix64` seeded by `derive_seed` (SHA-256 of the seed path).
The runner derives per-match and per-game seeds from the master seed, never from worker
scheduling. Iteration orders are explicit, hash-seed independent, and JSON outputs sort keys.
Replays store the seed plus the action list; re-applying the actions reproduces the game exactly.
