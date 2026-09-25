# Checklist: implementing or updating one card

Criterion 5 of `docs/SPEC.md`: every card number has an implemented, tested effect. The gate is
`tests/effects/test_card_coverage.py`. Every card must compile or be bound, and every
non-vanilla card needs a `@pytest.mark.card` test. `refresh coverage` lists the gaps.

## 1. Understand it

```bash
uv run python -m gcg_sim.tools.explain GD06-001 [GD06-002 ...]    # add --json for machines
```

This shows:
- the card data after overrides;
- the normalized text (`gcg_sim.effects.text.normalize`, reminder text removed per rule
  2-11-4);
- each ability line with its compiled DSL or its compile error;
- the binding module, and the final script;
- rulings, and conflict resolutions.

Read the raw text too. Explanatory parentheticals such as "(You choose first, then your
opponent.)" state the intended semantics.

Also collect:
- every ruling for the card: `explain` lists them, and the step-5 diff shows new ones;
- the FAQ entries that touch its keywords or timings;
- the rules it relies on (`src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md`).

## 2. Choose compiler or binding

- **Compiler.** When the failing phrase is a pattern that recurs, or will recur, across
  cards, extend the compiler.
  - `src/gcg_sim/effects/compiler/grammar.py` handles step templates (`@core(...)` functions),
    conditions, triggers, costs and constant effects.
  - `src/gcg_sim/effects/compiler/nouns.py` handles selectors and noun phrases.
  - Run `explain` on the other cards that share the phrase:
    `grep -l "<phrase>" src/gcg_sim/data/gcgapi/cards.ndjson`, or look it up in
    `docs/research/effect_templates.json`.
  - Afterwards, check that no previously compiled card changed:
    `uv run python -m gcg_sim.tools.golden --check`.
- **Binding.** For a one-off wording, write a binding in
  `src/gcg_sim/effects/bindings/<package>.py`. Use one module per work package, named like the
  existing ones (`wp_gd06_a.py`, `wp_gd06_b.py`). Modules are auto-discovered, so there is no
  registry edit.

  ```python
  from gcg_sim.cards.model import CardDef
  from gcg_sim.effects import dsl as d
  from gcg_sim.effects.bindings import card


  @card("GD06-001")
  def gd06_001(c: CardDef) -> d.CardScript:
      """【Deploy】Draw 1 for each friendly Unit in play, up to a maximum of 2."""
      units = d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE))
      deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.Draw(d.MinOf((units, 2))),))
      return d.CardScript(c.card_number, abilities=(deploy,), source="binding")
  ```

  - A binding replaces the compiler for the whole card, so bind every ability.
  - Reuse compiler output where it helps: `gcg_sim.effects.compiler.compile_parts(c)` returns
    each line's abilities or its error.
  - Pilot cards: abilities the paired Unit gains go in `unit_abilities` (rule 3-3-9).
    `【Burst】` and hand or trash abilities stay in `abilities`. Use
    `compiler.route_abilities` to split them.
  - Deterministic custom logic uses `@custom_step`, `@custom_cond`, `@custom_filter` or
    `@custom_value` in the same module, with names prefixed by the card number.
  - No `TODO`, `FIXME`, `NotImplementedError` or placeholder bodies anywhere in `src/`
    (`tests/test_no_placeholders.py`).

## 3. Test it

Put tests in `tests/cards/test_<package>.py` (e.g. `test_wp_gd06_a.py`), using
`gcg_sim.testkit`:

```python
@pytest.mark.card("GD06-001")
@pytest.mark.ruling("GD06-001:Q501")  # when the test pins a ruling
def test_gd06_001_deploy_draws_up_to_two() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, "GD01-060")  # vanilla filler Unit
    card = sc.add(0, "GD06-001", Zone.HAND)
    st = sc.start()
    before = len(st.zones[0][Zone.HAND])
    play(st, card)
    assert len(st.zones[0][Zone.HAND]) == before - 1 + 2
```

- Test each ability: the positive case, and a boundary or negative case (condition not met,
  no legal target, once per turn, 【During Pair】 gate off).
- Pin each ruling with `@pytest.mark.ruling("CARD:Qn")`, or give an N/A reason in
  `tests/meta/rulings_na/<package>.json` (e.g. `wp_gd06_a.json`): `{"CARD:Qn": "why no engine behaviour is involved"}`.
  New rules-FAQ entries follow the same pattern (`@pytest.mark.faq("Qn")` or
  `tests/meta/faq_na.json`).
- When a test exercises a comprehensive rule, tag it `@pytest.mark.rule("n-n-n")` too.
- testkit helpers include `Scenario.add/hand/resources/start`, `play`, `activate`, `attack`,
  `block`, `select`, `yes`, `no`, `pass_all`, `to_next_turn`, `ap`, `hp`, `keywords` and
  `zone_of`. The reference examples are `tests/cards/test_lead_examples.py` and
  `tests/rules/test_lead_examples.py`.

## 4. Freeze it

```bash
uv run pytest tests/cards/test_<package>.py -q
uv run python -m gcg_sim.tools.golden --write --prefix GD06
git diff tests/effects/golden/GD06.json          # review: the compiled behaviour must match the text
uv run python -m gcg_sim.tools.refresh coverage   # the card no longer appears
uv run pytest tests/effects -q
```

## Changed text on an implemented card

When `coverage` shows a `TEXT CHANGED` card or the diff shows a `wording` change:

1. Compare `old_normalized` and `new_normalized` in the diff JSON.
2. Re-run `explain`.
3. Update the binding or the compiler.
4. Update the tests, and add one for the new behaviour.
5. Regenerate the prefix's golden file.

A `typography` change needs no work: the golden `text_hash` stays the same.
