"""Official rules FAQ (src/gcg_sim/data/gcgapi/rules-faq.json): one or more engine tests per entry.

Entries with no engine-observable behaviour are listed in tests/meta/faq_na.json.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from gcg_sim.cards.db import read_data_text
from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import UnimplementedCardError, get_registry
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.game import SUPPORT_AID, DeckList, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import (
    PLAYER_TARGET,
    ActionKind,
    DecisionKind,
    EndReason,
    Step,
    Zone,
)
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    end_main,
    has_action,
    hp,
    keywords,
    no,
    options,
    pass_,
    pass_all,
    play,
    to_next_turn,
    vanilla_deck,
    yes,
    zone_of,
)
from gcg_sim.tools.marks import scan, values

A = ActionKind
TESTS = Path(__file__).resolve().parents[1]

RESOURCE = "R-001"
VANILLA = "GD01-060"  # Zaku Mariner: red Lv2 cost1 2/2 (Zeon), no effects
GM = "ST01-005"  # blue Lv2 cost1 2/2, no effects
GUNCANNON = "ST01-003"  # blue Lv3 cost2 2/4, no effects
MAGANAC = "ST02-005"  # green Lv2 cost2 3/2, no effects
SANDROCK = "ST02-004"  # green Lv4 cost2 4/3, no effects
ZAKU_I = "ST03-007"  # green Lv1 1/2 (Zeon), no effects
DRA_C = "ST03-005"  # red Lv1 1/2 (Neo Zeon), no effects
LAUNCHER_STRIKE = "GD01-072"  # white Lv4 3/4 <Blocker>
TRAGOS = "ST02-009"  # blue Lv1 1/1 <Blocker>
BURST_ADD = "GD01-097"  # Pilot Guel Jeturk: 【Burst】Add this card to your hand.
CLOSE_COMBAT = "ST03-013"  # 【Burst】Activate 【Main】; 【Main】/【Action】2 damage to 1 enemy Unit
SAYLA = "GD01-087"  # blue Pilot 1/1 (White Base Team): while this Unit is blue, <Repair 1>
MQUVE = "GD01-092"  # green Pilot 1/1 (Zeon): while this Unit is (Zeon), <Breach 1>
RICK_DOM = "GD01-030"  # green Lv3 3/3 (Zeon) <Breach 2>
TROWA = "GD05-099"  # Pilot 1/2: your turn, destroys enemy Unit by battle -> draw 1, discard 1
DUEL_AS = "ST14-009"  # Lv3 3/2: 【Destroyed】Place 1 EX Resource.
WING_BIRD = "ST02-002"  # green Lv3 cost3 2/2: 【Deploy】Place 1 EX Resource.
EARTH_HOUSE = "ST01-016"  # Base Lv2 cost1: 【Burst】deploy; 【Deploy】add 1 Shield to hand
REWLOOLA = "ST03-015"  # Base Lv3 cost2: 【Burst】deploy; 【Deploy】add 1 Shield to hand, ...
TESTING_SECTOR = "GD04-124"  # Base: 【Burst】deploy; 【Deploy】add 1 Shield to hand
TURN_A = "GD04-073"  # ∀ Gundam: 【Activate･Main】【Once per Turn】①: this Unit gets AP+2
ZUOOT = "GD01-061"  # red Lv1 0/2 <Support 1>
KYRIOS = "GD04-034"  # red Lv4 1/4 <First Strike>
ZAKRELLO = "GD04-028"  # green Lv3 4/1
PEACEFUL_TIMBRE = "ST02-013"  # 【Action】shield area protection; 【Pilot】[Quatre Raberba Winner]
INDIGNATION = "ST03-012"  # 【Main】/【Action】1 friendly Unit AP+2; 【Pilot】[Angelo Sauper]
THOROUGHLY_DAMAGED = "ST01-012"  # 【Main】1 rested enemy Unit 1 damage; 【Pilot】
PSYCHO_ZAKU = "EB01-045"  # white Lv7 4/6 <Suppression>
GQ_OMEGA = "GD03-034"  # red Lv8 6/5 <Suppression>
AWAKENED = "GD03-118"  # 【Action】return 1 rested enemy Unit Lv.4 or lower to hand


# ---------------------------------------------------------------------------------------------
# helpers


def _decks() -> tuple[DeckList, DeckList]:
    deck = vanilla_deck()
    return deck, deck


def _keep_all(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is DecisionKind.REDRAW:
        act(st, A.KEEP)


def _started_game(seed: int = 5) -> GameState:
    st = new_game(_decks(), seed, chooser=0)
    act(st, A.GO_FIRST, 0)
    _keep_all(st)
    return st


def _kinds(st: GameState) -> list[str]:
    return [h.kind for h in st.history]


def _events(st: GameState, kind: str) -> list[int]:
    return [h.uid for h in st.history if h.kind == kind]


def _ex_count(st: GameState, player: int) -> int:
    return sum(
        1
        for u in st.zones[player][Zone.RESOURCE_AREA]
        if core.card_type(st, u) is CardType.EX_RESOURCE
    )


def _batch_index(st: GameState, program_id: int) -> int:
    for i, t in enumerate(st.batches[-1]):
        if t.program_id == program_id:
            return i
    raise AssertionError(f"no pending trigger with program {program_id}")


def _select_uids(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def _pending(st: GameState) -> tuple[DecisionKind, int]:
    assert st.pending is not None, f"no decision pending (winner={st.winner})"
    return st.pending.kind, st.pending.player


def _is_kind(st: GameState, uid: int, kind: d.CardKind) -> bool:
    return V.matches(st, V.derived(st), V.Ctx(st.cards[uid].owner), uid, (d.IsKind((kind,)),))


# ---------------------------------------------------------------------------------------------
# Preparing to Play (Q1-Q12)


@pytest.mark.faq("Q7")
@pytest.mark.rule("5-17-4", "5-17-3-2-3")
def test_q7_tokens_used_up_are_placed_outside_the_game() -> None:
    sc = Scenario()
    _, ex = sc.resources(0, 1, rested=1, ex=1)
    gm = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, gm, ex=1)
    assert zone_of(st, ex) is Zone.OUTSIDE
    assert ex not in st.zones[0][Zone.REMOVAL] and ex not in st.zones[0][Zone.TRASH]
    assert all(ex not in z for z in st.zones[0])


@pytest.mark.faq("Q8")
@pytest.mark.rule("6-2-2", "4-6-4-1")
def test_q8_top_deck_card_becomes_the_bottom_shield() -> None:
    st = new_game(_decks(), 3, chooser=0)
    act(st, A.GO_FIRST, 0)
    act(st, A.KEEP)
    assert _pending(st) == (DecisionKind.REDRAW, 1)
    tops = {p: list(st.zones[p][Zone.DECK][:6]) for p in (0, 1)}
    act(st, A.KEEP)
    for p in (0, 1):
        shields = st.zones[p][Zone.SHIELD]
        assert shields == list(reversed(tops[p]))
        assert shields[-1] == tops[p][0]


@pytest.mark.faq("Q9")
@pytest.mark.rule("6-2-1-4", "6-2-1-5", "6-2-5")
def test_q9_winner_chooses_player_one_before_seeing_the_hand() -> None:
    st = new_game(_decks(), 7, chooser=1)
    assert _pending(st) == (DecisionKind.CHOOSE_FIRST, 1)
    assert set(options(st)) == {Action(A.GO_FIRST, 0), Action(A.GO_FIRST, 1)}
    assert not st.zones[0][Zone.HAND] and not st.zones[1][Zone.HAND]
    act(st, A.GO_FIRST, 0)
    assert len(st.zones[0][Zone.HAND]) == 5 and len(st.zones[1][Zone.HAND]) == 5
    assert _pending(st) == (DecisionKind.REDRAW, 0)
    _keep_all(st)
    assert st.turn == 1 and st.active == 0 and st.first_player == 0


@pytest.mark.faq("Q10")
@pytest.mark.rule("6-2-1-6", "6-2-1-6-1", "6-2-1-7", "4-2-4")
def test_q10_redraw_once_each_starting_with_player_one() -> None:
    st = new_game(_decks(), 11, chooser=0)
    act(st, A.GO_FIRST, 0)
    assert _pending(st) == (DecisionKind.REDRAW, 0)
    assert set(options(st)) == {Action(A.KEEP), Action(A.REDRAW)}
    old_hand = list(st.zones[0][Zone.HAND])
    top5 = list(st.zones[0][Zone.DECK][:5])
    unshuffled = list(st.zones[0][Zone.DECK][5:]) + old_hand
    act(st, A.REDRAW)
    new_hand = st.zones[0][Zone.HAND]
    assert sorted(new_hand) == sorted(top5)
    assert not set(new_hand) & set(old_hand)
    deck = st.zones[0][Zone.DECK]
    assert sorted(deck) == sorted(unshuffled)
    assert deck != unshuffled
    assert _pending(st) == (DecisionKind.REDRAW, 1)
    act(st, A.KEEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert st.redraws == [True, False]


@pytest.mark.faq("Q11")
@pytest.mark.rule("5-17-3-1-1", "5-17-3-1-2", "6-2-3")
def test_q11_both_players_start_with_an_active_ex_base_0ap_3hp() -> None:
    st = _started_game()
    for p in (0, 1):
        (base,) = st.zones[p][Zone.BASE]
        cd = V.cdef(st, base)
        assert cd.card_type is CardType.EX_BASE and cd.is_token
        assert ap(st, base) == 0 and hp(st, base) == 3
        assert not st.cards[base].rested


@pytest.mark.faq("Q12")
@pytest.mark.rule("5-17-3-2-2", "6-2-4")
def test_q12_player_two_starts_with_an_ex_resource() -> None:
    st = _started_game()
    assert _ex_count(st, 0) == 0
    (ex,) = st.zones[1][Zone.RESOURCE_AREA]
    assert V.cdef(st, ex).card_type is CardType.EX_RESOURCE and not st.cards[ex].rested


@pytest.mark.faq("Q12")
@pytest.mark.rule("13-2-6-1")
def test_q12_effects_place_ex_resources_during_the_game() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    play(st, wing)
    assert _ex_count(st, 0) == 1
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 4


# ---------------------------------------------------------------------------------------------
# Start / Draw / Resource phases (Q13-Q20)


@pytest.mark.faq("Q13")
@pytest.mark.rule("7-2-3-1", "7-2-3-2")
def test_q13_start_phase_sets_everything_active_without_a_choice() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, rested=True)
    res = sc.resources(0, 3, rested=3)
    base = sc.add(0, sc.db.ex_base.card_number, Zone.BASE, rested=True)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start(Step.ACTIVE_STEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert not any(st.cards[u].rested for u in (unit, base, *res))
    assert st.cards[enemy].rested


@pytest.mark.faq("Q14")
@pytest.mark.rule("7-3-1")
def test_q14_draw_phase_draw_is_mandatory() -> None:
    sc = Scenario()
    sc.deck(0, GUNCANNON)
    st = sc.start(Step.DRAW_STEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    (drawn,) = st.zones[0][Zone.HAND]
    assert V.cdef(st, drawn).card_number == GUNCANNON


@pytest.mark.faq("Q15")
@pytest.mark.rule("7-3-1")
def test_q15_player_one_draws_on_the_first_turn() -> None:
    st = _started_game()
    assert st.turn == 1 and _pending(st) == (DecisionKind.MAIN, 0)
    assert len(st.zones[0][Zone.HAND]) == 6
    assert len(st.zones[1][Zone.HAND]) == 5


@pytest.mark.faq("Q16")
@pytest.mark.rule("4-8-4", "7-6-5-1")
def test_q16_no_hand_maximum_but_discard_to_ten_in_your_end_phase() -> None:
    sc = Scenario()
    sc.hand(0, *([VANILLA] * 11))
    sc.hand(1, *([VANILLA] * 12))
    sc.resources(0, 4)
    draw2 = sc.add(0, "GD01-100", Zone.HAND)  # 【Main】Draw 2.
    st = sc.start()
    play(st, draw2)
    assert len(st.zones[0][Zone.HAND]) == 13
    end_main(st)
    pass_all(st)
    assert _pending(st) == (DecisionKind.DISCARD, 0)
    for _ in range(3):
        act(st, A.SELECT, options(st)[0].a)
    assert len(st.zones[0][Zone.HAND]) == 10
    assert _pending(st) == (DecisionKind.MAIN, 1)
    assert len(st.zones[1][Zone.HAND]) == 13


@pytest.mark.faq("Q17")
@pytest.mark.rule("1-2-2-2", "7-3-1-1", "11-2-1-2", "1-2-3")
def test_q17_drawing_the_last_card_loses_immediately() -> None:
    sc = Scenario(active=1, deck_size=1)
    st = sc.start()
    (last,) = st.zones[0][Zone.DECK]
    to_next_turn(st)
    assert st.active == 0 and st.turn == 4
    assert zone_of(st, last) is Zone.HAND
    assert st.winner == 1 and st.end_reason is EndReason.DECK_OUT
    assert st.pending is None


@pytest.mark.faq("Q18")
@pytest.mark.rule("7-4-1")
def test_q18_resource_placement_is_mandatory() -> None:
    sc = Scenario()
    sc.resource_deck(0, 2)
    st = sc.start(Step.RESOURCE_STEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    (res,) = st.zones[0][Zone.RESOURCE_AREA]
    assert not st.cards[res].rested
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 1


@pytest.mark.faq("Q19")
@pytest.mark.rule("4-4-2", "1-3-2")
def test_q19_resource_area_holds_at_most_15_cards() -> None:
    sc = Scenario()
    sc.resources(0, 10, ex=5)
    sc.resource_deck(0, 1)
    st = sc.start(Step.RESOURCE_STEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 15
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 1


@pytest.mark.faq("Q19")
@pytest.mark.rule("4-4-2-1")
def test_q19_at_most_five_ex_resources() -> None:
    sc = Scenario()
    sc.resources(0, 3, ex=5)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    play(st, wing, ex=0)
    assert _ex_count(st, 0) == 5
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 8


@pytest.mark.faq("Q20")
@pytest.mark.rule("7-4-1", "1-3-2")
def test_q20_empty_resource_deck_goes_straight_to_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    st = sc.start(Step.RESOURCE_STEP)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 2


# ---------------------------------------------------------------------------------------------
# Main phase: playing cards (Q21-Q29)


@pytest.mark.faq("Q21")
@pytest.mark.rule("2-9-1", "2-9-4", "7-5-2-2-2")
def test_q21_ex_resources_count_toward_your_level() -> None:
    sc = Scenario()
    sc.resources(0, 2, ex=1)
    card = sc.add(0, GUNCANNON, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, card)

    sc2 = Scenario()
    sc2.resources(0, 2)
    card2 = sc2.add(0, GUNCANNON, Zone.HAND)
    st2 = sc2.start()
    assert not has_action(st2, A.PLAY_UNIT, card2)


@pytest.mark.faq("Q22")
@pytest.mark.rule("5-17-3-2-3", "2-9-4")
def test_q22_paying_with_an_ex_resource_lowers_your_level() -> None:
    sc = Scenario()
    (_, _, ex) = sc.resources(0, 2, ex=1)
    gm = sc.add(0, GM, Zone.HAND)
    lv3 = sc.add(0, GUNCANNON, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, lv3)
    play(st, gm, ex=1)
    assert zone_of(st, ex) is Zone.OUTSIDE
    active = [u for u in st.zones[0][Zone.RESOURCE_AREA] if not st.cards[u].rested]
    assert len(active) == 2
    assert not has_action(st, A.PLAY_UNIT, lv3)


@pytest.mark.faq("Q23")
@pytest.mark.rule("3-4-6-2", "3-4-6-4", "5-9-1")
def test_q23_command_with_pilot_effect_pairs_instead_of_activating() -> None:
    sc = Scenario()
    res = sc.resources(0, 2)
    gm = sc.add(0, GM)
    cmd = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, cmd)
    assert has_action(st, A.PAIR, cmd, gm)
    play(st, cmd, onto=gm)
    assert zone_of(st, cmd) is Zone.PAIRED and st.cards[gm].pair == cmd
    assert sum(st.cards[u].rested for u in res) == 1
    assert st.cards[enemy].damage == 0
    assert hp(st, gm) == 3


@pytest.mark.faq("Q24")
@pytest.mark.rule("2-2-5")
def test_q24_units_with_the_same_name_may_be_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    sc.add(0, VANILLA)
    second = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    play(st, second)
    assert [V.cdef(st, u).name for u in st.zones[0][Zone.BATTLE]] == ["Zaku Mariner"] * 2


@pytest.mark.faq("Q25")
@pytest.mark.rule("2-2-5")
def test_q25_pilots_with_the_same_name_may_be_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, GM, pilot=SAYLA)
    other = sc.add(0, GUNCANNON)
    sayla2 = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    play(st, sayla2, onto=other)
    assert st.cards[other].pair == sayla2


@pytest.mark.faq("Q26")
@pytest.mark.rule("4-5-4", "11-4-1", "11-4-2")
def test_q26_battle_area_holds_at_most_six_units() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    old = [sc.add(0, VANILLA) for _ in range(6)]
    new = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, new)
    assert _pending(st) == (DecisionKind.EXCESS, 0)
    assert _select_uids(st) == set(old)
    act(st, A.SELECT, old[0])
    assert len(st.zones[0][Zone.BATTLE]) == 6
    assert new in st.zones[0][Zone.BATTLE] and zone_of(st, old[0]) is Zone.TRASH


@pytest.mark.faq("Q27")
@pytest.mark.rule("11-4-2-1", "5-10-4")
def test_q27_unit_trashed_for_excess_is_not_destroyed() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    duel = sc.add(0, DUEL_AS)
    for _ in range(5):
        sc.add(0, VANILLA)
    new = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, new)
    act(st, A.SELECT, duel)
    assert zone_of(st, duel) is Zone.TRASH
    assert _ex_count(st, 0) == 0
    assert duel in _events(st, "excess") and duel not in _events(st, "destroyed")


@pytest.mark.faq("Q28")
@pytest.mark.rule("4-6-3", "11-5-1", "11-5-2")
def test_q28_base_section_holds_one_base() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    ex_base = sc.base(0)
    sc.shields(0, VANILLA, VANILLA)
    base = sc.add(0, EARTH_HOUSE, Zone.HAND)
    st = sc.start()
    play(st, base)
    assert st.zones[0][Zone.BASE] == [base]
    assert zone_of(st, ex_base) is Zone.OUTSIDE


@pytest.mark.faq("Q29")
@pytest.mark.rule("11-5-2", "11-5-2-1", "5-10-4")
def test_q29_replaced_base_is_trashed_not_destroyed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    old = sc.add(0, EARTH_HOUSE, Zone.BASE)
    sc.shields(0, VANILLA, VANILLA)
    new = sc.add(0, REWLOOLA, Zone.HAND)
    st = sc.start()
    play(st, new)
    assert st.zones[0][Zone.BASE] == [new]
    assert zone_of(st, old) is Zone.TRASH
    assert old in _events(st, "excess") and old not in _events(st, "destroyed")


# ---------------------------------------------------------------------------------------------
# Main phase: activating effects and attacking (Q30-Q41)


@pytest.mark.faq("Q30")
@pytest.mark.rule("7-5-3-1", "13-2-1-1")
def test_q30_activate_main_on_the_turn_a_unit_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    turn_a = sc.add(0, TURN_A, Zone.HAND)
    zuoot = sc.add(0, ZUOOT, deployed_this_turn=True)
    st = sc.start()
    play(st, turn_a)
    assert not has_action(st, A.ATTACK, turn_a)
    assert has_action(st, A.ACTIVATE, zuoot, SUPPORT_AID)
    activate(st, turn_a)
    assert ap(st, turn_a) == 5


@pytest.mark.faq("Q31")
@pytest.mark.rule("3-2-4", "3-2-6-3")
def test_q31_player_one_may_attack_on_turn_one_but_not_with_new_units() -> None:
    sc = Scenario(turn=1)
    sc.resources(0, 6)
    gundam = sc.add(0, "ST01-001", Zone.HAND)  # link [Amuro Ray]
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    gm = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, gm)
    play(st, gundam)
    assert not has_action(st, A.ATTACK)
    play(st, amuro, onto=gundam)
    assert has_action(st, A.ATTACK, gundam)
    assert not has_action(st, A.ATTACK, gm)


@pytest.mark.faq("Q32")
@pytest.mark.rule("8-5-2-1", "8-5-2-2", "8-5-2-3", "8-5-2-4", "3-5-3", "4-6-1")
def test_q32_attack_on_player_hits_base_then_shield_then_player() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    base = sc.base(1)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.cards[base].damage == 2 and zone_of(st, shield) is Zone.SHIELD

    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert zone_of(st, shield) is Zone.TRASH and st.winner is None

    sc = Scenario()
    unit = sc.add(0, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.winner == 0 and st.end_reason is EndReason.BATTLE_DAMAGE


@pytest.mark.faq("Q33")
@pytest.mark.rule("8-4-2", "8-6-2")
def test_q33_attacker_leaving_during_action_step_skips_to_battle_end() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, VANILLA)
    sc.resources(1, 4)
    ret = sc.add(1, AWAKENED, Zone.HAND)
    st = sc.start()
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 1)
    act(st, A.PLAY_COMMAND, ret)
    pass_all(st)
    assert zone_of(st, unit) is Zone.HAND
    assert zone_of(st, shield) is Zone.SHIELD
    assert _pending(st) == (DecisionKind.MAIN, 0) and not st.battles


@pytest.mark.faq("Q33")
@pytest.mark.rule("8-2-4")
def test_q33_target_leaving_during_attack_step_skips_block_and_action_steps() -> None:
    sc = Scenario()
    flauros = sc.add(0, "GD05-060")  # 【Attack】destroy 1 enemy Unit Lv.2 or lower
    target = sc.add(1, VANILLA, rested=True)
    blocker = sc.add(1, LAUNCHER_STRIKE)
    sc.resources(1, 4)
    sc.add(1, INDIGNATION, Zone.HAND)
    st = sc.start()
    attack(st, flauros, target)
    assert zone_of(st, target) is Zone.TRASH
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert st.cards[flauros].damage == 0 and not st.cards[blocker].rested


@pytest.mark.faq("Q34")
@pytest.mark.rule("8-2-1", "7-5-4-1")
def test_q34_zero_ap_unit_can_attack() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert ap(st, zuoot) == 0
    assert has_action(st, A.ATTACK, zuoot, PLAYER_TARGET)
    assert has_action(st, A.ATTACK, zuoot, enemy)


@pytest.mark.faq("Q35")
@pytest.mark.rule("4-6-4-2", "5-5-5", "11-3-1-1")
def test_q35_zero_ap_attack_does_not_destroy_a_shield() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zuoot)
    pass_all(st)
    assert zone_of(st, shield) is Zone.SHIELD
    assert "shield_destroyed" not in _kinds(st)


@pytest.mark.faq("Q36")
@pytest.mark.rule("8-5-2-3", "4-6-4-1")
def test_q36_first_shield_is_destroyed_without_a_choice() -> None:
    sc = Scenario()
    unit = sc.add(0, SANDROCK)
    top, second, third = sc.shields(1, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, top) is Zone.TRASH
    assert st.zones[1][Zone.SHIELD] == [second, third]


@pytest.mark.faq("Q37")
@pytest.mark.rule("8-5-2-4", "8-5-2-4-1", "3-5-3")
def test_q37_base_takes_the_attack_instead_of_a_shield() -> None:
    sc = Scenario()
    unit = sc.add(0, SANDROCK)
    base = sc.base(1)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, base) is Zone.OUTSIDE  # EX Base destroyed by 4 damage, token leaves
    assert st.zones[1][Zone.SHIELD] == shields


@pytest.mark.faq("Q38")
@pytest.mark.rule("13-2-4-2")
def test_q38_action_command_with_pilot_cannot_pair_during_action_step() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    other = sc.add(0, GM)
    timbre = sc.add(0, PEACEFUL_TIMBRE, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, timbre, other)
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, timbre)
    assert not has_action(st, A.PAIR)


@pytest.mark.faq("Q39")
@pytest.mark.rule("8-5-3-1", "8-5-3-2", "8-5-3-2-1", "8-5-3-2-3", "5-5-3")
def test_q39_battling_units_deal_damage_simultaneously() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH and zone_of(st, enemy) is Zone.TRASH


@pytest.mark.faq("Q39")
@pytest.mark.rule("8-5-3-2-2", "13-1-5-2")
def test_q39_first_strike_attacker_is_not_damaged_by_a_destroyed_target() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS)
    enemy = sc.add(1, ZAKRELLO, rested=True)
    st = sc.start()
    attack(st, kyrios, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, kyrios) is Zone.BATTLE and st.cards[kyrios].damage == 0


@pytest.mark.faq("Q40")
@pytest.mark.rule("5-10-3", "8-5-2-3-1", "13-2-5-3")
def test_q40_destroyed_shield_is_revealed_and_burst_is_offered_before_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    assert st.cards[shield].known == 0
    attack(st, unit)
    pass_all(st)
    assert _pending(st) == (DecisionKind.BURST, 1)
    assert zone_of(st, shield) is Zone.RESOLVING
    assert st.cards[shield].known == core.BOTH_KNOW
    no(st)
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.faq("Q41")
@pytest.mark.rule("3-3-6", "5-10-2")
def test_q41_paired_pilot_follows_a_destroyed_unit_to_the_trash() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot=SAYLA)
    enemy = sc.add(1, SANDROCK, rested=True)
    st = sc.start()
    pilot = st.cards[unit].pair
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, unit) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.TRASH


@pytest.mark.faq("Q41")
@pytest.mark.rule("3-3-6")
def test_q41_paired_pilot_follows_a_returned_unit_to_the_hand() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    ret = sc.add(0, AWAKENED, Zone.HAND)
    enemy = sc.add(1, GUNCANNON, rested=True, pilot=SAYLA)
    st = sc.start()
    pilot = st.cards[enemy].pair
    end_main(st)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    act(st, A.PLAY_COMMAND, ret)
    assert zone_of(st, enemy) is Zone.HAND and zone_of(st, pilot) is Zone.HAND
    assert st.cards[pilot].owner == 1 and pilot in st.zones[1][Zone.HAND]


# ---------------------------------------------------------------------------------------------
# Fundamental terminology (Q42-Q50, Q487, Q488)


@pytest.mark.faq("Q42")
@pytest.mark.rule("5-7-1", "7-5-2-2-1", "7-5-2-2-3", "7-5-2-2-4", "2-10-1")
def test_q42_play_means_paying_the_cost_of_a_card_in_hand() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    card = sc.add(0, GUNCANNON, Zone.HAND, known=False)
    st = sc.start()
    play(st, card)
    assert zone_of(st, card) is Zone.BATTLE
    assert st.cards[card].known == core.BOTH_KNOW
    assert sum(st.cards[u].rested for u in res) == 2
    assert card in _events(st, "play")


@pytest.mark.faq("Q43")
@pytest.mark.rule("5-8-1", "13-2-6-1")
def test_q43_deploy_covers_units_entering_battle_and_bases_entering_base_section() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.shields(0, VANILLA, GM)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    base = sc.add(0, EARTH_HOUSE, Zone.HAND)
    st = sc.start()
    play(st, wing)
    assert zone_of(st, wing) is Zone.BATTLE and _ex_count(st, 0) == 1
    hand_before = len(st.zones[0][Zone.HAND])
    play(st, base, ex=1)
    assert zone_of(st, base) is Zone.BASE
    assert len(st.zones[0][Zone.HAND]) == hand_before  # base left, 1 Shield added
    assert set(_events(st, "deployed")) == {wing, base}


@pytest.mark.faq("Q44")
@pytest.mark.rule("5-14-1", "5-14-1-1")
def test_q44_draw_1_adds_the_top_card_of_the_deck() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.deck(0, SANDROCK, GUNCANNON)
    cmd = sc.add(0, "GD03-101", Zone.HAND)  # 【Main】Draw 1. ...
    st = sc.start()
    top, nxt = st.zones[0][Zone.DECK][:2]
    play(st, cmd)
    assert st.zones[0][Zone.HAND] == [top]
    assert st.zones[0][Zone.DECK][0] == nxt
    assert st.cards[top].known == 1 << 0


@pytest.mark.faq("Q45")
@pytest.mark.rule("5-11-1")
def test_q45_discard_1_chooses_a_card_in_hand_and_trashes_it() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    cmd = sc.add(0, "GD01-118", Zone.HAND)  # 【Main】Draw 2. Then, discard 1.
    keep = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert _pending(st) == (DecisionKind.DISCARD, 0)
    hand = st.zones[0][Zone.HAND]
    assert _select_uids(st) == set(hand) and len(hand) == 3
    victim = next(u for u in hand if u != keep)
    act(st, A.SELECT, victim)
    assert zone_of(st, victim) is Zone.TRASH and len(st.zones[0][Zone.HAND]) == 2


@pytest.mark.faq("Q46")
@pytest.mark.rule("5-6-1", "5-6-2")
def test_q46_recover_removes_damage_counters() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, GUNCANNON, damage=2)  # 2/4
    cmd = sc.add(0, "ST01-013", Zone.HAND)  # 【Main】1 friendly Unit recovers 3 HP
    st = sc.start()
    play(st, cmd)
    assert st.cards[unit].damage == 0 and hp(st, unit) == 4

    sc = Scenario()
    sc.resources(0, 3)
    big = sc.add(0, "GD03-002", damage=4)  # The-O 5/5
    cmd = sc.add(0, "ST01-013", Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert st.cards[big].damage == 1


@pytest.mark.faq("Q47")
@pytest.mark.rule("5-9-1", "3-3-1", "3-3-8-1", "2-7-3", "2-8-4", "4-5-3")
def test_q47_pair_places_a_pilot_beneath_a_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    pilot = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=unit)
    assert zone_of(st, pilot) is Zone.PAIRED
    assert st.cards[unit].pair == pilot and st.cards[pilot].pair == unit
    assert st.cards[pilot].known == core.BOTH_KNOW
    assert (ap(st, unit), hp(st, unit)) == (3, 3)


@pytest.mark.faq("Q48")
@pytest.mark.rule("3-2-6-2", "3-2-6-3", "2-12-1")
def test_q48_link_unit_can_attack_the_turn_it_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gundam = sc.add(0, "ST01-001", Zone.HAND)  # link [Amuro Ray]
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    st = sc.start()
    play(st, gundam)
    assert not has_action(st, A.ATTACK, gundam)
    play(st, amuro, onto=gundam)
    assert V.is_linked(V.derived(st), gundam)
    assert has_action(st, A.ATTACK, gundam)

    sc = Scenario()
    sc.resources(0, 3)
    other = sc.add(0, "ST01-001", deployed_this_turn=True)
    sayla = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    play(st, sayla, onto=other)
    assert not V.is_linked(V.derived(st), other)
    assert not has_action(st, A.ATTACK, other)


@pytest.mark.faq("Q49")
@pytest.mark.rule("5-17-1")
def test_q49_tokens_are_placed_from_outside_the_game() -> None:
    st = _started_game()
    decklist_ids = set(st.decklists[0]) | set(st.decklists[1])
    tokens = [*st.zones[0][Zone.BASE], *st.zones[1][Zone.BASE], *st.zones[1][Zone.RESOURCE_AREA]]
    assert len(tokens) == 3
    for u in tokens:
        assert V.cdef(st, u).is_token and st.cards[u].def_id not in decklist_ids

    sc = Scenario()
    sc.resources(0, 3)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    st = sc.start()
    before = len(st.cards)
    play(st, wing)
    (ex,) = [u for u in st.zones[0][Zone.RESOURCE_AREA] if u >= before]
    assert V.cdef(st, ex).is_token


@pytest.mark.faq("Q50")
@pytest.mark.rule("5-19-1", "2-5-3")
def test_q50_slash_between_traits_means_or() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    unit = sc.add(0, GM)
    ff = sc.add(0, "ST03-010", Zone.HAND)  # deploy 1 (Neo Zeon)/(Zeon) Unit card Lv.4-
    zeon = sc.add(0, ZAKU_I, Zone.HAND)
    neo = sc.add(0, DRA_C, Zone.HAND)
    sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, ff, onto=unit)
    yes(st)
    assert _pending(st) == (DecisionKind.SELECT, 0)
    assert _select_uids(st) == {zeon, neo}
    act(st, A.SELECT, neo)
    assert zone_of(st, neo) is Zone.BATTLE


@pytest.mark.faq("Q487")
@pytest.mark.rule("5-21-1")
def test_q487_reduce_lowers_damage_received() -> None:
    sc = Scenario()
    rey = sc.add(0, "GD04-053", pilot="GD04-093")  # linked: reduce damage from an enemy by 1
    enemy = sc.add(1, "ST14-009", rested=True)  # 3 AP
    st = sc.start()
    assert V.is_linked(V.derived(st), rey)
    attack(st, rey, enemy)
    pass_all(st)
    assert st.cards[rey].damage == 2


@pytest.mark.faq("Q487")
@pytest.mark.rule("5-21-2", "5-21-2-1")
def test_q487_reduction_at_least_the_damage_means_no_damage() -> None:
    sc = Scenario(active=1)
    silver = sc.add(0, "GD04-068")  # reduce enemy effect damage by 3
    sc.resources(1, 2)
    cc = sc.add(1, CLOSE_COMBAT, Zone.HAND)
    st = sc.start()
    play(st, cc)
    assert st.cards[silver].damage == 0


@pytest.mark.faq("Q488")
@pytest.mark.rule("5-21-1", "5-21-2")
def test_q488_damage_reductions_add_up() -> None:
    sc = Scenario(active=1)
    destiny = sc.add(0, "GD05-055", rested=True)  # reduce enemy battle damage by 2
    sc.resources(0, 3)
    dc = sc.add(0, "GD04-113", Zone.HAND)  # 【Action】reduce battle damage by 3 this battle
    gq = sc.add(1, GQ_OMEGA)  # 6 AP
    st = sc.start()
    attack(st, gq, destiny)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    act(st, A.PLAY_COMMAND, dc)
    pass_all(st)
    assert st.cards[destiny].damage == 1


# ---------------------------------------------------------------------------------------------
# Keyword effects (Q51-Q64, Q165-Q167, Q195)


def _end_turn_decision(st: GameState) -> tuple[DecisionKind, int]:
    end_main(st)
    pass_all(st)
    return _pending(st)


@pytest.mark.faq("Q51")
@pytest.mark.rule("13-1-1-1", "5-6-3")
def test_q51_repair_does_not_activate_on_an_undamaged_unit() -> None:
    sc = Scenario()
    hurt = sc.add(0, "GD01-017", damage=1)  # <Repair 1>
    fresh = sc.add(0, "GD01-017")
    st = sc.start()
    assert _end_turn_decision(st) == (DecisionKind.MAIN, 1)
    assert st.cards[hurt].damage == 0 and st.cards[fresh].damage == 0

    sc = Scenario()
    sc.add(0, "GD01-017", damage=1)
    sc.add(0, "GD01-017", damage=1)
    st = sc.start()
    assert _end_turn_decision(st) == (DecisionKind.ORDER_TRIGGER, 0)


@pytest.mark.faq("Q52")
@pytest.mark.rule("13-1-1-2", "13-1-1-1")
def test_q52_repair_amounts_add_up() -> None:
    sc = Scenario()
    unit = sc.add(0, "GD02-017", pilot=SAYLA, damage=3)  # blue <Repair 2> + <Repair 1>
    st = sc.start()
    assert keywords(st, unit)["Repair"] == 3
    to_next_turn(st)
    assert st.cards[unit].damage == 0


@pytest.mark.faq("Q53")
@pytest.mark.rule("13-1-2-1", "13-1-2-2")
def test_q53_breach_damages_the_base_first_otherwise_the_top_shield() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM)
    enemy = sc.add(1, VANILLA, rested=True)
    base = sc.base(1)
    shields = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[base].damage == 2 and st.zones[1][Zone.SHIELD] == shields

    sc = Scenario()
    rick = sc.add(0, RICK_DOM)
    enemy = sc.add(1, VANILLA, rested=True)
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH and st.zones[1][Zone.SHIELD] == [second]


@pytest.mark.faq("Q54")
@pytest.mark.rule("13-1-2-4")
def test_q54_breach_does_not_activate_with_an_empty_shield_area() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=TROWA)
    enemy = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 0)
    assert len(st.batches[-1]) == 2

    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=TROWA)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert len(_events(st, "draw")) == 1 and len(_events(st, "discard")) == 1


@pytest.mark.faq("Q55")
@pytest.mark.rule("13-1-2-3", "8-5-3-2-3")
def test_q55_breach_activates_when_both_units_are_destroyed() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM)
    enemy = sc.add(1, "ST14-009", rested=True)  # 3/2
    top, second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, rick) is Zone.TRASH and zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, top) is Zone.TRASH and st.zones[1][Zone.SHIELD] == [second]


@pytest.mark.faq("Q56")
@pytest.mark.rule("10-1-6-6")
def test_q56_active_players_breach_resolves_before_enemy_destroyed_effect() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=MQUVE)  # 4/4 <Breach 3>
    enemy = sc.add(1, DUEL_AS, rested=True)  # 【Destroyed】Place 1 EX Resource.
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    kinds = _kinds(st)
    assert "shield_destroyed" in kinds and "ex_resource" in kinds
    assert kinds.index("shield_destroyed") < kinds.index("ex_resource")


@pytest.mark.faq("Q57")
@pytest.mark.rule("13-1-2-5")
def test_q57_breach_amounts_add_up() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=MQUVE)
    enemy = sc.add(1, VANILLA, rested=True)
    base = sc.base(1)
    st = sc.start()
    assert keywords(st, rick)["Breach"] == 3
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, base) is Zone.OUTSIDE  # EX Base (3 HP) destroyed by <Breach 3>


@pytest.mark.faq("Q58")
@pytest.mark.rule("13-1-3-2", "13-1-3-1")
def test_q58_support_amounts_add_up() -> None:
    sc = Scenario()
    zuoot = sc.add(0, ZUOOT, pilot="GD04-089")  # <Support 1> + Pilot's <Support 2>
    other = sc.add(0, VANILLA)
    st = sc.start()
    assert keywords(st, zuoot)["Support"] == 3
    assert [o for o in options(st) if o.kind is A.ACTIVATE and o.a == zuoot] == [
        Action(A.ACTIVATE, zuoot, SUPPORT_AID, 0)
    ]
    activate(st, zuoot, SUPPORT_AID)
    assert st.cards[zuoot].rested
    assert ap(st, other) == 5


@pytest.mark.faq("Q59")
@pytest.mark.rule("8-3-1", "13-1-4-1")
def test_q59_rested_blocker_cannot_block() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    rested = sc.add(1, LAUNCHER_STRIKE, rested=True)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, shield) is Zone.TRASH and st.cards[rested].damage == 0

    sc = Scenario()
    unit = sc.add(0, VANILLA)
    blocker = sc.add(1, LAUNCHER_STRIKE)
    st = sc.start()
    attack(st, unit)
    assert _pending(st) == (DecisionKind.BLOCK, 1)
    assert has_action(st, A.BLOCK, blocker)


@pytest.mark.faq("Q60")
@pytest.mark.rule("13-1-4-2")
def test_q60_a_unit_has_at_most_one_blocker() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    blocker = sc.add(1, LAUNCHER_STRIKE, pilot="GD01-096")  # white: gains <Blocker>
    st = sc.start()
    assert keywords(st, blocker)["Blocker"] == 1
    attack(st, unit)
    assert [o for o in options(st) if o.kind is A.BLOCK] == [Action(A.BLOCK, blocker)]


@pytest.mark.faq("Q61")
@pytest.mark.rule("13-1-5-1", "13-1-5-2", "8-5-3-2-2")
def test_q61_first_strike_deals_damage_before_the_enemy_unit() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS)  # 1/4 <First Strike>
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, kyrios, enemy)
    pass_all(st)
    assert st.cards[enemy].damage == 1 and st.cards[kyrios].damage == 2

    sc = Scenario()
    plain = sc.add(0, GUNCANNON)  # 2/4, no <First Strike>
    enemy = sc.add(1, ZAKRELLO, rested=True)  # 4/1
    st = sc.start()
    attack(st, plain, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH and zone_of(st, plain) is Zone.TRASH


@pytest.mark.faq("Q62")
@pytest.mark.rule("13-1-5-2")
def test_q62_first_strike_kill_means_no_damage_back() -> None:
    sc = Scenario()
    kyrios = sc.add(0, KYRIOS)
    enemy = sc.add(1, ZAKRELLO, rested=True)
    st = sc.start()
    attack(st, kyrios, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH and st.cards[kyrios].damage == 0


@pytest.mark.faq("Q63")
@pytest.mark.rule("13-1-5-3")
def test_q63_a_unit_has_at_most_one_first_strike() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, VANILLA)
    b1 = sc.add(0, "ST04-014", Zone.HAND)  # 1 friendly Unit Lv.2- gains <First Strike>
    b2 = sc.add(0, "ST04-014", Zone.HAND)
    st = sc.start()
    play(st, b1)
    play(st, b2)
    assert keywords(st, unit)["First Strike"] == 1


@pytest.mark.faq("Q64")
@pytest.mark.rule("13-1-6-2", "13-1-6-1")
def test_q64_a_unit_has_at_most_one_high_maneuver() -> None:
    sc = Scenario()
    aerial = sc.add(0, "GD02-074", pilot="GD05-087")  # <High-Maneuver> + Lauda's grant
    blocker = sc.add(1, LAUNCHER_STRIKE)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    assert keywords(st, aerial)["High-Maneuver"] == 1
    attack(st, aerial)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, shield) is Zone.TRASH and not st.cards[blocker].rested


@pytest.mark.faq("Q165")
@pytest.mark.rule("13-1-7-1", "13-1-7-4")
def test_q165_suppression_destroys_two_shields_and_reveals_them_together() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU)
    s1, s2, s3 = sc.shields(1, BURST_ADD, BURST_ADD, VANILLA)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 1)
    for s in (s1, s2):
        assert zone_of(st, s) is Zone.RESOLVING and st.cards[s].known == core.BOTH_KNOW
    assert st.zones[1][Zone.SHIELD] == [s3]
    act(st, A.ORDER, options(st)[0].a)
    yes(st)
    yes(st)
    assert zone_of(st, s1) is Zone.HAND and zone_of(st, s2) is Zone.HAND


@pytest.mark.faq("Q166")
@pytest.mark.rule("13-1-7-4", "10-1-6-5")
def test_q166_shield_owner_orders_simultaneous_bursts() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU)
    s1, s2 = sc.shields(1, BURST_ADD, CLOSE_COMBAT)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 1)
    assert {st.batches[-1][o.a].card_uid for o in options(st)} == {s1, s2}
    second_first = next(o for o in options(st) if st.batches[-1][o.a].card_uid == s2)
    act(st, A.ORDER, second_first.a)
    assert _pending(st) == (DecisionKind.BURST, 1)
    yes(st)
    assert st.cards[zaku].damage == 2
    assert zone_of(st, s2) is Zone.TRASH and zone_of(st, s1) is Zone.RESOLVING


@pytest.mark.faq("Q167")
@pytest.mark.rule("13-1-7-3")
def test_q167_suppression_with_one_shield_does_not_hit_the_player() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU)
    (only,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert zone_of(st, only) is Zone.TRASH
    assert st.winner is None and _pending(st) == (DecisionKind.MAIN, 0)


@pytest.mark.faq("Q195")
@pytest.mark.rule("13-1-7-4", "10-1-6-8-1")
def test_q195_suppression_shields_leave_together_before_a_base_burst() -> None:
    sc = Scenario()
    zaku = sc.add(0, PSYCHO_ZAKU)
    base, other, rest = sc.shields(1, TESTING_SECTOR, BURST_ADD, VANILLA)
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 1)
    base_first = next(o for o in options(st) if st.batches[-1][o.a].card_uid == base)
    act(st, A.ORDER, base_first.a)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, rest) is Zone.HAND
    assert zone_of(st, other) is Zone.RESOLVING
    assert _pending(st) == (DecisionKind.BURST, 1)


# ---------------------------------------------------------------------------------------------
# Timing keywords (Q65-Q83)


@pytest.mark.faq("Q65")
@pytest.mark.rule("13-2-6-1")
def test_q65_deploy_effect_activates_when_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    wing = sc.add(0, WING_BIRD, Zone.HAND)
    in_play = sc.add(0, WING_BIRD)
    st = sc.start()
    assert _ex_count(st, 0) == 0
    play(st, wing)
    assert _ex_count(st, 0) == 1
    assert in_play in st.zones[0][Zone.BATTLE]


@pytest.mark.faq("Q66")
@pytest.mark.rule("13-2-7-1", "8-2-2")
def test_q66_attack_effect_activates_when_the_unit_attacks() -> None:
    sc = Scenario()
    zaku = sc.add(0, "ST03-008")  # 1/2: 【Attack】This Unit gets AP+2 during this turn.
    sc.shields(1, VANILLA)
    st = sc.start()
    assert ap(st, zaku) == 1
    attack(st, zaku)
    assert ap(st, zaku) == 3


@pytest.mark.faq("Q67")
@pytest.mark.rule("8-2-1", "8-2-2", "8-3-1")
def test_q67_attack_effect_resolves_after_the_attack_is_declared() -> None:
    sc = Scenario()
    wz = sc.add(0, "GD05-067")  # 【Attack】Choose 1 enemy Unit. Rest it.
    blocker = sc.add(1, LAUNCHER_STRIKE)
    other = sc.add(1, GM)
    st = sc.start()
    attack(st, wz)
    assert _pending(st) == (DecisionKind.SELECT, 0)
    assert st.cards[wz].rested and st.battle is not None and st.battle.attacker == wz
    act(st, A.SELECT, other)
    assert st.cards[other].rested
    assert _pending(st) == (DecisionKind.BLOCK, 1)
    assert has_action(st, A.BLOCK, blocker)


@pytest.mark.faq("Q68")
@pytest.mark.rule("13-2-8-1")
def test_q68_destroyed_effect_activates_when_destroyed() -> None:
    sc = Scenario()
    duel = sc.add(0, DUEL_AS)
    enemy = sc.add(1, SANDROCK, rested=True)
    st = sc.start()
    attack(st, duel, enemy)
    pass_all(st)
    assert zone_of(st, duel) is Zone.TRASH
    assert _ex_count(st, 0) == 1


UNICORN = "GD01-005"  # 【During Link】【Destroyed】Return this Unit's paired Pilot ... discard 1.
BANAGHER = "GD01-088"  # Pilot matching Unicorn's link condition [Banagher Links]
TITUS = "GD02-027"  # 5/5


@pytest.mark.faq("Q69")
@pytest.mark.rule("13-2-8-2", "13-2-8-2-1")
def test_q69_destroyed_effect_activates_from_the_trash_with_its_last_state() -> None:
    sc = Scenario()
    unicorn = sc.add(0, UNICORN, pilot=BANAGHER)
    enemy = sc.add(1, TITUS, rested=True)
    sc.hand(0, GM, GM)
    st = sc.start()
    assert V.is_linked(V.derived(st), unicorn)
    attack(st, unicorn, enemy)
    pass_all(st)
    assert zone_of(st, unicorn) is Zone.TRASH
    assert _pending(st) == (DecisionKind.DISCARD, 0)

    sc = Scenario()
    unicorn = sc.add(0, UNICORN, pilot=SAYLA)
    enemy = sc.add(1, TITUS, rested=True)
    sc.hand(0, GM, GM)
    st = sc.start()
    sayla = st.cards[unicorn].pair
    attack(st, unicorn, enemy)
    pass_all(st)
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert zone_of(st, sayla) is Zone.TRASH and len(st.zones[0][Zone.HAND]) == 2


@pytest.mark.faq("Q69")
@pytest.mark.rule("13-2-8-2-1", "3-3-6")
def test_q69_destroyed_effect_finds_the_pilot_paired_before_destruction() -> None:
    sc = Scenario()
    unicorn = sc.add(0, UNICORN, pilot=BANAGHER)
    enemy = sc.add(1, TITUS, rested=True)
    sc.hand(0, GM, GM)
    st = sc.start()
    banagher = st.cards[unicorn].pair
    attack(st, unicorn, enemy)
    pass_all(st)
    assert zone_of(st, banagher) is Zone.HAND


@pytest.mark.faq("Q70")
@pytest.mark.rule("13-2-1-1", "10-1-7-2", "7-5-3-1")
def test_q70_activate_main_is_used_in_your_main_phase_only() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    turn_a = sc.add(0, TURN_A)
    unit = sc.add(0, VANILLA)
    sc.add(0, INDIGNATION, Zone.HAND)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert has_action(st, A.ACTIVATE, turn_a)
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND)
    assert not has_action(st, A.ACTIVATE)


@pytest.mark.faq("Q71")
@pytest.mark.rule("13-2-2-1", "9-3-2", "7-6-3-1")
def test_q71_activate_action_is_used_in_either_players_action_steps() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    galluss = sc.add(0, "GD01-058")  # 【Activate･Action】①: 1 Unit Lv.4+ AP+1
    attacker = sc.add(0, SANDROCK)  # Lv4
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, galluss)
    attack(st, attacker)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.ACTIVATE, galluss)

    sc = Scenario(active=1)
    sc.resources(0, 2)
    galluss = sc.add(0, "GD01-058")
    sc.add(1, SANDROCK)
    st = sc.start()
    end_main(st)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.ACTIVATE, galluss)


@pytest.mark.faq("Q72")
@pytest.mark.rule("10-1-7-3")
def test_q72_circled_number_is_a_resource_cost() -> None:
    sc = Scenario()
    res = sc.resources(0, 1)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, turn_a)
    assert st.cards[res[0]].rested and ap(st, turn_a) == 5

    sc = Scenario()
    sc.resources(0, 1, rested=1)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, turn_a)


@pytest.mark.faq("Q73")
@pytest.mark.rule("10-1-7-5", "13-2-2-1")
def test_q73_activated_effect_without_conditions_is_activated_by_declaring() -> None:
    sc = Scenario(active=1)
    sky = sc.add(0, "GD01-014", pilot=SAYLA, damage=1)  # linked: 【Activate･Action】recover 1
    st = sc.start()
    end_main(st)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert not st.zones[0][Zone.RESOURCE_AREA]
    activate(st, sky)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        act(st, A.SELECT, sky)
    assert st.cards[sky].damage == 0


@pytest.mark.faq("Q74")
@pytest.mark.rule("13-2-9-1")
def test_q74_when_paired_activates_when_a_pilot_is_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guncannon = sc.add(0, "GD01-004")  # 【When Paired】rest 1 enemy Unit with 2 or less HP
    pilot = sc.add(0, SAYLA, Zone.HAND)
    enemy = sc.add(1, GM)
    st = sc.start()
    assert not st.cards[enemy].rested
    play(st, pilot, onto=guncannon)
    assert st.cards[enemy].rested


@pytest.mark.faq("Q75")
@pytest.mark.rule("13-2-9-2")
def test_q75_qualified_when_paired_needs_a_matching_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, "GD01-032")  # 【When Paired･(Zeon) Pilot】destroy 1 Lv.2- enemy <Blocker>
    zeon_pilot = sc.add(0, MQUVE, Zone.HAND)
    tragos = sc.add(1, TRAGOS)
    st = sc.start()
    play(st, zeon_pilot, onto=gyan)
    assert zone_of(st, tragos) is Zone.TRASH

    sc = Scenario()
    sc.resources(0, 3)
    gyan = sc.add(0, "GD01-032")
    other_pilot = sc.add(0, SAYLA, Zone.HAND)
    tragos = sc.add(1, TRAGOS)
    st = sc.start()
    play(st, other_pilot, onto=gyan)
    assert zone_of(st, tragos) is Zone.BATTLE


@pytest.mark.faq("Q76")
@pytest.mark.rule("13-2-10-1")
def test_q76_during_pair_applies_while_paired() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gundam = sc.add(0, "ST01-001")  # 【During Pair】During your turn, all your Units get AP+1.
    gm = sc.add(0, GM)
    pilot = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    assert ap(st, gm) == 2
    play(st, pilot, onto=gundam)
    assert ap(st, gm) == 3


@pytest.mark.faq("Q77")
@pytest.mark.rule("13-2-10-2")
def test_q77_during_pair_attack_needs_a_paired_pilot() -> None:
    for paired, expected in ((False, 4), (True, 2)):
        sc = Scenario()
        striker = sc.add(0, "EB01-046", pilot=SAYLA if paired else None)
        enemy = sc.add(1, SANDROCK)
        sc.shields(1, VANILLA)
        sc.resources(0, 2)
        sc.add(0, INDIGNATION, Zone.HAND)
        st = sc.start()
        attack(st, striker)
        assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
        assert ap(st, enemy) == expected


@pytest.mark.faq("Q78")
@pytest.mark.rule("13-2-13-1")
def test_q78_once_per_turn_effect_is_used_once_each_turn() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    turn_a = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, turn_a)
    assert not has_action(st, A.ACTIVATE, turn_a)
    to_next_turn(st)
    to_next_turn(st)
    assert st.active == 0
    assert has_action(st, A.ACTIVATE, turn_a)


@pytest.mark.faq("Q79")
@pytest.mark.rule("13-2-13-2")
def test_q79_each_copy_may_use_its_once_per_turn_effect() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    first = sc.add(0, TURN_A)
    second = sc.add(0, TURN_A)
    st = sc.start()
    activate(st, first)
    assert has_action(st, A.ACTIVATE, second)
    activate(st, second)
    assert ap(st, first) == 5 and ap(st, second) == 5
    assert not has_action(st, A.ACTIVATE)


@pytest.mark.faq("Q80")
@pytest.mark.rule("13-2-5-1")
def test_q80_burst_is_activated_before_the_shield_is_trashed() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, BURST_ADD)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert _pending(st) == (DecisionKind.BURST, 1)
    assert zone_of(st, shield) is not Zone.TRASH
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.faq("Q81")
@pytest.mark.rule("13-2-5-2")
def test_q81_declining_a_burst_trashes_the_card() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, CLOSE_COMBAT)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    no(st)
    assert zone_of(st, shield) is Zone.TRASH and st.cards[unit].damage == 0


@pytest.mark.faq("Q82")
@pytest.mark.rule("13-2-5-3", "4-9-1")
def test_q82_card_goes_to_the_trash_after_its_burst_resolves() -> None:
    sc = Scenario()
    unit = sc.add(0, GUNCANNON)
    (shield,) = sc.shields(1, CLOSE_COMBAT)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    yes(st)
    assert st.cards[unit].damage == 2
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.faq("Q83")
@pytest.mark.rule("13-2-5-1")
def test_q83_shield_added_to_hand_by_an_effect_cannot_burst() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    base = sc.add(0, EARTH_HOUSE, Zone.HAND)
    (shield,) = sc.shields(0, BURST_ADD)
    st = sc.start()
    play(st, base)
    assert zone_of(st, shield) is Zone.HAND
    assert _pending(st) == (DecisionKind.MAIN, 0)
    assert "burst_revealed" not in _kinds(st)


# ---------------------------------------------------------------------------------------------
# Card types (Q84-Q93, Q163)


@pytest.mark.faq("Q84")
@pytest.mark.rule("13-1-3-1")
def test_q84_friendly_and_your_select_the_same_units_in_1v1() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    a = sc.add(0, VANILLA)
    b = sc.add(0, GM)
    zuoot = sc.add(0, ZUOOT)  # <Support 1>: "choose one other friendly unit"
    sc.add(1, VANILLA)
    friendly = sc.add(0, INDIGNATION, Zone.HAND)  # "Choose 1 friendly Unit"
    yours = sc.add(0, "ST05-013", Zone.HAND)  # "Choose 1 of your Units"
    st = sc.start()
    activate(st, zuoot, SUPPORT_AID)
    support_set = _select_uids(st)
    act(st, A.SELECT, a)
    play(st, friendly)
    friendly_set = _select_uids(st)
    act(st, A.SELECT, a)
    play(st, yours)
    assert support_set == {a, b}
    assert friendly_set == _select_uids(st) == {a, b, zuoot}


@pytest.mark.faq("Q85")
@pytest.mark.rule("3-3-3")
def test_q85_pilot_cannot_enter_the_battle_area_alone() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    pilot = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    assert not any(o.a == pilot for o in options(st))


@pytest.mark.faq("Q86")
@pytest.mark.rule("3-3-4", "3-3-5")
def test_q86_cannot_swap_the_pilot_of_a_paired_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    paired = sc.add(0, GM, pilot=SAYLA)
    free = sc.add(0, GUNCANNON)
    pilot = sc.add(0, MQUVE, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PAIR, pilot, free)
    assert not has_action(st, A.PAIR, pilot, paired)


@pytest.mark.faq("Q87")
@pytest.mark.rule("3-3-5")
def test_q87_paired_pilot_cannot_be_moved_to_another_unit() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    paired = sc.add(0, GM, pilot=SAYLA)
    sc.add(0, GUNCANNON)
    st = sc.start()
    pilot = st.cards[paired].pair
    assert not any(o.a == pilot or o.b == pilot for o in options(st))


@pytest.mark.faq("Q88")
@pytest.mark.rule("3-3-9-2", "2-11-3")
def test_q88_pilot_effect_resolves_as_the_units_effect() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, pilot="GD05-085")  # Amuro: destroys by battle -> Unit recovers 2
    enemy = sc.add(1, MAGANAC, rested=True)  # 3/2
    st = sc.start()
    attack(st, unit, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[unit].damage == 1


@pytest.mark.faq("Q89")
@pytest.mark.rule("3-3-8-1", "3-2-6-2", "3-3-9-2")
def test_q89_non_linking_pilot_still_adds_stats_and_effects() -> None:
    sc = Scenario()
    res = sc.resources(0, 2, rested=2)
    gundam = sc.add(0, "ST01-001", pilot="ST01-011")  # [Amuro Ray] + Suletta Mercury (+1/+2)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert not V.is_linked(V.derived(st), gundam)
    assert (ap(st, gundam), hp(st, gundam)) == (5, 6)  # 3+1 +1 (own 【During Pair】), 4+2
    attack(st, gundam)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        act(st, A.SELECT, res[0])
    assert sum(not st.cards[u].rested for u in res) == 1

    sc = Scenario()
    sc.resources(0, 4)
    fresh = sc.add(0, "ST01-001", deployed_this_turn=True)
    suletta = sc.add(0, "ST01-011", Zone.HAND)
    st = sc.start()
    play(st, suletta, onto=fresh)
    assert not has_action(st, A.ATTACK, fresh)


@pytest.mark.faq("Q89")
@pytest.mark.rule("3-3-8-1", "3-2-6-2")
@pytest.mark.xfail(
    strict=True,
    raises=UnimplementedCardError,
    reason="ENGINE: ST03-011 Char Aznable (named by FAQ Q89) does not compile",
)
def test_q89_char_aznable_on_an_amuro_ray_unit() -> None:
    sc = Scenario()
    gundam = sc.add(0, "ST01-001", pilot="ST03-011")
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert not V.is_linked(V.derived(st), gundam)
    assert (ap(st, gundam), hp(st, gundam)) == (5, 5)
    attack(st, gundam, enemy)
    assert ap(st, gundam) == 6
    assert "High-Maneuver" not in keywords(st, gundam)


@pytest.mark.faq("Q90")
@pytest.mark.rule("13-2-3-1", "3-4-5", "7-5-2-1")
def test_q90_main_command_is_played_in_your_main_phase() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    unit = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD01-105", Zone.HAND)  # 【Main】All your Units get AP+2
    sc.add(0, INDIGNATION, Zone.HAND)
    sc.shields(1, VANILLA)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, cmd)
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert not has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.faq("Q91")
@pytest.mark.rule("13-2-4-1", "9-3-1", "8-4-1")
def test_q91_action_command_is_played_in_either_players_action_steps() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA)
    timbre = sc.add(0, PEACEFUL_TIMBRE, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, timbre)
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, timbre)

    sc = Scenario(active=1)
    sc.resources(0, 4)
    timbre = sc.add(0, PEACEFUL_TIMBRE, Zone.HAND)
    st = sc.start()
    end_main(st)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, timbre)


@pytest.mark.faq("Q92")
@pytest.mark.rule("13-2-3-2", "13-2-4-3")
def test_q92_main_action_command_is_played_at_either_time() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unit = sc.add(0, VANILLA)
    cmd = sc.add(0, INDIGNATION, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, cmd)
    attack(st, unit)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert has_action(st, A.PLAY_COMMAND, cmd)


@pytest.mark.faq("Q93")
@pytest.mark.rule("2-4-3")
def test_q93_paired_pilot_does_not_change_the_units_color() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    blue = sc.add(0, GM, pilot="ST14-012")  # green Pilot Banagher Links
    green = sc.add(0, RICK_DOM)
    rasid = sc.add(0, "GD01-043", Zone.HAND)  # 【Deploy】Choose 1 of your green Units
    st = sc.start()
    play(st, rasid)
    assert _pending(st) == (DecisionKind.SELECT, 0)
    assert _select_uids(st) == {green, rasid}
    assert blue not in _select_uids(st)

    sc = Scenario()
    green_unit = sc.add(0, RICK_DOM, pilot=SAYLA)  # blue Pilot: "while this Unit is blue"
    blue_unit = sc.add(0, GM, pilot=SAYLA)
    st = sc.start()
    assert "Repair" not in keywords(st, green_unit)
    assert keywords(st, blue_unit)["Repair"] == 1


@pytest.mark.faq("Q163")
@pytest.mark.rule("3-4-6-3", "3-4-6-4")
def test_q163_command_with_pilot_is_a_pilot_only_while_paired() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, GM)
    timbre = sc.add(0, PEACEFUL_TIMBRE, Zone.HAND)
    st = sc.start()
    assert _is_kind(st, timbre, d.CardKind.COMMAND)
    assert not _is_kind(st, timbre, d.CardKind.PILOT)
    play(st, timbre, onto=unit)
    assert _is_kind(st, timbre, d.CardKind.PILOT)
    assert not _is_kind(st, timbre, d.CardKind.COMMAND)


# ---------------------------------------------------------------------------------------------
# AP / HP / Damage / Target (Q94-Q100)


@pytest.mark.faq("Q94")
@pytest.mark.rule("1-3-6")
def test_q94_ap_reduced_below_zero_is_treated_as_zero() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    enemy = sc.add(1, VANILLA, rested=True)  # 2 AP
    attacker = sc.add(0, GM)
    cmd = sc.add(0, "ST01-014", Zone.HAND)  # 1 enemy Unit AP-3
    st = sc.start()
    play(st, cmd)
    assert ap(st, enemy) == 0
    attack(st, attacker, enemy)
    pass_all(st)
    assert st.cards[attacker].damage == 0
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.faq("Q95")
@pytest.mark.rule("1-3-6")
def test_q95_ap_modifiers_are_summed_before_clamping() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zaku = sc.add(1, ZAKU_I)  # 1 AP
    spark = sc.add(0, "GD05-118", Zone.HAND)  # 【Main】1 enemy Unit AP-2
    sc.add(0, GM)
    sc.add(0, INDIGNATION, Zone.HAND)
    sc.resources(1, 3)
    chips = sc.add(1, "EB01-083", Zone.HAND)  # 【Action】opponent's turn: 1 Unit AP+3
    st = sc.start()
    play(st, spark)
    assert ap(st, zaku) == 0
    end_main(st)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 1)
    act(st, A.PLAY_COMMAND, chips)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        act(st, A.SELECT, zaku)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert ap(st, zaku) == 2


def test_q96_control_hp_filter_matches_an_undamaged_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guntank = sc.add(0, "ST01-004", Zone.HAND)
    gm = sc.add(1, GM)  # 2 HP
    st = sc.start()
    play(st, guntank)
    assert st.cards[gm].rested


@pytest.mark.faq("Q96")
@pytest.mark.rule("2-8-2", "5-5-1-1")
def test_q96_hp_in_card_text_is_current_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    guntank = sc.add(0, "ST01-004", Zone.HAND)  # 【Deploy】rest 1 enemy Unit with 2 or less HP
    damaged = sc.add(1, GUNCANNON, damage=2)  # 2/4 with 2 damage: current HP 2
    st = sc.start()
    play(st, guntank)
    assert st.cards[damaged].rested


@pytest.mark.faq("Q97")
@pytest.mark.rule("5-5-3", "5-5-4")
def test_q97_battle_damage_is_damage_dealt_by_ap_in_battle() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    unit = sc.add(0, LAUNCHER_STRIKE)  # 3/4
    enemy = sc.add(1, MAGANAC, rested=True)  # 3 AP
    dc = sc.add(0, "GD04-113", Zone.HAND)  # 【Action】reduce battle damage it receives by 3
    sc.resources(1, 2)
    cc = sc.add(1, CLOSE_COMBAT, Zone.HAND)  # 2 effect damage
    st = sc.start()
    attack(st, unit, enemy)
    pass_(st)
    act(st, A.PLAY_COMMAND, dc)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 1)
    act(st, A.PLAY_COMMAND, cc)
    assert st.cards[unit].damage == 2
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[unit].damage == 2


@pytest.mark.faq("Q98")
@pytest.mark.rule("5-5-3", "5-10-1")
def test_q98_destroyed_with_damage_includes_battle_damage() -> None:
    sc = Scenario()
    tallgeese = sc.add(0, "ST12-003")  # destroys enemy Unit with damage -> 1 dmg to <=3 AP
    enemy = sc.add(1, VANILLA, rested=True)
    bystander = sc.add(1, GUNCANNON)
    st = sc.start()
    attack(st, tallgeese, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[bystander].damage == 1


@pytest.mark.faq("Q99")
@pytest.mark.rule("5-5-1-1", "5-18-1-1")
def test_q99_damage_stays_at_the_end_of_the_turn() -> None:
    sc = Scenario()
    unit = sc.add(0, GUNCANNON, damage=2)
    enemy_unit = sc.add(1, GUNCANNON, damage=1)
    st = sc.start()
    to_next_turn(st)
    to_next_turn(st)
    assert st.cards[unit].damage == 2 and st.cards[enemy_unit].damage == 1


@pytest.mark.faq("Q100")
@pytest.mark.rule("10-1-8-1-1", "10-2-2", "10-3-3-1")
def test_q100_no_target_means_the_card_or_effect_cannot_be_used() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gm = sc.add(0, GM)
    geara = sc.add(0, "GD01-053")  # 【Activate･Main】①: 1 enemy Unit with 2 or less AP
    cmd = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)  # 【Main】1 rested enemy Unit
    sc.add(1, SANDROCK)  # active, 4 AP
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, cmd)
    assert has_action(st, A.PAIR, cmd, gm)
    assert not has_action(st, A.ACTIVATE, geara)

    sc = Scenario()
    sc.resources(0, 3)
    geara = sc.add(0, "GD01-053")
    cmd = sc.add(0, THOROUGHLY_DAMAGED, Zone.HAND)
    sc.add(1, VANILLA, rested=True)
    st = sc.start()
    assert has_action(st, A.PLAY_COMMAND, cmd)
    assert has_action(st, A.ACTIVATE, geara)


# ---------------------------------------------------------------------------------------------
# Tokens (Q101-Q104)


@pytest.mark.faq("Q101")
@pytest.mark.rule("5-17-2-3", "2-4-2")
def test_q101_tokens_have_no_color() -> None:
    sc = Scenario()
    token = sc.add(0, "T-001")  # Gundam token
    st = sc.start()
    assert V.cdef(st, token).color is None
    for color in ("Blue", "Green", "Red", "White", "Purple"):
        assert not V.matches(st, V.derived(st), V.Ctx(0), token, (d.HasColor((color,)),))


@pytest.mark.faq("Q102")
@pytest.mark.rule("5-17-2-4", "2-9-3", "2-10-3")
def test_q102_token_level_and_cost_are_zero() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    token = sc.add(1, "T-005")  # Tallgeese token 4/2
    big = sc.add(1, "GD02-027")  # Lv7
    inspector = sc.add(0, "GD04-112", Zone.HAND)  # 1 damage to all Units Lv.2 or lower
    st = sc.start()
    assert V.level_of(st, token) == 0 and V.cost_of(st, token) == 0
    play(st, inspector)
    assert st.cards[token].damage == 1 and st.cards[big].damage == 0


@pytest.mark.faq("Q103")
@pytest.mark.rule("5-17-2-2")
def test_q103_pilot_can_pair_with_a_unit_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    token = sc.add(0, "T-002")  # Guncannon token 2/2
    pilot = sc.add(0, SAYLA, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=token)
    assert st.cards[token].pair == pilot
    assert (ap(st, token), hp(st, token)) == (3, 3)


@pytest.mark.faq("Q104")
@pytest.mark.rule("5-17-2-1", "5-17-2-5", "11-4-2")
def test_q104_unit_tokens_count_toward_the_six_unit_limit() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    token = sc.add(0, "T-002")
    for _ in range(5):
        sc.add(0, VANILLA)
    new = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, new)
    assert _pending(st) == (DecisionKind.EXCESS, 0)
    assert token in _select_uids(st)
    act(st, A.SELECT, token)
    assert zone_of(st, token) is Zone.OUTSIDE
    assert len(st.zones[0][Zone.BATTLE]) == 6


# ---------------------------------------------------------------------------------------------
# Miscellaneous (Q105-Q112)


@pytest.mark.faq("Q105")
def test_q105_all_your_units_buff_does_not_reach_units_deployed_later() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    old = sc.add(0, VANILLA)
    cmd = sc.add(0, "GD01-105", Zone.HAND)
    new = sc.add(0, GM, Zone.HAND)
    st = sc.start()
    play(st, cmd)
    assert ap(st, old) == 4
    play(st, new)
    assert ap(st, new) == 2


@pytest.mark.faq("Q106")
@pytest.mark.rule("7-6-6-1", "10-1-7-2")
def test_q106_during_this_turn_effect_outlives_its_source() -> None:
    sc = Scenario()
    worker = sc.add(0, "ST05-003")  # 【Activate･Main】Rest: 1 of your Units: 1 damage, AP+1
    other = sc.add(0, VANILLA)
    sc.resources(0, 2)
    sc.add(0, INDIGNATION, Zone.HAND)
    sc.resources(1, 2)
    cc = sc.add(1, CLOSE_COMBAT, Zone.HAND)
    st = sc.start()
    activate(st, worker)
    act(st, A.SELECT, other)
    assert ap(st, other) == 3 and st.cards[other].damage == 1
    end_main(st)
    act(st, A.PLAY_COMMAND, cc)
    act(st, A.SELECT, worker)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert zone_of(st, worker) is Zone.TRASH
    assert ap(st, other) == 3


@pytest.mark.faq("Q106")
@pytest.mark.rule("7-6-6-1", "13-1-3-1")
def test_q106_support_bonus_outlives_the_support_unit() -> None:
    sc = Scenario()
    gaza = sc.add(0, "ST03-004")  # 2/1 <Support 2>
    other = sc.add(0, VANILLA)
    sc.resources(0, 2)
    sc.add(0, INDIGNATION, Zone.HAND)
    sc.resources(1, 2)
    cc = sc.add(1, CLOSE_COMBAT, Zone.HAND)
    st = sc.start()
    activate(st, gaza, SUPPORT_AID)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        act(st, A.SELECT, other)
    assert ap(st, other) == 4
    end_main(st)
    act(st, A.PLAY_COMMAND, cc)
    if st.pending is not None and st.pending.kind is DecisionKind.SELECT:
        act(st, A.SELECT, gaza)
    assert _pending(st) == (DecisionKind.ACTION_STEP, 0)
    assert zone_of(st, gaza) is Zone.TRASH
    assert ap(st, other) == 4


@pytest.mark.faq("Q107")
@pytest.mark.rule("10-1-6-5")
def test_q107_you_choose_the_order_of_your_simultaneous_effects() -> None:
    for draw_first in (True, False):
        sc = Scenario()
        sc.resources(0, 4)
        ma = sc.add(0, "ST01-002")  # 【When Paired･(White Base Team) Pilot】Draw 1.
        amuro = sc.add(0, "ST01-010", Zone.HAND)  # 【When Paired】rest 1 enemy Unit, 5 or less HP
        e1 = sc.add(1, GM)
        sc.add(1, VANILLA)
        st = sc.start()
        play(st, amuro, onto=ma)
        assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 0)
        hand = len(st.zones[0][Zone.HAND])
        pick = next(o for o in options(st) if (st.batches[-1][o.a].card_uid == ma) == draw_first)
        act(st, A.ORDER, pick.a)
        if draw_first:
            assert len(st.zones[0][Zone.HAND]) == hand + 1
        assert _pending(st) == (DecisionKind.SELECT, 0)
        if not draw_first:
            assert len(st.zones[0][Zone.HAND]) == hand
        act(st, A.SELECT, e1)
        assert st.cards[e1].rested and len(st.zones[0][Zone.HAND]) == hand + 1


@pytest.mark.faq("Q108")
@pytest.mark.rule("10-1-6-6", "8-5-3-2-3")
def test_q108_active_players_effects_resolve_before_standby_players() -> None:
    sc = Scenario()
    mine = sc.add(0, DUEL_AS)
    theirs = sc.add(1, DUEL_AS, rested=True)
    st = sc.start()
    attack(st, mine, theirs)
    pass_all(st)
    assert zone_of(st, mine) is Zone.TRASH and zone_of(st, theirs) is Zone.TRASH
    placed_for = [h.player for h in st.history if h.kind == "ex_resource"]
    assert placed_for == [0, 1]


@pytest.mark.faq("Q109")
@pytest.mark.rule("10-1-6-7")
def test_q109_new_effect_resolves_before_already_waiting_effects() -> None:
    sc = Scenario()
    geara = sc.add(0, "GD01-056")  # 2/3 【Destroyed】1 damage to 1 enemy Unit with <=5 AP
    duel = sc.add(1, DUEL_AS, rested=True)  # 3/2 【Destroyed】Place 1 EX Resource.
    raider = sc.add(1, "GD02-010")  # 4/4: receives enemy effect damage -> draw 1
    st = sc.start()
    attack(st, geara, duel)
    pass_all(st)
    assert st.cards[raider].damage == 1
    p1_events = [h.kind for h in st.history if h.player == 1 and h.kind in ("draw", "ex_resource")]
    assert p1_events == ["draw", "ex_resource"]


@pytest.mark.faq("Q109")
@pytest.mark.rule("10-1-6-7", "10-1-6-6")
def test_q109_simultaneous_new_effects_resolve_active_player_first() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    mine = sc.add(0, VANILLA, pilot="GD03-095")  # Azee: receives effect damage -> enemy AP-1
    theirs = sc.add(1, VANILLA, pilot="GD03-095")
    bystander = sc.add(1, GUNCANNON)  # Lv3: not hit by Inspector
    inspector = sc.add(0, "GD04-112", Zone.HAND)  # 1 damage to all Units Lv.2 or lower
    st = sc.start()
    play(st, inspector)
    assert st.cards[mine].damage == 1 and st.cards[theirs].damage == 1
    assert _pending(st) == (DecisionKind.SELECT, 0)
    assert _select_uids(st) == {theirs, bystander}
    assert ap(st, mine) == 3
    act(st, A.SELECT, bystander)
    assert ap(st, bystander) == 1
    assert ap(st, mine) == 2


@pytest.mark.faq("Q110")
@pytest.mark.rule("10-1-6-8", "10-1-6-7")
def test_q110_burst_revealed_by_breach_resolves_before_waiting_effects() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=TROWA)  # 4/5 <Breach 2>
    enemy = sc.add(1, MAGANAC, rested=True)  # 3/2
    (shield,) = sc.shields(1, CLOSE_COMBAT)
    sc.hand(0, GM)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert _pending(st) == (DecisionKind.ORDER_TRIGGER, 0)
    act(st, A.ORDER, _batch_index(st, get_registry().breach_program))
    assert _pending(st) == (DecisionKind.BURST, 1)
    assert "draw" not in _kinds(st)
    yes(st)
    assert zone_of(st, rick) is Zone.TRASH  # 3 battle + 2 Close Combat damage on 5 HP
    assert zone_of(st, shield) is Zone.TRASH
    assert _pending(st) == (DecisionKind.DISCARD, 0)


@pytest.mark.faq("Q111")
@pytest.mark.rule("10-1-6-4")
def test_q111_waiting_effect_resolves_after_its_unit_is_destroyed() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM, pilot=TROWA)
    enemy = sc.add(1, MAGANAC, rested=True)
    sc.shields(1, CLOSE_COMBAT)
    sc.hand(0, GM)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    act(st, A.ORDER, _batch_index(st, get_registry().breach_program))
    yes(st)
    assert zone_of(st, rick) is Zone.TRASH
    assert _pending(st) == (DecisionKind.DISCARD, 0)
    assert len(_events(st, "draw")) == 1


@pytest.mark.faq("Q112")
@pytest.mark.rule("1-3-3", "10-1-5-6")
def test_q112_cant_effects_take_priority() -> None:
    sc = Scenario()
    rick = sc.add(0, RICK_DOM)
    enemy = sc.add(1, VANILLA, rested=True)
    argama = sc.add(1, "GD02-129", Zone.BASE)  # This Base can't receive enemy effect damage
    shields = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, rick, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[argama].damage == 0 and st.zones[1][Zone.SHIELD] == shields


# ---------------------------------------------------------------------------------------------
# FAQ bookkeeping


def test_every_faq_entry_is_tested_or_listed_na() -> None:
    nums = {e["num"] for e in json.loads(read_data_text("gcgapi", "rules-faq.json"))}
    na = json.loads((TESTS / "meta" / "faq_na.json").read_text(encoding="utf-8"))
    tested = values(scan(TESTS, ("faq",)), "faq")
    assert not set(na) & tested
    assert set(na) <= nums and tested <= nums
    assert nums - tested - set(na) == set()
