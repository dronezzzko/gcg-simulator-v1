"""Card behaviour and ruling tests for GD06-001..GD06-065 (work package WP-GD06-A)."""

from __future__ import annotations

import pytest

from gcg_sim.engine import view as V
from gcg_sim.engine.game import advance, apply
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import (
    Scenario,
    activate,
    ap,
    attack,
    block,
    has_action,
    keywords,
    options,
    order,
    pass_all,
    play,
    select,
    select_if_asked,
    to_next_turn,
    yes,
    zone_of,
)

A = ActionKind
VANILLA = "GD01-060"  # Zaku Mariner: Lv2, cost 1, AP2 HP2, no effects
BIG = "GD03-047"  # DINN (Commander Type): Lv4, AP3 HP4, no effects
BLOCKER = "GD01-072"  # Launcher Strike Gundam: AP3 HP4 <Blocker>
RIDDHE = "GD01-089"  # Pilot, AP+1 HP+1, no trigger
KOU = "GD06-080"  # (Earth Federation)(Albion Squad) Pilot "Kou Uraki"
AMATE = "GD06-081"  # (Alphacide)(Newtype) Pilot "Amate Yuzuriha (Machu)"
ATHRUN = "GD06-082"  # (Orb) Pilot "Athrun Zala"
CHALLIA = "GD06-083"  # (Zeon)(Newtype) Pilot "Challia Bull (GQ)"
PLE = "GD06-084"  # (Neo Zeon)(Newtype) Pilot "Elpeo Ple"
GYM = "GD06-085"  # (Ghingnham Fleet)(Moonrace) Pilot "Gym Ghingnham"
MASHYMRE = "GD06-086"  # (Neo Zeon) Pilot "Mashymre Cello"
JUDAU = "GD06-088"  # (AEUG)(Gundam Team) Pilot "Judau Ashta"
MCGILLIS = "GD06-089"  # (Gjallarhorn) Pilot "McGillis Fareed"
VIDAR_PILOT = "GD06-090"  # (Gjallarhorn) Pilot, name also [Gaelio Bauduin]
NEO = "GD06-092"  # (Phantom Pain)(Orb) Pilot "Neo Lorrnoke", name also [Mu La Flaga]
GATO = "GD06-093"  # (Zeon)(Delaz Fleet) Pilot "Anavel Gato"
NYAAN = "GD06-094"  # (Zeon)(Alphacide) Pilot "Nyaan"
KELLEY = "GD06-095"  # (Zeon) Pilot "Kelley Layzner"
ORB_UNIT = "GD06-016"  # Dom Trooper: vanilla (Orb) Unit, Lv3 3/3
ALPHACIDE = "GD06-063"  # GFreD: vanilla (Zeon)(Alphacide) Unit, Lv2 2/2
NEO_ZEON = "GD06-028"  # Gaza D: vanilla (Neo Zeon) Unit, Lv2 2/2
KSHATRIYA = "GD01-051"  # vanilla (Neo Zeon) Unit, Lv4 3/4
FLEET = "GD06-031"  # Zssan: vanilla (Ghingnham Fleet) Unit, Lv2 3/2
CORE_TOP = "GD06-044"  # vanilla (AEUG)(Gundam Team) Unit, Lv3 3/3
MK2 = "GD06-040"  # vanilla (AEUG)(Gundam Team) Unit, Lv4 3/4
NEMO = "GD02-080"  # vanilla (AEUG) Unit, Lv2 2/2
DOM_TROPEN = "GD06-059"  # vanilla (Zeon)(Delaz Fleet) Unit, Lv3 3/3
GRAZE = "ST05-009"  # vanilla (Gjallarhorn) Unit, Lv2 2/2
DINN = "GD01-064"  # vanilla (ZAFT) Unit, Lv2 3/2
GOOHN = "GD01-062"  # vanilla (ZAFT) Unit, Lv1 1/2
IRON_FISTED = "GD01-119"  # Command: an enemy Unit Lv.4 or lower gets AP-2 during this turn
EF_BASE = "GD06-122"  # Albion: (Earth Federation) Base
DRAW_TWO = "GD01-100"  # Command: 【Main】Draw 2.
GYUNEI = "GD05-057"  # 【Activate･Main】destroy 1 of your other Units
BECOME_A_SHIELD = "GD05-117"  # Command: deal 1 damage to 1 of your Units and 1 enemy Unit
DARKNESS_FINGER = "GD05-110"  # Command: deal 2 damage to 1 enemy Unit
FAMILIAL_LOVE = "GD06-110"  # Command: 【Action】1 of your Units gets AP+2 during this battle
VETERANS_PRIDE = (
    "GD05-116"  # Command: 【Main】/【Action】destroy 1 enemy Unit that is Lv.2 or lower
)
ELIMINATE_TARGET = "GD03-110"  # Command: destroy 1 Pilot paired with an enemy Unit Lv.5 or lower


def hand_count(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.HAND])


def trash_count(st: GameState, player: int = 0) -> int:
    return len(st.zones[player][Zone.TRASH])


def pending(st: GameState) -> DecisionKind | None:
    return st.pending.kind if st.pending is not None else None


def select_options(st: GameState) -> set[int]:
    return {o.a for o in options(st) if o.kind is A.SELECT}


def order_first(st: GameState, card_number: str) -> None:
    """Resolve the waiting trigger of ``card_number`` first (an ORDER_TRIGGER decision)."""
    def_id = V.reg().db[card_number].def_id
    batch = st.batches[-1]
    for o in options(st):
        if o.kind is A.ORDER and batch[o.a].ability_key[0] == def_id:
            apply(st, o)
            return
    raise AssertionError(f"no waiting trigger of {card_number}")


def play_action(st: GameState, uid: int) -> None:
    for o in options(st):
        if o.kind is A.PLAY_COMMAND and o.a == uid:
            apply(st, o)
            return
    raise AssertionError("command not playable")


def destroy_with_gyunei(st: GameState, gyunei: int, victim: int) -> None:
    activate(st, gyunei)
    select_if_asked(st, victim, done=False)


def settle(st: GameState) -> None:
    """Pass action steps and blocks and resolve waiting triggers in their default order."""
    for _ in range(20):
        pass_all(st)
        if pending(st) is DecisionKind.ORDER_TRIGGER:
            order(st)
        else:
            return


def battle(st: GameState, attacker: int, target: int = PLAYER_TARGET) -> None:
    """Attack and pass every later decision of the battle (no blocks)."""
    attack(st, attacker, target)
    settle(st)


# ---------------------------------------------------------------------------------------------
# GD06-001 Gundam GP01Fb


@pytest.mark.card("GD06-001")
def test_gd06_001_draws_once_per_turn_when_attacked() -> None:
    sc = Scenario(active=1)
    gp01 = sc.add(0, "GD06-001", rested=True)
    a = sc.add(1, VANILLA)
    b = sc.add(1, VANILLA)
    st = sc.start()
    before = hand_count(st)
    battle(st, a, gp01)
    assert hand_count(st) == before + 1
    battle(st, b, gp01)
    assert hand_count(st) == before + 1


@pytest.mark.card("GD06-001")
def test_gd06_001_no_draw_when_the_player_is_attacked() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-001", rested=True)
    a = sc.add(1, VANILLA)
    st = sc.start()
    before = hand_count(st)
    battle(st, a, PLAYER_TARGET)
    assert hand_count(st) == before


@pytest.mark.card("GD06-001")
def test_gd06_001_high_maneuver_only_while_paired() -> None:
    sc = Scenario()
    plain = sc.add(0, "GD06-001")
    paired = sc.add(0, "GD06-001", pilot=RIDDHE)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, plain)
    assert "High-Maneuver" in keywords(st, paired)


# ---------------------------------------------------------------------------------------------
# GD06-002 Infinite Justice Gundam


@pytest.mark.card("GD06-002")
@pytest.mark.ruling("GD06-002:Q489")
def test_gd06_002_heals_an_orb_unit_after_destroying_a_shield_once_per_turn() -> None:
    sc = Scenario()
    justice = sc.add(0, "GD06-002")
    hurt = sc.add(0, ORB_UNIT, damage=2)
    sc.shields(1, VANILLA, VANILLA)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    battle(st, justice, PLAYER_TARGET)
    select_if_asked(st, hurt)
    assert st.cards[hurt].damage == 0
    st.cards[hurt].damage = 2
    st.cards[justice].rested = False  # a second attack this turn
    st.pending = None
    st.touch()
    advance(st)
    battle(st, justice, victim)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[hurt].damage == 2


@pytest.mark.card("GD06-002")
@pytest.mark.ruling("GD06-002:Q489")
def test_gd06_002_heals_after_destroying_an_enemy_unit() -> None:
    sc = Scenario()
    justice = sc.add(0, "GD06-002")
    hurt = sc.add(0, ORB_UNIT, damage=2)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    battle(st, justice, victim)
    select_if_asked(st, hurt)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[hurt].damage == 0


@pytest.mark.card("GD06-002")
def test_gd06_002_deploy_exiles_three_units_to_bounce_an_active_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    justice = sc.add(0, "GD06-002", Zone.HAND)
    trash = sc.trash(0, VANILLA, VANILLA, VANILLA)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, justice)
    select(st, *trash)
    select_if_asked(st, enemy)
    assert all(zone_of(st, u) is Zone.REMOVAL for u in trash)
    assert zone_of(st, enemy) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# GD06-003 GQuuuuuuX (Omega Psycommu Released)


@pytest.mark.card("GD06-003")
def test_gd06_003_ap_grows_with_hand_size() -> None:
    sc = Scenario()
    gq = sc.add(0, "GD06-003")
    sc.hand(0, VANILLA, VANILLA, VANILLA)
    st = sc.start()
    assert ap(st, gq) == 2 + 3


@pytest.mark.card("GD06-003")
def test_gd06_003_attack_draws_with_another_alphacide_and_small_hand() -> None:
    sc = Scenario()
    gq = sc.add(0, "GD06-003")
    sc.add(0, ALPHACIDE)
    st = sc.start()
    before = hand_count(st)
    battle(st, gq)
    assert hand_count(st) == before + 1


@pytest.mark.card("GD06-003")
def test_gd06_003_attack_does_not_draw_with_six_cards_in_hand() -> None:
    sc = Scenario()
    gq = sc.add(0, "GD06-003")
    sc.add(0, ALPHACIDE)
    sc.hand(0, *[VANILLA] * 6)
    st = sc.start()
    battle(st, gq)
    assert hand_count(st) == 6


# ---------------------------------------------------------------------------------------------
# GD06-004 Gundam GP01


@pytest.mark.card("GD06-004")
def test_gd06_004_linked_destroyed_on_your_turn_returns_pilot_then_discards() -> None:
    sc = Scenario()
    gp01 = sc.add(0, "GD06-004", pilot=KOU)
    gyunei = sc.add(0, GYUNEI)
    (spare,) = sc.hand(0, VANILLA)
    st = sc.start()
    pilot = st.cards[gp01].pair
    destroy_with_gyunei(st, gyunei, gp01)
    select(st, spare)
    assert zone_of(st, gp01) is Zone.TRASH
    assert zone_of(st, pilot) is Zone.HAND
    assert zone_of(st, spare) is Zone.TRASH


@pytest.mark.card("GD06-004")
def test_gd06_004_unlinked_pilot_goes_to_trash() -> None:
    sc = Scenario()
    gp01 = sc.add(0, "GD06-004", pilot=RIDDHE)
    gyunei = sc.add(0, GYUNEI)
    st = sc.start()
    pilot = st.cards[gp01].pair
    destroy_with_gyunei(st, gyunei, gp01)
    assert zone_of(st, pilot) is Zone.TRASH
    assert hand_count(st) == 0


# ---------------------------------------------------------------------------------------------
# GD06-005 Akatsuki (Shiranui)


@pytest.mark.card("GD06-005")
def test_gd06_005_attack_may_bounce_a_damaged_low_level_enemy() -> None:
    sc = Scenario()
    akatsuki = sc.add(0, "GD06-005")
    hurt = sc.add(1, VANILLA, damage=1)
    healthy = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, akatsuki)
    assert select_options(st) == {hurt}
    select(st, hurt)
    pass_all(st)
    assert zone_of(st, hurt) is Zone.HAND
    assert zone_of(st, healthy) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD06-006 GQuuuuuuX (Omega Psycommu)


@pytest.mark.card("GD06-006")
def test_gd06_006_deploy_with_another_alphacide_bounces_a_small_hp_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    gq = sc.add(0, "GD06-006", Zone.HAND)
    sc.add(0, ALPHACIDE)
    small = sc.add(1, VANILLA)
    sc.add(1, BIG)
    st = sc.start()
    play(st, gq)
    assert select_options(st) == {small}
    select(st, small)
    assert zone_of(st, small) is Zone.HAND


@pytest.mark.card("GD06-006")
def test_gd06_006_deploy_without_another_alphacide_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    gq = sc.add(0, "GD06-006", Zone.HAND)
    small = sc.add(1, VANILLA)
    st = sc.start()
    play(st, gq)
    assert pending(st) is DecisionKind.MAIN
    assert zone_of(st, small) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD06-007 Kikeroga (MS Mode) (GQ)


@pytest.mark.card("GD06-007")
def test_gd06_007_repair_and_deploy_damage_to_a_rested_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    kikeroga = sc.add(0, "GD06-007", Zone.HAND)
    rested = sc.add(1, BIG, rested=True)
    active = sc.add(1, BIG)
    st = sc.start()
    play(st, kikeroga)
    select_if_asked(st, rested)
    assert st.cards[rested].damage == 2
    assert st.cards[active].damage == 0
    assert keywords(st, kikeroga)["Repair"] == 2


# ---------------------------------------------------------------------------------------------
# GD06-008 Powered GM


@pytest.mark.card("GD06-008")
def test_gd06_008_attack_gains_ap_with_a_federation_base() -> None:
    sc = Scenario()
    gm = sc.add(0, "GD06-008")
    sc.base(0, EF_BASE)
    st = sc.start()
    attack(st, gm)
    assert ap(st, gm) == 4
    pass_all(st)


@pytest.mark.card("GD06-008")
def test_gd06_008_no_bonus_with_an_ex_base() -> None:
    sc = Scenario()
    gm = sc.add(0, "GD06-008")
    sc.base(0)
    st = sc.start()
    attack(st, gm)
    assert ap(st, gm) == 2
    pass_all(st)


# ---------------------------------------------------------------------------------------------
# GD06-009 GM Custom / GD06-010 GM Cannon Ⅱ / GD06-011 Red Gundam


@pytest.mark.card("GD06-009")
def test_gd06_009_deploy_may_bounce_an_enemy_with_1_hp() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gm = sc.add(0, "GD06-009", Zone.HAND)
    weak = sc.add(1, VANILLA, damage=1)
    sc.add(1, VANILLA)
    st = sc.start()
    play(st, gm)
    assert select_options(st) == {weak}
    select(st, weak)
    assert zone_of(st, weak) is Zone.HAND


@pytest.mark.card("GD06-010")
def test_gd06_010_when_linked_draws() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gm = sc.add(0, "GD06-010")
    kou = sc.add(0, KOU, Zone.HAND)
    st = sc.start()
    before = hand_count(st)
    play(st, kou, onto=gm)
    assert hand_count(st) == before - 1 + 1


@pytest.mark.card("GD06-010")
def test_gd06_010_unlinked_pairing_does_not_draw() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    gm = sc.add(0, "GD06-010")
    riddhe = sc.add(0, RIDDHE, Zone.HAND)
    st = sc.start()
    before = hand_count(st)
    play(st, riddhe, onto=gm)
    assert hand_count(st) == before - 1


@pytest.mark.card("GD06-011")
def test_gd06_011_when_linked_all_your_units_gain_repair_1() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    red = sc.add(0, "GD06-011")
    other = sc.add(0, VANILLA)
    amate = sc.add(0, AMATE, Zone.HAND)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    play(st, amate, onto=red)
    assert keywords(st, other).get("Repair") == 1
    assert keywords(st, red).get("Repair", 0) >= 1
    assert "Repair" not in keywords(st, enemy)


# ---------------------------------------------------------------------------------------------
# GD06-014 GM Type C / GD06-015 Waldfeld's Gaia Gundam / GD06-017 GQuuuuuuX


@pytest.mark.card("GD06-014")
def test_gd06_014_blocker_with_three_enemy_units() -> None:
    sc = Scenario()
    gm = sc.add(0, "GD06-014")
    sc.add(1, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    assert "Blocker" not in keywords(st, gm)
    sc.add(1, VANILLA)
    st.touch()
    assert "Blocker" in keywords(st, gm)


@pytest.mark.card("GD06-014", "GD06-015", "GD06-017")
@pytest.mark.ruling("GD06-014:Q490", "GD06-015:Q491", "GD06-017:Q492")
@pytest.mark.parametrize("number", ["GD06-014", "GD06-015", "GD06-017"])
def test_gd06_014_015_017_block_stays_after_losing_blocker(number: str) -> None:
    """In the action step after the block, a Command removes the card that gives the guard
    <Blocker>: an enemy Unit (GD06-014), its (Orb) Pilot (GD06-015) or the other (Alphacide)
    Unit (GD06-017). The block stands."""
    sc = Scenario(active=1)
    sc.shields(0, VANILLA)
    if number == "GD06-014":
        guard = sc.add(0, number)
        sc.resources(0, 2)
        removal = sc.add(0, VETERANS_PRIDE, Zone.HAND)
        attacker = sc.add(1, BIG)
        enabler = sc.add(1, VANILLA)
        sc.add(1, VANILLA)
        remover = 0
    elif number == "GD06-015":
        guard = sc.add(0, number, pilot=ATHRUN)
        sc.resources(1, 6)
        removal = sc.add(1, ELIMINATE_TARGET, Zone.HAND)
        attacker = sc.add(1, BIG)
        enabler = -1
        remover = 1
    else:
        guard = sc.add(0, number)
        enabler = sc.add(0, ALPHACIDE)
        sc.resources(1, 2)
        removal = sc.add(1, VETERANS_PRIDE, Zone.HAND)
        attacker = sc.add(1, BIG)
        remover = 1
    st = sc.start()
    if number == "GD06-015":
        enabler = st.cards[guard].pair
    assert "Blocker" in keywords(st, guard)
    attack(st, attacker, PLAYER_TARGET)
    block(st, guard)
    pass_until(st, remover)
    play_action(st, removal)
    select_if_asked(st, enabler)
    assert zone_of(st, enabler) is Zone.TRASH
    assert "Blocker" not in keywords(st, guard)
    settle(st)
    assert len(st.zones[0][Zone.SHIELD]) == 1
    if number == "GD06-014":
        assert zone_of(st, guard) is Zone.TRASH  # 2/1 against 3 AP
    else:
        assert st.cards[guard].damage == 3


@pytest.mark.card("GD06-015")
def test_gd06_015_blocker_only_with_an_orb_pilot() -> None:
    sc = Scenario()
    orb = sc.add(0, "GD06-015", pilot=ATHRUN)
    other = sc.add(0, "GD06-015", pilot=RIDDHE)
    bare = sc.add(0, "GD06-015")
    st = sc.start()
    assert "Blocker" in keywords(st, orb)
    assert "Blocker" not in keywords(st, other)
    assert "Blocker" not in keywords(st, bare)


@pytest.mark.card("GD06-017")
def test_gd06_017_blocker_only_with_another_alphacide() -> None:
    sc = Scenario()
    gq = sc.add(0, "GD06-017")
    st = sc.start()
    assert "Blocker" not in keywords(st, gq)
    sc.add(0, ALPHACIDE)
    st.touch()
    assert "Blocker" in keywords(st, gq)


# ---------------------------------------------------------------------------------------------
# helpers for the green to white sections

STELLAR = "GD05-090"  # (Phantom Pain)(Biological CPU) Pilot, Lv3
CARTA = (
    "GD02-073"  # Lv4 5/4: the enemy Unit battling it gains <First Strike> on the opponent's turn
)
AWAKENED_POWER = (
    "GD02-110"  # Command: pay the cost of a Lv.5 or lower Unit card in trash to deploy it
)
BRIDGE_CREW = "GD03-105"  # Command: 1 friendly Unit may attack active unpaired enemy Units
MIDAIR = "GD01-121"  # Command: set 1 rested Unit with <Blocker> active
SCHUZRUM = "GD04-040"  # vanilla (Neo Zeon) Unit, Lv4 4/3
ENDRA = "GD06-123"  # (Neo Zeon) Base
LAGOWE = "GD01-050"  # (ZAFT) Unit, link trait (ZAFT)
LUNAMARIA = "GD04-095"  # (ZAFT) Pilot
CORE_BASE = "GD06-045"


def pass_until(st: GameState, player: int) -> None:
    """Pass action-step priority until ``player`` holds it."""
    for _ in range(10):
        if pending(st) is DecisionKind.ACTION_STEP and st.pending is not None:
            if st.pending.player == player:
                return
            act_pass(st)
        else:
            return


def act_pass(st: GameState) -> None:
    for o in options(st):
        if o.kind is A.PASS:
            apply(st, o)
            return
    raise AssertionError("no pass")


# ---------------------------------------------------------------------------------------------
# GD06-018 Elpeo Ple's Qubeley Mk-Ⅱ


@pytest.mark.card("GD06-018")
@pytest.mark.ruling("GD06-018:Q493")
def test_gd06_018_battle_damage_is_dealt_to_the_chosen_neo_zeon_unit() -> None:
    sc = Scenario()
    qubeley = sc.add(0, "GD06-018", pilot=PLE)
    shield = sc.add(0, KSHATRIYA)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, qubeley, enemy)
    pass_until(st, 0)
    activate(st, qubeley)
    select_if_asked(st, shield)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[qubeley].damage == 0
    assert st.cards[shield].damage == 3
    assert any(h.kind == "destroys_by_battle" for h in st.history)


@pytest.mark.card("GD06-018")
def test_gd06_018_battle_damage_can_go_to_a_neo_zeon_base() -> None:
    sc = Scenario()
    qubeley = sc.add(0, "GD06-018", pilot=PLE)
    base = sc.base(0, ENDRA)
    enemy = sc.add(1, BIG, rested=True)
    st = sc.start()
    attack(st, qubeley, enemy)
    pass_until(st, 0)
    activate(st, qubeley)
    select_if_asked(st, base)
    pass_all(st)
    assert st.cards[qubeley].damage == 0
    assert st.cards[base].damage == 3


@pytest.mark.card("GD06-018")
@pytest.mark.ruling("GD06-018:Q494")
def test_gd06_018_two_mutual_redirections_leave_the_damage_on_the_original_unit() -> None:
    sc = Scenario()
    a = sc.add(0, "GD06-018", pilot=PLE)
    b = sc.add(0, "GD06-018", pilot=PLE)
    enemy = sc.add(1, GOOHN, rested=True)
    st = sc.start()
    attack(st, a, enemy)
    pass_until(st, 0)
    activate(st, a)
    select(st, b)
    pass_until(st, 0)
    activate(st, b)
    select(st, a)
    pass_all(st)
    assert st.cards[a].damage == 1
    assert st.cards[b].damage == 0


@pytest.mark.card("GD06-018")
def test_gd06_018_needs_a_pilot_and_is_once_per_turn() -> None:
    sc = Scenario()
    bare = sc.add(0, "GD06-018")
    paired = sc.add(0, "GD06-018", pilot=PLE)
    sc.add(0, KSHATRIYA)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, paired, enemy)
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, bare)
    activate(st, paired)
    select_if_asked(st, bare)
    pass_until(st, 0)
    assert not has_action(st, A.ACTIVATE, paired)
    pass_all(st)
    assert keywords(st, paired)["Breach"] == 1


# ---------------------------------------------------------------------------------------------
# GD06-019 Turn X


@pytest.mark.card("GD06-019")
def test_gd06_019_enemy_units_must_attack_a_rested_unit_token() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-019")
    token = sc.add(0, "T-031", rested=True)
    sc.add(0, VANILLA, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    targets = {o.b for o in options(st) if o.kind is A.ATTACK and o.a == attacker}
    assert targets == {token}


@pytest.mark.card("GD06-019")
@pytest.mark.ruling("GD06-019:Q495")
def test_gd06_019_with_two_attractors_the_attacker_chooses_one() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-019")
    arms = sc.add(0, "T-031", rested=True)
    legs = sc.add(0, "T-032", rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    targets = {o.b for o in options(st) if o.kind is A.ATTACK and o.a == attacker}
    assert targets == {arms, legs}


@pytest.mark.card("GD06-019")
def test_gd06_019_active_tokens_do_not_attract() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-019")
    sc.add(0, "T-031")
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    assert has_action(st, A.ATTACK, attacker, PLAYER_TARGET)


@pytest.mark.card("GD06-019")
def test_gd06_019_attack_deploys_arms_and_legs_without_fleet_tokens() -> None:
    sc = Scenario()
    turn_x = sc.add(0, "GD06-019")
    st = sc.start()
    battle(st, turn_x)
    names = sorted(V.cdef(st, u).name for u in st.zones[0][Zone.BATTLE] if u != turn_x)
    assert names == ["Turn X (Arms)", "Turn X (Legs)"]
    assert all(st.cards[u].rested for u in st.zones[0][Zone.BATTLE])


@pytest.mark.card("GD06-019")
def test_gd06_019_no_tokens_while_a_fleet_token_is_in_play() -> None:
    sc = Scenario()
    turn_x = sc.add(0, "GD06-019")
    sc.add(0, "T-031")
    st = sc.start()
    battle(st, turn_x)
    assert len(st.zones[0][Zone.BATTLE]) == 2


# ---------------------------------------------------------------------------------------------
# GD06-020 Hamma-Hamma


@pytest.mark.card("GD06-020")
@pytest.mark.ruling("GD06-020:Q496")
def test_gd06_020_recovers_when_a_neo_zeon_unit_destroys_a_unit_or_a_shield() -> None:
    sc = Scenario()
    hamma = sc.add(0, "GD06-020", damage=3)
    a = sc.add(0, SCHUZRUM)
    b = sc.add(0, SCHUZRUM)
    sc.shields(1, VANILLA)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    battle(st, a, victim)
    assert st.cards[hamma].damage == 2
    battle(st, b, PLAYER_TARGET)
    assert st.cards[hamma].damage == 1


@pytest.mark.card("GD06-020")
@pytest.mark.ruling("GD06-020:Q497")
def test_gd06_020_recovers_when_breach_effect_damage_destroys_a_shield() -> None:
    sc = Scenario()
    hamma = sc.add(0, "GD06-020", damage=3)
    qubeley = sc.add(0, "GD06-018", pilot=PLE)
    sc.shields(1, VANILLA)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    battle(st, qubeley, victim)
    assert zone_of(st, victim) is Zone.TRASH
    assert len(st.zones[1][Zone.SHIELD]) == 0
    assert st.cards[hamma].damage == 1


@pytest.mark.card("GD06-020")
def test_gd06_020_not_on_the_opponents_turn() -> None:
    sc = Scenario(active=1)
    hamma = sc.add(0, "GD06-020", damage=2)
    kshatriya = sc.add(0, KSHATRIYA, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    battle(st, attacker, kshatriya)
    assert zone_of(st, attacker) is Zone.TRASH
    assert st.cards[hamma].damage == 2


# ---------------------------------------------------------------------------------------------
# GD06-021 Turn X (Top)


@pytest.mark.card("GD06-021")
@pytest.mark.ruling("GD06-021:Q498")
def test_gd06_021_deploys_a_fresh_rested_token_of_the_same_kind() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    top = sc.add(0, "GD06-021", Zone.HAND)
    token = sc.add(0, "T-031", damage=2)
    st = sc.start()
    play(st, top)
    select_if_asked(st, token)
    copies = [u for u in st.zones[0][Zone.BATTLE] if V.cdef(st, u).card_number == "T-031"]
    assert len(copies) == 2
    fresh = next(u for u in copies if u != token)
    assert st.cards[fresh].rested and st.cards[fresh].damage == 0


@pytest.mark.card("GD06-021")
def test_gd06_021_without_a_token_nothing_is_deployed() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    top = sc.add(0, "GD06-021", Zone.HAND)
    st = sc.start()
    play(st, top)
    assert st.zones[0][Zone.BATTLE] == [top]


# ---------------------------------------------------------------------------------------------
# GD06-022 Merrybell's Bandit


@pytest.mark.card("GD06-022")
@pytest.mark.ruling("GD06-022:Q499", "GD06-022:Q501")
def test_gd06_022_when_linked_fights_a_small_enemy_with_only_the_damage_step() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    bandit = sc.add(0, "GD06-022")
    gym = sc.add(0, GYM, Zone.HAND)
    victim = sc.add(1, VANILLA)
    sc.add(1, BIG)
    st = sc.start()
    play(st, gym, onto=bandit)
    select_if_asked(st, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[bandit].damage == 2
    assert not st.cards[bandit].rested
    assert any(h.kind == "destroys_by_battle" and h.uid == bandit for h in st.history)
    assert pending(st) is DecisionKind.MAIN


@pytest.mark.card("GD06-022")
@pytest.mark.ruling("GD06-022:Q500")
def test_gd06_022_battle_effects_apply_in_the_effect_battle() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    bandit = sc.add(0, "GD06-022")
    gym = sc.add(0, GYM, Zone.HAND)
    iron = sc.add(0, IRON_FISTED, Zone.HAND)
    carta = sc.add(1, CARTA)
    st = sc.start()
    play(st, iron)
    select_if_asked(st, carta)
    assert ap(st, carta) == 3
    play(st, gym, onto=bandit)
    select_if_asked(st, carta)
    settle(st)
    assert zone_of(st, carta) is Zone.TRASH
    assert st.cards[bandit].damage == 0  # it gained <First Strike> from the Graze Ritter


# ---------------------------------------------------------------------------------------------
# GD06-023 Galluss J / GD06-024 R-Jarja


@pytest.mark.card("GD06-023")
def test_gd06_023_boosting_a_link_unit_sets_itself_active() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    galluss = sc.add(0, "GD06-023", Zone.HAND)
    linked = sc.add(0, "GD06-020", pilot=MASHYMRE)
    st = sc.start()
    play(st, galluss)
    select_if_asked(st, linked)
    assert ap(st, linked) == 3 + 0 + 1
    assert not st.cards[galluss].rested


@pytest.mark.card("GD06-023")
def test_gd06_023_boosting_a_non_link_unit_stays_rested() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    galluss = sc.add(0, "GD06-023", Zone.HAND)
    plain = sc.add(0, KSHATRIYA)
    st = sc.start()
    play(st, galluss)
    select_if_asked(st, plain)
    assert ap(st, plain) == 4
    assert st.cards[galluss].rested


@pytest.mark.card("GD06-024")
def test_gd06_024_destroyed_destroys_a_small_enemy_blocker() -> None:
    sc = Scenario()
    jarja = sc.add(0, "GD06-024")
    gyunei = sc.add(0, GYUNEI)
    blocker = sc.add(1, BLOCKER)
    plain = sc.add(1, VANILLA)
    st = sc.start()
    destroy_with_gyunei(st, gyunei, jarja)
    select_if_asked(st, blocker)
    assert zone_of(st, blocker) is Zone.TRASH
    assert zone_of(st, plain) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD06-026 Stero's Mahiroo / GD06-027 Mashymre's Zssa / GD06-029 / GD06-030 Mahiroo


@pytest.mark.card("GD06-026")
def test_gd06_026_attack_with_three_other_fleet_units_boosts_a_unit() -> None:
    sc = Scenario()
    stero = sc.add(0, "GD06-026")
    fleet = [sc.add(0, FLEET) for _ in range(3)]
    st = sc.start()
    attack(st, stero)
    select_if_asked(st, fleet[0])
    assert ap(st, fleet[0]) == 4
    assert keywords(st, fleet[0])["Repair"] == 2
    pass_all(st)


@pytest.mark.card("GD06-026")
def test_gd06_026_attack_with_two_other_fleet_units_does_nothing() -> None:
    sc = Scenario()
    stero = sc.add(0, "GD06-026")
    fleet = [sc.add(0, FLEET) for _ in range(2)]
    st = sc.start()
    attack(st, stero)
    pass_all(st)
    assert ap(st, fleet[0]) == 3


@pytest.mark.card("GD06-027")
def test_gd06_027_deploy_lets_a_neo_zeon_unit_attack_an_active_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    zssa = sc.add(0, "GD06-027", Zone.HAND)
    kshatriya = sc.add(0, KSHATRIYA)
    small = sc.add(1, VANILLA)
    big = sc.add(1, "GD04-040")
    st = sc.start()
    assert not has_action(st, A.ATTACK, kshatriya, small)
    play(st, zssa)
    select_if_asked(st, kshatriya)
    assert has_action(st, A.ATTACK, kshatriya, small)
    assert has_action(st, A.ATTACK, kshatriya, big)


@pytest.mark.card("GD06-029")
def test_gd06_029_has_breach_3() -> None:
    sc = Scenario()
    q = sc.add(0, "GD06-029")
    st = sc.start()
    assert keywords(st, q)["Breach"] == 3


@pytest.mark.card("GD06-029")
def test_gd06_029_breach_3_destroys_the_first_shield_after_a_battle_kill() -> None:
    sc = Scenario()
    q = sc.add(0, "GD06-029")
    first, second = sc.shields(1, VANILLA, VANILLA)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, q, victim)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, first) is Zone.TRASH  # <Breach 3> on the first card of the shield area
    assert zone_of(st, second) is Zone.SHIELD


@pytest.mark.card("GD06-030")
def test_gd06_030_ap_plus_2_with_three_other_fleet_units() -> None:
    sc = Scenario()
    mahiroo = sc.add(0, "GD06-030")
    sc.add(0, FLEET)
    sc.add(0, FLEET)
    st = sc.start()
    assert ap(st, mahiroo) == 2
    sc.add(0, "T-031")
    st.touch()
    assert ap(st, mahiroo) == 4


# ---------------------------------------------------------------------------------------------
# GD06-032 Bellona


@pytest.mark.card("GD06-032")
@pytest.mark.ruling("GD06-032:Q502")
def test_gd06_032_destroyed_looks_at_two_and_may_add_a_fleet_unit() -> None:
    sc = Scenario()
    bellona = sc.add(0, "GD06-032")
    gyunei = sc.add(0, GYUNEI)
    sc.deck(0, FLEET, VANILLA)
    st = sc.start()
    assert ap(st, bellona) == 0
    deck = list(st.zones[0][Zone.DECK])
    fleet, other = deck[0], deck[1]
    destroy_with_gyunei(st, gyunei, bellona)
    yes(st)
    select_if_asked(st, fleet)
    assert zone_of(st, fleet) is Zone.HAND
    assert st.zones[0][Zone.DECK][-1] == other


# ---------------------------------------------------------------------------------------------
# GD06-033 ZZ Gundam


@pytest.mark.card("GD06-033")
@pytest.mark.ruling("GD06-033:Q504")
def test_gd06_033_deploy_deals_three_to_every_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 9)
    zz = sc.add(0, "GD06-033", Zone.HAND)
    big = [sc.add(1, BIG), sc.add(1, BIG)]
    mine = sc.add(0, BIG)
    st = sc.start()
    play(st, zz)
    assert [st.cards[u].damage for u in big] == [3, 3]
    assert st.cards[mine].damage == 0


@pytest.mark.card("GD06-033")
@pytest.mark.ruling("GD06-033:Q503")
def test_gd06_033_aeug_battle_damage_to_enemy_units_is_increased() -> None:
    sc = Scenario()
    sc.add(0, "GD06-033")
    nemo = sc.add(0, NEMO)
    other = sc.add(0, VANILLA)
    a = sc.add(1, BIG, rested=True)
    b = sc.add(1, BIG, rested=True)
    sc.shields(1, VANILLA, VANILLA)
    st = sc.start()
    battle(st, nemo, a)
    assert st.cards[a].damage == 3
    assert zone_of(st, nemo) is Zone.TRASH
    battle(st, other, b)
    assert st.cards[b].damage == 2


@pytest.mark.card("GD06-033")
def test_gd06_033_enemy_damage_to_your_units_is_not_increased() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-033")
    mk2 = sc.add(0, MK2, rested=True)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    battle(st, attacker, mk2)
    assert zone_of(st, attacker) is Zone.TRASH  # 3 + 1 from the (AEUG) Unit
    assert zone_of(st, mk2) is Zone.BATTLE
    assert st.cards[mk2].damage == 2  # the enemy's 2, not increased


# ---------------------------------------------------------------------------------------------
# GD06-034 Gundam Bael


@pytest.mark.card("GD06-034")
@pytest.mark.ruling("GD06-034:Q505", "GD06-034:Q506", "GD06-034:Q507")
def test_gd06_034_deploy_puts_a_gjallarhorn_unit_into_play_for_free() -> None:
    sc = Scenario()
    res = sc.resources(0, 7)
    bael = sc.add(0, "GD06-034", Zone.HAND)
    sc.trash(0, GRAZE, GRAZE, GRAZE, GRAZE)
    sc.deck(0, "GD06-048", VANILLA, VANILLA, VANILLA)
    victim = sc.add(1, VANILLA)
    st = sc.start()
    helmwige = st.zones[0][Zone.DECK][0]
    play(st, bael)
    yes(st)
    select_if_asked(st, helmwige)
    select_if_asked(st, victim)
    assert zone_of(st, helmwige) is Zone.BATTLE
    assert sum(st.cards[r].rested for r in res) == 6
    assert st.cards[victim].damage == 1  # the deployed Unit's 【Deploy】 resolved
    assert "High-Maneuver" in keywords(st, bael)


@pytest.mark.card("GD06-034")
def test_gd06_034_deploy_needs_four_gjallarhorn_cards_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    bael = sc.add(0, "GD06-034", Zone.HAND)
    sc.trash(0, GRAZE, GRAZE, GRAZE)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    play(st, bael)
    assert pending(st) is DecisionKind.MAIN
    assert st.zones[0][Zone.DECK][0] == top


# ---------------------------------------------------------------------------------------------
# GD06-035 Zeta Gundam / GD06-036 Gundam Vidar


@pytest.mark.card("GD06-035")
@pytest.mark.ruling("GD06-035:Q508")
def test_gd06_035_trigger_fires_even_if_both_units_are_destroyed() -> None:
    sc = Scenario()
    zeta = sc.add(0, "GD06-035")
    victim = sc.add(1, SCHUZRUM, rested=True)
    small = sc.add(1, VANILLA)
    st = sc.start()
    battle(st, zeta, victim)
    select_if_asked(st, small)
    assert zone_of(st, zeta) is Zone.TRASH
    assert zone_of(st, victim) is Zone.TRASH
    assert zone_of(st, small) is Zone.TRASH


@pytest.mark.card("GD06-036")
def test_gd06_036_when_linked_deals_1_damage() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    vidar = sc.add(0, "GD06-036")
    pilot = sc.add(0, VIDAR_PILOT, Zone.HAND)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, pilot, onto=vidar)
    select_if_asked(st, enemy)
    assert st.cards[enemy].damage == 1


# ---------------------------------------------------------------------------------------------
# GD06-037 Reginlaze Julia


@pytest.mark.card("GD06-037")
@pytest.mark.ruling("GD06-037:Q509")
def test_gd06_037_first_strike_while_battling_a_damaged_enemy() -> None:
    sc = Scenario()
    sc.hand(0, FAMILIAL_LOVE)
    sc.resources(0, 2)
    julia = sc.add(0, "GD06-037")
    hurt = sc.add(1, BIG, rested=True, damage=1)
    st = sc.start()
    assert "First Strike" not in keywords(st, julia)
    attack(st, julia, hurt)
    pass_until(st, 0)
    assert "First Strike" in keywords(st, julia)
    st.cards[hurt].damage = 0  # the battled Unit recovers
    st.touch()
    assert "First Strike" not in keywords(st, julia)
    pass_all(st)


@pytest.mark.card("GD06-037")
def test_gd06_037_enemy_effects_cannot_set_enemy_units_active_while_paired() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-037", pilot=RIDDHE)
    sc.resources(1, 3)
    midair = sc.add(1, MIDAIR, Zone.HAND)
    blocker = sc.add(1, BLOCKER, rested=True)
    st = sc.start()
    play(st, midair)
    select_if_asked(st, blocker)
    assert st.cards[blocker].rested


@pytest.mark.card("GD06-037")
def test_gd06_037_start_phase_still_sets_enemy_units_active() -> None:
    sc = Scenario()
    sc.add(0, "GD06-037", pilot=RIDDHE)
    enemy = sc.add(1, BLOCKER, rested=True)
    st = sc.start()
    to_next_turn(st)
    assert not st.cards[enemy].rested


@pytest.mark.card("GD06-037")
def test_gd06_037_unpaired_does_not_lock_enemy_units() -> None:
    sc = Scenario(active=1)
    sc.add(0, "GD06-037")
    sc.resources(1, 3)
    midair = sc.add(1, MIDAIR, Zone.HAND)
    blocker = sc.add(1, BLOCKER, rested=True)
    st = sc.start()
    play(st, midair)
    select_if_asked(st, blocker)
    assert not st.cards[blocker].rested


# ---------------------------------------------------------------------------------------------
# GD06-038 Gundam Kimaris Vidar / GD06-039 Neo Core Fighter


@pytest.mark.card("GD06-038")
@pytest.mark.ruling("GD06-038:Q510")
def test_gd06_038_when_paired_damages_one_of_the_highest_level_enemies() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    kimaris = sc.add(0, "GD06-038")
    riddhe = sc.add(0, RIDDHE, Zone.HAND)
    low = sc.add(1, VANILLA)
    high_a = sc.add(1, BIG)
    high_b = sc.add(1, BIG)
    st = sc.start()
    play(st, riddhe, onto=kimaris)
    assert select_options(st) == {high_a, high_b}
    select(st, high_b)
    assert st.cards[high_b].damage == 3
    assert st.cards[low].damage == 0
    assert keywords(st, kimaris)["Breach"] == 3


@pytest.mark.card("GD06-039")
@pytest.mark.ruling("GD06-039:Q511", "GD06-039:Q512")
def test_gd06_039_combines_into_a_lv8_gundam_team_unit_from_hand() -> None:
    sc = Scenario()
    res = sc.resources(0, 2)
    fighter = sc.add(0, "GD06-039")
    top = sc.add(0, CORE_TOP)
    base = sc.add(0, CORE_BASE)
    zz = sc.add(0, "GD06-033", Zone.HAND)
    enemy = sc.add(1, BIG)
    other = sc.add(1, BIG)
    st = sc.start()
    activate(st, fighter)
    select_if_asked(st, top)
    select_if_asked(st, base)
    assert all(zone_of(st, u) is Zone.TRASH for u in (fighter, top, base))
    assert zone_of(st, zz) is Zone.BATTLE
    assert not any(st.cards[r].rested for r in res)
    order_first(st, "GD06-033")  # ZZ's 【Deploy】, then Core Base's 【Destroyed】
    assert st.cards[enemy].damage == st.cards[other].damage == 2 + 1
    select(st, enemy)
    assert zone_of(st, enemy) is Zone.TRASH


@pytest.mark.card("GD06-039")
def test_gd06_039_needs_two_different_card_names() -> None:
    sc = Scenario()
    fighter = sc.add(0, "GD06-039")
    sc.add(0, CORE_TOP)
    sc.add(0, CORE_TOP)
    sc.add(0, "GD06-033", Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.ACTIVATE, fighter)


# ---------------------------------------------------------------------------------------------
# GD06-041 / GD06-042 Destroy Gundam / GD06-043 Julieta's Reginlaze


@pytest.mark.card("GD06-041")
def test_gd06_041_deploy_deals_1_damage_to_an_enemy_with_5_or_less_ap() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    windam = sc.add(0, "GD06-041", Zone.HAND)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, windam)
    select_if_asked(st, enemy)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD06-042")
@pytest.mark.ruling("GD06-042:Q513", "GD06-042:Q514")
def test_gd06_042_deploy_pairs_a_biological_cpu_pilot_from_the_top_four() -> None:
    sc = Scenario()
    res = sc.resources(0, 6)
    destroy = sc.add(0, "GD06-042", Zone.HAND)
    sc.deck(0, VANILLA, STELLAR, VANILLA, VANILLA, BIG)
    st = sc.start()
    stellar = st.zones[0][Zone.DECK][1]
    fifth = st.zones[0][Zone.DECK][4]
    play(st, destroy)
    yes(st)
    select_if_asked(st, stellar)
    assert st.cards[destroy].pair == stellar
    assert sum(st.cards[r].rested for r in res) == 6
    assert st.zones[0][Zone.DECK][0] == fifth


@pytest.mark.card("GD06-043")
def test_gd06_043_linked_boosts_other_gjallarhorn_units_on_your_turn() -> None:
    sc = Scenario()
    julieta = sc.add(0, "GD06-043", pilot=MCGILLIS)
    graze = sc.add(0, GRAZE)
    enemy_graze = sc.add(1, GRAZE)
    st = sc.start()
    assert ap(st, graze) == 3
    assert ap(st, julieta) == 4 + 1
    del enemy_graze
    to_next_turn(st)
    assert ap(st, graze) == 2


# ---------------------------------------------------------------------------------------------
# GD06-045 Core Base / GD06-046 Hyaku-Shiki / GD06-048 Helmwige Reincar


@pytest.mark.card("GD06-045")
def test_gd06_045_destroyed_deals_1_damage_to_a_lv5_or_lower_enemy() -> None:
    sc = Scenario()
    core_base = sc.add(0, CORE_BASE)
    gyunei = sc.add(0, GYUNEI)
    enemy = sc.add(1, BIG)
    st = sc.start()
    destroy_with_gyunei(st, gyunei, core_base)
    select_if_asked(st, enemy)
    assert st.cards[enemy].damage == 1


@pytest.mark.card("GD06-046")
def test_gd06_046_attack_gives_another_gundam_team_unit_ap_plus_2() -> None:
    sc = Scenario()
    hyaku = sc.add(0, "GD06-046")
    mk2 = sc.add(0, MK2)
    st = sc.start()
    attack(st, hyaku)
    select_if_asked(st, mk2)
    assert ap(st, mk2) == 5
    pass_all(st)


@pytest.mark.card("GD06-048")
def test_gd06_048_deploy_targets_lv2_or_lower_without_gjallarhorn_trash() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    helmwige = sc.add(0, "GD06-048", Zone.HAND)
    small = sc.add(1, VANILLA)
    big = sc.add(1, BIG)
    st = sc.start()
    play(st, helmwige)
    select_if_asked(st, small)
    assert st.cards[small].damage == 1
    assert st.cards[big].damage == 0


@pytest.mark.card("GD06-048")
def test_gd06_048_deploy_targets_any_enemy_with_four_gjallarhorn_cards() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    helmwige = sc.add(0, "GD06-048", Zone.HAND)
    sc.trash(0, GRAZE, GRAZE, GRAZE, GRAZE)
    sc.add(1, VANILLA)
    big = sc.add(1, BIG)
    st = sc.start()
    play(st, helmwige)
    select(st, big)
    assert st.cards[big].damage == 1


# ---------------------------------------------------------------------------------------------
# GD06-049 Gundam GP02A


@pytest.mark.card("GD06-049")
def test_gd06_049_when_paired_with_ten_zeon_units_in_trash_destroys_the_enemy_base() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gp02 = sc.add(0, "GD06-049")
    gato = sc.add(0, GATO, Zone.HAND)
    sc.trash(0, *[VANILLA] * 10)
    base = sc.base(1)
    st = sc.start()
    play(st, gato, onto=gp02)
    assert zone_of(st, base) is not Zone.BASE


@pytest.mark.card("GD06-049")
def test_gd06_049_when_paired_with_nine_zeon_units_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    gp02 = sc.add(0, "GD06-049")
    gato = sc.add(0, GATO, Zone.HAND)
    sc.trash(0, *[VANILLA] * 9)
    base = sc.base(1)
    st = sc.start()
    play(st, gato, onto=gp02)
    assert zone_of(st, base) is Zone.BASE


@pytest.mark.card("GD06-049")
@pytest.mark.ruling("GD06-049:Q515")
def test_gd06_049_destroyed_adds_a_delaz_unit_and_trashes_the_rest() -> None:
    sc = Scenario()
    gp02 = sc.add(0, "GD06-049")
    gyunei = sc.add(0, GYUNEI)
    sc.deck(0, VANILLA, DOM_TROPEN, BIG, VANILLA)
    st = sc.start()
    top3 = list(st.zones[0][Zone.DECK][:3])
    fourth = st.zones[0][Zone.DECK][3]
    destroy_with_gyunei(st, gyunei, gp02)
    yes(st)
    select_if_asked(st, top3[1])
    assert zone_of(st, top3[1]) is Zone.HAND
    assert zone_of(st, top3[0]) is Zone.TRASH and zone_of(st, top3[2]) is Zone.TRASH
    assert st.zones[0][Zone.DECK][0] == fourth


# ---------------------------------------------------------------------------------------------
# GD06-050 Legend Gundam


@pytest.mark.card("GD06-050")
def test_gd06_050_in_trash_gets_lv_minus_3_with_a_zaft_link_unit() -> None:
    sc = Scenario()
    res = sc.resources(0, 6)
    sc.add(0, LAGOWE, pilot=LUNAMARIA)
    legend = sc.add(0, "GD06-050", Zone.TRASH)
    awakened = sc.add(0, AWAKENED_POWER, Zone.HAND)
    small = [sc.add(1, VANILLA), sc.add(1, VANILLA)]
    st = sc.start()
    play(st, awakened)
    select_if_asked(st, legend)
    select(st, *sorted(small))
    assert zone_of(st, legend) is Zone.BATTLE
    assert sum(st.cards[r].rested for r in res) == 2 + 2
    assert all(zone_of(st, u) is Zone.TRASH for u in small)


@pytest.mark.card("GD06-050")
def test_gd06_050_without_a_zaft_link_unit_stays_lv6_in_trash() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    sc.add(0, LAGOWE)
    sc.add(0, "GD06-050", Zone.TRASH)
    awakened = sc.add(0, AWAKENED_POWER, Zone.HAND)
    st = sc.start()
    assert not has_action(st, A.PLAY_COMMAND, awakened)


@pytest.mark.card("GD06-050")
def test_gd06_050_deploy_from_hand_destroys_one_lv3_or_lower_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    legend = sc.add(0, "GD06-050", Zone.HAND)
    a = sc.add(1, VANILLA)
    b = sc.add(1, VANILLA)
    st = sc.start()
    play(st, legend)
    select(st, a)
    assert zone_of(st, a) is Zone.TRASH
    assert zone_of(st, b) is Zone.BATTLE


# ---------------------------------------------------------------------------------------------
# GD06-051 GFreD


@pytest.mark.card("GD06-051")
@pytest.mark.ruling("GD06-051:Q516", "GD06-051:Q517")
def test_gd06_051_activate_deploys_an_alphacide_unit_from_trash_for_free() -> None:
    sc = Scenario()
    res = sc.resources(0, 3)
    gfred = sc.add(0, "GD06-051")
    gq = sc.add(0, "GD06-006", Zone.TRASH)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    activate(st, gfred)
    select_if_asked(st, gq)
    select_if_asked(st, enemy)
    assert zone_of(st, gq) is Zone.BATTLE
    assert st.cards[gfred].rested
    assert sum(st.cards[r].rested for r in res) == 2
    assert zone_of(st, enemy) is Zone.HAND
    assert not has_action(st, A.ACTIVATE, gfred)


@pytest.mark.card("GD06-051")
@pytest.mark.ruling("GD06-051:Q518", "GD06-051:Q519")
def test_gd06_051_end_of_turn_effect_chooses_any_two_resources() -> None:
    sc = Scenario()
    normal = sc.resources(0, 6)
    ex = sc.resources(0, 0, ex=1)
    gfred = sc.add(0, "GD06-051", Zone.HAND)
    sc.add(0, ALPHACIDE)
    st = sc.start()
    play(st, gfred)
    rested = [r for r in normal if st.cards[r].rested]
    assert len(rested) == 5
    from gcg_sim.testkit import end_main

    end_main(st)
    pass_all(st)
    assert pending(st) is DecisionKind.SELECT
    assert {*rested, *ex} <= select_options(st)
    select(st, rested[0], ex[0])  # an active EX Resource may be chosen
    assert not st.cards[rested[0]].rested
    assert all(st.cards[r].rested for r in rested[1:])


@pytest.mark.card("GD06-051")
def test_gd06_051_deploy_alone_grants_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 6)
    gfred = sc.add(0, "GD06-051", Zone.HAND)
    st = sc.start()
    play(st, gfred)
    from gcg_sim.testkit import end_main

    end_main(st)
    pass_all(st)
    assert pending(st) is not DecisionKind.SELECT


# ---------------------------------------------------------------------------------------------
# GD06-052 Val Walo / GD06-053 Xavier's Gyan Hakuji-Packs (GQ)


@pytest.mark.card("GD06-052")
def test_gd06_052_high_maneuver_with_five_zeon_units_in_trash() -> None:
    sc = Scenario()
    walo = sc.add(0, "GD06-052")
    sc.trash(0, *[VANILLA] * 4)
    st = sc.start()
    assert "High-Maneuver" not in keywords(st, walo)
    sc.add(0, VANILLA, Zone.TRASH)
    st.touch()
    assert "High-Maneuver" in keywords(st, walo)


@pytest.mark.card("GD06-052")
def test_gd06_052_deploy_mills_a_zeon_card_to_rest_an_enemy() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    walo = sc.add(0, "GD06-052", Zone.HAND)
    sc.deck(0, VANILLA)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, walo)
    select_if_asked(st, enemy)
    assert st.cards[enemy].rested


@pytest.mark.card("GD06-052")
def test_gd06_052_deploy_milling_a_non_zeon_card_does_nothing() -> None:
    sc = Scenario()
    sc.resources(0, 5)
    walo = sc.add(0, "GD06-052", Zone.HAND)
    sc.deck(0, BIG)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, walo)
    assert not st.cards[enemy].rested


@pytest.mark.card("GD06-053")
@pytest.mark.ruling("GD06-053:Q520", "GD06-053:Q521")
def test_gd06_053_destroyed_sets_any_resource_active_ex_resources_included() -> None:
    sc = Scenario()
    normal = sc.resources(0, 2, rested=1)
    ex = sc.resources(0, 0, ex=1)
    gyan = sc.add(0, "GD06-053")
    gyunei = sc.add(0, GYUNEI)
    st = sc.start()
    st.cards[ex[0]].rested = True
    st.touch()
    destroy_with_gyunei(st, gyunei, gyan)
    assert {*normal, *ex} <= select_options(st)
    select(st, ex[0])
    assert not st.cards[ex[0]].rested


# ---------------------------------------------------------------------------------------------
# GD06-054 GFreD / GD06-055 Gundam GP02A / GD06-056 Xamel / GD06-057


@pytest.mark.card("GD06-054")
def test_gd06_054_deploy_may_damage_a_friendly_and_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    gfred = sc.add(0, "GD06-054", Zone.HAND)
    mine = sc.add(0, BIG)
    enemy = sc.add(1, BIG)
    st = sc.start()
    play(st, gfred)
    yes(st)
    select(st, mine)
    select_if_asked(st, enemy)
    assert st.cards[mine].damage == 1
    assert st.cards[enemy].damage == 1
    assert keywords(st, gfred).get("Suppression") is not None


@pytest.mark.card("GD06-054")
@pytest.mark.ruling("GD06-054:Q522")
def test_gd06_054_does_not_activate_without_an_enemy_unit() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    gfred = sc.add(0, "GD06-054", Zone.HAND)
    mine = sc.add(0, BIG)
    st = sc.start()
    play(st, gfred)
    assert pending(st) is DecisionKind.MAIN
    assert st.cards[mine].damage == 0


@pytest.mark.card("GD06-055")
def test_gd06_055_paired_attack_may_take_1_damage_for_breach_2() -> None:
    sc = Scenario()
    gp02 = sc.add(0, "GD06-055", pilot=RIDDHE)
    base = sc.base(1)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, gp02, victim)
    yes(st)
    settle(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.cards[gp02].damage == 1 + 2  # its own effect, then the battle
    assert st.cards[base].damage == 2  # <Breach 2>
    assert "Breach" not in keywords(st, gp02)  # only during that battle


@pytest.mark.card("GD06-055")
def test_gd06_055_unpaired_attack_has_no_option() -> None:
    sc = Scenario()
    gp02 = sc.add(0, "GD06-055")
    st = sc.start()
    attack(st, gp02)
    assert pending(st) is not DecisionKind.YES_NO
    pass_all(st)


@pytest.mark.card("GD06-056")
def test_gd06_056_attack_may_pay_2_to_destroy_a_small_enemy() -> None:
    sc = Scenario()
    res = sc.resources(0, 2)
    xamel = sc.add(0, "GD06-056")
    small = sc.add(1, VANILLA)
    st = sc.start()
    attack(st, xamel)
    yes(st)
    select_if_asked(st, small)
    pass_all(st)
    assert zone_of(st, small) is Zone.TRASH
    assert all(st.cards[r].rested for r in res)


@pytest.mark.card("GD06-057")
@pytest.mark.ruling("GD06-057:Q523")
def test_gd06_057_blocker_with_another_delaz_unit_and_block_survives_its_loss() -> None:
    sc = Scenario(active=1)
    sc.shields(0, VANILLA)
    rick_dom = sc.add(0, "GD06-057")
    partner = sc.add(0, "GD06-060")  # Zaku Ⅱ F2: a Lv.2 (Delaz Fleet) Unit
    sc.resources(1, 2)
    removal = sc.add(1, VETERANS_PRIDE, Zone.HAND)
    attacker = sc.add(1, VANILLA)
    st = sc.start()
    assert "Blocker" in keywords(st, rick_dom)
    attack(st, attacker, PLAYER_TARGET)
    block(st, rick_dom)
    pass_until(st, 1)
    play_action(st, removal)
    select_if_asked(st, partner)
    assert zone_of(st, partner) is Zone.TRASH
    assert "Blocker" not in keywords(st, rick_dom)
    settle(st)
    assert len(st.zones[0][Zone.SHIELD]) == 1
    assert zone_of(st, attacker) is Zone.TRASH


# ---------------------------------------------------------------------------------------------
# GD06-058 Blast Impulse Gundam / GD06-060 / GD06-061 / GD06-064


@pytest.mark.card("GD06-058")
def test_gd06_058_attacks_rested_units_on_the_deploy_turn() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    impulse = sc.add(0, "GD06-058", Zone.HAND)
    rested = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, impulse)
    assert has_action(st, A.ATTACK, impulse, rested)
    assert not has_action(st, A.ATTACK, impulse, PLAYER_TARGET)


@pytest.mark.card("GD06-058")
@pytest.mark.ruling("GD06-058:Q524")
def test_gd06_058_bridge_crew_does_not_extend_the_deploy_turn_attack() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    impulse = sc.add(0, "GD06-058", Zone.HAND)
    crew = sc.add(0, BRIDGE_CREW, Zone.HAND)
    active = sc.add(1, VANILLA)
    st = sc.start()
    play(st, impulse)
    play(st, crew)
    select_if_asked(st, impulse)
    assert not has_action(st, A.ATTACK, impulse, active)


@pytest.mark.card("GD06-060")
def test_gd06_060_ap_plus_1_with_four_zeon_units_in_trash() -> None:
    sc = Scenario()
    zaku = sc.add(0, "GD06-060")
    sc.trash(0, *[VANILLA] * 3)
    st = sc.start()
    assert ap(st, zaku) == 2
    sc.add(0, VANILLA, Zone.TRASH)
    st.touch()
    assert ap(st, zaku) == 3


@pytest.mark.card("GD06-061")
def test_gd06_061_support_1() -> None:
    sc = Scenario()
    dra_c = sc.add(0, "GD06-061")
    other = sc.add(0, VANILLA)
    st = sc.start()
    from gcg_sim.engine.game import SUPPORT_AID

    activate(st, dra_c, SUPPORT_AID)
    select_if_asked(st, other)
    assert ap(st, other) == 3
    assert st.cards[dra_c].rested


@pytest.mark.card("GD06-064")
def test_gd06_064_blocker_and_ap_plus_3_against_a_lv5_enemy() -> None:
    sc = Scenario()
    altron = sc.add(0, "GD06-064")
    st = sc.start()
    assert ap(st, altron) == 3
    assert "Blocker" in keywords(st, altron)
    sc.add(1, "GD06-005")
    st.touch()
    assert ap(st, altron) == 6


# ---------------------------------------------------------------------------------------------
# GD06-065 SUMO Gold Type


@pytest.mark.card("GD06-065")
@pytest.mark.ruling("GD06-065:Q525")
def test_gd06_065_sets_blocker_units_active_and_they_cannot_attack() -> None:
    sc = Scenario()
    res = sc.resources(0, 4)
    gold = sc.add(0, "GD06-065", rested=True)
    cannon = sc.add(0, "GD06-077", rested=True)
    plain = sc.add(0, "GD06-074", rested=True)
    st = sc.start()
    activate(st, gold)
    assert not st.cards[gold].rested and not st.cards[cannon].rested
    assert st.cards[plain].rested
    assert all(st.cards[r].rested for r in res)
    assert not has_action(st, A.ATTACK, gold)
    assert not has_action(st, A.ATTACK, cannon)
    assert not has_action(st, A.ACTIVATE, gold)
