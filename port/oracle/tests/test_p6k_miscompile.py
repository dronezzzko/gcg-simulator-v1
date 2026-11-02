"""W-66: the reviewed hits of the IR miscompile scan, on the Python testkit.

The scan (``crates/gcg-effects/tests/p6k_ir_scans.rs``) finds five compiled cards whose step
acts on two chosen targets with no all-targets guard (the shape of the GD04-015 miscompile,
``wp_gd04_a.py:229``): GD01-103, GD02-055, GD03-116, GD04-114 and GD05-117. These scenarios
show the engine's multi-target pre-check (rules 10-1-8-1-1, 10-2-2, 10-3-3-1) covers them, so
none is a real miscompile. ``crates/gcg-engine/tests/p6k_behaviour.rs`` runs the same scenarios
on the Rust testkit (``w66_two_target_commands_need_both_targets``,
``w66_gd02_055_deploy_needs_both_targets``).

    uv run pytest port/oracle/tests -q
"""

from __future__ import annotations

import pytest

from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, has_action, play, zone_of

VANILLA_3_4 = "GD01-013"  # Gundam, Lv4 3/4, no effects
EF_4_3 = "GD01-018"  # blue (Earth Federation) Lv3 4/3
VAGAN_3_3 = "GD03-065"  # vanilla Lv3 3/3 (Vagan)
TWO_TARGET_COMMANDS = (
    ("GD01-103", EF_4_3, 1),
    ("GD03-116", VAGAN_3_3, 3),
    ("GD04-114", VANILLA_3_4, 2),
    ("GD05-117", VANILLA_3_4, 3),
)


@pytest.mark.parametrize(("number", "friendly", "resources"), TWO_TARGET_COMMANDS)
@pytest.mark.rule("10-1-8-1-1", "10-2-2")
def test_two_target_command_needs_both_targets(number: str, friendly: str, resources: int) -> None:
    sc = Scenario()
    sc.resources(0, resources)
    sc.add(0, friendly)
    cmd = sc.add(0, number, Zone.HAND)
    st = sc.start()
    assert not has_action(st, ActionKind.PLAY_COMMAND, cmd)

    sc = Scenario()
    sc.resources(0, resources)
    mine = sc.add(0, friendly)
    foe = sc.add(1, VANILLA_3_4)
    cmd = sc.add(0, number, Zone.HAND)
    st = sc.start()
    assert has_action(st, ActionKind.PLAY_COMMAND, cmd)
    play(st, cmd)
    for uid in (mine, foe):
        assert st.cards[uid].rested or st.cards[uid].damage > 0


@pytest.mark.card("GD02-055")
@pytest.mark.rule("10-3-3-1", "10-2-2")
def test_gd02_055_deploy_needs_both_targets() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    friend = sc.add(0, "GD02-015")
    gusion = sc.add(0, "GD02-055", Zone.HAND)
    st = sc.start()
    play(st, gusion)
    assert zone_of(st, gusion) is Zone.BATTLE
    assert st.pending is not None
    assert st.pending.kind is DecisionKind.MAIN
    assert st.cards[friend].damage == 0
    assert st.cards[gusion].damage == 0
