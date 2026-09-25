"""Rules tests: game overview and winning/losing (1-1, 1-2), fundamental game rules (1-3),
preparing to play (6-1-2, 6-2) and game progression (7): phase and step order, the draw,
resource, main and end phases."""

from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import ExitStack
from typing import Any, NamedTuple

import pytest

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import core, game, interp
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, IllegalActionError, apply, legal_actions, new_game
from gcg_sim.engine.observe import is_hidden_from
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import (
    NO_ARG,
    PLAYER_TARGET,
    ActionKind,
    DecisionKind,
    EndReason,
    Phase,
    Step,
    Zone,
)
from gcg_sim.rng import SplitMix64
from gcg_sim.testkit import (
    Scenario,
    act,
    ap,
    attack,
    block,
    end_main,
    has_action,
    hp,
    keywords,
    legal_kinds,
    options,
    order,
    pass_,
    pass_all,
    play,
    select,
    to_next_turn,
    zone_of,
)
from gcg_sim.testkit import card_text as override_card_text

A = ActionKind

VANILLA = "GD01-060"  # Zaku Mariner: Lv.2 cost 1, 2/2, no text
VANILLA_LV3 = "GD01-018"  # ReZEL: Lv.3 cost 2, 4/3, no text
VANILLA_3_AP = "GD01-051"  # Kshatriya: Lv.4 cost 2, 3/4, no text
HOST = "GD01-062"  # GOOhN: vanilla Unit that receives test-only text
HOST_PLAYABLE = "GD01-064"  # DINN: vanilla Lv.2 cost 2 Unit that receives test-only text
ZERO_AP = "EB01-052"  # Hildolfr: 0/2
BLOCKER = "GD01-072"  # Launcher Strike Gundam: <Blocker>, 3/4
BREACH = "GD05-045"  # Chaos Gundam (MA Mode): <Breach 3>, 3/4
REPAIR = "GD01-017"  # Stark Jegan: <Repair 1>, 3/3
REPAIR_TRIGGER = "EB01-004"  # <Repair 2>; when this Unit recovers HP, choose 1 rested enemy Unit
DEPLOY_CHOICE = "EB01-006"  # 【Deploy】Choose 1 of your Units. It gains <Repair 1> ...
ACTIVATE_MAIN = "GD01-053"  # 【Activate･Main】【Once per Turn】①: deal 1 damage to an enemy Unit
BASE = "GD01-126"  # Underground Desert Base: Lv.2 cost 1
PILOT = "GD01-089"  # Riddhe Marcenas: Lv.3 cost 1
DRAW_2_DISCARD_1 = "GD01-118"  # 【Main】Draw 2. Then, discard 1.
ACTION_ONLY = (
    "GD01-114"  # 【Action】Choose 2 friendly Units (needs 2, Q121). They get AP+1 during this turn.
)
DRAW_TWO = "GD01-100"  # A Show of Resolve: 【Main】Draw 2.
ALL_AP_PLUS_2 = "GD01-105"  # 【Main】All your Units get AP+2 during this turn.
AP_MINUS_3 = "ST01-014"  # 【Main】/【Action】Choose 1 enemy Unit. It gets AP-3 during this turn.
AP_PLUS_3 = "EB01-083"  # 【Action】If it is your opponent's turn, choose 1 Unit. It gets AP+3.
DISCARD_THEN_DRAW = "GD05-111"  # 【Main】Discard 1. If you do, draw 2.
DAMAGE_PER_TOKEN = "GD03-107"  # deal damage equal to the number of friendly Unit tokens
DRAW_ON_EFFECT_DAMAGE = "GD02-010"  # when this Unit receives enemy effect damage, draw 1
UNIT_TOKEN = "T-001"
PATULIA = "GD03-041"  # 【Deploy】Deal 3 damage to all Bases.
ARGAMA = "GD02-129"  # This Base can't receive enemy effect damage.
GUAIZ = "GD03-038"  # rested by an effect during your turn: a (ZAFT) Unit gets AP+2
REST_ENEMY = "GD03-104"  # 【Main】/【Action】Choose 1 enemy Unit with 3 or less HP. Rest it.
TAURUS = "GD05-074"  # 3/1, 【Destroyed】Draw 1. Then, discard 1.
REST_1_TO_2 = "GD01-099"  # 【Main】/【Action】Choose 1 to 2 enemy Units ... Rest them.
DUO_LEO = "GD01-042"  # may choose an active enemy Unit that is Lv.2 or lower as its attack target
KYRIOS = "GD05-048"  # on its deploy turn, may attack a rested enemy Unit
ARMORY_ONE = "GD04-128"  # Base 0/6, 【Destroyed】All players draw 1.
GRAHAM_UNIT = "GD04-071"  # link condition [Graham Aker]
GRAHAM = "GD03-098"  # 【During Link】When this rested Unit is set as active by an effect, ...
SET_ACTIVE_BY_EFFECT = "EB01-005"  # 【Deploy】Choose 1 rested Unit ... Set it as active. Draw 1.

RED_VANILLAS = (
    "GD01-051",
    "GD01-057",
    "GD01-060",
    "GD01-062",
    "GD01-064",
    "GD02-048",
    "GD02-050",
    "GD02-051",
    "GD02-052",
    "GD03-046",
    "GD03-047",
    "GD04-040",
)
MAIN_DECK = tuple(sorted(RED_VANILLAS * 4 + ("GD04-047",) * 2))
RESOURCE_DECK = ("R-001",) * 10
DECK = DeckList(MAIN_DECK, RESOURCE_DECK)


def _new(seed: int = 1, chooser: int | None = 0) -> GameState:
    return new_game((DECK, DECK), seed, chooser=chooser)


def _finish_setup(
    st: GameState, *, player_one: int, redraws: tuple[bool, bool] = (False, False)
) -> None:
    """Choose Player One, then Player One's and Player Two's redraw decisions."""
    apply(st, Action(A.GO_FIRST, player_one))
    for redraw in redraws:
        apply(st, Action(A.REDRAW if redraw else A.KEEP))


def _type(st: GameState, uid: int) -> CardType:
    return V.cdef(st, uid).card_type


def _hand(st: GameState, player: int) -> list[int]:
    return st.zones[player][Zone.HAND]


class Seen(NamedTuple):
    """Game state when an event was emitted, seen from the active player."""

    ev: d.Ev
    phase: Phase
    step: Step
    turn: int
    active: int
    hand: int
    rested: tuple[int, ...]


@pytest.fixture
def events(monkeypatch: pytest.MonkeyPatch) -> list[Seen]:
    """Record every event the engine emits, in order."""
    log: list[Seen] = []
    original = core.emit

    def spy(st: GameState, ev: d.Ev, subject: int = NO_ARG, **kw: Any) -> None:
        p = st.active
        rested = tuple(
            u
            for z in (Zone.BATTLE, Zone.RESOURCE_AREA, Zone.BASE)
            for u in st.zones[p][z]
            if st.cards[u].rested
        )
        log.append(Seen(ev, st.phase, st.step, st.turn, p, len(st.zones[p][Zone.HAND]), rested))
        original(st, ev, subject, **kw)

    monkeypatch.setattr(core, "emit", spy)
    return log


@pytest.fixture
def card_text() -> Iterator[Callable[[str, str], None]]:
    """Give a vanilla card an extra triggered ability, compiled from ``text``, for one test.

    No implemented card prints an "at the start/end of your turn" trigger or a "during this
    turn" HP bonus, so these tests compile the template sentence with the real compiler and
    attach it to a vanilla Unit (gcg_sim.testkit.card_text restores the registry afterwards).
    """
    with ExitStack() as stack:

        def install(number: str, text: str) -> None:
            stack.enter_context(override_card_text(number, text))

        yield install


# ---------------------------------------------------------------------------------------------
# 1-1, 1-2: players, winning and losing


@pytest.mark.rule("1-1-1")
def test_game_is_played_between_two_players() -> None:
    st = new_game((DECK, DECK), 5)
    assert len(st.zones) == 2
    rng = SplitMix64(9)
    deciders = set()
    while st.pending is not None:
        deciders.add(st.pending.player)
        opts = st.pending.options
        apply(st, opts[rng.randrange(len(opts))])
    assert deciders == {0, 1}
    assert st.winner in (0, 1)
    assert st.end_reason in (EndReason.BATTLE_DAMAGE, EndReason.DECK_OUT)


@pytest.mark.rule("1-2-1", "1-2-2-1")
def test_defeat_ends_the_game_and_the_other_player_wins() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.winner == 0
    assert st.end_reason is EndReason.BATTLE_DAMAGE
    assert st.phase is Phase.GAME_OVER
    assert st.pending is None
    assert legal_actions(st) == ()
    with pytest.raises(IllegalActionError):
        apply(st, Action(A.END_MAIN))


@pytest.mark.rule("1-2-2-1")
def test_battle_damage_to_a_base_does_not_defeat_the_player() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    base = sc.base(1)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.winner is None
    assert st.cards[base].damage == 2
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("1-2-2-1")
def test_battle_damage_to_a_shield_does_not_defeat_the_player() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    (shield,) = sc.shields(1, VANILLA)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.winner is None
    assert zone_of(st, shield) is Zone.TRASH


@pytest.mark.rule("1-2-2-1")
def test_blocked_attack_does_not_damage_the_player() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA)
    blocker = sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, unit)
    block(st, blocker)
    pass_all(st)
    assert st.winner is None
    assert st.cards[blocker].damage == 2


@pytest.mark.rule("1-2-2-1")
def test_zero_ap_attack_on_an_empty_shield_area_does_not_defeat() -> None:
    """Zero damage is not dealt (5-5-5), so no battle damage is received."""
    sc = Scenario()
    unit = sc.add(0, ZERO_AP)
    st = sc.start()
    attack(st, unit)
    pass_all(st)
    assert st.winner is None
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("1-2-2-1")
def test_breach_effect_damage_on_an_empty_shield_area_does_not_defeat() -> None:
    sc = Scenario()
    breacher = sc.add(0, BREACH)
    victim = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    attack(st, breacher, victim)
    pass_all(st)
    assert zone_of(st, victim) is Zone.TRASH
    assert st.winner is None


@pytest.mark.rule("1-2-2-2", "7-3-1-1", "1-2-1")
@pytest.mark.faq("Q17")
def test_drawing_the_last_card_loses_immediately() -> None:
    sc = Scenario(active=1, deck_size=0)
    sc.deck(0, VANILLA)
    sc.deck(1, *[VANILLA] * 5)
    st = sc.start()
    last = st.zones[0][Zone.DECK][0]
    end_main(st)
    pass_all(st)
    assert zone_of(st, last) is Zone.HAND
    assert (st.turn, st.active) == (4, 0)
    assert st.winner == 1
    assert st.end_reason is EndReason.DECK_OUT
    assert st.pending is None


@pytest.mark.rule("1-2-2-2", "1-2-3")
@pytest.mark.faq("Q17")
def test_deck_emptied_by_an_effect_loses_before_the_effect_continues() -> None:
    sc = Scenario(deck_size=0)
    sc.deck(0, VANILLA, VANILLA)
    sc.deck(1, *[VANILLA] * 5)
    sc.resources(0, 2)
    card = sc.add(0, DRAW_2_DISCARD_1, Zone.HAND)
    st = sc.start()
    play(st, card)
    assert len(_hand(st, 0)) == 2
    assert st.winner == 1
    assert st.end_reason is EndReason.DECK_OUT
    assert st.pending is None


@pytest.mark.rule("1-2-2-2")
def test_standby_player_whose_deck_empties_loses_during_the_opponents_turn() -> None:
    sc = Scenario(deck_size=0)
    sc.deck(0, *[VANILLA] * 5)
    sc.deck(1, VANILLA)
    attacker = sc.add(0, VANILLA_3_AP)
    sc.base(1, ARMORY_ONE, damage=5)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.active == 0
    assert st.winner == 0
    assert st.end_reason is EndReason.DECK_OUT


@pytest.mark.rule("1-2-3")
def test_all_players_meeting_a_defeat_condition_are_defeated_together() -> None:
    st = Scenario(deck_size=0).start()
    assert st.winner == -1
    assert st.end_reason is EndReason.BOTH_DEFEATED


@pytest.mark.rule("1-2-3", "1-2-2-2")
def test_all_players_draw_empties_both_decks_and_defeats_both() -> None:
    """Armory One's "All players draw 1" is performed active player first but treated as
    simultaneous (ruling EB01-023 Q319), so both players meet the deck-out condition before the
    next rules management."""
    sc = Scenario(deck_size=0)
    sc.deck(0, VANILLA)
    sc.deck(1, VANILLA)
    attacker = sc.add(0, VANILLA_3_AP)
    sc.base(1, ARMORY_ONE, damage=5)
    st = sc.start()
    attack(st, attacker)
    pass_all(st)
    assert st.zones[0][Zone.DECK] == []
    assert st.zones[1][Zone.DECK] == []
    assert st.winner == -1
    assert st.end_reason is EndReason.BOTH_DEFEATED


@pytest.mark.rule("1-2-4", "1-2-5")
def test_either_player_may_concede_at_any_time() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.add(1, BLOCKER)
    st = sc.start()
    attack(st, attacker)
    assert st.pending is not None and st.pending.player == 1
    game.concede(st, 0)
    assert st.winner == 1
    assert st.end_reason is EndReason.CONCEDE
    assert st.pending is None
    assert st.phase is Phase.GAME_OVER


@pytest.mark.rule("1-2-5")
def test_no_card_effect_or_substitution_can_involve_conceding() -> None:
    reg = get_registry()
    assert not [c.card_number for c in reg.db if "concede" in c.effect.lower()]
    assert not [n for n in dir(d) if "concede" in n.lower()]
    # substitution effects (d.Replacement) are keyed by an event; none of them is a loss
    assert not [e for e in d.Ev if any(w in e.value for w in ("concede", "lose", "defeat"))]


# ---------------------------------------------------------------------------------------------
# 1-3: fundamental game rules


@pytest.mark.rule("1-3-1")
def test_card_text_overrides_the_rested_attack_target_rule() -> None:
    sc = Scenario()
    leo = sc.add(0, DUO_LEO)
    plain = sc.add(0, VANILLA)
    active_lv2 = sc.add(1, VANILLA)
    active_lv3 = sc.add(1, VANILLA_LV3)
    st = sc.start()
    assert has_action(st, A.ATTACK, leo, active_lv2)
    assert not has_action(st, A.ATTACK, leo, active_lv3)
    assert not has_action(st, A.ATTACK, plain, active_lv2)


@pytest.mark.rule("1-3-1")
def test_card_text_overrides_the_deploy_turn_attack_rule() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    kyrios = sc.add(0, KYRIOS, Zone.HAND)
    plain = sc.add(0, VANILLA, Zone.HAND)
    rested_enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, kyrios)
    play(st, plain)
    assert has_action(st, A.ATTACK, kyrios, rested_enemy)
    assert not has_action(st, A.ATTACK, kyrios, PLAYER_TARGET)
    assert not has_action(st, A.ATTACK, plain)


@pytest.mark.rule("1-3-2")
def test_impossible_instruction_is_not_performed() -> None:
    sc = Scenario()
    sc.resources(0, 1)
    card = sc.add(0, DISCARD_THEN_DRAW, Zone.HAND)
    st = sc.start()
    deck = len(st.zones[0][Zone.DECK])
    play(st, card)
    assert _hand(st, 0) == []
    assert len(st.zones[0][Zone.DECK]) == deck
    assert zone_of(st, card) is Zone.TRASH
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN


@pytest.mark.rule("1-3-2")
def test_partly_possible_instruction_is_performed_as_far_as_possible() -> None:
    """ "Draw 2" with one card left draws that card (as much as possible); the empty deck then
    defeats the player at rules management. Targeted choices are the exception: ruling
    GD01-003:Q121 requires every target of a choice (see tests/rules/test_effects.py)."""
    sc = Scenario(deck_size=1)
    sc.resources(0, 4)
    draw_two = sc.add(0, DRAW_TWO, Zone.HAND)
    st = sc.start()
    hand_before = len(_hand(st, 0))
    play(st, draw_two)
    assert len(_hand(st, 0)) == hand_before - 1 + 1
    assert st.zones[0][Zone.DECK] == []
    assert st.winner == 1 and st.end_reason is EndReason.DECK_OUT


@pytest.mark.rule("1-3-2-1")
@pytest.mark.parametrize(("already_rested", "expected_ap"), [(True, 4), (False, 6)])
def test_resting_an_already_rested_unit_is_not_performed(
    already_rested: bool, expected_ap: int
) -> None:
    """GuAIZ gets AP+2 when it is rested by an effect; an effect that tries to rest it while it
    is already rested does nothing, so nothing triggers."""
    sc = Scenario()
    guaiz = sc.add(0, GUAIZ, rested=already_rested)
    sc.add(0, VANILLA_3_AP)  # a second friendly Unit so the held Action card stays playable
    sc.resources(0, 1)
    sc.hand(0, ACTION_ONLY)
    sc.resources(1, 3)
    rest_it = sc.add(1, REST_ENEMY, Zone.HAND)
    st = sc.start()
    end_main(st)
    act(st, A.PLAY_COMMAND, rest_it)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert st.cards[guaiz].rested
    assert ap(st, guaiz) == expected_ap


@pytest.mark.rule("1-3-2-2")
@pytest.mark.parametrize("tokens", [0, 1])
def test_zero_repetitions_of_an_action_are_not_performed(tokens: int) -> None:
    """Zero damage is not dealt at all, so "when this Unit receives damage" does not trigger."""
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, DAMAGE_PER_TOKEN, Zone.HAND)
    for _ in range(tokens):
        sc.add(0, UNIT_TOKEN)
    target = sc.add(1, DRAW_ON_EFFECT_DAMAGE)
    st = sc.start()
    hand = len(_hand(st, 1))
    play(st, card)
    assert st.cards[target].damage == tokens
    assert len(_hand(st, 1)) == hand + tokens


@pytest.mark.rule("1-3-2-2")
def test_negative_repetitions_do_not_perform_the_opposite_action() -> None:
    sc = Scenario()
    sc.hand(0, VANILLA)
    st = sc.start()
    hand, deck = list(_hand(st, 0)), list(st.zones[0][Zone.DECK])
    assert interp.draw(st, 0, -2) == 0
    assert _hand(st, 0) == hand
    assert st.zones[0][Zone.DECK] == deck


@pytest.mark.rule("1-3-3", "1-3-2")
def test_prevention_takes_precedence_over_an_effect_requiring_the_action() -> None:
    sc = Scenario()
    sc.resources(0, 7)
    patulia = sc.add(0, PATULIA, Zone.HAND)
    own_base = sc.base(0)
    argama = sc.base(1, ARGAMA)
    st = sc.start()
    play(st, patulia)
    assert zone_of(st, argama) is Zone.BASE
    assert st.cards[argama].damage == 0
    assert own_base not in st.zones[0][Zone.BASE]


@pytest.mark.rule("1-3-4")
def test_active_player_makes_simultaneous_selections_first() -> None:
    sc = Scenario()
    mine = sc.add(0, TAURUS)
    theirs = sc.add(1, TAURUS, rested=True)
    sc.hand(0, VANILLA)
    sc.hand(1, VANILLA)
    st = sc.start()
    attack(st, mine, theirs)
    pass_all(st)
    assert zone_of(st, mine) is Zone.TRASH and zone_of(st, theirs) is Zone.TRASH
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    assert len(_hand(st, 1)) == 1
    select(st, dec.options[0].a)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 1


@pytest.mark.rule("1-3-5")
def test_a_chosen_number_is_a_whole_number_of_at_least_one() -> None:
    """For "Choose 1 to 2" the player picks how many, and that number can't be zero."""
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, REST_1_TO_2, Zone.HAND)
    enemies = [sc.add(1, VANILLA) for _ in range(3)]
    st = sc.start()
    play(st, card)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.SELECT
    assert (dec.ctx("min"), dec.ctx("max")) == (1, 2)
    assert not has_action(st, A.DONE)
    select(st, enemies[0], done=False)
    assert has_action(st, A.DONE)
    select(st, enemies[1], done=False)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert [st.cards[u].rested for u in enemies] == [True, True, False]


@pytest.mark.rule("1-3-6")
@pytest.mark.faq("Q95")
def test_negative_ap_counts_as_zero_until_modified_again() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    minus3 = sc.add(0, AP_MINUS_3, Zone.HAND)
    sc.add(0, VANILLA)
    sc.add(0, VANILLA)
    sc.hand(0, ACTION_ONLY)
    enemy = sc.add(1, VANILLA)
    sc.resources(1, 3)
    plus3 = sc.add(1, AP_PLUS_3, Zone.HAND)
    st = sc.start()
    play(st, minus3)
    assert ap(st, enemy) == 0
    end_main(st)
    act(st, A.PLAY_COMMAND, plus3)
    select(st, enemy)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    assert ap(st, enemy) == 2


# ---------------------------------------------------------------------------------------------
# 6: preparing to play


@pytest.mark.rule("6-1-2", "6-2-3", "6-2-4")
@pytest.mark.faq("Q11", "Q12")
def test_ex_base_for_each_player_and_ex_resource_for_player_two() -> None:
    db = get_registry().db
    assert db.ex_base.card_type is CardType.EX_BASE
    assert (db.ex_base.ap, db.ex_base.hp) == (0, 3)
    assert db.ex_resource.card_type is CardType.EX_RESOURCE
    st = _new(chooser=0)
    _finish_setup(st, player_one=1)
    for p in (0, 1):
        (base,) = st.zones[p][Zone.BASE]
        assert _type(st, base) is CardType.EX_BASE
        assert not st.cards[base].rested
        assert len(st.zones[p][Zone.DECK]) + len(_hand(st, p)) + len(st.zones[p][Zone.SHIELD]) == 50
    ex = {
        p: [u for u in st.zones[p][Zone.RESOURCE_AREA] if _type(st, u) is CardType.EX_RESOURCE]
        for p in (0, 1)
    }
    assert ex[1] == []
    (player_two_ex,) = ex[0]
    assert not st.cards[player_two_ex].rested


@pytest.mark.rule("6-2-1-1")
def test_the_presented_deck_and_resource_deck_are_used() -> None:
    db = get_registry().db
    st = _new()
    for p in (0, 1):
        assert st.decklists[p] == tuple(sorted(db[n].def_id for n in MAIN_DECK))
        assert st.resource_decklists[p] == tuple(sorted(db[n].def_id for n in RESOURCE_DECK))
        assert sorted(st.cards[u].def_id for u in st.zones[p][Zone.DECK]) == list(st.decklists[p])
        rdeck = sorted(st.cards[u].def_id for u in st.zones[p][Zone.RESOURCE_DECK])
        assert rdeck == list(st.resource_decklists[p])


@pytest.mark.rule("6-2-1-1")
@pytest.mark.parametrize(
    "deck",
    [
        DeckList(MAIN_DECK[:49], RESOURCE_DECK),
        DeckList((*MAIN_DECK[:-1], VANILLA), RESOURCE_DECK),
        DeckList(MAIN_DECK, RESOURCE_DECK[:9]),
    ],
    ids=["49-card-deck", "five-copies", "9-card-resource-deck"],
)
def test_decks_that_do_not_conform_to_deck_construction_are_rejected(deck: DeckList) -> None:
    with pytest.raises(ValueError):
        new_game((deck, DECK), 1)


@pytest.mark.rule("6-2-1-2")
def test_decks_are_shuffled_and_placed_face_down() -> None:
    orders = []
    for seed in (1, 2):
        st = _new(seed)
        assert st.pending is not None and st.pending.kind is DecisionKind.CHOOSE_FIRST
        for p in (0, 1):
            assert len(st.zones[p][Zone.DECK]) == 50
            assert all(is_hidden_from(st, u, q) for u in st.zones[p][Zone.DECK] for q in (0, 1))
        orders.append([st.cards[u].def_id for u in st.zones[0][Zone.DECK]])
    assert orders[0] != sorted(orders[0])
    assert orders[0] != orders[1]
    again = _new(1)
    assert [again.cards[u].def_id for u in again.zones[0][Zone.DECK]] == orders[0]


@pytest.mark.rule("6-2-1-3")
def test_resource_deck_is_placed_face_down() -> None:
    st = _new()
    for p in (0, 1):
        rdeck = st.zones[p][Zone.RESOURCE_DECK]
        assert len(rdeck) == 10
        assert all(_type(st, u) is CardType.RESOURCE for u in rdeck)
        assert all(is_hidden_from(st, u, 1 - p) for u in rdeck)


@pytest.mark.rule("6-2-1-4", "6-2-5")
@pytest.mark.faq("Q9")
@pytest.mark.parametrize("pick", [0, 1])
def test_winner_of_the_roll_decides_who_is_player_one(pick: int) -> None:
    st = _new(seed=3, chooser=1)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.CHOOSE_FIRST and dec.player == 1
    assert set(dec.options) == {Action(A.GO_FIRST, 0), Action(A.GO_FIRST, 1)}
    assert _hand(st, 0) == [] and _hand(st, 1) == []
    _finish_setup(st, player_one=pick)
    assert st.first_player == pick
    assert (st.turn, st.active) == (1, pick)
    assert st.pending is not None and st.pending.player == pick


@pytest.mark.rule("6-2-1-4")
def test_a_seeded_roll_decides_the_chooser_when_none_is_given() -> None:
    choosers = set()
    for seed in range(12):
        st = _new(seed, chooser=None)
        assert st.pending is not None and st.pending.kind is DecisionKind.CHOOSE_FIRST
        choosers.add(st.pending.player)
        assert _new(seed, chooser=None).pending == st.pending
    assert choosers == {0, 1}


@pytest.mark.rule("6-2-1-5")
def test_each_player_draws_five_cards_as_the_starting_hand() -> None:
    st = _new()
    tops = {p: list(st.zones[p][Zone.DECK][:5]) for p in (0, 1)}
    apply(st, Action(A.GO_FIRST, 0))
    for p in (0, 1):
        assert _hand(st, p) == tops[p]
        assert len(st.zones[p][Zone.DECK]) == 45
        assert not any(is_hidden_from(st, u, p) for u in tops[p])
        assert all(is_hidden_from(st, u, 1 - p) for u in tops[p])


@pytest.mark.rule("6-2-1-6", "6-2-1-7")
@pytest.mark.faq("Q10")
def test_player_one_decides_on_a_redraw_first_and_may_keep() -> None:
    st = _new(chooser=0)
    apply(st, Action(A.GO_FIRST, 1))
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.REDRAW and dec.player == 1
    assert set(dec.options) == {Action(A.KEEP), Action(A.REDRAW)}
    hand = list(_hand(st, 1))
    apply(st, Action(A.KEEP))
    assert _hand(st, 1) == hand
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.REDRAW and dec.player == 0


@pytest.mark.rule("6-2-1-6-1", "6-2-1-7")
@pytest.mark.faq("Q10")
def test_redraw_returns_the_hand_to_the_bottom_draws_five_then_shuffles() -> None:
    st = _new(chooser=0)
    apply(st, Action(A.GO_FIRST, 0))
    old = list(_hand(st, 0))
    deck = list(st.zones[0][Zone.DECK])
    apply(st, Action(A.REDRAW))
    assert _hand(st, 0) == deck[:5]
    assert sorted(st.zones[0][Zone.DECK]) == sorted(deck[5:] + old)
    assert st.zones[0][Zone.DECK] != deck[5:] + old
    assert all(is_hidden_from(st, u, 0) for u in old)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.REDRAW and dec.player == 1
    old = list(_hand(st, 1))
    deck = list(st.zones[1][Zone.DECK])
    apply(st, Action(A.REDRAW))
    assert _hand(st, 1) == deck[:5]
    assert not set(old) & set(_hand(st, 1))
    assert st.redraws == [True, True]


@pytest.mark.rule("6-2-2")
@pytest.mark.faq("Q8")
def test_six_shields_are_placed_from_the_top_of_the_deck() -> None:
    st = _new(chooser=0)
    apply(st, Action(A.GO_FIRST, 0))
    apply(st, Action(A.KEEP))
    tops = {p: list(st.zones[p][Zone.DECK][:6]) for p in (0, 1)}
    apply(st, Action(A.KEEP))
    for p in (0, 1):
        shields = st.zones[p][Zone.SHIELD]
        assert shields == tops[p][::-1]
        assert all(is_hidden_from(st, u, q) for u in shields for q in (0, 1))
    assert len(st.zones[1][Zone.DECK]) == 50 - 5 - 6


@pytest.mark.rule("6-2-5", "7-3-1", "7-4-1")
@pytest.mark.faq("Q15")
def test_the_game_begins_with_player_ones_turn_including_its_draw() -> None:
    st = _new(chooser=0)
    _finish_setup(st, player_one=1)
    assert (st.turn, st.active, st.phase) == (1, 1, Phase.MAIN)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.MAIN and dec.player == 1
    assert (len(_hand(st, 1)), len(_hand(st, 0))) == (6, 5)
    placed = [u for u in st.zones[1][Zone.RESOURCE_AREA] if _type(st, u) is CardType.RESOURCE]
    assert len(placed) == 1
    assert len(st.zones[1][Zone.RESOURCE_DECK]) == 9
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 10


# ---------------------------------------------------------------------------------------------
# 7-1, 7-2: turn flow and the start phase


@pytest.mark.rule("7-1-1", "7-2-1", "7-2-5", "7-3-1", "7-4-1")
def test_turn_runs_start_draw_resource_main_end_phases_in_order(events: list[Seen]) -> None:
    sc = Scenario()
    sc.resource_deck(0, 2)
    sc.add(0, VANILLA, rested=True)
    st = sc.start(Step.ACTIVE_STEP)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.phase is Phase.MAIN
    end_main(st)
    pass_all(st)
    marks = (d.Ev.TURN_START, d.Ev.DRAWN, d.Ev.RESOURCE_PLACED, d.Ev.TURN_END)
    turn = [e for e in events if e.turn == 3 and e.ev in marks]
    assert [(e.ev, e.phase) for e in turn] == [
        (d.Ev.TURN_START, Phase.START),
        (d.Ev.DRAWN, Phase.DRAW),
        (d.Ev.RESOURCE_PLACED, Phase.RESOURCE),
        (d.Ev.TURN_END, Phase.END),
    ]
    start = turn[0]
    assert start.rested == ()
    assert start.hand == 0
    assert st.active == 1


@pytest.mark.rule("7-1-2", "7-6-7")
def test_players_take_turns_as_the_active_player() -> None:
    sc = Scenario()
    sc.resource_deck(0, 3)
    sc.resource_deck(1, 3)
    st = sc.start()
    seen = []
    for _ in range(3):
        hands = [len(_hand(st, p)) for p in (0, 1)]
        resources = [len(st.zones[p][Zone.RESOURCE_AREA]) for p in (0, 1)]
        to_next_turn(st)
        a, s = st.active, st.standby
        seen.append((st.turn, a))
        assert st.pending is not None and st.pending.player == a
        assert len(_hand(st, a)) == hands[a] + 1
        assert len(st.zones[a][Zone.RESOURCE_AREA]) == resources[a] + 1
        assert len(_hand(st, s)) == hands[s]
        assert len(st.zones[s][Zone.RESOURCE_AREA]) == resources[s]
    assert seen == [(4, 1), (5, 0), (6, 1)]


@pytest.mark.rule("7-1-3", "7-6-2")
def test_end_phase_does_not_advance_while_triggered_effects_are_waiting() -> None:
    sc = Scenario()
    lupus = sc.add(0, REPAIR_TRIGGER, damage=3)
    first, _second = sc.add(1, VANILLA, rested=True), sc.add(1, VANILLA, rested=True)
    sc.hand(0, *[VANILLA] * 11)
    st = sc.start()
    end_main(st)
    pass_all(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.SELECT and dec.player == 0
    assert st.cards[lupus].damage == 1
    assert len(_hand(st, 0)) == 11
    assert (st.turn, st.active, st.phase) == (3, 0, Phase.END)
    select(st, first)
    assert st.cards[first].damage == 1
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    select(st, _hand(st, 0)[0])
    assert (st.turn, st.active) == (4, 1)


@pytest.mark.rule("7-6-2")
def test_end_phase_step_is_reported_until_its_effects_have_resolved() -> None:
    sc = Scenario()
    sc.add(0, REPAIR_TRIGGER, damage=3)
    first, _second = sc.add(1, VANILLA, rested=True), sc.add(1, VANILLA, rested=True)
    sc.hand(0, *[VANILLA] * 11)
    st = sc.start()
    end_main(st)
    pass_all(st)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert st.step.name.startswith("END_STEP")
    select(st, first)
    assert st.pending is not None and st.pending.kind is DecisionKind.DISCARD
    assert st.step.name.startswith("HAND_STEP")


START_CHOICE = (
    "At the start of your turn, choose 1 of your Units. It gains <Repair 1> during this turn."
)


@pytest.mark.rule("7-1-3", "7-2-1", "7-2-2", "7-2-4-1", "7-2-5")
def test_start_step_effects_resolve_before_the_draw_phase(
    card_text: Callable[[str, str], None],
) -> None:
    card_text(HOST, START_CHOICE)
    sc = Scenario()
    sc.resource_deck(0, 2)
    sc.add(0, HOST)
    other = sc.add(0, VANILLA, rested=True)
    st = sc.start(Step.ACTIVE_STEP)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.SELECT and dec.player == 0
    assert not st.cards[other].rested
    assert _hand(st, 0) == []
    assert st.zones[0][Zone.RESOURCE_AREA] == []
    select(st, other)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert len(_hand(st, 0)) == 1
    assert len(st.zones[0][Zone.RESOURCE_AREA]) == 1
    assert keywords(st, other) == {"Repair": 1}


@pytest.mark.rule("7-2-2", "7-2-5")
def test_start_phase_is_reported_until_start_step_effects_have_resolved(
    card_text: Callable[[str, str], None],
) -> None:
    card_text(HOST, START_CHOICE)
    sc = Scenario()
    sc.add(0, HOST)
    sc.add(0, VANILLA)
    st = sc.start(Step.ACTIVE_STEP)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert st.phase is Phase.START
    assert st.step.name.startswith("START_STEP")


@pytest.mark.rule("7-2-3-1")
@pytest.mark.faq("Q13")
def test_active_step_sets_the_active_players_rested_cards_active() -> None:
    sc = Scenario()
    unit = sc.add(0, VANILLA, rested=True)
    resources = sc.resources(0, 2, rested=2)
    base = sc.add(0, sc.db.ex_base.card_number, Zone.BASE, rested=True)
    enemy = sc.add(1, VANILLA, rested=True)
    enemy_resources = sc.resources(1, 1, rested=1)
    st = sc.start(Step.ACTIVE_STEP)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert not any(st.cards[u].rested for u in (unit, *resources, base))
    assert st.cards[enemy].rested
    assert all(st.cards[u].rested for u in enemy_resources)


@pytest.mark.rule("7-2-3-2")
def test_active_step_readies_all_cards_at_once(events: list[Seen]) -> None:
    sc = Scenario()
    sc.add(0, VANILLA, rested=True)
    sc.add(0, VANILLA, rested=True)
    sc.resources(0, 2, rested=2)
    st = sc.start(Step.ACTIVE_STEP)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert events[0].ev is d.Ev.TURN_START
    assert events[0].rested == ()
    assert not [e for e in events if e.ev is d.Ev.SET_ACTIVE]


@pytest.mark.rule("7-2-3-1", "7-2-3-2")
@pytest.mark.parametrize("by_effect", [False, True])
def test_active_step_readying_is_not_setting_active_by_an_effect(by_effect: bool) -> None:
    """Graham Aker: "When this rested Unit is set as active by an effect, choose 1 enemy Unit
    with 3 or less HP. Return it to its owner's hand." The active step does not trigger it."""
    if by_effect:
        sc = Scenario(active=1)
        linked = sc.add(0, GRAHAM_UNIT, rested=True, pilot=GRAHAM)
        enemy = sc.add(1, VANILLA)
        sc.resources(1, 7)
        zeta = sc.add(1, SET_ACTIVE_BY_EFFECT, Zone.HAND)
        st = sc.start()
        play(st, zeta)
    else:
        sc = Scenario()
        linked = sc.add(0, GRAHAM_UNIT, rested=True, pilot=GRAHAM)
        enemy = sc.add(1, VANILLA)
        st = sc.start(Step.ACTIVE_STEP)
    assert not st.cards[linked].rested
    assert zone_of(st, enemy) is (Zone.HAND if by_effect else Zone.BATTLE)


@pytest.mark.rule("7-2-4-1")
def test_start_of_turn_effects_activate_at_the_start_of_that_players_turn(
    card_text: Callable[[str, str], None], events: list[Seen]
) -> None:
    card_text(HOST, "At the start of your turn, draw 1.")
    sc = Scenario(active=1)
    sc.add(0, HOST)
    st = sc.start()
    to_next_turn(st)
    assert st.active == 0
    assert len(_hand(st, 0)) == 2
    to_next_turn(st)
    assert st.active == 1
    assert len(_hand(st, 0)) == 2
    starts = [(e.turn, e.active, e.phase, e.step) for e in events if e.ev is d.Ev.TURN_START]
    assert starts == [(4, 0, Phase.START, Step.START_STEP), (5, 1, Phase.START, Step.START_STEP)]


# ---------------------------------------------------------------------------------------------
# 7-3, 7-4: draw and resource phases


@pytest.mark.rule("7-3-1")
@pytest.mark.faq("Q14")
def test_draw_phase_draws_the_top_card_of_the_deck() -> None:
    sc = Scenario(active=1)
    sc.hand(1, VANILLA)
    st = sc.start()
    top = st.zones[0][Zone.DECK][0]
    deck = len(st.zones[0][Zone.DECK])
    standby_hand = list(_hand(st, 1))
    to_next_turn(st)
    assert _hand(st, 0) == [top]
    assert len(st.zones[0][Zone.DECK]) == deck - 1
    assert _hand(st, 1) == standby_hand


@pytest.mark.rule("7-4-1")
@pytest.mark.faq("Q18")
def test_resource_phase_places_the_top_resource_face_up_and_active() -> None:
    sc = Scenario(active=1)
    sc.resource_deck(0, 3)
    st = sc.start()
    top = st.zones[0][Zone.RESOURCE_DECK][0]
    to_next_turn(st)
    assert st.zones[0][Zone.RESOURCE_AREA] == [top]
    assert not st.cards[top].rested
    assert not is_hidden_from(st, top, 1)
    assert len(st.zones[0][Zone.RESOURCE_DECK]) == 2


@pytest.mark.rule("7-4-1")
@pytest.mark.faq("Q20")
def test_resource_phase_with_an_empty_resource_deck_moves_on() -> None:
    st = Scenario(active=1).start()
    to_next_turn(st)
    assert st.active == 0
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert st.zones[0][Zone.RESOURCE_AREA] == []


# ---------------------------------------------------------------------------------------------
# 7-5: main phase


@pytest.mark.rule("7-5-1", "7-5-5-1")
def test_main_phase_actions_in_any_order_as_often_as_allowed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    first, second = sc.hand(0, VANILLA, VANILLA)
    geara = sc.add(0, ACTIVATE_MAIN)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    assert {A.PLAY_UNIT, A.ACTIVATE, A.ATTACK, A.END_MAIN} <= legal_kinds(st)
    act(st, A.ACTIVATE, geara)
    assert st.cards[enemy].damage == 1
    play(st, first)
    play(st, second)
    assert zone_of(st, first) is Zone.BATTLE and zone_of(st, second) is Zone.BATTLE
    assert has_action(st, A.ATTACK, geara)
    assert has_action(st, A.END_MAIN)


@pytest.mark.rule("7-5-1")
def test_main_phase_actions_wait_until_triggered_effects_are_resolved() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    amatsu = sc.add(0, DEPLOY_CHOICE, Zone.HAND)
    sc.hand(0, VANILLA)
    other = sc.add(0, VANILLA)
    st = sc.start()
    play(st, amatsu)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert not legal_kinds(st) & {A.PLAY_UNIT, A.ACTIVATE, A.ATTACK, A.END_MAIN}
    select(st, other)
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert {A.PLAY_UNIT, A.ATTACK, A.END_MAIN} <= legal_kinds(st)


@pytest.mark.rule("7-5-2-1", "7-5-2-2-4")
def test_units_bases_pilots_and_main_commands_are_played_from_hand() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    unit = sc.add(0, VANILLA, Zone.HAND)
    base = sc.add(0, BASE, Zone.HAND)
    pilot = sc.add(0, PILOT, Zone.HAND)
    command = sc.add(0, DRAW_2_DISCARD_1, Zone.HAND)
    action_only = sc.add(0, ACTION_ONLY, Zone.HAND)
    host = sc.add(0, VANILLA)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, unit)
    assert has_action(st, A.PLAY_BASE, base)
    assert has_action(st, A.PAIR, pilot, host)
    assert has_action(st, A.PLAY_COMMAND, command)
    assert not has_action(st, A.PLAY_COMMAND, action_only)
    drawn = list(st.zones[0][Zone.DECK][:2])
    play(st, command)
    select(st, drawn[0])
    play(st, pilot, onto=host)
    play(st, base)
    play(st, unit)
    assert zone_of(st, command) is Zone.TRASH
    assert zone_of(st, drawn[0]) is Zone.TRASH and zone_of(st, drawn[1]) is Zone.HAND
    assert zone_of(st, pilot) is Zone.PAIRED and st.cards[host].pair == pilot
    assert st.zones[0][Zone.BASE] == [base]
    assert zone_of(st, unit) is Zone.BATTLE


@pytest.mark.rule("7-5-2-2-1")
def test_a_played_card_is_revealed() -> None:
    sc = Scenario()
    sc.resources(0, 3)
    card = sc.add(0, AP_MINUS_3, Zone.HAND, known=False)
    sc.add(1, VANILLA)
    sc.add(1, VANILLA)
    st = sc.start()
    assert is_hidden_from(st, card, 1)
    play(st, card)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert zone_of(st, card) is Zone.RESOLVING
    assert st.cards[card].known & (1 << 1)


@pytest.mark.rule("7-5-2-2-2")
@pytest.mark.faq("Q21")
@pytest.mark.parametrize(
    ("normal", "rested", "ex", "playable"),
    [(1, 0, 0, False), (2, 1, 0, True), (1, 0, 1, True)],
    ids=["one-resource", "one-of-two-rested", "resource-plus-ex"],
)
def test_level_condition_counts_every_resource(
    normal: int, rested: int, ex: int, playable: bool
) -> None:
    sc = Scenario()
    sc.resources(0, normal, rested=rested, ex=ex)
    card = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    assert has_action(st, A.PLAY_UNIT, card) is playable


@pytest.mark.rule("7-5-2-2-3")
def test_cost_is_paid_by_resting_the_chosen_resources() -> None:
    sc = Scenario()
    normal = sc.resources(0, 2)
    (ex,) = sc.resources(0, 0, ex=1)
    card = sc.add(0, VANILLA, Zone.HAND)
    st = sc.start()
    assert {o.c for o in options(st) if o.kind is A.PLAY_UNIT and o.a == card} == {0, 1}
    with_ex = st.clone()
    act(st, A.PLAY_UNIT, card, None, 0)
    assert sorted(st.cards[u].rested for u in normal) == [False, True]
    assert zone_of(st, ex) is Zone.RESOURCE_AREA and not st.cards[ex].rested
    act(with_ex, A.PLAY_UNIT, card, None, 1)
    assert not any(with_ex.cards[u].rested for u in normal)
    assert zone_of(with_ex, ex) is Zone.OUTSIDE


@pytest.mark.rule("7-5-3-1")
def test_activate_main_effects_are_activated_in_the_main_phase() -> None:
    sc = Scenario()
    (resource,) = sc.resources(0, 1)
    geara = sc.add(0, ACTIVATE_MAIN)
    enemy = sc.add(1, VANILLA)
    st = sc.start()
    act(st, A.ACTIVATE, geara)
    assert st.cards[resource].rested
    assert st.cards[enemy].damage == 1
    assert st.pending is not None and st.pending.kind is DecisionKind.MAIN
    assert not has_action(st, A.ACTIVATE, geara)


@pytest.mark.rule("7-5-5-1", "7-5-5-2")
def test_declaring_the_end_of_the_main_phase_enters_the_end_phase() -> None:
    sc = Scenario()
    attacker = sc.add(0, VANILLA)
    sc.resources(0, 2)
    (unplayed,) = sc.hand(0, VANILLA)
    sc.add(1, VANILLA)
    sc.add(1, VANILLA)
    sc.resources(1, 1)
    sc.hand(1, ACTION_ONLY)
    st = sc.start()
    assert has_action(st, A.END_MAIN)
    assert has_action(st, A.ATTACK, attacker) and has_action(st, A.PLAY_UNIT, unplayed)
    end_main(st)
    assert st.phase is Phase.END
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.ACTION_STEP and dec.player == 1
    pass_(st)
    assert (st.turn, st.active) == (4, 1)
    assert zone_of(st, unplayed) is Zone.HAND


# ---------------------------------------------------------------------------------------------
# 7-6: end phase


@pytest.mark.rule("7-6-1")
def test_end_phase_steps_run_action_end_hand_cleanup_in_order() -> None:
    sc = Scenario()
    jegan = sc.add(0, REPAIR, damage=1)
    sc.resources(0, 4)
    rally = sc.add(0, ALL_AP_PLUS_2, Zone.HAND)
    sc.hand(0, *[VANILLA] * 11)
    sc.add(1, VANILLA)
    sc.add(1, VANILLA)
    sc.resources(1, 1)
    sc.hand(1, ACTION_ONLY)
    st = sc.start()
    play(st, rally)
    assert ap(st, jegan) == 5
    end_main(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.ACTION_STEP and dec.player == 1
    assert st.cards[jegan].damage == 1
    pass_(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    assert st.cards[jegan].damage == 0
    assert ap(st, jegan) == 5
    select(st, _hand(st, 0)[0])
    assert (st.turn, st.active) == (4, 1)
    assert len(_hand(st, 0)) == 10
    assert ap(st, jegan) == 3


@pytest.mark.rule("7-6-4-1")
def test_end_of_turn_effects_activate_in_the_end_step(
    card_text: Callable[[str, str], None],
) -> None:
    card_text(HOST, "At the end of your turn, draw 1.")
    sc = Scenario()
    sc.add(0, HOST)
    jegan = sc.add(0, REPAIR, damage=2)
    enemy_jegan = sc.add(1, REPAIR, damage=2)
    sc.hand(0, *[VANILLA] * 10)
    st = sc.start()
    end_main(st)
    pass_all(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.ORDER_TRIGGER and dec.player == 0
    assert len(dec.options) == 2
    assert len(_hand(st, 0)) == 10
    order(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    assert len(_hand(st, 0)) == 11
    assert st.cards[jegan].damage == 1
    assert st.cards[enemy_jegan].damage == 2


@pytest.mark.rule("7-6-5-1")
@pytest.mark.faq("Q16")
def test_hand_step_discards_down_to_ten_cards_of_the_players_choice() -> None:
    sc = Scenario()
    hand = sc.hand(0, *[VANILLA] * 10, VANILLA_LV3)
    sc.hand(1, *[VANILLA] * 12)
    st = sc.start()
    end_main(st)
    pass_all(st)
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    assert {o.a for o in dec.options} == set(hand)
    select(st, hand[-1])
    assert zone_of(st, hand[-1]) is Zone.TRASH
    assert len(_hand(st, 0)) == 10
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.MAIN and dec.player == 1
    assert len(_hand(st, 1)) == 13


@pytest.mark.rule("7-6-5-1")
def test_a_hand_of_exactly_ten_cards_is_kept() -> None:
    sc = Scenario()
    sc.hand(0, *[VANILLA] * 10)
    st = sc.start()
    end_main(st)
    pass_all(st)
    assert st.active == 1
    assert len(_hand(st, 0)) == 10
    assert st.zones[0][Zone.TRASH] == []


@pytest.mark.rule("7-6-6-1")
def test_during_this_turn_effects_end_in_the_cleanup_step() -> None:
    sc = Scenario()
    sc.resources(0, 4)
    rally = sc.add(0, ALL_AP_PLUS_2, Zone.HAND)
    unit = sc.add(0, VANILLA)
    st = sc.start()
    play(st, rally)
    assert ap(st, unit) == 4
    to_next_turn(st)
    assert ap(st, unit) == 2


@pytest.mark.rule("7-6-6-1")
def test_effects_caused_by_the_cleanup_step_resolve_before_the_turn_passes(
    card_text: Callable[[str, str], None],
) -> None:
    card_text(HOST_PLAYABLE, "【Deploy】Choose 1 of your Units. It gets HP+2 during this turn.")
    sc = Scenario()
    sc.resources(0, 2)
    booster = sc.add(0, HOST_PLAYABLE, Zone.HAND)
    taurus = sc.add(0, TAURUS)
    sc.hand(0, VANILLA)
    enemy = sc.add(1, VANILLA, rested=True)
    st = sc.start()
    play(st, booster)
    select(st, taurus)
    assert hp(st, taurus) == 3
    attack(st, taurus, enemy)
    pass_all(st)
    assert zone_of(st, enemy) is Zone.TRASH
    assert st.cards[taurus].damage == 2
    end_main(st)
    pass_all(st)
    assert zone_of(st, taurus) is Zone.TRASH
    dec = st.pending
    assert dec is not None and dec.kind is DecisionKind.DISCARD and dec.player == 0
    assert (st.turn, st.active) == (3, 0)
    select(st, dec.options[0].a)
    assert (st.turn, st.active) == (4, 1)
