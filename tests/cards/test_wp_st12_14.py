"""Behaviour and ruling tests for ST12, ST13 and ST14 (work package WP-ST12-14)."""

from __future__ import annotations

import pytest

from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    card_numbers,
    choose_option,
    has_action,
    keywords,
    no,
    options,
    pass_,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA_2_2 = "GD01-060"  # Zaku Mariner, Lv2 2/2
VANILLA_1_2 = "EB01-051"  # Ze'Gok, Lv1 1/2
VANILLA_2_3 = "GD01-022"  # Cancer, Lv2 2/3
VANILLA_3_3 = "GD02-015"  # Marasai, Lv3 3/3
VANILLA_3_4 = "GD01-013"  # Gundam, Lv4 3/4
VANILLA_4_3 = "GD01-040"  # Wing Gundam, Lv5 4/3
VANILLA_4_3_LV4 = "GD01-031"  # Gelgoog, Lv4 4/3
VANILLA_5_4 = "ST07-003"  # Gundam Virtue, Lv5 5/4
VANILLA_6_4 = "ST14-008"  # Gundam Heavyarms Custom (EW), Lv6 6/4
BLOCKER = "GD01-072"  # Launcher Strike Gundam, Lv4 3/4 <Blocker>
FIRST_STRIKE_1_4 = "GD04-034"  # Gundam Kyrios, Lv4 1/4 <First Strike>
PILOT_LV3 = "GD01-089"  # Riddhe Marcenas, AP+1 HP+1
PILOT_LV4 = "ST02-010"  # Heero Yuy, AP+2 HP+1
PILOT_LV5 = "GD01-088"  # Banagher Links, AP+2 HP+2
MQUVE = "GD01-092"  # M'Quve, Lv3 Pilot
BREACH_COMMAND = "ST02-012"  # 【Main】Choose 1 of your Units. It gains <Breach 3> during this turn.
SET_RESOURCE_ACTIVE_PILOT = "ST01-011"  # 【Attack】【Once per Turn】Set 1 of your Resources active.
MIDAIR = "GD01-121"  # 【Main】Choose 1 rested Unit with <Blocker>. Set it as active. ...

EPYON = "ST12-001"
SHINING = "ST12-002"
TALLGEESE = "ST12-003"
EXIA = "ST12-004"
GQUUUUUUX = "ST12-005"
BANSHEE_DM = "ST12-006"
GYAN = "ST12-007"
BANSHEE_UM = "ST12-009"
MILLIARDO = "ST12-011"
PLE_TWELVE = "ST12-012"
FINAL_VICTOR = "ST12-013"
WISE_LEADER = "ST12-014"
TWO_UNICORNS = "ST12-015"
LIBRA = "ST12-016"
QUBELEY = "ST13-001"
ELMETH = "ST13-002"
BERTIGO = "ST13-003"
GX_BIT = "ST13-004"
GFRED = "ST13-005"
AERIAL = "ST13-006"
JAGD_DOGA = "ST13-007"
PHARACT = "ST13-009"
RED_GUNDAM = "ST13-010"
HAMAN = "ST13-011"
SULETTA = "ST13-012"
NEWTYPE = "ST13-013"
FINAL_DUTY = "ST13-014"
SOLOMON = "ST13-015"
SODON = "ST13-016"
THE_O = "ST14-001"
NT1 = "ST14-002"
PALACE_ATHENE = "ST14-003"
GEARA_DOGA = "ST14-004"
G_FALCON = "ST14-005"
FA_UNICORN_DM = "ST14-006"
FA_UNICORN_UM = "ST14-007"
DUEL = "ST14-009"
SCIROCCO = "ST14-011"
BANAGHER = "ST14-012"
NATURAL_TALENT = "ST14-013"
BLAZING = "ST14-014"
EMOTIONS = "ST14-015"
GRYPHIOS = "ST14-016"

BIT_FUNNEL = "Bit / Funnel"


def _aid(number: str, index: int = 0, *, unit: bool = False) -> int:
    reg = get_registry()
    entry = reg.cards[reg.db[number].def_id]
    return (entry.unit if unit else entry.own)[index]


def _kind(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def _tokens(st: GameState, player: int) -> list[int]:
    return [u for u in st.zones[player][Zone.BATTLE] if V.cdef(st, u).name == BIT_FUNNEL]


def _active_resources(st: GameState, player: int) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if not st.cards[u].rested)


def _to_action_step(st: GameState, player: int = 0) -> None:
    """Decline blocks and pass priority until ``player`` may act in the action step."""
    for _ in range(4):
        dec = st.pending
        if dec is not None and dec.kind is DecisionKind.BLOCK:
            act(st, A.NO_BLOCK)
        elif dec is not None and dec.kind is DecisionKind.ACTION_STEP and dec.player != player:
            pass_(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.pending.player == player


def _mode_options(st: GameState) -> list[int]:
    assert st.pending is not None and st.pending.ctx("mode") == 1
    return [o.a for o in options(st)]


# ---------------------------------------------------------------------------------------------
# ST12-001 Gundam Epyon


@pytest.mark.card("ST12-001")
def test_st12_001_deploy_deals_5_damage_to_enemy_base() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    epyon = sc.add(0, EPYON, Zone.HAND)
    base = sc.base(1, GRYPHIOS)
    st = sc.start()
    play(st, epyon)
    assert zone_of(st, base) is Zone.TRASH


def _epyon_battle(pilot: str | None) -> tuple[GameState, dict[str, int]]:
    sc = Scenario()
    epyon = sc.add(0, EPYON, pilot=pilot)
    ids = {
        "epyon": epyon,
        "target": sc.add(1, VANILLA_2_2, rested=True),
        "ap5": sc.add(1, VANILLA_5_4),
        "ap6": sc.add(1, VANILLA_6_4),
        "ap2": sc.add(1, VANILLA_2_2),
    }
    st = sc.start()
    attack(st, epyon, ids["target"])
    pass_all(st)
    return st, ids


@pytest.mark.card("ST12-001")
@pytest.mark.rule("13-2-10")
def test_st12_001_lv5_pilot_battle_destruction_sweeps_enemies_with_5_or_less_ap() -> None:
    st, ids = _epyon_battle(PILOT_LV5)
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert zone_of(st, ids["ap2"]) is Zone.TRASH
    assert st.cards[ids["ap5"]].damage == 2
    assert st.cards[ids["ap6"]].damage == 0


@pytest.mark.card("ST12-001")
@pytest.mark.parametrize("pilot", [PILOT_LV4, None])
def test_st12_001_no_sweep_without_lv5_pilot(pilot: str | None) -> None:
    st, ids = _epyon_battle(pilot)
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert zone_of(st, ids["ap2"]) is Zone.BATTLE
    assert st.cards[ids["ap5"]].damage == 0


@pytest.mark.card("ST12-001")
@pytest.mark.ruling("ST12-001:Q436")
@pytest.mark.rule("8-5-3-2-3", "10-1-6-4")
def test_st12_001_q436_triggers_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    epyon = sc.add(0, EPYON, pilot=PILOT_LV5, damage=4)
    target = sc.add(1, VANILLA_3_4, rested=True)
    other = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, epyon, target)
    pass_all(st)
    assert zone_of(st, epyon) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, other) is Zone.TRASH


@pytest.mark.card("ST12-001")
@pytest.mark.ruling("ST12-001:Q437")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: destruction by a Unit's own effect damage never counts as that Unit destroying it",
)
def test_st12_001_q437_triggers_when_destroying_with_effect_damage() -> None:
    sc = Scenario()
    epyon = sc.add(0, EPYON, pilot=MILLIARDO)
    target = sc.add(1, VANILLA_2_3, rested=True, damage=1)
    other = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, epyon, target)
    _to_action_step(st)
    activate(st, epyon, _aid(MILLIARDO, 0, unit=True))
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, other) is Zone.TRASH


@pytest.mark.card("ST12-001", "ST12-013")
@pytest.mark.rule("10-1-6-1-1")
def test_st12_001_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    epyon = sc.add(0, EPYON, pilot=PILOT_LV5)
    target = sc.add(1, VANILLA_2_2, rested=True)
    second = sc.add(1, VANILLA_3_4)
    bystander = sc.add(1, VANILLA_5_4)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    attack(st, epyon, target)
    pass_all(st)
    assert st.cards[bystander].damage == 2
    play(st, victor)
    select(st, second)
    assert zone_of(st, second) is Zone.TRASH
    assert zone_of(st, epyon) is Zone.BATTLE
    assert st.cards[bystander].damage == 2


@pytest.mark.card("ST12-001", "ST12-013")
@pytest.mark.rule("10-1-6-1-1", "10-1-6-4")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: Once per Turn key of a last-known-information trigger uses the moved card's new zone_seq",
)
def test_st12_001_once_per_turn_when_destroyed_in_the_second_battle() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    epyon = sc.add(0, EPYON, pilot=PILOT_LV5)
    target = sc.add(1, VANILLA_2_2, rested=True)
    second = sc.add(1, VANILLA_6_4)
    bystander = sc.add(1, VANILLA_5_4)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    attack(st, epyon, target)
    pass_all(st)
    assert st.cards[bystander].damage == 2
    play(st, victor)
    select(st, second)
    assert zone_of(st, second) is Zone.TRASH
    assert zone_of(st, epyon) is Zone.TRASH
    assert zone_of(st, bystander) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# ST12-002 Shining Gundam


@pytest.mark.card("ST12-002")
def test_st12_002_deploy_gives_breach_3_to_itself_as_only_5_ap_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    other = sc.add(0, VANILLA_4_3)
    shining = sc.add(0, SHINING, Zone.HAND)
    st = sc.start()
    play(st, shining)
    assert keywords(st, shining).get("Breach") == 3
    assert "Breach" not in keywords(st, other)
    to_next_turn(st)
    to_next_turn(st)
    assert "Breach" not in keywords(st, shining)


@pytest.mark.card("ST12-002")
def test_st12_002_deploy_may_choose_another_5_ap_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    big = sc.add(0, VANILLA_6_4)
    shining = sc.add(0, SHINING, Zone.HAND)
    st = sc.start()
    play(st, shining)
    assert _kind(st) is DecisionKind.SELECT
    select(st, big)
    assert keywords(st, big).get("Breach") == 3
    assert "Breach" not in keywords(st, shining)


# ---------------------------------------------------------------------------------------------
# ST12-003 Tallgeese III


def _tallgeese_battle(*, damage: int = 0) -> tuple[GameState, dict[str, int]]:
    sc = Scenario()
    tall = sc.add(0, TALLGEESE, damage=damage)
    ids = {
        "tall": tall,
        "target": sc.add(1, VANILLA_2_2, rested=True),
        "ap1": sc.add(1, VANILLA_1_2),
        "ap4": sc.add(1, VANILLA_4_3_LV4),
    }
    st = sc.start()
    attack(st, tall, ids["target"])
    pass_all(st)
    return st, ids


@pytest.mark.card("ST12-003")
def test_st12_003_battle_destruction_deals_1_to_enemies_with_3_or_less_ap() -> None:
    st, ids = _tallgeese_battle()
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert st.cards[ids["ap1"]].damage == 1
    assert st.cards[ids["ap4"]].damage == 0


@pytest.mark.card("ST12-003")
def test_st12_003_not_during_opponents_turn() -> None:
    sc = Scenario(active=1)
    tall = sc.add(0, TALLGEESE, rested=True)
    attacker = sc.add(1, VANILLA_2_2)
    bystander = sc.add(1, VANILLA_1_2)
    st = sc.start()
    attack(st, attacker, tall)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[bystander].damage == 0


@pytest.mark.card("ST12-003")
@pytest.mark.ruling("ST12-003:Q438")
def test_st12_003_q438_triggers_when_both_units_are_destroyed() -> None:
    st, ids = _tallgeese_battle(damage=3)
    assert zone_of(st, ids["tall"]) is Zone.TRASH
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert st.cards[ids["ap1"]].damage == 1


@pytest.mark.card("ST12-003")
@pytest.mark.ruling("ST12-003:Q439")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: destruction by a Unit's own effect damage never counts as that Unit destroying it",
)
def test_st12_003_q439_triggers_when_destroying_with_effect_damage() -> None:
    sc = Scenario()
    tall = sc.add(0, TALLGEESE, pilot=MILLIARDO)
    target = sc.add(1, VANILLA_2_3, rested=True, damage=1)
    bystander = sc.add(1, VANILLA_1_2)
    st = sc.start()
    attack(st, tall, target)
    _to_action_step(st)
    activate(st, tall, _aid(MILLIARDO, 0, unit=True))
    assert zone_of(st, target) is Zone.TRASH
    assert st.cards[bystander].damage == 1


# ---------------------------------------------------------------------------------------------
# ST12-004 Gundam Exia, ST14-007 Full Armor Unicorn Gundam (Unicorn Mode)


@pytest.mark.card("ST12-004", "ST14-007")
@pytest.mark.rule("13-1-2-1")
@pytest.mark.parametrize("number", [EXIA, FA_UNICORN_UM])
def test_breach_3_destroys_top_shield_after_battle_destruction(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, number)
    target = sc.add(1, VANILLA_2_2, rested=True)
    top, second = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    assert keywords(st, unit).get("Breach") == 3
    attack(st, unit, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("ST12-004")
@pytest.mark.rule("13-1-2-3")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: <Breach> amount is read from the destroyed Unit, so it deals 0 damage",
)
def test_st12_004_breach_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    exia = sc.add(0, EXIA, damage=2)
    target = sc.add(1, VANILLA_3_3, rested=True)
    top, _ = sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, exia, target)
    pass_all(st)
    assert zone_of(st, exia) is Zone.TRASH
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# ST12-005 GQuuuuuuX (Omega Psycommu)


@pytest.mark.card("ST12-005")
def test_st12_005_attack_may_discard_then_draw() -> None:
    sc = Scenario()
    unit = sc.add(0, GQUUUUUUX)
    first, second = sc.hand(0, VANILLA_2_2, VANILLA_1_2)
    sc.deck(0, BERTIGO)
    st = sc.start()
    attack(st, unit)
    yes(st)
    assert _kind(st) is DecisionKind.DISCARD
    select(st, first)
    assert zone_of(st, first) is Zone.TRASH
    assert zone_of(st, second) is Zone.HAND
    assert card_numbers(st, st.zones[0][Zone.HAND]).count(BERTIGO) == 1


@pytest.mark.card("ST12-005")
def test_st12_005_declining_discard_draws_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, GQUUUUUUX)
    (card,) = sc.hand(0, VANILLA_2_2)
    st = sc.start()
    attack(st, unit)
    no(st)
    assert list(st.zones[0][Zone.HAND]) == [card]


@pytest.mark.card("ST12-005")
@pytest.mark.rule("5-20-1")
def test_st12_005_empty_hand_discards_nothing_and_draws_nothing() -> None:
    sc = Scenario()
    unit = sc.add(0, GQUUUUUUX)
    st = sc.start()
    attack(st, unit)
    yes(st)
    assert st.zones[0][Zone.HAND] == []


# ---------------------------------------------------------------------------------------------
# ST12-006 Unicorn Gundam 02 Banshee (Destroy Mode)


@pytest.mark.card("ST12-006")
@pytest.mark.rule("13-2-10")
def test_st12_006_paired_exiles_4_for_first_strike() -> None:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE_DM, pilot=PILOT_LV3)
    trash = sc.trash(0, *([VANILLA_2_2] * 4))
    st = sc.start()
    activate(st, banshee, _aid(BANSHEE_DM, 0))
    assert "First Strike" in keywords(st, banshee)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)


@pytest.mark.card("ST12-006")
def test_st12_006_first_strike_needs_pair_and_4_trash_cards() -> None:
    sc = Scenario()
    unpaired = sc.add(0, BANSHEE_DM)
    paired = sc.add(0, BANSHEE_DM, pilot=PILOT_LV3)
    sc.trash(0, *([VANILLA_2_2] * 3))
    st = sc.start()
    aid = _aid(BANSHEE_DM, 0)
    assert not has_action(st, A.ACTIVATE, unpaired, aid)
    assert not has_action(st, A.ACTIVATE, paired, aid)


@pytest.mark.card("ST12-006")
@pytest.mark.ruling("ST12-006:Q440")
@pytest.mark.rule("13-1-5-3")
def test_st12_006_q440_can_activate_while_having_first_strike() -> None:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE_DM, pilot=PILOT_LV3)
    sc.trash(0, *([VANILLA_2_2] * 8))
    st = sc.start()
    aid = _aid(BANSHEE_DM, 0)
    activate(st, banshee, aid)
    select(st, *st.zones[0][Zone.TRASH][:4])
    assert has_action(st, A.ACTIVATE, banshee, aid)
    activate(st, banshee, aid)
    assert st.zones[0][Zone.TRASH] == []
    assert keywords(st, banshee).get("First Strike") == 1


def _banshee_attack(trash: int) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE_DM)
    sc.trash(0, *([VANILLA_2_2] * trash))
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, banshee)
    _to_action_step(st)
    return st, banshee, shields


@pytest.mark.card("ST12-006")
@pytest.mark.rule("13-1-7-1", "8-6-1")
def test_st12_006_action_suppression_during_this_battle() -> None:
    st, banshee, shields = _banshee_attack(4)
    activate(st, banshee, _aid(BANSHEE_DM, 1))
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]
    assert st.battle is None
    assert "Suppression" not in keywords(st, banshee)


@pytest.mark.card("ST12-006")
@pytest.mark.ruling("ST12-006:Q441")
@pytest.mark.rule("13-1-7-2")
def test_st12_006_q441_second_activation_adds_no_second_suppression() -> None:
    st, banshee, shields = _banshee_attack(8)
    aid = _aid(BANSHEE_DM, 1)
    activate(st, banshee, aid)
    select(st, *st.zones[0][Zone.TRASH][:4])
    assert keywords(st, banshee).get("Suppression") == 1
    _to_action_step(st)
    activate(st, banshee, aid)
    assert st.zones[0][Zone.TRASH] == []
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]


@pytest.mark.card("ST12-006")
@pytest.mark.rule("8-2-3", "8-6-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'during this battle' effect created outside a battle never expires",
)
def test_st12_006_suppression_activated_outside_a_battle_does_not_persist() -> None:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE_DM)
    sc.trash(0, *([VANILLA_2_2] * 4))
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    activate(st, banshee, _aid(BANSHEE_DM, 1))
    to_next_turn(st)
    to_next_turn(st)
    assert "Suppression" not in keywords(st, banshee)


# ---------------------------------------------------------------------------------------------
# ST12-007 Gyan


@pytest.mark.card("ST12-007")
@pytest.mark.rule("13-2-11")
def test_st12_007_when_linked_gains_first_strike_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, GYAN)
    pilot = sc.add(0, MQUVE, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=gyan)
    assert "First Strike" in keywords(st, gyan)
    to_next_turn(st)
    assert "First Strike" not in keywords(st, gyan)


@pytest.mark.card("ST12-007")
def test_st12_007_non_link_pilot_gives_no_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, GYAN)
    pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=gyan)
    assert "First Strike" not in keywords(st, gyan)


@pytest.mark.card("ST12-007", "ST12-014")
@pytest.mark.rule("3-4-6")
def test_st12_014_pilot_mquve_links_gyan() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, GYAN)
    pride = sc.add(0, WISE_LEADER, Zone.HAND)
    st = sc.start()
    play(st, pride, onto=gyan)
    assert gyan in V.derived(st).linked
    assert "First Strike" in keywords(st, gyan)


# ---------------------------------------------------------------------------------------------
# ST12-009 Unicorn Gundam 02 Banshee (Unicorn Mode)


def _banshee_um_destroyed(
    my_shields: int, opp_shields: int, *, base: bool = False
) -> tuple[GameState, int, int]:
    sc = Scenario()
    banshee = sc.add(0, BANSHEE_UM)
    killer = sc.add(1, VANILLA_6_4, rested=True)
    sc.shields(0, *([VANILLA_2_2] * my_shields))
    sc.shields(1, *([VANILLA_2_2] * opp_shields))
    if base:
        sc.base(0)
    (lv6,) = sc.trash(0, BANSHEE_DM)
    sc.trash(0, "ST12-010")
    (kept,) = sc.hand(0, VANILLA_1_2)
    st = sc.start()
    attack(st, banshee, killer)
    pass_all(st)
    assert zone_of(st, banshee) is Zone.TRASH
    return st, lv6, kept


@pytest.mark.card("ST12-009")
@pytest.mark.ruling("ST12-009:Q442")
def test_st12_009_q442_own_3_shields_returns_lv6_unit_then_discards() -> None:
    st, lv6, kept = _banshee_um_destroyed(3, 6)
    assert _kind(st) is DecisionKind.DISCARD
    select(st, kept)
    assert zone_of(st, lv6) is Zone.HAND
    assert zone_of(st, kept) is Zone.TRASH


@pytest.mark.card("ST12-009")
def test_st12_009_opponent_with_3_or_less_shields_also_counts() -> None:
    st, lv6, kept = _banshee_um_destroyed(6, 2)
    select(st, lv6)
    assert zone_of(st, lv6) is Zone.TRASH
    assert zone_of(st, kept) is Zone.HAND
    assert _kind(st) is DecisionKind.MAIN


@pytest.mark.card("ST12-009")
@pytest.mark.ruling("ST12-009:Q443")
def test_st12_009_q443_base_is_not_a_shield() -> None:
    st, lv6, kept = _banshee_um_destroyed(3, 6, base=True)
    select(st, kept)
    assert zone_of(st, lv6) is Zone.HAND


@pytest.mark.card("ST12-009")
def test_st12_009_nothing_when_every_player_has_4_or_more_shields() -> None:
    st, lv6, kept = _banshee_um_destroyed(4, 4)
    assert _kind(st) is DecisionKind.MAIN
    assert zone_of(st, lv6) is Zone.TRASH
    assert zone_of(st, kept) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST12-011 Milliardo Peacecraft


@pytest.mark.card("ST12-011", "ST12-001")
@pytest.mark.rule("2-2-4", "3-2-6-3")
def test_st12_011_name_alias_links_zechs_units() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    epyon = sc.add(0, EPYON, deployed_this_turn=True)
    pilot = sc.add(0, MILLIARDO, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, epyon)
    play(st, pilot, onto=epyon)
    assert epyon in V.derived(st).linked
    assert has_action(st, A.ATTACK, epyon, PLAYER_TARGET)


@pytest.mark.card("ST12-011")
@pytest.mark.rule("13-2-5-1")
def test_st12_011_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    (shield,) = sc.shields(0, MILLIARDO)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert _kind(st) is DecisionKind.BURST
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


def _milliardo_attack(target_damage: int) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA_4_3, pilot=MILLIARDO)
    target = sc.add(1, VANILLA_3_4, rested=True, damage=target_damage)
    bystander = sc.add(1, VANILLA_2_2, damage=1)
    sc.add(0, TWO_UNICORNS, Zone.HAND)
    st = sc.start()
    attack(st, unit, target)
    return st, unit, target, bystander


@pytest.mark.card("ST12-011")
def test_st12_011_deals_2_to_damaged_enemy_battling_this_unit_once_per_turn() -> None:
    st, unit, target, bystander = _milliardo_attack(1)
    _to_action_step(st)
    aid = _aid(MILLIARDO, 0, unit=True)
    activate(st, unit, aid)
    _to_action_step(st)
    assert st.cards[target].damage == 3
    assert st.cards[bystander].damage == 1
    assert not has_action(st, A.ACTIVATE, unit, aid)


@pytest.mark.card("ST12-011")
def test_st12_011_needs_a_damaged_enemy_battling_this_unit() -> None:
    st, unit, _, _ = _milliardo_attack(0)
    _to_action_step(st)
    assert not has_action(st, A.ACTIVATE, unit, _aid(MILLIARDO, 0, unit=True))


# ---------------------------------------------------------------------------------------------
# ST12-012 Ple-Twelve


def _ple_twelve_links(
    my_shields: int, opp_shields: int, *, base: bool = False
) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    banshee = sc.add(0, BANSHEE_DM)
    pilot = sc.add(0, PLE_TWELVE, Zone.HAND)
    sc.shields(0, *([VANILLA_2_2] * my_shields))
    sc.shields(1, *([VANILLA_2_2] * opp_shields))
    if base:
        sc.base(0)
    sc.deck(0, "ST12-008", "ST12-010")
    st = sc.start()
    first, second = st.zones[0][Zone.DECK][:2]
    play(st, pilot, onto=banshee)
    assert banshee in V.derived(st).linked
    assert _kind(st) is DecisionKind.SELECT
    return st, first, second


@pytest.mark.card("ST12-012", "ST12-006")
@pytest.mark.rule("2-2-4", "13-2-11")
def test_st12_012_when_linked_keeps_one_on_top_and_trashes_the_other() -> None:
    st, first, second = _ple_twelve_links(4, 4)
    select(st, second)
    assert zone_of(st, first) is Zone.TRASH
    assert st.zones[0][Zone.DECK][0] == second


@pytest.mark.card("ST12-012")
@pytest.mark.ruling("ST12-012:Q444")
def test_st12_012_q444_own_3_shields_adds_kept_card_to_hand() -> None:
    st, first, second = _ple_twelve_links(3, 6)
    select(st, first)
    assert zone_of(st, first) is Zone.HAND
    assert zone_of(st, second) is Zone.TRASH


@pytest.mark.card("ST12-012")
@pytest.mark.ruling("ST12-012:Q445")
def test_st12_012_q445_base_is_not_a_shield() -> None:
    st, first, _ = _ple_twelve_links(3, 6, base=True)
    select(st, first)
    assert zone_of(st, first) is Zone.HAND


@pytest.mark.card("ST12-012")
def test_st12_012_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    (shield,) = sc.shields(0, PLE_TWELVE)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST12-013 The Final Victor


@pytest.mark.card("ST12-013")
@pytest.mark.ruling("ST12-013:Q446")
@pytest.mark.rule("5-22-4", "5-22-4-1")
def test_st12_013_q446_damage_step_only_battle_between_chosen_units() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    mine = sc.add(0, VANILLA_6_4)
    small = sc.add(1, VANILLA_2_2)
    chosen = sc.add(1, VANILLA_3_4)
    sc.add(1, BLOCKER)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    play(st, victor)
    assert st.pending is not None and st.pending.player == 1
    select(st, chosen)
    assert _kind(st) is DecisionKind.MAIN
    assert zone_of(st, chosen) is Zone.TRASH
    assert st.cards[mine].damage == 3
    assert not st.cards[mine].rested
    assert st.cards[small].damage == 0
    assert st.battle is None


@pytest.mark.card("ST12-013", "ST12-003")
@pytest.mark.ruling("ST12-013:Q448")
def test_st12_013_q448_effect_battle_destruction_is_battle_destruction() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, TALLGEESE)
    chosen = sc.add(1, VANILLA_2_2)
    bystander = sc.add(1, VANILLA_1_2)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    play(st, victor)
    select(st, chosen)
    assert zone_of(st, chosen) is Zone.TRASH
    assert st.cards[bystander].damage == 1


@pytest.mark.card("ST12-013", "ST14-006")
@pytest.mark.ruling("ST12-013:Q447")
def test_st12_013_q447_enemy_battle_effects_apply_in_effect_battles() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    mine = sc.add(0, VANILLA_3_4)
    unicorn = sc.add(1, FA_UNICORN_DM, pilot=PILOT_LV3)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    play(st, victor)
    assert st.cards[unicorn].damage == 0
    assert zone_of(st, mine) is Zone.TRASH


@pytest.mark.card("ST12-013")
@pytest.mark.rule("13-1-5-4", "5-22-4")
def test_st12_013_first_chosen_unit_attacks_with_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    mine = sc.add(0, FIRST_STRIKE_1_4, damage=3)
    theirs = sc.add(1, GYAN)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    play(st, victor)
    assert zone_of(st, theirs) is Zone.TRASH
    assert zone_of(st, mine) is Zone.BATTLE


@pytest.mark.card("ST12-013")
@pytest.mark.rule("10-1-8-1-1")
def test_st12_013_needs_units_on_both_sides() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, VANILLA_2_2)
    victor = sc.add(0, FINAL_VICTOR, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, victor)


@pytest.mark.card("ST12-013")
def test_st12_013_burst_deals_1_damage_to_enemy_unit() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_3_4)
    sc.shields(0, FINAL_VICTOR)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert st.cards[attacker].damage == 1


# ---------------------------------------------------------------------------------------------
# ST12-014 Wise Leader's Pride


def _pride_battle(
    unit: str, target: str, *, pilot: str | None = None, damage: int = 0
) -> tuple[GameState, dict[str, int]]:
    sc = Scenario()
    sc.resources(0, 6)
    ids = {
        "unit": sc.add(0, unit, pilot=pilot, damage=damage),
        "target": sc.add(1, target, rested=True),
        "ap1": sc.add(1, VANILLA_1_2),
        "ap3": sc.add(1, VANILLA_3_4),
        "pride": sc.add(0, WISE_LEADER, Zone.HAND),
        "victor": sc.add(0, FINAL_VICTOR, Zone.HAND),
    }
    st = sc.start()
    attack(st, ids["unit"], ids["target"])
    _to_action_step(st)
    play(st, ids["pride"])
    pass_all(st)
    return st, ids


@pytest.mark.card("ST12-014")
def test_st12_014_granted_effect_destroys_enemy_with_2_or_less_ap() -> None:
    st, ids = _pride_battle(VANILLA_6_4, VANILLA_2_2, pilot=PILOT_LV5)
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert zone_of(st, ids["ap1"]) is Zone.TRASH
    assert zone_of(st, ids["ap3"]) is Zone.BATTLE


@pytest.mark.card("ST12-014")
@pytest.mark.ruling("ST12-014:Q449")
def test_st12_014_q449_triggers_when_both_units_are_destroyed() -> None:
    st, ids = _pride_battle(VANILLA_3_4, VANILLA_3_3, damage=2)
    assert zone_of(st, ids["unit"]) is Zone.TRASH
    assert zone_of(st, ids["target"]) is Zone.TRASH
    assert zone_of(st, ids["ap1"]) is Zone.TRASH


@pytest.mark.card("ST12-014")
@pytest.mark.rule("8-6-1")
def test_st12_014_granted_effect_ends_with_the_battle() -> None:
    st, ids = _pride_battle(VANILLA_6_4, VANILLA_2_2, pilot=PILOT_LV5)
    play(st, ids["victor"])
    assert zone_of(st, ids["ap3"]) is Zone.TRASH
    assert zone_of(st, ids["unit"]) is Zone.BATTLE
    assert _kind(st) is DecisionKind.MAIN


@pytest.mark.card("ST12-014")
@pytest.mark.rule("8-2-3", "8-6-1")
@pytest.mark.xfail(
    strict=True,
    reason="ENGINE: a 'during this battle' effect created outside a battle never expires",
)
def test_st12_014_played_outside_a_battle_grants_nothing_lasting() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA_6_4, rested=True)
    attacker = sc.add(1, VANILLA_2_2)
    bystander = sc.add(1, VANILLA_1_2)
    pride = sc.add(0, WISE_LEADER, Zone.HAND)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    play(st, pride)
    assert st.active == 1 and _kind(st) is DecisionKind.MAIN
    attack(st, attacker, unit)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, bystander) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# ST12-015 Two Unicorns


def _two_unicorns(enemies: tuple[str, ...], *, friendly: bool = True) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    if friendly:
        sc.add(0, VANILLA_3_4)
    for number in enemies:
        sc.add(1, number)
    card = sc.add(0, TWO_UNICORNS, Zone.HAND)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    return st, card


@pytest.mark.card("ST12-015")
def test_st12_015_mode_1_destroys_enemy_lv2_or_lower() -> None:
    st, card = _two_unicorns((VANILLA_2_2, VANILLA_5_4))
    small, big = st.zones[1][Zone.BATTLE]
    play(st, card)
    assert _mode_options(st) == [0, 1]
    choose_option(st, 0)
    assert zone_of(st, small) is Zone.TRASH
    assert st.cards[big].damage == 0


@pytest.mark.card("ST12-015")
def test_st12_015_mode_2_deals_2_to_friendly_and_enemy_lv5_or_higher() -> None:
    st, card = _two_unicorns((VANILLA_2_2, VANILLA_5_4))
    small, big = st.zones[1][Zone.BATTLE]
    (mine,) = st.zones[0][Zone.BATTLE]
    play(st, card)
    choose_option(st, 1)
    assert st.cards[mine].damage == 2
    assert st.cards[big].damage == 2
    assert st.cards[small].damage == 0


@pytest.mark.card("ST12-015")
@pytest.mark.ruling("ST12-015:Q450")
@pytest.mark.rule("10-1-8-1-1")
def test_st12_015_q450_mode_1_needs_an_enemy_lv2_or_lower() -> None:
    st, card = _two_unicorns((VANILLA_5_4,))
    play(st, card)
    assert _mode_options(st) == [1]


@pytest.mark.card("ST12-015")
@pytest.mark.ruling("ST12-015:Q451")
@pytest.mark.parametrize("friendly", [True, False])
def test_st12_015_q451_mode_2_needs_friendly_and_enemy_lv5_or_higher(friendly: bool) -> None:
    enemies = (VANILLA_2_2,) if friendly else (VANILLA_2_2, VANILLA_5_4)
    st, card = _two_unicorns(enemies, friendly=friendly)
    play(st, card)
    assert _mode_options(st) == [0]


@pytest.mark.card("ST12-015")
@pytest.mark.rule("10-1-8-1-1")
@pytest.mark.xfail(
    strict=True, reason="ENGINE: a modal Command is playable even when no mode has a legal target"
)
def test_st12_015_not_playable_without_a_legal_mode() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, VANILLA_3_4)
    sc.add(1, VANILLA_3_4)
    card = sc.add(0, TWO_UNICORNS, Zone.HAND)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    assert not has_action(st, A.PLAY_COMMAND, card)


# ---------------------------------------------------------------------------------------------
# ST12-016 Libra


@pytest.mark.card("ST12-016")
@pytest.mark.rule("13-2-5-1")
def test_st12_016_burst_deploys_libra() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    shield, other = sc.shields(0, LIBRA, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.BASE
    assert zone_of(st, other) is Zone.HAND


@pytest.mark.card("ST12-016")
def test_st12_016_deploy_adds_top_shield_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    libra = sc.add(0, LIBRA, Zone.HAND)
    top, second = sc.shields(0, VANILLA_2_2, VANILLA_1_2)
    st = sc.start()
    play(st, libra)
    assert zone_of(st, libra) is Zone.BASE
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, second) is Zone.SHIELD


def _libra_after_battle(
    *, pilot: str | None, unit: str = VANILLA_6_4, damage: int = 0, pair_after: bool = False
) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    libra = sc.base(0, LIBRA)
    attacker = sc.add(0, unit, pilot=pilot, damage=damage)
    target = sc.add(1, VANILLA_3_3, rested=True)
    victim = sc.add(1, VANILLA_3_4)
    late_pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    st = sc.start()
    attack(st, attacker, target)
    pass_all(st)
    assert zone_of(st, target) is Zone.TRASH
    if pair_after:
        play(st, late_pilot, onto=attacker)
    activate(st, libra, _aid(LIBRA, 2))
    assert st.cards[libra].rested
    return st, libra, victim


@pytest.mark.card("ST12-016")
def test_st12_016_no_damage_after_unpaired_unit_destroys() -> None:
    st, _, victim = _libra_after_battle(pilot=None)
    assert st.cards[victim].damage == 0


@pytest.mark.card("ST12-016")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no turn-history record of battle destructions by paired Units",
)
def test_st12_016_damage_after_paired_unit_destroys_with_battle_damage() -> None:
    st, _, victim = _libra_after_battle(pilot=PILOT_LV3)
    assert st.cards[victim].damage == 1


@pytest.mark.card("ST12-016")
@pytest.mark.ruling("ST12-016:Q452")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no turn-history record of battle destructions by paired Units",
)
def test_st12_016_q452_counts_when_both_units_are_destroyed() -> None:
    st, _, victim = _libra_after_battle(pilot=PILOT_LV3, unit=VANILLA_3_4, damage=2)
    assert st.cards[victim].damage == 1


@pytest.mark.card("ST12-016")
@pytest.mark.ruling("ST12-016:Q453")
def test_st12_016_q453_pairing_after_the_destruction_does_not_count() -> None:
    st, _, victim = _libra_after_battle(pilot=None, pair_after=True)
    assert st.cards[victim].damage == 0


# ---------------------------------------------------------------------------------------------
# ST13-001 Qubeley


@pytest.mark.card("ST13-001")
@pytest.mark.rule("5-17")
def test_st13_001_pairing_deploys_1_to_2_bit_funnels_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    qubeley = sc.add(0, QUBELEY)
    other = sc.add(0, VANILLA_2_2)
    first, second = sc.hand(0, PILOT_LV3, PILOT_LV3)
    st = sc.start()
    play(st, first, onto=other)
    assert _kind(st) is DecisionKind.YES_NO
    yes(st)
    tokens = _tokens(st, 0)
    assert len(tokens) == 2
    assert all(ap(st, t) == 2 and V.hp_of(st, V.derived(st), t) == 2 for t in tokens)
    assert all(V.cdef(st, t).traits == ("Long-Range Weapon",) for t in tokens)
    play(st, second, onto=qubeley)
    assert len(_tokens(st, 0)) == 2


@pytest.mark.card("ST13-001")
def test_st13_001_second_token_is_optional() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    qubeley = sc.add(0, QUBELEY)
    pilot = sc.add(0, PILOT_LV3, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=qubeley)
    no(st)
    assert len(_tokens(st, 0)) == 1


def _qubeley_with_tokens(*enemies: str, extra_hand: tuple[str, ...] = ()) -> GameState:
    sc = Scenario()
    sc.resources(0, 5)
    sc.add(0, QUBELEY)
    newtype = sc.add(0, NEWTYPE, Zone.HAND)
    for number in extra_hand:
        sc.add(0, number, Zone.HAND)
    for number in enemies:
        sc.add(1, number)
    sc.shields(1, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    play(st, newtype)
    yes(st)
    return st


@pytest.mark.card("ST13-001")
@pytest.mark.ruling("ST13-001:Q454")
@pytest.mark.rule("5-22-4")
def test_st13_001_q454_token_battles_enemy_unit_with_damage_step_only() -> None:
    st = _qubeley_with_tokens(VANILLA_2_2, BLOCKER)
    qubeley = st.zones[0][Zone.BATTLE][0]
    token = _tokens(st, 0)[0]
    target, blocker = st.zones[1][Zone.BATTLE]
    aid = _aid(QUBELEY, 1)
    activate(st, qubeley, aid)
    select(st, token)
    select(st, target)
    assert _kind(st) is DecisionKind.MAIN
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, token) is Zone.OUTSIDE
    assert not st.cards[blocker].rested
    assert len(_tokens(st, 0)) == 1
    assert not has_action(st, A.ACTIVATE, qubeley, aid)


@pytest.mark.card("ST13-001", "ST14-006")
@pytest.mark.ruling("ST13-001:Q455")
def test_st13_001_q455_enemy_battle_effects_apply_in_effect_battles() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    qubeley = sc.add(0, QUBELEY)
    elmeth = sc.add(0, ELMETH, Zone.HAND)
    unicorn = sc.add(1, FA_UNICORN_DM, pilot=PILOT_LV3)
    st = sc.start()
    play(st, elmeth)
    (token,) = _tokens(st, 0)
    activate(st, qubeley, _aid(QUBELEY, 1))
    assert st.cards[unicorn].damage == 0
    assert zone_of(st, token) is Zone.OUTSIDE


@pytest.mark.card("ST13-001")
@pytest.mark.ruling("ST13-001:Q456")
@pytest.mark.rule("13-1-2-1")
def test_st13_001_q456_effect_battle_destruction_triggers_breach() -> None:
    st = _qubeley_with_tokens(VANILLA_1_2, extra_hand=(BREACH_COMMAND,))
    qubeley = st.zones[0][Zone.BATTLE][0]
    token = _tokens(st, 0)[0]
    (breach,) = [u for u in st.zones[0][Zone.HAND] if V.cdef(st, u).card_number == BREACH_COMMAND]
    (target,) = st.zones[1][Zone.BATTLE]
    top, second = st.zones[1][Zone.SHIELD]
    play(st, breach)
    select(st, token)
    assert keywords(st, token).get("Breach") == 3
    activate(st, qubeley, _aid(QUBELEY, 1))
    select(st, token)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, token) is Zone.BATTLE
    assert zone_of(st, top) is Zone.TRASH
    assert zone_of(st, second) is Zone.SHIELD


# ---------------------------------------------------------------------------------------------
# ST13-002 Elmeth, ST13-013 I'm a Newtype, ST13-016 Sodon


@pytest.mark.card("ST13-002")
def test_st13_002_deploy_deploys_one_bit_funnel() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    elmeth = sc.add(0, ELMETH, Zone.HAND)
    st = sc.start()
    play(st, elmeth)
    (token,) = _tokens(st, 0)
    assert ap(st, token) == 2
    assert V.cdef(st, token).is_token


@pytest.mark.card("ST13-013")
@pytest.mark.parametrize(("second", "expected"), [(True, 2), (False, 1)])
def test_st13_013_deploys_1_to_2_bit_funnels(second: bool, expected: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, NEWTYPE, Zone.HAND)
    st = sc.start()
    play(st, card)
    if second:
        yes(st)
    else:
        no(st)
    assert len(_tokens(st, 0)) == expected
    assert zone_of(st, card) is Zone.TRASH


@pytest.mark.card("ST13-013", "ST13-003")
@pytest.mark.rule("3-4-6")
def test_st13_013_pilot_carris_nautilus_links_bertigo() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    bertigo = sc.add(0, BERTIGO)
    card = sc.add(0, NEWTYPE, Zone.HAND)
    st = sc.start()
    play(st, card, onto=bertigo)
    assert bertigo in V.derived(st).linked


@pytest.mark.card("ST13-016")
@pytest.mark.rule("5-20-2")
@pytest.mark.parametrize("shields", [2, 0])
def test_st13_016_deploy_adds_shield_then_deploys_bit_funnel(shields: int) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sodon = sc.add(0, SODON, Zone.HAND)
    stack = sc.shields(0, *([VANILLA_2_2] * shields))
    st = sc.start()
    play(st, sodon)
    assert zone_of(st, sodon) is Zone.BASE
    assert [zone_of(st, s) for s in stack] == [Zone.HAND, Zone.SHIELD][:shields]
    assert len(_tokens(st, 0)) == 1


@pytest.mark.card("ST13-016")
@pytest.mark.rule("13-2-5-1")
def test_st13_016_burst_deploys_sodon() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    shield, other = sc.shields(0, SODON, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.BASE
    assert zone_of(st, other) is Zone.HAND
    assert len(_tokens(st, 0)) == 1


# ---------------------------------------------------------------------------------------------
# ST13-004 GX-Bit


@pytest.mark.card("ST13-004")
@pytest.mark.parametrize("bottom", [True, False])
def test_st13_004_deploy_returns_top_card_to_top_or_bottom(bottom: bool) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, GX_BIT, Zone.HAND)
    sc.deck(0, BERTIGO)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, unit)
    assert _kind(st) is DecisionKind.ARRANGE
    act(st, A.SELECT, 1 if bottom else 0)
    assert st.zones[0][Zone.DECK][-1 if bottom else 0] == top


# ---------------------------------------------------------------------------------------------
# ST13-005 GFreD


def _gfred(enemies: int) -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GFRED, Zone.HAND)
    for _ in range(enemies):
        sc.add(1, VANILLA_2_2)
    sc.deck(0, SCIROCCO, "ST12-008", "ST12-008", "ST12-008", "ST12-008", "ST12-010")
    st = sc.start()
    top6 = list(st.zones[0][Zone.DECK][:6])
    play(st, unit)
    return st, top6


@pytest.mark.card("ST13-005")
def test_st13_005_reveals_pilot_from_top_5_with_4_enemy_units() -> None:
    st, top6 = _gfred(4)
    yes(st)
    assert zone_of(st, top6[0]) is Zone.HAND
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top6[5]
    assert sorted(deck[-4:]) == sorted(top6[1:5])


@pytest.mark.card("ST13-005")
@pytest.mark.ruling("ST13-005:Q457")
def test_st13_005_q457_look_is_mandatory_adding_is_optional() -> None:
    st, top6 = _gfred(4)
    assert _kind(st) is DecisionKind.YES_NO
    no(st)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top6[5]
    assert sorted(deck[-5:]) == sorted(top6[:5])


@pytest.mark.card("ST13-005")
def test_st13_005_nothing_with_3_enemy_units() -> None:
    st, top6 = _gfred(3)
    assert _kind(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.DECK][:6] == top6


# ---------------------------------------------------------------------------------------------
# ST13-006 Gundam Aerial


def _aerial_attack(*, pilot: str | None, at_unit: bool) -> tuple[GameState, int, int]:
    sc = Scenario()
    aerial = sc.add(0, AERIAL, pilot=pilot)
    rested = sc.add(1, VANILLA_3_4, rested=True)
    bystander = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, aerial, rested if at_unit else PLAYER_TARGET)
    if _kind(st) is DecisionKind.SELECT:
        select(st, bystander)
    pass_all(st)
    return st, rested, bystander


@pytest.mark.card("ST13-006")
@pytest.mark.rule("13-2-10")
def test_st13_006_paired_attack_on_player_deals_2_to_enemy_unit() -> None:
    st, rested, bystander = _aerial_attack(pilot=PILOT_LV3, at_unit=False)
    assert zone_of(st, bystander) is Zone.TRASH
    assert st.cards[rested].damage == 0


@pytest.mark.card("ST13-006")
@pytest.mark.parametrize(("pilot", "at_unit"), [(PILOT_LV3, True), (None, False)])
def test_st13_006_no_damage_when_attacking_a_unit_or_unpaired(
    pilot: str | None, at_unit: bool
) -> None:
    st, _, bystander = _aerial_attack(pilot=pilot, at_unit=at_unit)
    assert zone_of(st, bystander) is Zone.BATTLE
    assert st.cards[bystander].damage == 0


@pytest.mark.card("ST13-006")
def test_st13_006_rest_friendly_unit_to_deal_1_to_enemy_lv4_or_lower_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aerial = sc.add(0, AERIAL)
    helper = sc.add(0, VANILLA_2_2)
    lv4 = sc.add(1, VANILLA_3_4)
    lv5 = sc.add(1, VANILLA_4_3)
    sc.add(1, VANILLA_1_2)
    sc.add(0, TWO_UNICORNS, Zone.HAND)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    aid = _aid(AERIAL, 1)
    activate(st, aerial, aid)
    select(st, helper)
    assert st.cards[helper].rested
    assert not st.cards[aerial].rested
    select(st, lv4)
    assert st.cards[lv4].damage == 1
    assert st.cards[lv5].damage == 0
    _to_action_step(st)
    assert not has_action(st, A.ACTIVATE, aerial, aid)


@pytest.mark.card("ST13-006")
@pytest.mark.rule("10-2-2")
def test_st13_006_enemy_lv5_is_not_a_legal_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    aerial = sc.add(0, AERIAL)
    sc.add(0, VANILLA_2_2)
    sc.add(1, VANILLA_4_3)
    solomon = sc.add(0, SOLOMON, Zone.HAND)
    sc.trash(0, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    assert has_action(st, A.PLAY_COMMAND, solomon)
    assert not has_action(st, A.ACTIVATE, aerial, _aid(AERIAL, 1))


# ---------------------------------------------------------------------------------------------
# ST13-007 Gyunei's Jagd Doga


@pytest.mark.card("ST13-007")
@pytest.mark.rule("13-1-3")
def test_st13_007_support_1() -> None:
    sc = Scenario()
    jagd = sc.add(0, JAGD_DOGA)
    other = sc.add(0, VANILLA_2_2)
    st = sc.start()
    activate(st, jagd, SUPPORT_AID)
    assert st.cards[jagd].rested
    assert ap(st, other) == 3


# ---------------------------------------------------------------------------------------------
# ST13-009 Gundam Pharact, ST14-003 Palace Athene


@pytest.mark.card("ST13-009")
def test_st13_009_attack_opponent_exiles_2_unit_cards_from_their_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, PHARACT)
    a, b, c = sc.trash(1, VANILLA_2_2, VANILLA_1_2, VANILLA_3_4)
    (command,) = sc.trash(1, FINAL_VICTOR)
    st = sc.start()
    attack(st, unit)
    assert st.pending is not None and st.pending.player == 1
    select(st, a, c)
    assert [zone_of(st, u) for u in (a, b, c, command)] == [
        Zone.REMOVAL,
        Zone.TRASH,
        Zone.REMOVAL,
        Zone.TRASH,
    ]


@pytest.mark.card("ST13-009")
def test_st13_009_needs_2_unit_cards_in_enemy_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, PHARACT)
    (only,) = sc.trash(1, VANILLA_2_2)
    sc.trash(1, FINAL_VICTOR)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, only) is Zone.TRASH


@pytest.mark.card("ST14-003")
@pytest.mark.parametrize("units_in_trash", [2, 1])
def test_st14_003_deploy_opponent_exiles_2_unit_cards(units_in_trash: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    athene = sc.add(0, PALACE_ATHENE, Zone.HAND)
    trash = sc.trash(1, *([VANILLA_2_2] * units_in_trash))
    st = sc.start()
    play(st, athene)
    expected = Zone.REMOVAL if units_in_trash == 2 else Zone.TRASH
    assert all(zone_of(st, u) is expected for u in trash)


# ---------------------------------------------------------------------------------------------
# ST13-010 Red Gundam (0079)


@pytest.mark.card("ST13-010")
def test_st13_010_destroy_token_for_breach_3_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    red = sc.add(0, RED_GUNDAM)
    newtype = sc.add(0, NEWTYPE, Zone.HAND)
    st = sc.start()
    aid = _aid(RED_GUNDAM, 0)
    assert not has_action(st, A.ACTIVATE, red, aid)
    play(st, newtype)
    yes(st)
    first, second = _tokens(st, 0)
    activate(st, red, aid)
    select(st, first)
    assert zone_of(st, first) is Zone.OUTSIDE
    assert zone_of(st, second) is Zone.BATTLE
    assert keywords(st, red).get("Breach") == 3
    assert not has_action(st, A.ACTIVATE, red, aid)


# ---------------------------------------------------------------------------------------------
# ST13-011 Haman Karn


def _haman_paired() -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, VANILLA_4_3)
    pilot = sc.add(0, HAMAN, Zone.HAND)
    sc.deck(0, HAMAN, SCIROCCO, "ST12-008", "ST12-008", "ST12-008", "ST12-010")
    st = sc.start()
    top6 = list(st.zones[0][Zone.DECK][:6])
    play(st, pilot, onto=unit)
    return st, top6


@pytest.mark.card("ST13-011")
def test_st13_011_reveals_pilot_with_lv_up_to_this_units_lv() -> None:
    st, top6 = _haman_paired()
    yes(st)
    assert zone_of(st, top6[1]) is Zone.HAND
    assert zone_of(st, top6[0]) is Zone.DECK
    assert st.zones[0][Zone.DECK][0] == top6[5]


@pytest.mark.card("ST13-011")
@pytest.mark.ruling("ST13-011:Q458")
def test_st13_011_q458_look_is_mandatory_adding_is_optional() -> None:
    st, top6 = _haman_paired()
    assert _kind(st) is DecisionKind.YES_NO
    no(st)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top6[5]
    assert sorted(deck[-5:]) == sorted(top6[:5])


# ---------------------------------------------------------------------------------------------
# ST13-012 Suletta Mercury


@pytest.mark.card("ST13-012", "ST13-006")
def test_st13_012_draws_when_enemy_destroyed_by_effect_damage_while_attacking() -> None:
    sc = Scenario()
    aerial = sc.add(0, AERIAL, pilot=SULETTA)
    enemy = sc.add(1, VANILLA_2_2)
    sc.deck(0, BERTIGO)
    st = sc.start()
    attack(st, aerial)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert card_numbers(st, st.zones[0][Zone.HAND]) == [BERTIGO]


def _suletta_attack(resources: int = 4) -> tuple[GameState, dict[str, int]]:
    sc = Scenario()
    sc.resources(0, resources)
    ids = {
        "unit": sc.add(0, VANILLA_4_3, pilot=SULETTA),
        "damaged": sc.add(1, VANILLA_2_2, damage=1),
        "unicorns": sc.add(0, TWO_UNICORNS, Zone.HAND),
        "solomon": sc.add(0, SOLOMON, Zone.HAND),
    }
    sc.trash(0, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    return sc.start(), ids


@pytest.mark.card("ST13-012")
@pytest.mark.ruling("ST13-012:Q459")
def test_st13_012_q459_destroy_effect_on_damaged_unit_does_not_draw() -> None:
    st, ids = _suletta_attack()
    attack(st, ids["unit"])
    _to_action_step(st)
    play(st, ids["unicorns"])
    choose_option(st, 0)
    assert zone_of(st, ids["damaged"]) is Zone.TRASH
    assert card_numbers(st, st.zones[0][Zone.HAND]) == [SOLOMON]


@pytest.mark.card("ST13-012", "ST13-015")
def test_st13_012_effect_damage_from_a_command_counts() -> None:
    st, ids = _suletta_attack()
    attack(st, ids["unit"])
    _to_action_step(st)
    play(st, ids["solomon"])
    assert zone_of(st, ids["damaged"]) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 2


@pytest.mark.card("ST13-012", "ST13-006")
@pytest.mark.rule("10-1-6-1-1")
def test_st13_012_draws_once_per_turn() -> None:
    sc = Scenario()
    aerial = sc.add(0, AERIAL, pilot=SULETTA)
    sc.add(0, VANILLA_2_2)
    first = sc.add(1, VANILLA_2_2)
    second = sc.add(1, VANILLA_1_2, damage=1)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, aerial)
    select(st, first)
    assert zone_of(st, first) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1
    _to_action_step(st)
    activate(st, aerial, _aid(AERIAL, 1))
    assert zone_of(st, second) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST13-012", "ST13-006")
def test_st13_012_activated_effect_damage_during_the_attack_draws() -> None:
    sc = Scenario()
    aerial = sc.add(0, AERIAL, pilot=SULETTA)
    sc.add(0, VANILLA_2_2)
    target = sc.add(1, VANILLA_3_4, rested=True)
    weak = sc.add(1, VANILLA_1_2, damage=1)
    st = sc.start()
    attack(st, aerial, target)
    _to_action_step(st)
    activate(st, aerial, _aid(AERIAL, 1))
    select(st, weak)
    assert zone_of(st, weak) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST13-012")
def test_st13_012_no_draw_when_not_attacking() -> None:
    st, ids = _suletta_attack()
    play(st, ids["solomon"])
    assert zone_of(st, ids["damaged"]) is Zone.TRASH
    assert card_numbers(st, st.zones[0][Zone.HAND]) == [TWO_UNICORNS]


@pytest.mark.card("ST13-012")
def test_st13_012_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    (shield,) = sc.shields(0, SULETTA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST13-014 Final Duty


def _final_duty(*, own_unit: bool = True) -> tuple[GameState, int, list[int]]:
    sc = Scenario()
    sc.resources(0, 6)
    if own_unit:
        sc.add(0, VANILLA_2_2)
    card = sc.add(0, FINAL_DUTY, Zone.HAND)
    sc.deck(0, ELMETH, "ST12-010", THE_O, BERTIGO, "ST12-008")
    st = sc.start()
    return st, card, list(st.zones[0][Zone.DECK][:5])


@pytest.mark.card("ST13-014")
@pytest.mark.ruling("ST13-014:Q460", "ST13-014:Q461")
def test_st13_014_q460_q461_destroy_own_unit_then_deploy_lv4_or_lower_for_free() -> None:
    st, card, top5 = _final_duty()
    (mine,) = st.zones[0][Zone.BATTLE]
    elmeth, lv2, the_o, bertigo, fifth = top5
    play(st, card)
    assert zone_of(st, mine) is Zone.TRASH
    yes(st)
    assert st.pending is not None
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {elmeth, lv2, bertigo}
    select(st, elmeth)
    assert zone_of(st, elmeth) is Zone.BATTLE
    assert len(_tokens(st, 0)) == 1
    assert _active_resources(st, 0) == 4
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == fifth
    assert sorted(deck[-3:]) == sorted((lv2, the_o, bertigo))


@pytest.mark.card("ST13-014")
def test_st13_014_declining_deploy_returns_all_4_to_bottom() -> None:
    st, card, top5 = _final_duty()
    play(st, card)
    no(st)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top5[4]
    assert sorted(deck[-4:]) == sorted(top5[:4])


@pytest.mark.card("ST13-014")
@pytest.mark.rule("10-1-8-1-1")
def test_st13_014_needs_one_of_your_units() -> None:
    st, card, _ = _final_duty(own_unit=False)
    assert not has_action(st, A.PLAY_COMMAND, card)


# ---------------------------------------------------------------------------------------------
# ST13-015 Operation to Intercept Solomon


@pytest.mark.card("ST13-015")
def test_st13_015_exile_3_unit_cards_to_deal_3_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, SOLOMON, Zone.HAND)
    trash = sc.trash(0, VANILLA_2_2, VANILLA_1_2, VANILLA_3_4)
    enemy = sc.add(1, VANILLA_3_3)
    st = sc.start()
    play(st, card)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("ST13-015")
@pytest.mark.rule("10-1-8-1-1", "10-2-2-1")
def test_st13_015_needs_3_unit_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, SOLOMON, Zone.HAND)
    sc.trash(0, VANILLA_2_2, VANILLA_1_2, FINAL_VICTOR)
    sc.add(1, VANILLA_3_3)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, card)


@pytest.mark.card("ST13-015")
@pytest.mark.rule("10-1-8-1-2")
def test_st13_015_enemy_unit_is_not_required_to_play() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, SOLOMON, Zone.HAND)
    trash = sc.trash(0, VANILLA_2_2, VANILLA_1_2, VANILLA_3_4)
    st = sc.start(Step.END_ACTION)
    _to_action_step(st)
    play(st, card)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)


@pytest.mark.card("ST13-015")
def test_st13_015_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    (shield,) = sc.shields(0, SOLOMON)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST14-001 The-O


def _the_o_opponent_turn(
    enemies: tuple[tuple[str, bool], ...], *, opp_hand: tuple[str, ...] = ()
) -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.add(0, THE_O)
    ids = [sc.add(1, number, rested=rested) for number, rested in enemies]
    sc.resources(1, 3)
    for number in opp_hand:
        sc.add(1, number, Zone.HAND)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 1 and _kind(st) is DecisionKind.MAIN
    return st, ids


@pytest.mark.card("ST14-001")
@pytest.mark.ruling("ST14-001:Q462")
@pytest.mark.rule("7-2-3-1")
def test_st14_001_q462_all_tied_lowest_lv_rested_units_stay_rested() -> None:
    st, (lv3a, lv3b, lv5) = _the_o_opponent_turn(
        ((VANILLA_3_3, True), (VANILLA_3_3, True), (VANILLA_4_3, True))
    )
    assert st.cards[lv3a].rested
    assert st.cards[lv3b].rested
    assert not st.cards[lv5].rested


@pytest.mark.card("ST14-001")
def test_st14_001_lowest_lv_is_taken_among_rested_units() -> None:
    st, (lv1, lv3, lv5) = _the_o_opponent_turn(
        ((VANILLA_1_2, False), (VANILLA_3_3, True), (VANILLA_4_3, True))
    )
    assert not st.cards[lv1].rested
    assert st.cards[lv3].rested
    assert not st.cards[lv5].rested


@pytest.mark.card("ST14-001")
def test_st14_001_freeze_spares_your_units_and_repeats_each_enemy_start_phase() -> None:
    sc = Scenario(active=1)
    sc.add(0, THE_O)
    mine = sc.add(0, VANILLA_1_2, rested=True)
    theirs = sc.add(1, VANILLA_3_3, rested=True)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert not st.cards[mine].rested
    assert st.cards[theirs].rested
    to_next_turn(st)
    assert st.active == 1
    assert st.cards[theirs].rested


@pytest.mark.card("ST14-001")
@pytest.mark.ruling("ST14-001:Q463")
def test_st14_001_q463_effects_may_set_them_active_outside_the_start_phase() -> None:
    st, (blocker, lv5) = _the_o_opponent_turn(
        ((BLOCKER, True), (VANILLA_4_3, True)), opp_hand=(MIDAIR,)
    )
    assert st.cards[blocker].rested
    (midair,) = [u for u in st.zones[1][Zone.HAND] if V.cdef(st, u).card_number == MIDAIR]
    play(st, midair)
    assert not st.cards[blocker].rested
    assert not st.cards[lv5].rested


@pytest.mark.card("ST14-001")
@pytest.mark.rule("13-1-7-1")
def test_st14_001_suppression() -> None:
    sc = Scenario()
    the_o = sc.add(0, THE_O)
    shields = sc.shields(1, VANILLA_2_2, VANILLA_2_2, VANILLA_2_2)
    st = sc.start()
    attack(st, the_o)
    pass_all(st)
    assert [zone_of(st, s) for s in shields] == [Zone.TRASH, Zone.TRASH, Zone.SHIELD]


# ---------------------------------------------------------------------------------------------
# ST14-002 Gundam NT-1 Full Armor, ST14-004 Geara Doga (Heavy Armed Type)


@pytest.mark.card("ST14-002", "ST14-004")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", [NT1, GEARA_DOGA])
def test_blocker_units_can_block(number: str) -> None:
    sc = Scenario(active=1)
    blocker = sc.add(0, number)
    attacker = sc.add(1, VANILLA_2_2)
    st = sc.start()
    attack(st, attacker)
    assert has_action(st, A.BLOCK, blocker)


@pytest.mark.card("ST14-004")
@pytest.mark.ruling("ST14-004:Q465")
def test_st14_004_q465_cannot_attack_the_player_base_or_shields() -> None:
    sc = Scenario()
    geara = sc.add(0, GEARA_DOGA)
    rested = sc.add(1, VANILLA_2_2, rested=True)
    sc.shields(1, VANILLA_2_2)
    sc.base(1)
    st = sc.start()
    assert not has_action(st, A.ATTACK, geara, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, geara, rested)


# ---------------------------------------------------------------------------------------------
# ST14-005 G-Falcon DX


def _g_falcon(trash: int, *, rested: bool) -> tuple[GameState, int, int]:
    sc = Scenario()
    sc.resources(0, 7)
    card = sc.add(0, G_FALCON, Zone.HAND)
    lv6 = sc.add(1, VANILLA_6_4, rested=rested)
    lv7 = sc.add(1, G_FALCON)
    sc.trash(0, *([VANILLA_2_2] * trash))
    st = sc.start()
    play(st, card)
    return st, lv6, lv7


@pytest.mark.card("ST14-005")
@pytest.mark.rule("5-20-2")
@pytest.mark.parametrize(("trash", "expected_ap"), [(7, 4), (6, 6)])
def test_st14_005_rests_enemy_lv6_or_lower_then_ap_minus_2_with_7_trash(
    trash: int, expected_ap: int
) -> None:
    st, lv6, lv7 = _g_falcon(trash, rested=False)
    assert st.cards[lv6].rested
    assert not st.cards[lv7].rested
    assert ap(st, lv6) == expected_ap
    to_next_turn(st)
    assert ap(st, lv6) == 6


@pytest.mark.card("ST14-005")
@pytest.mark.ruling("ST14-005:Q466")
def test_st14_005_q466_already_rested_unit_still_gets_ap_minus_2() -> None:
    st, lv6, _ = _g_falcon(7, rested=True)
    assert ap(st, lv6) == 4


# ---------------------------------------------------------------------------------------------
# ST14-006 Full Armor Unicorn Gundam (Destroy Mode)


def _fa_unicorn_deploy() -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 8)
    card = sc.add(0, FA_UNICORN_DM, Zone.HAND)
    sc.deck(0, BERTIGO, FINAL_VICTOR, "ST12-008", "ST12-010")
    st = sc.start()
    top4 = list(st.zones[0][Zone.DECK][:4])
    play(st, card)
    return st, top4


@pytest.mark.card("ST14-006")
def test_st14_006_deploy_adds_1_of_top_3_to_hand() -> None:
    st, top4 = _fa_unicorn_deploy()
    yes(st)
    select(st, top4[1])
    assert zone_of(st, top4[1]) is Zone.HAND
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top4[3]
    assert sorted(deck[-2:]) == sorted((top4[0], top4[2]))


@pytest.mark.card("ST14-006")
@pytest.mark.ruling("ST14-006:Q467")
def test_st14_006_q467_look_is_mandatory_adding_is_optional() -> None:
    st, top4 = _fa_unicorn_deploy()
    assert _kind(st) is DecisionKind.YES_NO
    no(st)
    deck = st.zones[0][Zone.DECK]
    assert deck[0] == top4[3]
    assert sorted(deck[-3:]) == sorted(top4[:3])


def _fa_unicorn_attacked(
    attackers: tuple[str, ...], *, pilot: str | None = PILOT_LV3
) -> tuple[GameState, int, list[int]]:
    sc = Scenario(active=1)
    unicorn = sc.add(0, FA_UNICORN_DM, pilot=pilot, rested=True)
    ids = [sc.add(1, number) for number in attackers]
    st = sc.start()
    return st, unicorn, ids


@pytest.mark.card("ST14-006")
@pytest.mark.rule("13-2-10")
def test_st14_006_paired_ignores_first_battle_damage_from_lower_ap_once_per_turn() -> None:
    st, unicorn, (first, second) = _fa_unicorn_attacked((VANILLA_3_4, VANILLA_3_4))
    assert ap(st, unicorn) == 4
    attack(st, first, unicorn)
    pass_all(st)
    assert st.cards[unicorn].damage == 0
    assert zone_of(st, first) is Zone.TRASH
    attack(st, second, unicorn)
    pass_all(st)
    assert st.cards[unicorn].damage == 3


@pytest.mark.card("ST14-006")
@pytest.mark.parametrize(("attacker", "pilot"), [(VANILLA_5_4, PILOT_LV3), (VANILLA_3_4, None)])
def test_st14_006_damage_from_higher_ap_or_while_unpaired_is_received(
    attacker: str, pilot: str | None
) -> None:
    st, unicorn, (enemy,) = _fa_unicorn_attacked((attacker,), pilot=pilot)
    attack(st, enemy, unicorn)
    pass_all(st)
    assert st.cards[unicorn].damage == ap(st, enemy)


# ---------------------------------------------------------------------------------------------
# ST14-009 Duel Gundam (Assault Shroud)


@pytest.mark.card("ST14-009")
def test_st14_009_destroyed_places_ex_resource() -> None:
    sc = Scenario()
    duel = sc.add(0, DUEL)
    killer = sc.add(1, VANILLA_6_4, rested=True)
    st = sc.start()
    attack(st, duel, killer)
    pass_all(st)
    assert zone_of(st, duel) is Zone.TRASH
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert [V.cdef(st, u).card_type.value for u in area] == ["EX RESOURCE"]


# ---------------------------------------------------------------------------------------------
# ST14-011 Paptimus Scirocco


def _scirocco_attack() -> tuple[GameState, int, dict[str, int]]:
    sc = Scenario()
    unit = sc.add(0, VANILLA_4_3, pilot=SCIROCCO)
    ids = {
        "r1": sc.add(1, VANILLA_2_2, rested=True),
        "r2": sc.add(1, VANILLA_3_4, rested=True),
        "blocker": sc.add(1, BLOCKER),
    }
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    attack(st, unit)
    return st, unit, ids


@pytest.mark.card("ST14-011")
@pytest.mark.rule("8-6-1")
def test_st14_011_attack_reduces_ap_by_rested_enemy_count_during_this_battle() -> None:
    st, _, ids = _scirocco_attack()
    select(st, ids["r2"])
    assert ap(st, ids["r2"]) == 1
    act(st, A.NO_BLOCK)
    pass_all(st)
    assert st.battle is None
    assert ap(st, ids["r2"]) == 3


@pytest.mark.card("ST14-011")
def test_st14_011_reduction_is_fixed_when_the_effect_resolves() -> None:
    st, unit, ids = _scirocco_attack()
    select(st, ids["blocker"])
    assert ap(st, ids["blocker"]) == 1
    act(st, A.BLOCK, ids["blocker"])
    pass_all(st)
    assert st.cards[unit].damage == 1


@pytest.mark.card("ST14-011")
def test_st14_011_burst_adds_to_hand() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    (shield,) = sc.shields(0, SCIROCCO)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST14-012 Banagher Links


@pytest.mark.card("ST14-012")
def test_st14_012_lv5_unit_may_attack_active_enemy_with_5_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    lv4 = sc.add(0, VANILLA_3_4)
    lv5 = sc.add(0, VANILLA_4_3)
    small = sc.add(1, VANILLA_5_4)
    big = sc.add(1, VANILLA_6_4)
    pilot = sc.add(0, BANAGHER, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ATTACK, lv5, small)
    play(st, pilot, onto=lv4)
    assert has_action(st, A.ATTACK, lv5, small)
    assert not has_action(st, A.ATTACK, lv5, big)
    assert not has_action(st, A.ATTACK, lv4, small)


# ---------------------------------------------------------------------------------------------
# ST14-013 Natural Talent


def _natural_talent(enemies: tuple[str, ...]) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 4)
    attacker = sc.add(0, VANILLA_2_2)
    for number in enemies:
        sc.add(1, number)
    sc.shields(1, VANILLA_2_2)
    card = sc.add(0, NATURAL_TALENT, Zone.HAND)
    st = sc.start()
    attack(st, attacker)
    _to_action_step(st)
    return st, card


@pytest.mark.card("ST14-013")
def test_st14_013_mode_1_rests_1_to_2_enemies_with_3_or_less_hp() -> None:
    st, card = _natural_talent((VANILLA_2_2, VANILLA_1_2, VANILLA_3_4))
    a, b, big = st.zones[1][Zone.BATTLE]
    play(st, card)
    assert _mode_options(st) == [0, 1]
    choose_option(st, 0)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {a, b}
    select(st, a, b)
    assert st.cards[a].rested and st.cards[b].rested
    assert not st.cards[big].rested


@pytest.mark.card("ST14-013")
def test_st14_013_mode_1_may_rest_just_one() -> None:
    st, card = _natural_talent((VANILLA_2_2, VANILLA_1_2))
    a, b = st.zones[1][Zone.BATTLE]
    play(st, card)
    choose_option(st, 0)
    select(st, a, done=True)
    assert st.cards[a].rested
    assert not st.cards[b].rested


@pytest.mark.card("ST14-013")
@pytest.mark.ruling("ST14-013:Q468")
def test_st14_013_q468_mode_1_needs_an_enemy_with_3_or_less_hp() -> None:
    st, card = _natural_talent((VANILLA_3_4,))
    (enemy,) = st.zones[1][Zone.BATTLE]
    play(st, card)
    assert _mode_options(st) == [1]
    choose_option(st, 1)
    assert ap(st, enemy) == 0


@pytest.mark.card("ST14-013")
@pytest.mark.ruling("ST14-013:Q469")
@pytest.mark.rule("10-1-8-1-1")
@pytest.mark.xfail(
    strict=True, reason="ENGINE: a modal Command is playable even when no mode has a legal target"
)
def test_st14_013_q469_not_playable_without_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, NATURAL_TALENT, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, card)


@pytest.mark.card("ST14-013")
def test_st14_013_burst_gives_enemy_ap_minus_3() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_5_4)
    sc.shields(0, NATURAL_TALENT)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert ap(st, attacker) == 2


# ---------------------------------------------------------------------------------------------
# ST14-014 Blazing Mobile Suit Rider


def _blazing(commands: int, *enemies: str) -> tuple[GameState, int]:
    sc = Scenario()
    sc.resources(0, 5)
    card = sc.add(0, BLAZING, Zone.HAND)
    for number in enemies:
        sc.add(1, number)
    sc.trash(0, *([FINAL_VICTOR] * commands), VANILLA_2_2)
    return sc.start(), card


@pytest.mark.card("ST14-014")
def test_st14_014_enemy_lv5_or_lower_gets_ap_minus_3() -> None:
    st, card = _blazing(3, VANILLA_5_4)
    (enemy,) = st.zones[1][Zone.BATTLE]
    play(st, card)
    assert ap(st, enemy) == 2


@pytest.mark.card("ST14-014")
@pytest.mark.rule("10-1-8-1-1")
def test_st14_014_lv6_enemy_needs_4_command_cards_in_trash() -> None:
    st, card = _blazing(3, VANILLA_6_4)
    assert not has_action(st, A.PLAY_COMMAND, card)
    st, card = _blazing(4, VANILLA_6_4, VANILLA_5_4)
    big, small = st.zones[1][Zone.BATTLE]
    play(st, card)
    assert {o.a for o in options(st) if o.kind is A.SELECT} == {big, small}
    select(st, big)
    assert ap(st, big) == 3
    assert ap(st, small) == 5


# ---------------------------------------------------------------------------------------------
# ST14-015 Battlefield Emotions


def _emotions(resources: int, copies: int, *, rested: int = 0) -> tuple[Scenario, list[int]]:
    sc = Scenario()
    sc.resources(0, resources, rested=rested)
    sc.resource_deck(0, 3)
    cards = [sc.add(0, EMOTIONS, Zone.HAND) for _ in range(copies)]
    return sc, cards


@pytest.mark.card("ST14-015")
def test_st14_015_places_rested_resource_then_sets_one_active() -> None:
    sc, (card,) = _emotions(4, 1)
    st = sc.start()
    play(st, card)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 5
    assert _active_resources(st, 0) == 1


@pytest.mark.card("ST14-015")
def test_st14_015_second_copy_in_a_turn_does_not_set_active() -> None:
    sc, (first, second) = _emotions(8, 2)
    st = sc.start()
    play(st, first)
    assert _active_resources(st, 0) == 5
    play(st, second)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 10
    assert _active_resources(st, 0) == 1


@pytest.mark.card("ST14-015")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: Resources set active by other effects are not in the turn history",
)
def test_st14_015_resource_set_active_by_another_effect_this_turn() -> None:
    sc, (card,) = _emotions(5, 1, rested=1)
    unit = sc.add(0, VANILLA_2_2, pilot=SET_RESOURCE_ACTIVE_PILOT)
    sc.shields(1, VANILLA_2_2)
    st = sc.start()
    rested_resource = st.zones[0][Zone.RESOURCE_AREA][0]
    attack(st, unit)
    select(st, rested_resource)
    pass_all(st)
    assert _active_resources(st, 0) == 5
    play(st, card)
    assert _active_resources(st, 0) == 1


@pytest.mark.card("ST14-015")
def test_st14_015_burst_places_ex_resource() -> None:
    sc = Scenario(active=1)
    attacker = sc.add(1, VANILLA_2_2)
    sc.shields(0, EMOTIONS)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    area = st.zones[0][Zone.RESOURCE_AREA]
    assert [V.cdef(st, u).card_type.value for u in area] == ["EX RESOURCE"]


# ---------------------------------------------------------------------------------------------
# ST14-016 Gryphios 2


@pytest.mark.card("ST14-016")
def test_st14_016_deploy_adds_top_shield_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    card = sc.add(0, GRYPHIOS, Zone.HAND)
    top, second = sc.shields(0, VANILLA_2_2, VANILLA_1_2)
    st = sc.start()
    play(st, card)
    assert zone_of(st, card) is Zone.BASE
    assert zone_of(st, top) is Zone.HAND
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("ST14-016")
@pytest.mark.rule("10-1-6-1-1")
def test_st14_016_link_gives_enemy_lv5_or_lower_ap_minus_1_once_per_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, GRYPHIOS)
    wing_a = sc.add(0, VANILLA_4_3)
    wing_b = sc.add(0, VANILLA_4_3)
    enemy = sc.add(1, VANILLA_5_4)
    sc.add(1, VANILLA_6_4)
    heero_a, heero_b = sc.hand(0, PILOT_LV4, PILOT_LV4)
    st = sc.start()
    play(st, heero_a, onto=wing_a)
    assert ap(st, enemy) == 4
    play(st, heero_b, onto=wing_b)
    assert ap(st, enemy) == 4


@pytest.mark.card("ST14-016")
def test_st14_016_pairing_without_link_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, GRYPHIOS)
    unit = sc.add(0, VANILLA_3_4)
    enemy = sc.add(1, VANILLA_5_4)
    pilot = sc.add(0, PILOT_LV4, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert ap(st, enemy) == 5
