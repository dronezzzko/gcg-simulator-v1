"""P6-F's synthetic cards: IR fragments that give one vanilla Unit a synthetic text, for tests
that need a card the packaged pool does not have.

- ``synthetic_burst_fragment.json``: W-163's negative control, the synthetic Burst "Discard 2
  cards. Draw 1." on EB01-007 (docs/port/WEAKNESSES.md W-163, verification).
- ``synthetic_order_fragment.json``: W-29's scenario, two simultaneous end-of-turn triggers whose
  order matters (at the end of your turn, "draw 1" and "if you have 1 or fewer cards in your
  hand, draw 3") on GD02-084 (1/3), and the comparison Unit GD01-022 (2/3) with "【Deploy】Draw 2."
- ``synthetic_modal_fragment.json``: W-24's scenario, a modal end-of-turn trigger on GD02-084 (one
  mode draws 1, the other 3) and GD01-022 with "【Deploy】Draw 2."
- ``synthetic_end_phase_fragment.json``: W-28's scenario, a Base (GD01-126) with "【Activate･Action】
  Rest this Base：Choose 1 enemy Unit with 3 or less HP. Rest it. Then, draw 1."

The Python package is only read: ``testkit.card_text`` overrides the card's text in this
process, and ``port/oracle/export_ir.build_fragment`` exports the override as a fragment over
the committed IR (PLAN §3.4 "Test overrides"). The Rust tests load each with
``Registry::with_fragment`` (``crates/gcg-search/src/mech/p6f/tests.rs``,
``crates/gcg-search/tests/scenarios/p6f_w29_order.rs``).

Regenerate with ``uv run python docs/port/eval/P6-F/synthetic_cards.py`` from the repository
root; it rewrites the fragments next to this file.
"""

from __future__ import annotations

import contextlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "port" / "oracle"))

import export_ir  # noqa: E402

from gcg_sim import testkit  # noqa: E402
from gcg_sim.effects.registry import get_registry  # noqa: E402

IR = ROOT / "crates" / "gcg-effects" / "ir" / "registry.json"
CASES = (
    ("synthetic_burst_fragment.json", (("EB01-007", "【Burst】Discard 2 cards. Draw 1."),)),
    (
        "synthetic_order_fragment.json",
        (
            (
                "GD02-084",
                "At the end of your turn, draw 1.\n"
                "At the end of your turn, if you have 1 or fewer cards in your hand, draw 3.",
            ),
            ("GD01-022", "【Deploy】Draw 2."),
        ),
    ),
    (
        "synthetic_modal_fragment.json",
        (
            (
                "GD02-084",
                "At the end of your turn, choose 1 of the following effects and activate it:\n"
                "■Draw 1.\n"
                "■Draw 3.",
            ),
            ("GD01-022", "【Deploy】Draw 2."),
        ),
    ),
    (
        "synthetic_end_phase_fragment.json",
        (
            (
                "GD01-126",
                "【Activate･Action】Rest this Base：Choose 1 enemy Unit with 3 or less HP. Rest it. "
                "Then, draw 1.",
            ),
        ),
    ),
)


def write_case(index: int) -> None:
    """One case, in this process (a fragment numbers its abilities after the base IR's, so
    each fragment needs a fresh registry)."""
    name, overrides = CASES[index]
    reg = get_registry()
    base = json.loads(IR.read_text(encoding="utf-8"))
    defs = [reg.db[card].def_id for card, _ in overrides]
    with contextlib.ExitStack() as stack:
        for card, text in overrides:
            stack.enter_context(testkit.card_text(card, text))
        frag = export_ir.build_fragment(reg, base, defs)
        scripts = {card: repr(reg.cards[reg.db[card].def_id].script) for card, _ in overrides}
    doc = {
        "card_numbers": [card for card, _ in overrides],
        "texts": dict(overrides),
        "scripts": scripts,
        "fragment": frag,
    }
    out = Path(__file__).with_name(name)
    out.write_text(
        json.dumps(doc, indent=1, sort_keys=True, ensure_ascii=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {out.relative_to(ROOT)}: {sorted(scripts)}")


def main() -> None:
    if len(sys.argv) > 1:
        write_case(int(sys.argv[1]))
        return
    for i in range(len(CASES)):
        subprocess.run([sys.executable, __file__, str(i)], check=True)


if __name__ == "__main__":
    main()
