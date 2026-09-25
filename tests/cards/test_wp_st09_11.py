"""Card behaviour tests for starter decks ST09, ST10 and ST11."""

from __future__ import annotations

import pytest

from gcg_sim.effects.dsl import RuleKind
from gcg_sim.engine import view as V
from gcg_sim.engine.game import ALT_PLAY_BASE
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Step, Zone
from gcg_sim.testkit import arrange_if_asked
from gcg_sim.testkit import (
    Scenario,
    act,
    activate,
    ap,
    attack,
    block,
    has_action,
    keywords,
    no,
    numbers_in,
    order,
    pass_all,
    play,
    select,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner: red (Zeon) Lv2 cost1 2/2, no effects
BLOCKER_3_4 = "GD01-072"  # Launcher Strike Gundam: white Lv4 3/4 <Blocker>
PILOT_PLAIN = "GD01-089"  # Riddhe Marcenas: Lv3 Pilot, +1/+1, no triggered effects
ATHRUN = "ST04-011"  # Athrun Zala (Pilot) — satisfies Saviour Gundam's link condition
SIMULTANEOUS_FIRE = "ST02-012"  # 【Main】Choose 1 of your Units. It gains <Breach 3> this turn.
ZSSA = "GD04-043"  # 【Deploy】Choose 1 enemy Base. Deal 1 damage to it.

IMPULSE = "ST09-001"
FORCE_IMPULSE = "ST09-002"
SAVIOUR = "ST09-003"
FREEDOM = "ST09-004"
ZAKU_WARRIOR = "ST09-005"
SWORD_IMPULSE = "ST09-006"
BLAST_IMPULSE = "ST09-007"
SHINN = "ST09-008"
GIANT_KILLING = "ST09-009"
MINERVA = "ST09-010"

ZETA_EX = "ST10-001"
ZETA = "ST10-002"
SUPER_GUNDAM = "ST10-004"
NEMO = "ST10-005"
PHOENIX = "ST10-006"
BARBATOS_4TH = "ST10-007"
BARBATOS_1ST = "ST10-008"
GRAZE = "ST10-009"
MOBILE_WORKER = "ST10-010"
KAMILLE = "ST10-011"
MARK_GUILDER = "ST10-012"
TACTICAL_TRAINING = "ST10-013"
UNLOCKING = "ST10-014"
DIFFUSE_BEAM = "ST10-015"
LUNA_MANA = "ST10-016"

ZGOK = "ST11-001"
ACGUY = "ST11-002"
ZOCK = "ST11-003"
ZNO = "ST11-004"
ASH = "ST11-005"
SHAMBLO = "ST11-006"
LEOPARD = "ST11-007"
DAUGHSEAT = "ST11-008"
KAPOOL = "ST11-009"
ABYSS = "ST11-010"
CHAR = "ST11-011"
LONI = "ST11-012"
POORLY_PLANNED = "ST11-013"
ORCA = "ST11-014"
TWINKLE = "ST11-015"
MAD_ANGLER = "ST11-016"


def rested_resources(st: GameState, player: int) -> int:
    return sum(1 for u in st.zones[player][Zone.RESOURCE_AREA] if st.cards[u].rested)


def resolve_trigger_order(st: GameState) -> None:
    while st.pending is not None and st.pending.kind is DecisionKind.ORDER_TRIGGER:
        order(st, 0)


def attack_options(st: GameState, attacker: int) -> set[int]:
    return {
        o.b
        for o in (st.pending.options if st.pending else ())
        if o.kind is A.ATTACK and o.a == attacker
    }


def select_options(st: GameState) -> set[int]:
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    return {o.a for o in st.pending.options if o.kind is A.SELECT}


# ---------------------------------------------------------------------------------------------
# ST09


@pytest.mark.card("ST09-001")
@pytest.mark.ruling("ST09-001:Q256")
@pytest.mark.rule("10-1-7-4", "3-3-6")
def test_st09_001_returns_itself_and_deploys_impulse_without_paying_its_cost() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    impulse = sc.add(0, IMPULSE, pilot=PILOT_PLAIN)
    force = sc.trash(0, FORCE_IMPULSE)[0]
    st = sc.start()
    pilot = st.cards[impulse].pair
    activate(st, impulse)
    arrange_if_asked(st)  # rule 4-1-6: the owner orders the Unit and its Pilot
    deck = st.zones[0][Zone.DECK]
    assert zone_of(st, impulse) is Zone.DECK and zone_of(st, pilot) is Zone.DECK
    assert set(deck[-2:]) == {impulse, pilot}
    assert zone_of(st, force) is Zone.BATTLE and not st.cards[force].rested
    assert rested_resources(st, 0) == 2  # Q256: only ② was paid, not Force Impulse's cost 4


@pytest.mark.card("ST09-001", "ST09-006")
@pytest.mark.ruling("ST09-001:Q257")
def test_st09_001_deployed_sword_impulse_triggers_its_deploy_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    impulse = sc.add(0, IMPULSE)
    sword = sc.trash(0, SWORD_IMPULSE)[0]
    small = sc.add(1, VANILLA)
    big = sc.add(1, BLOCKER_3_4)
    st = sc.start()
    activate(st, impulse)
    assert zone_of(st, sword) is Zone.BATTLE
    assert zone_of(st, small) is Zone.TRASH  # Sword Impulse 【Deploy】 from trash (Lv.3 or lower)
    assert zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("ST09-001")
@pytest.mark.rule("10-2-2")
def test_st09_001_needs_lv4_impulse_card_in_trash_and_two_resources() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    impulse = sc.add(0, IMPULSE)
    sc.trash(0, IMPULSE, BLOCKER_3_4)  # Lv.3 Impulse and a Lv.4 non-Impulse Unit
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, impulse)

    sc = Scenario()
    sc.resources(0, 1)
    impulse = sc.add(0, IMPULSE)
    sc.trash(0, BLAST_IMPULSE)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, impulse)


@pytest.mark.card("ST09-002")
@pytest.mark.rule("13-2-8-1")
def test_st09_002_destroyed_adds_other_minerva_squad_unit_card() -> None:
    sc = Scenario()
    force = sc.add(0, FORCE_IMPULSE, damage=3)
    other_force = sc.trash(0, FORCE_IMPULSE)[0]
    zaku = sc.trash(0, ZAKU_WARRIOR)[0]
    sc.trash(0, VANILLA)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, force, enemy)
    pass_all(st)
    assert zone_of(st, force) is Zone.TRASH
    assert zone_of(st, zaku) is Zone.HAND
    assert zone_of(st, other_force) is Zone.TRASH


@pytest.mark.card("ST09-002")
def test_st09_002_no_eligible_card_adds_nothing() -> None:
    sc = Scenario()
    force = sc.add(0, FORCE_IMPULSE, damage=3)
    other_force = sc.trash(0, FORCE_IMPULSE)[0]
    zaku_mariner = sc.trash(0, VANILLA)[0]
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    hand = len(st.zones[0][Zone.HAND])
    attack(st, force, enemy)
    pass_all(st)
    assert zone_of(st, force) is Zone.TRASH
    assert zone_of(st, other_force) is Zone.TRASH and zone_of(st, zaku_mariner) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == hand


def _saviour_link(purple_in_trash: int) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    saviour = sc.add(0, SAVIOUR)
    mine = sc.add(0, VANILLA)
    theirs = sc.add(1, VANILLA)
    sc.trash(0, *([ZAKU_WARRIOR] * purple_in_trash), VANILLA, VANILLA)
    athrun = sc.add(0, ATHRUN, Zone.HAND)
    st = sc.start()
    play(st, athrun, onto=saviour)
    resolve_trigger_order(st)
    return st, saviour, mine, theirs


@pytest.mark.card("ST09-003")
@pytest.mark.ruling("ST09-003:Q258")
@pytest.mark.rule("13-2-11-1")
def test_st09_003_when_linked_damages_all_units_with_5_or_less_ap() -> None:
    st, saviour, mine, theirs = _saviour_link(5)
    assert zone_of(st, theirs) is Zone.TRASH
    assert zone_of(st, mine) is Zone.TRASH  # Q258: friendly Units are damaged too
    assert st.cards[saviour].damage == 0  # 6 AP while linked with Athrun (+1)
    assert keywords(st, saviour).get("Breach") == 3


@pytest.mark.card("ST09-003")
def test_st09_003_needs_five_purple_cards_in_trash() -> None:
    st, _saviour, mine, theirs = _saviour_link(4)
    assert zone_of(st, theirs) is Zone.BATTLE and st.cards[theirs].damage == 0
    assert zone_of(st, mine) is Zone.BATTLE and st.cards[mine].damage == 0


@pytest.mark.card("ST09-004")
@pytest.mark.ruling("ST09-004:Q259")
@pytest.mark.rule("5-17-3-1-1", "10-1-5-3")
def test_st09_004_ex_base_grants_suppression() -> None:
    sc = Scenario()
    freedom = sc.add(0, FREEDOM)
    sc.base(0)
    st = sc.start()
    assert keywords(st, freedom) == {"Blocker": 1, "Suppression": 1}


@pytest.mark.card("ST09-004")
def test_st09_004_no_base_no_suppression_but_blocks() -> None:
    sc = Scenario(active=1)
    freedom = sc.add(0, FREEDOM)
    attacker = sc.add(1, VANILLA)
    sc.shields(0, VANILLA)
    st = sc.start()
    assert keywords(st, freedom) == {"Blocker": 1}
    attack(st, attacker)
    block(st, freedom)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH


@pytest.mark.card("ST09-006")
def test_st09_006_deployed_from_hand_destroys_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sword = sc.add(0, SWORD_IMPULSE, Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, sword)
    assert zone_of(st, sword) is Zone.BATTLE
    assert zone_of(st, enemy) is Zone.BATTLE


@pytest.mark.card("ST09-007", "ST10-009", "ST10-010", "ST11-010")
@pytest.mark.rule("13-1-4-1")
@pytest.mark.parametrize("number", [BLAST_IMPULSE, GRAZE, MOBILE_WORKER, ABYSS])
def test_blocker_units_can_block(number: str) -> None:
    sc = Scenario(active=1)
    blocker = sc.add(0, number)
    attacker = sc.add(1, VANILLA)
    top = sc.shields(0, VANILLA)[0]
    st = sc.start()
    assert keywords(st, blocker) == {"Blocker": 1}
    attack(st, attacker)
    block(st, blocker)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, top) is Zone.SHIELD


@pytest.mark.card("ST09-008")
@pytest.mark.ruling("ST09-008:Q260", "ST09-008:Q261")
@pytest.mark.rule("3-3-9-2")
def test_st09_008_attack_sets_chosen_resource_active_including_ex() -> None:
    sc = Scenario()
    rested = sc.resources(0, 2, rested=2)
    active = sc.resources(0, 1)
    ex = sc.add(0, sc.db.ex_resource.card_number, Zone.RESOURCE_AREA, rested=True)
    zaku = sc.add(0, ZAKU_WARRIOR, pilot=SHINN)
    st = sc.start()
    attack(st, zaku)
    assert select_options(st) == {*rested, *active, ex}  # Q260 active, Q261 EX Resource
    select(st, ex)
    assert not st.cards[ex].rested
    assert all(st.cards[u].rested for u in rested)


@pytest.mark.card("ST09-008")
@pytest.mark.ruling("ST09-008:Q260")
def test_st09_008_choosing_an_active_resource_changes_nothing() -> None:
    sc = Scenario()
    rested = sc.resources(0, 1, rested=1)
    active = sc.resources(0, 1)
    zaku = sc.add(0, ZAKU_WARRIOR, pilot=SHINN)
    st = sc.start()
    attack(st, zaku)
    select(st, active[0])
    assert st.cards[rested[0]].rested and not st.cards[active[0]].rested


@pytest.mark.card("ST09-008")
def test_st09_008_non_minerva_unit_sets_nothing_active() -> None:
    sc = Scenario()
    rested = sc.resources(0, 2, rested=2)
    zaku = sc.add(0, VANILLA, pilot=SHINN)
    top = sc.shields(1, VANILLA)[0]
    st = sc.start()
    attack(st, zaku)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert all(st.cards[u].rested for u in rested)


@pytest.mark.card("ST09-008", "ST10-011", "ST10-012", "ST11-011", "ST11-012")
@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize("number", [SHINN, KAMILLE, MARK_GUILDER, CHAR, LONI])
def test_pilot_burst_adds_to_hand(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    shield = sc.shields(1, number)[0]
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


@pytest.mark.card("ST09-009")
@pytest.mark.rule("10-1-8-1-1")
def test_st09_009_destroys_active_enemy_with_4_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kill = sc.add(0, GIANT_KILLING, Zone.HAND)
    target = sc.add(1, VANILLA)
    rested = sc.add(1, VANILLA, rested=True)
    big = sc.add(1, FORCE_IMPULSE)
    st = sc.start()
    play(st, kill)
    assert zone_of(st, target) is Zone.TRASH
    assert zone_of(st, rested) is Zone.BATTLE and zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("ST09-009")
@pytest.mark.rule("10-1-8-1-1")
def test_st09_009_cannot_be_played_without_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    kill = sc.add(0, GIANT_KILLING, Zone.HAND)
    sc.add(1, VANILLA, rested=True)
    sc.add(1, FORCE_IMPULSE)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, kill)


@pytest.mark.card("ST09-010")
@pytest.mark.rule("5-20-2")
def test_st09_010_deploy_on_your_turn_keeps_one_of_top_two() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    minerva = sc.add(0, MINERVA, Zone.HAND)
    shield = sc.shields(0, VANILLA)[0]
    sc.deck(0, ZAKU_WARRIOR, NEMO)
    st = sc.start()
    first, second = st.zones[0][Zone.DECK][:2]
    play(st, minerva)
    assert zone_of(st, minerva) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND
    assert select_options(st) == {first, second}
    select(st, second)
    assert st.zones[0][Zone.DECK][0] == second
    assert zone_of(st, first) is Zone.TRASH


@pytest.mark.card("ST09-010")
@pytest.mark.rule("13-2-5-1")
def test_st09_010_burst_deploys_and_skips_look_on_opponent_turn() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    minerva, second = sc.shields(1, MINERVA, VANILLA)
    st = sc.start()
    deck = list(st.zones[1][Zone.DECK])
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, minerva) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND
    assert st.zones[1][Zone.DECK] == deck
    assert st.zones[1][Zone.TRASH] == []


# ---------------------------------------------------------------------------------------------
# ST10


@pytest.mark.card("ST10-001")
@pytest.mark.rule("8-5-2-3")
def test_st10_001_destroying_a_shield_sets_active_and_bars_attacking_the_player() -> None:
    sc = Scenario()
    zeta = sc.add(0, ZETA_EX)
    rested_enemy = sc.add(1, VANILLA, rested=True)
    top, _second = sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    attack(st, zeta)
    pass_all(st)
    assert zone_of(st, top) is Zone.TRASH
    assert not st.cards[zeta].rested
    assert attack_options(st, zeta) == {rested_enemy}


@pytest.mark.card("ST10-001")
def test_st10_001_destroying_a_base_with_battle_damage_triggers() -> None:
    sc = Scenario()
    zeta = sc.add(0, ZETA_EX)
    base = sc.base(1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zeta)
    pass_all(st)
    assert zone_of(st, base) is not Zone.BASE
    assert not st.cards[zeta].rested
    assert PLAYER_TARGET not in attack_options(st, zeta)


@pytest.mark.card("ST10-001")
def test_st10_001_destroying_a_unit_does_not_trigger() -> None:
    sc = Scenario()
    zeta = sc.add(0, ZETA_EX)
    enemy = sc.add(1, VANILLA, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, zeta, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[zeta].rested


def _zeta_deploy(
    g_gen_in_trash: int, *enemies: tuple[str, int]
) -> tuple[GameState, list[int], list[int]]:
    sc = Scenario()
    sc.resources(0, 5)
    zeta = sc.add(0, ZETA, Zone.HAND)
    fuel = sc.trash(0, *([NEMO] * g_gen_in_trash))
    targets = [sc.add(1, number, damage=damage) for number, damage in enemies]
    st = sc.start()
    play(st, zeta)
    return st, fuel, targets


@pytest.mark.card("ST10-002")
@pytest.mark.rule("13-1-8-1", "13-1-8-2")
@pytest.mark.faq("Q96")
def test_st10_002_development_rests_enemy_with_4_or_less_current_hp() -> None:
    st, fuel, (damaged, healthy) = _zeta_deploy(2, (BARBATOS_4TH, 1), (BARBATOS_4TH, 0))
    assert st.pending is not None and st.pending.kind is DecisionKind.YES_NO
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in fuel)
    assert st.cards[damaged].rested  # 5 HP with 1 damage = 4 current HP (FAQ Q96)
    assert not st.cards[healthy].rested


@pytest.mark.card("ST10-002")
@pytest.mark.ruling("ST10-002:Q303")
def test_st10_002_development_may_exile_without_a_target() -> None:
    st, fuel, (healthy,) = _zeta_deploy(2, (BARBATOS_4TH, 0))
    yes(st)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in fuel)
    assert not st.cards[healthy].rested


@pytest.mark.card("ST10-002")
def test_st10_002_development_optional_and_needs_two_g_generation_cards() -> None:
    st, fuel, (target,) = _zeta_deploy(2, (VANILLA, 0))
    no(st)
    assert all(zone_of(st, u) is Zone.TRASH for u in fuel)
    assert not st.cards[target].rested

    st, fuel, (target,) = _zeta_deploy(1, (VANILLA, 0))
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert zone_of(st, fuel[0]) is Zone.TRASH
    assert not st.cards[target].rested


@pytest.mark.card("ST10-004", "ST11-005")
@pytest.mark.rule("13-1-1-1")
@pytest.mark.parametrize("number", [SUPER_GUNDAM, ASH])
def test_repair_2_recovers_two_at_end_of_turn(number: str) -> None:
    sc = Scenario()
    unit = sc.add(0, number, damage=3)
    st = sc.start()
    assert keywords(st, unit) == {"Repair": 2}
    to_next_turn(st)
    assert st.cards[unit].damage == 1


@pytest.mark.card("ST10-006")
@pytest.mark.rule("13-2-10-1")
@pytest.mark.faq("Q96")
def test_st10_006_during_pair_battle_destruction_returns_low_hp_enemy() -> None:
    sc = Scenario()
    phoenix = sc.add(0, PHOENIX, pilot=MARK_GUILDER)
    victim = sc.add(1, VANILLA, rested=True)
    damaged = sc.add(1, BLOCKER_3_4, rested=True, damage=1)
    healthy = sc.add(1, BLOCKER_3_4, rested=True)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, phoenix, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, damaged) is Zone.HAND  # 4 HP with 1 damage = 3 current HP
    assert zone_of(st, healthy) is Zone.BATTLE


@pytest.mark.card("ST10-006")
def test_st10_006_unpaired_does_nothing() -> None:
    sc = Scenario()
    phoenix = sc.add(0, PHOENIX)
    victim = sc.add(1, VANILLA, rested=True)
    damaged = sc.add(1, VANILLA, rested=True, damage=1)
    sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, phoenix, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, damaged) is Zone.BATTLE


@pytest.mark.card("ST10-006")
def test_st10_006_not_on_opponent_turn() -> None:
    sc = Scenario(active=1)
    phoenix = sc.add(0, PHOENIX, pilot=MARK_GUILDER, rested=True)
    attacker = sc.add(1, VANILLA)
    damaged = sc.add(1, VANILLA, rested=True, damage=1)
    st = sc.start()
    attack(st, attacker, phoenix)
    pass_all(st)
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, damaged) is Zone.BATTLE


def _barbatos_4th_link(*trash: str) -> tuple[GameState, list[int]]:
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, BARBATOS_4TH)
    cards = sc.trash(0, *trash)
    pilot = sc.add(0, MARK_GUILDER, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=barbatos)
    resolve_trigger_order(st)
    return st, cards


@pytest.mark.card("ST10-007")
@pytest.mark.rule("13-1-8-1", "13-2-11-1")
def test_st10_007_when_linked_development_adds_command_lv4_or_lower() -> None:
    st, (nemo1, nemo2, training, twinkle) = _barbatos_4th_link(
        NEMO, NEMO, TACTICAL_TRAINING, TWINKLE
    )
    yes(st)
    assert zone_of(st, nemo1) is Zone.REMOVAL and zone_of(st, nemo2) is Zone.REMOVAL
    assert zone_of(st, training) is Zone.HAND
    assert zone_of(st, twinkle) is Zone.TRASH  # Lv.5


@pytest.mark.card("ST10-007")
@pytest.mark.ruling("ST10-007:Q304")
def test_st10_007_development_may_exile_without_a_command() -> None:
    st, (nemo1, nemo2, twinkle) = _barbatos_4th_link(NEMO, NEMO, TWINKLE)
    yes(st)
    assert zone_of(st, nemo1) is Zone.REMOVAL and zone_of(st, nemo2) is Zone.REMOVAL
    assert zone_of(st, twinkle) is Zone.TRASH


@pytest.mark.card("ST10-007")
def test_st10_007_paired_without_link_does_not_trigger() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, BARBATOS_4TH)
    nemo1, nemo2, training = sc.trash(0, NEMO, NEMO, TACTICAL_TRAINING)
    pilot = sc.add(0, CHAR, Zone.HAND)
    st = sc.start()
    play(st, pilot, onto=barbatos)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert all(zone_of(st, u) is Zone.TRASH for u in (nemo1, nemo2, training))


@pytest.mark.card("ST10-008")
@pytest.mark.rule("13-1-8-1", "5-20-2")
def test_st10_008_development_draws_one_then_discards_one() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, BARBATOS_1ST, Zone.HAND)
    keep = sc.add(0, VANILLA, Zone.HAND)
    nemo1, nemo2 = sc.trash(0, NEMO, NEMO)
    st = sc.start()
    deck_before = len(st.zones[0][Zone.DECK])
    top = st.zones[0][Zone.DECK][0]
    play(st, barbatos)
    yes(st)
    assert zone_of(st, nemo1) is Zone.REMOVAL and zone_of(st, nemo2) is Zone.REMOVAL
    assert len(st.zones[0][Zone.DECK]) == deck_before - 1
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    select(st, top)
    assert zone_of(st, top) is Zone.TRASH
    assert st.zones[0][Zone.HAND] == [keep]


@pytest.mark.card("ST10-008")
def test_st10_008_declining_development_draws_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    barbatos = sc.add(0, BARBATOS_1ST, Zone.HAND)
    sc.trash(0, NEMO, NEMO)
    st = sc.start()
    deck_before = len(st.zones[0][Zone.DECK])
    play(st, barbatos)
    no(st)
    assert len(st.zones[0][Zone.DECK]) == deck_before
    assert st.zones[0][Zone.HAND] == []


def _kamille_link(*, enemy_filler_rested: bool) -> tuple[GameState, int, int, int, int]:
    sc = Scenario()
    sc.resources(0, 4)
    zeta = sc.add(0, ZETA)
    sc.add(0, VANILLA, rested=True)
    filler = sc.add(1, VANILLA, rested=enemy_filler_rested)
    same_lv = sc.add(1, FORCE_IMPULSE)
    higher = sc.add(1, SAVIOUR)
    kamille = sc.add(0, KAMILLE, Zone.HAND)
    st = sc.start()
    play(st, kamille, onto=zeta)
    return st, zeta, filler, same_lv, higher


@pytest.mark.card("ST10-011")
@pytest.mark.ruling("ST10-011:Q307", "ST10-011:Q308")
@pytest.mark.rule("13-2-11-1", "3-3-9-2")
def test_st10_011_when_linked_rests_enemy_up_to_paired_unit_lv() -> None:
    st, zeta, filler, same_lv, higher = _kamille_link(enemy_filler_rested=True)
    assert V.is_linked(V.derived(st), zeta)
    assert select_options(st) == {filler, same_lv}  # Q308: Lv.6 is above Zeta Gundam's Lv.5
    select(st, same_lv)
    assert st.cards[same_lv].rested  # Q307: one friendly and one enemy rested Unit suffice
    assert not st.cards[higher].rested


@pytest.mark.card("ST10-011")
def test_st10_011_needs_two_rested_units() -> None:
    st, _zeta, filler, same_lv, higher = _kamille_link(enemy_filler_rested=False)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert not any(st.cards[u].rested for u in (filler, same_lv, higher))


@pytest.mark.card("ST10-012")
@pytest.mark.rule("13-2-9-1")
def test_st10_012_when_paired_gives_enemy_ap_minus_2_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    nemo = sc.add(0, NEMO)
    target = sc.add(1, FORCE_IMPULSE)
    sc.add(1, SAVIOUR)
    guilder = sc.add(0, MARK_GUILDER, Zone.HAND)
    st = sc.start()
    play(st, guilder, onto=nemo)
    assert ap(st, target) == 3
    to_next_turn(st)
    assert ap(st, target) == 5


@pytest.mark.card("ST10-013")
@pytest.mark.rule("5-6-1")
def test_st10_013_recovers_2_and_gives_ap_plus_2() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    training = sc.add(0, TACTICAL_TRAINING, Zone.HAND)
    zeta = sc.add(0, ZETA, damage=3)
    sc.add(0, NEMO)
    st = sc.start()
    play(st, training)
    assert st.cards[zeta].damage == 1
    assert ap(st, zeta) == 6
    to_next_turn(st)
    assert ap(st, zeta) == 4


@pytest.mark.card("ST10-013")
@pytest.mark.ruling("ST10-013:Q309")
def test_st10_013_may_choose_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    training = sc.add(0, TACTICAL_TRAINING, Zone.HAND)
    mine = sc.add(0, ZETA)
    theirs = sc.add(1, ZETA, damage=2)
    st = sc.start()
    play(st, training)
    assert select_options(st) == {mine, theirs}
    select(st, theirs)
    assert st.cards[theirs].damage == 0 and ap(st, theirs) == 6
    assert ap(st, mine) == 4


@pytest.mark.card("ST10-013")
@pytest.mark.rule("10-1-8-1-1", "13-2-5-1")
def test_st10_013_needs_target_and_burst_adds_to_hand() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    training = sc.add(0, TACTICAL_TRAINING, Zone.HAND)
    sc.add(0, NEMO)
    sc.add(1, FORCE_IMPULSE)
    attacker = sc.add(0, VANILLA)
    shield = sc.shields(1, TACTICAL_TRAINING)[0]
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, training)
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, shield) is Zone.HAND


def _play_command_options(st: GameState, uid: int) -> list[int]:
    return [
        o.c
        for o in (st.pending.options if st.pending else ())
        if o.kind is A.PLAY_COMMAND and o.a == uid
    ]


@pytest.mark.card("ST10-014")
@pytest.mark.rule("2-9-1", "2-10-1")
def test_st10_014_discarding_g_generation_unit_plays_it_for_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unlocking = sc.add(0, UNLOCKING, Zone.HAND)
    nemo = sc.add(0, NEMO, Zone.HAND)
    other = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    assert _play_command_options(st, unlocking) == [0, ALT_PLAY_BASE]
    act(st, A.PLAY_COMMAND, unlocking, None, ALT_PLAY_BASE)
    assert zone_of(st, nemo) is Zone.TRASH
    assert zone_of(st, other) is Zone.HAND
    assert rested_resources(st, 0) == 2
    assert len(st.zones[0][Zone.HAND]) == 3
    assert zone_of(st, unlocking) is Zone.TRASH


@pytest.mark.card("ST10-014")
def test_st10_014_normal_play_costs_4_and_no_alt_without_g_generation_unit() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unlocking = sc.add(0, UNLOCKING, Zone.HAND)
    sc.add(0, SHINN, Zone.HAND)
    st = sc.start()
    assert _play_command_options(st, unlocking) == [0]
    play(st, unlocking)
    assert rested_resources(st, 0) == 4
    assert len(st.zones[0][Zone.HAND]) == 3


@pytest.mark.card("ST10-014")
@pytest.mark.rule("2-9-1")
def test_st10_014_alt_play_offered_with_only_two_resources() -> None:
    sc = Scenario()
    sc.resources(0, 2)
    unlocking = sc.add(0, UNLOCKING, Zone.HAND)
    nemo = sc.add(0, NEMO, Zone.HAND)
    st = sc.start()
    act(st, A.PLAY_COMMAND, unlocking, None, ALT_PLAY_BASE)
    assert zone_of(st, nemo) is Zone.TRASH
    assert rested_resources(st, 0) == 2


def _diffuse_battle(attacker_number: str) -> tuple[GameState, int, int, int]:
    sc = Scenario()
    sc.resources(0, 3)
    attacker = sc.add(0, attacker_number)
    target = sc.add(1, BLOCKER_3_4, rested=True)
    cannon = sc.add(0, DIFFUSE_BEAM, Zone.HAND)
    st = sc.start()
    attack(st, attacker, target)
    play(st, cannon)
    return st, attacker, target, cannon


@pytest.mark.card("ST10-015")
@pytest.mark.rule("8-4-1", "8-6-1")
def test_st10_015_enemy_gets_ap_minus_3_during_this_battle() -> None:
    st, nemo, target, cannon = _diffuse_battle(NEMO)
    pass_all(st)
    assert st.cards[nemo].damage == 0 and zone_of(st, nemo) is Zone.BATTLE
    assert st.cards[target].damage == 2
    assert zone_of(st, cannon) is Zone.TRASH
    assert ap(st, target) == 3


@pytest.mark.card("ST10-015")
def test_st10_015_no_friendly_g_generation_unit_does_nothing() -> None:
    st, zaku, target, _cannon = _diffuse_battle(VANILLA)
    pass_all(st)
    assert zone_of(st, zaku) is Zone.TRASH
    assert st.cards[target].damage == 2


@pytest.mark.card("ST10-015")
@pytest.mark.rule("10-1-8-1-1")
@pytest.mark.faq("Q100")
def test_st10_015_not_playable_with_g_generation_unit_but_no_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, NEMO)
    cannon = sc.add(0, DIFFUSE_BEAM, Zone.HAND)
    st = sc.start()
    act(st, A.END_MAIN)
    assert not has_action(st, A.PLAY_COMMAND, cannon)


@pytest.mark.card("ST10-015")
@pytest.mark.rule("3-4-6-2")
def test_st10_015_pairs_as_claire_heathrow_pilot() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    nemo = sc.add(0, NEMO)
    cannon = sc.add(0, DIFFUSE_BEAM, Zone.HAND)
    st = sc.start()
    play(st, cannon, onto=nemo)
    assert zone_of(st, cannon) is Zone.PAIRED
    assert ap(st, nemo) == 3


@pytest.mark.card("ST10-015")
@pytest.mark.rule("8-6-1", "7-6-6-1")
def test_st10_015_played_outside_battle_does_not_persist_past_the_turn() -> None:
    sc = Scenario()
    mine = sc.add(0, FORCE_IMPULSE)
    sc.resources(1, 3)
    sc.add(1, NEMO)
    cannon = sc.add(1, DIFFUSE_BEAM, Zone.HAND)
    st = sc.start()
    act(st, A.END_MAIN)
    play(st, cannon)
    assert zone_of(st, cannon) is Zone.TRASH
    to_next_turn(st)
    to_next_turn(st)
    assert ap(st, mine) == 5


@pytest.mark.card("ST10-016")
@pytest.mark.rule("5-20-2", "5-6-1")
def test_st10_016_deploy_adds_shield_then_g_generation_units_recover_1() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    luna = sc.add(0, LUNA_MANA, Zone.HAND)
    nemo = sc.add(0, NEMO, damage=1)
    zeta = sc.add(0, ZETA, damage=2)
    zaku = sc.add(0, VANILLA, damage=1)
    enemy = sc.add(1, NEMO, damage=1)
    shield = sc.shields(0, VANILLA)[0]
    st = sc.start()
    play(st, luna)
    assert zone_of(st, luna) is Zone.BASE
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[nemo].damage == 0 and st.cards[zeta].damage == 1
    assert st.cards[zaku].damage == 1 and st.cards[enemy].damage == 1


@pytest.mark.card("ST10-016", "ST11-016")
@pytest.mark.rule("13-2-5-1")
@pytest.mark.parametrize("number", [LUNA_MANA, MAD_ANGLER])
def test_base_burst_deploys_and_adds_shield(number: str) -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    base, second = sc.shields(1, number, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, base) is Zone.BASE
    assert zone_of(st, second) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# ST11


def _zgok_under_attack(*, paired: bool, other_marines: int) -> tuple[GameState, int, int]:
    sc = Scenario(active=1)
    zgok = sc.add(0, ZGOK, rested=True, pilot=PILOT_PLAIN if paired else None)
    for _ in range(other_marines):
        sc.add(0, DAUGHSEAT)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    return st, zgok, attacker


@pytest.mark.card("ST11-001")
@pytest.mark.rule("13-2-10-1", "10-1-5-3")
def test_st11_001_paired_with_two_other_marines_cannot_be_attacked() -> None:
    st, _zgok, attacker = _zgok_under_attack(paired=True, other_marines=2)
    assert attack_options(st, attacker) == {PLAYER_TARGET}


@pytest.mark.card("ST11-001")
@pytest.mark.parametrize(("paired", "others"), [(True, 1), (False, 2)])
def test_st11_001_attackable_without_pair_or_two_other_marines(paired: bool, others: int) -> None:
    st, zgok, attacker = _zgok_under_attack(paired=paired, other_marines=others)
    assert attack_options(st, attacker) == {PLAYER_TARGET, zgok}


@pytest.mark.card("ST11-001")
@pytest.mark.ruling("ST11-001:Q427")
@pytest.mark.rule("3-3-6", "13-2-6-1")
def test_st11_001_deploy_returns_lv2_enemy_and_its_pilot_to_deck_bottom() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zgok = sc.add(0, ZGOK, Zone.HAND)
    sc.add(0, DAUGHSEAT)
    small = sc.add(1, VANILLA, pilot=PILOT_PLAIN)
    big = sc.add(1, BLOCKER_3_4)
    st = sc.start()
    pilot = st.cards[small].pair
    play(st, zgok)
    assert set(st.zones[1][Zone.DECK][-2:]) == {small, pilot}
    assert zone_of(st, big) is Zone.BATTLE


@pytest.mark.card("ST11-001")
def test_st11_001_deploy_needs_another_marine() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    zgok = sc.add(0, ZGOK, Zone.HAND)
    sc.add(0, VANILLA)
    small = sc.add(1, VANILLA)
    st = sc.start()
    play(st, zgok)
    assert zone_of(st, small) is Zone.BATTLE


def _mad_angler_vs_acguy(*, acguy_rested: bool) -> tuple[GameState, dict[str, int]]:
    sc = Scenario(active=1)
    units = {
        "acguy": sc.add(0, ACGUY, rested=acguy_rested),
        "daughseat": sc.add(0, DAUGHSEAT, rested=True),
        "leopard_damaged": sc.add(0, LEOPARD, rested=True, damage=1),
        "leopard": sc.add(0, LEOPARD, rested=True),
    }
    sc.resources(1, 3)
    angler = sc.add(1, MAD_ANGLER, Zone.HAND)
    st = sc.start()
    play(st, angler)
    return st, units


@pytest.mark.card("ST11-002", "ST11-016")
@pytest.mark.faq("Q96")
@pytest.mark.parametrize(
    ("target", "damage_after"),
    [("acguy", 0), ("daughseat", 0), ("leopard_damaged", 1), ("leopard", 2)],
)
def test_st11_002_rested_on_opponent_turn_protects_marines_with_2_or_less_hp(
    target: str, damage_after: int
) -> None:
    st, units = _mad_angler_vs_acguy(acguy_rested=True)
    select(st, units[target])
    assert st.cards[units[target]].damage == damage_after
    assert zone_of(st, units[target]) is Zone.BATTLE


@pytest.mark.card("ST11-002")
def test_st11_002_active_acguy_protects_nothing() -> None:
    st, units = _mad_angler_vs_acguy(acguy_rested=False)
    select(st, units["daughseat"])
    assert zone_of(st, units["daughseat"]) is Zone.TRASH


@pytest.mark.card("ST11-002", "ST11-016")
def test_st11_002_no_protection_during_your_own_turn() -> None:
    sc = Scenario()
    sc.add(0, ACGUY, rested=True)
    daughseat = sc.add(0, DAUGHSEAT, rested=True)
    sc.add(0, LEOPARD, rested=True)
    attacker = sc.add(0, VANILLA)
    sc.shields(1, MAD_ANGLER, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    select(st, daughseat)
    assert zone_of(st, daughseat) is Zone.TRASH


@pytest.mark.card("ST11-003")
@pytest.mark.ruling("ST11-003:Q428")
@pytest.mark.rule("8-3-1", "13-1-4-1")
def test_st11_003_block_stands_after_losing_blocker() -> None:
    sc = Scenario(active=1)
    zock = sc.add(0, ZOCK)
    daughseat = sc.add(0, DAUGHSEAT)
    shield = sc.shields(0, VANILLA)[0]
    attacker = sc.add(1, VANILLA)
    sc.resources(1, 4)
    kill = sc.add(1, GIANT_KILLING, Zone.HAND)
    st = sc.start()
    assert keywords(st, zock) == {"Blocker": 1}
    attack(st, attacker)
    block(st, zock)
    play(st, kill)
    assert zone_of(st, daughseat) is Zone.TRASH
    assert keywords(st, zock) == {}
    pass_all(st)
    assert st.cards[zock].damage == 2
    assert zone_of(st, attacker) is Zone.TRASH
    assert zone_of(st, shield) is Zone.SHIELD


@pytest.mark.card("ST11-003")
def test_st11_003_no_blocker_without_another_marine() -> None:
    sc = Scenario(active=1)
    zock = sc.add(0, ZOCK)
    sc.add(0, VANILLA)
    sc.shields(0, VANILLA)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    assert keywords(st, zock) == {}
    attack(st, attacker)
    assert st.pending is not None and st.pending.kind is not DecisionKind.BLOCK


@pytest.mark.card("ST11-004")
@pytest.mark.rule("5-17-2-1", "13-2-6-1")
def test_st11_004_deploys_rested_goohn_token() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zno = sc.add(0, ZNO, Zone.HAND)
    st = sc.start()
    play(st, zno)
    battle = st.zones[0][Zone.BATTLE]
    assert len(battle) == 2
    token = next(u for u in battle if u != zno)
    cd = V.cdef(st, token)
    assert cd.name == "GOOhN" and cd.is_token and cd.traits == ("ZAFT", "Marine")
    assert (ap(st, token), st.cards[token].rested, st.cards[zno].rested) == (1, True, False)


@pytest.mark.card("ST11-004")
@pytest.mark.ruling("ST11-004:Q429")
@pytest.mark.rule("11-4-2")
def test_st11_004_token_with_full_battle_area_trashes_an_existing_unit() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zno = sc.add(0, ZNO, Zone.HAND)
    fillers = [sc.add(0, VANILLA) for _ in range(5)]
    st = sc.start()
    play(st, zno)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    act(st, A.SELECT, fillers[0])
    assert zone_of(st, fillers[0]) is Zone.TRASH
    battle = st.zones[0][Zone.BATTLE]
    assert len(battle) == 6 and zno in battle
    assert numbers_in(st, 0, Zone.BATTLE).count("T-028") == 1


@pytest.mark.card("ST11-006")
@pytest.mark.rule("13-2-6-1")
def test_st11_006_deploy_adds_marine_unit_card_from_trash() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    shamblo = sc.add(0, SHAMBLO, Zone.HAND)
    daughseat, zaku = sc.trash(0, DAUGHSEAT, VANILLA)
    st = sc.start()
    play(st, shamblo)
    assert zone_of(st, daughseat) is Zone.HAND
    assert zone_of(st, zaku) is Zone.TRASH


def _shamblo_opponent_turn(
    *, other_marine: bool, base: bool, fire: bool = False
) -> tuple[GameState, int, list[int], int]:
    sc = Scenario(active=1)
    sc.add(0, SHAMBLO)
    sc.add(0, DAUGHSEAT if other_marine else VANILLA)
    victim = sc.add(0, VANILLA, rested=True)
    shields = sc.shields(0, VANILLA, VANILLA)
    my_base = sc.base(0) if base else -1
    saviour = sc.add(1, SAVIOUR)
    if fire:
        sc.resources(1, 4)
        sc.add(1, SIMULTANEOUS_FIRE, Zone.HAND)
    st = sc.start(Step.START_STEP)
    if fire:
        play(st, st.zones[1][Zone.HAND][0])
        assert keywords(st, saviour)["Breach"] == 6
    attack(st, saviour, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    return st, victim, shields, my_base


@pytest.mark.card("ST11-006")
@pytest.mark.ruling("ST11-006:Q431")
@pytest.mark.rule("13-1-2-1", "7-2-4-1")
def test_st11_006_enemy_breach_is_reduced_on_opponent_turn() -> None:
    st, _victim, shields, _base = _shamblo_opponent_turn(other_marine=True, base=False)
    assert all(zone_of(st, u) is Zone.SHIELD for u in shields)


@pytest.mark.card("ST11-006")
@pytest.mark.ruling("ST11-006:Q431")
def test_st11_006_enemy_breach_on_base_is_reduced() -> None:
    st, _victim, shields, base = _shamblo_opponent_turn(other_marine=True, base=True)
    assert zone_of(st, base) is Zone.BASE and st.cards[base].damage == 0
    assert all(zone_of(st, u) is Zone.SHIELD for u in shields)


@pytest.mark.card("ST11-006")
@pytest.mark.parametrize(("other_marine", "damage"), [(True, 0), (False, 1)])
def test_st11_006_direct_enemy_effect_damage_to_base_is_reduced(
    other_marine: bool, damage: int
) -> None:
    sc = Scenario(active=1)
    sc.add(0, SHAMBLO)
    sc.add(0, DAUGHSEAT if other_marine else VANILLA)
    base = sc.base(0)
    sc.resources(1, 3)
    zssa = sc.add(1, ZSSA, Zone.HAND)
    st = sc.start(Step.START_STEP)
    play(st, zssa)
    assert st.cards[base].damage == damage


@pytest.mark.card("ST11-006")
def test_st11_006_needs_another_marine_at_start_of_opponent_turn() -> None:
    st, _victim, shields, _base = _shamblo_opponent_turn(other_marine=False, base=False)
    assert zone_of(st, shields[0]) is Zone.TRASH
    assert zone_of(st, shields[1]) is Zone.SHIELD


@pytest.mark.card("ST11-006")
@pytest.mark.ruling("ST11-006:Q430")
@pytest.mark.xfail(
    strict=True,
    reason="DSL: no player-level damage reduction for shield area cards; modelled as prevention",
)
def test_st11_006_reduces_six_breach_damage_to_one() -> None:
    st, _victim, _shields, base = _shamblo_opponent_turn(other_marine=True, base=True, fire=True)
    assert st.cards[base].damage == 1


@pytest.mark.card("ST11-009")
@pytest.mark.ruling("ST11-009:Q432")
@pytest.mark.rule("13-2-8-2-1")
def test_st11_009_destroyed_on_opponent_turn_draws_even_as_last_marine() -> None:
    sc = Scenario(active=1)
    kapool = sc.add(0, KAPOOL, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    hand = len(st.zones[0][Zone.HAND])
    attack(st, attacker, kapool)
    pass_all(st)
    assert zone_of(st, kapool) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == hand + 1


@pytest.mark.card("ST11-009")
def test_st11_009_destroyed_on_your_turn_draws_nothing() -> None:
    sc = Scenario()
    kapool = sc.add(0, KAPOOL)
    sc.add(0, DAUGHSEAT)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    hand = len(st.zones[0][Zone.HAND])
    attack(st, kapool, enemy)
    pass_all(st)
    assert zone_of(st, kapool) is Zone.TRASH
    assert len(st.zones[0][Zone.HAND]) == hand


@pytest.mark.card("ST11-011")
@pytest.mark.rule("13-2-9-1")
def test_st11_011_when_paired_marines_get_ap_plus_1_this_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    daughseat = sc.add(0, DAUGHSEAT)
    leopard = sc.add(0, LEOPARD)
    zaku = sc.add(0, VANILLA)
    enemy = sc.add(1, DAUGHSEAT)
    char = sc.add(0, CHAR, Zone.HAND)
    st = sc.start()
    play(st, char, onto=daughseat)
    assert (ap(st, daughseat), ap(st, leopard), ap(st, zaku), ap(st, enemy)) == (5, 5, 2, 2)
    to_next_turn(st)
    assert (ap(st, daughseat), ap(st, leopard)) == (4, 4)


@pytest.mark.card("ST11-012")
@pytest.mark.ruling("ST11-012:Q433")
@pytest.mark.rule("8-5-3-1")
def test_st11_012_battle_damage_from_enemy_unit_reduced_by_2() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    daughseat = sc.add(0, DAUGHSEAT)
    enemy = sc.add(1, SWORD_IMPULSE, rested=True)
    loni = sc.add(0, LONI, Zone.HAND)
    st = sc.start()
    play(st, loni, onto=daughseat)
    attack(st, daughseat, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert zone_of(st, daughseat) is Zone.BATTLE and st.cards[daughseat].damage == 2


@pytest.mark.card("ST11-012")
def test_st11_012_only_the_chosen_marine_is_protected() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    daughseat = sc.add(0, DAUGHSEAT)
    leopard = sc.add(0, LEOPARD)
    enemy = sc.add(1, SWORD_IMPULSE, rested=True)
    loni = sc.add(0, LONI, Zone.HAND)
    st = sc.start()
    play(st, loni, onto=daughseat)
    assert select_options(st) == {daughseat, leopard}
    select(st, leopard)
    attack(st, daughseat, enemy)
    pass_all(st)
    assert zone_of(st, daughseat) is Zone.TRASH


@pytest.mark.card("ST11-013")
@pytest.mark.rule("5-20-1")
@pytest.mark.faq("Q96")
def test_st11_013_returns_rested_enemy_with_3_or_less_current_hp_and_draws() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    offensive = sc.add(0, POORLY_PLANNED, Zone.HAND)
    damaged = sc.add(1, BLOCKER_3_4, rested=True, damage=1)
    healthy = sc.add(1, BLOCKER_3_4, rested=True)
    active = sc.add(1, VANILLA)
    st = sc.start()
    play(st, offensive)
    assert zone_of(st, damaged) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE and zone_of(st, active) is Zone.BATTLE
    assert len(st.zones[0][Zone.HAND]) == 1


@pytest.mark.card("ST11-013")
@pytest.mark.rule("10-1-8-1-1")
def test_st11_013_needs_a_target() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    offensive = sc.add(0, POORLY_PLANNED, Zone.HAND)
    sc.add(1, BLOCKER_3_4, rested=True)
    sc.add(1, VANILLA)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, offensive)


@pytest.mark.card("ST11-013")
@pytest.mark.rule("13-2-5-1")
def test_st11_013_burst_returns_the_rested_attacker() -> None:
    sc = Scenario(active=1)
    offensive = sc.shields(0, POORLY_PLANNED)[0]
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    yes(st)
    assert zone_of(st, attacker) is Zone.HAND
    assert zone_of(st, offensive) is Zone.TRASH


@pytest.mark.card("ST11-014")
@pytest.mark.rule("9-3-1")
def test_st11_014_enemy_units_cannot_attack_chosen_marine_this_turn() -> None:
    sc = Scenario(active=1)
    daughseat = sc.add(0, DAUGHSEAT, rested=True)
    sc.shields(0, VANILLA, VANILLA)
    sc.resources(0, 3)
    orca = sc.add(0, ORCA, Zone.HAND)
    first = sc.add(1, VANILLA)
    second = sc.add(1, VANILLA)
    st = sc.start()
    assert daughseat in attack_options(st, second)
    attack(st, first)
    play(st, orca)
    pass_all(st)
    assert attack_options(st, second) == {PLAYER_TARGET}
    to_next_turn(st)
    assert not V.rules_of(V.derived(st), daughseat, RuleKind.CANT_BE_ATTACKED)


@pytest.mark.card("ST11-014", "ST11-004")
@pytest.mark.rule("3-4-6-2", "3-2-6-2")
def test_st11_014_pairs_as_marco_morassim_and_links_zno() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zno = sc.add(0, ZNO)
    orca = sc.add(0, ORCA, Zone.HAND)
    st = sc.start()
    play(st, orca, onto=zno)
    assert V.is_linked(V.derived(st), zno)
    assert ap(st, zno) == 4


@pytest.mark.card("ST11-015", "ST11-004")
@pytest.mark.ruling("ST11-015:Q434", "ST11-015:Q435")
@pytest.mark.rule("5-8-1")
def test_st11_015_deploys_marine_from_trash_rested_for_free() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    twinkle = sc.add(0, TWINKLE, Zone.HAND)
    zno = sc.trash(0, ZNO)[0]
    st = sc.start()
    play(st, twinkle)
    assert zone_of(st, zno) is Zone.BATTLE and st.cards[zno].rested
    assert rested_resources(st, 0) == 2  # Q434: ZnO's cost 3 is not paid
    assert numbers_in(st, 0, Zone.BATTLE).count("T-028") == 1  # Q435: its 【Deploy】 resolved


@pytest.mark.card("ST11-015")
@pytest.mark.rule("10-1-8-1-1")
def test_st11_015_needs_marine_lv4_or_lower_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    twinkle = sc.add(0, TWINKLE, Zone.HAND)
    sc.trash(0, ASH, VANILLA)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, twinkle)


@pytest.mark.card("ST11-016")
@pytest.mark.rule("5-20-2")
def test_st11_016_deploy_damages_rested_enemy_with_four_enemy_units() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    angler = sc.add(0, MAD_ANGLER, Zone.HAND)
    shield = sc.shields(0, VANILLA)[0]
    rested = sc.add(1, BLOCKER_3_4, rested=True)
    others = [sc.add(1, VANILLA) for _ in range(3)]
    st = sc.start()
    play(st, angler)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[rested].damage == 2
    assert all(st.cards[u].damage == 0 for u in others)


@pytest.mark.card("ST11-016")
def test_st11_016_three_enemy_units_no_damage() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    angler = sc.add(0, MAD_ANGLER, Zone.HAND)
    shield = sc.shields(0, VANILLA)[0]
    rested = sc.add(1, BLOCKER_3_4, rested=True)
    sc.add(1, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    play(st, angler)
    assert zone_of(st, shield) is Zone.HAND
    assert st.cards[rested].damage == 0
