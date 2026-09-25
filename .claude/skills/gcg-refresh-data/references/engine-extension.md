# Extending the engine: keywords, timings, card types, rules

New sets sometimes add a keyword effect (rule 13-1, `<Name N>`), a keyword (rule 13-2,
`【Name】`), a pilot qualifier, an event, or a card type. Implement the concept once, with rule
tests, before binding the cards that use it.

## 1. A new keyword effect `<Name>` (rule 13-1)

The keyword vocabulary appears in several places. Update all of them.

| Where | What |
| --- | --- |
| `src/gcg_sim/effects/dsl.py` | `class Kw(StrEnum)`: add the member. Add it to `STACKING_KEYWORDS` when copies add up (13-1-1-2 style). |
| `src/gcg_sim/effects/compiler/abilities.py` | `_KW`: the regex for keyword-only lines (`parse_keyword_line`) |
| `src/gcg_sim/effects/compiler/nouns.py` | `KW_RE`: selectors "with <X>" / "without <X>" |
| `src/gcg_sim/effects/compiler/grammar.py` | `_KWS`: grants such as "gains <X>" (`KeywordGrant`); conditions "has <X>" |
| `src/gcg_sim/engine/view.py` | `has_kw` / `kw_amount` read keywords from the derived view; usually no change is needed |
| engine behaviour | Put it where the rule acts. Existing keywords are the model: Blocker and High-Maneuver in `engine/battle.py` (`eligible_blockers`); First Strike and Suppression in `damage_step`; Breach damage in `battle.py`; Repair at end of turn in `engine/core.py`; Support as an activated action in `engine/game.py`. |
| keyword programs | `src/gcg_sim/effects/registry.py`: `REPAIR_STEPS` / `BREACH_STEPS` / `SUPPORT_STEPS`, registered in `build_registry`. Add one when the keyword resolves as an effect with steps. |
| `src/gcg_sim/sources/conflicts.py` | `KEYWORD_VOCABULARY`: used for marker checks against gcg-api `keyword_effects` |
| AI evaluation | `src/gcg_sim/ai/` (when present): give the keyword a value in the evaluation features, so search does not ignore it |

## 2. A new timing keyword `【Name】` (rule 13-2) or qualifier

| Where | What |
| --- | --- |
| `src/gcg_sim/effects/compiler/grammar.py` | `compile_marked` handles the gates (`Once per Turn`, `During Pair[･qualifier]`, `During Link`). `_compile_timed` maps markers to abilities: `Burst`, `Deploy`, `Attack`, `Destroyed`, `When Linked`, `When Paired[･qualifier]`, `Activate･Main/Action`, `Main/Action`, `･Development N`. Pilot qualifiers such as `Lv.5 or Higher Pilot`, `(Trait) Pilot` and `Red Pilot` are in `_gate_qualifier`. |
| `src/gcg_sim/effects/text.py` | `split_abilities` peels leading `【...】` markers. `normalize` unifies `・`/`･` and spacing. Add a normalization only if the source writes the marker inconsistently. |
| `src/gcg_sim/effects/dsl.py` | `Ev` (a new event kind), `Trigger` fields (subject, `whose_turn`, `pilot_filters`, `battle_only`, ...), `Timing`, `Gate`, `Where` |
| engine events | Emit the event where the game action happens: `core.emit(st, d.Ev.X, subject, ...)` in `engine/core.py`, `battle.py`, `game.py` or `interp.py`. `registry._index_events` records which events each card listens to. Events that fire for every card (like `TURN_END`) go in its `always` set, or the fast path will skip them. |
| `src/gcg_sim/sources/conflicts.py` | `TIMING_VOCABULARY`: marker checks |

## 3. A new card type or zone

| Where | What |
| --- | --- |
| `src/gcg_sim/cards/model.py` | `CardType`, `MAIN_DECK_TYPES`, `is_token`, `is_unit`, `is_base` |
| `src/gcg_sim/cards/db.py` | `_make_def` (stats, pilot name, link parsing) |
| `src/gcg_sim/effects/compiler/__init__.py` | `REMINDER_ONLY_TYPES`; `route_abilities` for text another card gains |
| `src/gcg_sim/effects/dsl.py` | `CardKind`, `Loc` |
| deck rules | `src/gcg_sim/data/official/deck_construction.json` (`allowed_card_types`); deck validation and its tests |
| engine | zones in `engine/types.py` (`Zone`), play and deploy in `engine/game.py` and `engine/core.py`, invariants in `engine/invariants.py` (card conservation) |

## 4. Tests and traceability

- Rule tests go in `tests/rules/`, tagged `@pytest.mark.rule("13-1-9", "13-1-9-1")` with every
  rule id they verify. Include the positive case, the interaction cases the rule text names
  (stacking, "gains another copy", "cannot be ...") and one negative case.
  `tests/rules/test_lead_examples.py` is the template.
- Card tests for the first cards that use the concept go in `tests/cards/`.
- New rule ids that are untestable in 1v1 get a reason in `src/gcg_sim/data/rules/rules_na.json`.
- Compiler changes must not alter existing cards: run
  `uv run python -m gcg_sim.tools.golden --check`. Regenerate only the prefixes you reviewed.
- Robustness smoke test (random legal games with invariant checks on every action):
  `uv run python -m gcg_sim.tools.fuzz --games 500 --workers 4`. The full criterion-6 run is in
  `tests/slow/test_robustness.py`.
- Traceability: `uv run pytest -m "" --junitxml=build/junit.xml`, then
  `uv run python -m gcg_sim.tools.traceability --junit build/junit.xml --check`. Every testable
  rule needs at least one passing tagged test.
- Record each interpretation you had to make in `docs/ASSUMPTIONS.md`. If an official source
  disagrees with another, add a curated conflict with its resolution (see
  `conflict-policies.md`).
