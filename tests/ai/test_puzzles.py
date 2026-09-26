"""Tactical puzzles with known best play, solved by the default MCTS preset (criterion 7b).

Each puzzle is a position built with the testkit. The agent under test plays one side with
the ``standard`` preset; the other side is played by the same kind of agent, so the solution
must hold against a strong defence, not a passive one.
"""

from __future__ import annotations

import pytest
from tests.ai.situations import burst, choose_first

from gcg_sim.ai import make_agent
from gcg_sim.ai.base import Agent
from gcg_sim.engine import view as V
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, attack, play, zone_of

A = ActionKind
SEEDS = (1, 2)
FILLER = "GD01-060"


def agents(seed: int) -> tuple[Agent, Agent]:
    return make_agent("mcts", seed=seed), make_agent("mcts", seed=seed + 100)


def play_turns(st: GameState, players: tuple[Agent, Agent], turns: int = 1) -> None:
    """Let the agents play until ``turns`` turn boundaries pass or the game ends."""
    stop = st.turn + turns
    while st.pending is not None and st.turn < stop:
        p = st.pending.player
        apply(st, players[p].choose(st, p))


def choice(st: GameState, agent: Agent) -> Action:
    assert st.pending is not None
    return agent.choose(st, st.pending.player)


# ---------------------------------------------------------------------------------------------
# lethal


@pytest.mark.parametrize("seed", SEEDS)
def test_lethal_attack_the_exposed_player(seed: int) -> None:
    sc = Scenario()
    sc.add(0, "ST01-005")
    sc.add(1, "ST04-005", rested=True)
    sc.shields(0, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
def test_lethal_goes_face_instead_of_taking_a_free_kill(seed: int) -> None:
    sc = Scenario()
    sc.add(0, "ST01-005")
    sc.add(0, "ST01-005")
    sc.add(1, "ST01-008", rested=True)
    sc.shields(1, FILLER)
    sc.shields(0, FILLER, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
def test_lethal_rests_the_blocker_before_attacking(seed: int) -> None:
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, "ST01-005")
    sc.hand(0, "ST02-014")  # Siege Ploy: rest 1 enemy Unit with 5 or less HP
    sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
    sc.shields(0, FILLER, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
def test_lethal_pairs_the_linking_pilot_to_attack_on_deploy_turn(seed: int) -> None:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "ST01-001", deployed_this_turn=True)  # Gundam, link [Amuro Ray]
    sc.hand(0, "ST01-010")  # Amuro Ray
    sc.shields(0, FILLER, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
def test_pairing_chooses_the_pilot_and_unit_that_link(seed: int) -> None:
    sc = Scenario()
    sc.resources(0, 4, rested=3)  # one Pilot only
    sc.add(0, "ST01-005")  # GM, can attack
    gundam = sc.add(0, "ST01-001", deployed_this_turn=True)
    amuro, _ = sc.hand(0, "ST01-010", "ST01-011")  # Amuro Ray links; Suletta does not
    sc.shields(1, FILLER)
    sc.shields(0, FILLER, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.cards[amuro].pair == gundam
    assert st.winner == 0


@pytest.mark.parametrize("seed", SEEDS)
def test_burst_aware_attack_order(seed: int) -> None:
    """Every enemy card is Siege Ploy (【Burst】rest 1 enemy Unit with 5 or less HP): attack the
    last Shield with the vulnerable Unit first so the 6-HP Unit is still active for lethal."""
    sc = Scenario()
    sc.add(0, "GD01-027")  # Big Zam 5/6
    sc.add(0, "ST04-005")  # Strike Dagger 3/2
    sc.shields(1, "ST02-014")
    sc.deck(1, *["ST02-014"] * 20)
    sc.shields(0, FILLER, FILLER, FILLER)
    st = sc.start()
    play_turns(st, agents(seed))
    assert st.winner == 0


# ---------------------------------------------------------------------------------------------
# holding cards


@pytest.mark.parametrize("seed", SEEDS)
def test_holds_removal_for_next_turns_lethal(seed: int) -> None:
    """Resting the Blocker now is wasted (it stands up again); keep Siege Ploy for next turn."""
    sc = Scenario()
    sc.resources(0, 4, rested=3)
    sc.add(0, "GD01-013", deployed_this_turn=True)  # Gundam 3/4, cannot attack yet
    ploy = sc.add(0, "ST02-014", Zone.HAND)
    sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
    sc.resources(1, 2)
    sc.shields(0, FILLER, FILLER, FILLER, FILLER)
    st = sc.start()
    players = agents(seed)
    play_turns(st, players)
    assert zone_of(st, ploy) is Zone.HAND
    play_turns(st, players, turns=2)
    assert st.winner == 0


# ---------------------------------------------------------------------------------------------
# trades and blocks


@pytest.mark.parametrize("seed", SEEDS)
def test_takes_the_favourable_trade(seed: int) -> None:
    sc = Scenario()
    gundam = sc.add(0, "GD01-013")  # 3/4
    dagger = sc.add(1, "ST04-005", rested=True)  # 3/2, rested after attacking
    sc.shields(0, FILLER, FILLER)
    sc.shields(1, *[FILLER] * 5)
    st = sc.start()
    play_turns(st, agents(seed))
    assert zone_of(st, dagger) is Zone.TRASH
    assert zone_of(st, gundam) is Zone.BATTLE


@pytest.mark.parametrize("seed", SEEDS)
def test_does_not_attack_into_a_bigger_blocker(seed: int) -> None:
    sc = Scenario()
    gm = sc.add(0, "ST01-005")  # 2/2
    sc.add(1, "ST04-001")  # Aile Strike Gundam 4/4 <Blocker>
    sc.shields(0, *[FILLER] * 6)
    sc.shields(1, *[FILLER] * 6)
    st = sc.start()
    play_turns(st, agents(seed))
    assert zone_of(st, gm) is Zone.BATTLE


@pytest.mark.parametrize("seed", SEEDS)
def test_blocks_to_survive(seed: int) -> None:
    sc = Scenario(active=1)
    gm = sc.add(1, "ST01-005")
    blocker = sc.add(0, "ST01-008")
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, gm)
    assert choice(st, agents(seed)[0]) == Action(A.BLOCK, blocker)


@pytest.mark.parametrize("seed", SEEDS)
def test_blocks_when_the_blocker_wins_the_fight(seed: int) -> None:
    sc = Scenario(active=1)
    dagger = sc.add(1, "ST04-005")  # 3/2
    blocker = sc.add(0, "GD01-072")  # Launcher Strike Gundam 3/4 <Blocker>
    sc.shields(0, *[FILLER] * 5)
    sc.shields(1, *[FILLER] * 5)
    st = sc.start()
    attack(st, dagger)
    assert choice(st, agents(seed)[0]) == Action(A.BLOCK, blocker)


@pytest.mark.parametrize("seed", SEEDS)
def test_uses_an_action_step_trick_to_survive(seed: int) -> None:
    """No Shields, no Blocker: Unforeseen Incident (AP-3) on the 2-AP attacker in the action
    step means it deals no damage (rules 5-5-5, 8-5-2-2)."""
    sc = Scenario(active=1)
    gm = sc.add(1, "ST01-005")
    sc.resources(0, 3)
    sc.hand(0, "ST01-014")
    sc.shields(1, FILLER, FILLER)
    st = sc.start()
    attack(st, gm)
    assert st.pending is not None and st.pending.kind is DecisionKind.ACTION_STEP
    play_turns(st, agents(seed))
    assert st.winner is None


# ---------------------------------------------------------------------------------------------
# targeting, excess, bursts, mulligans


@pytest.mark.parametrize("seed", SEEDS)
def test_removal_targets_the_most_valuable_unit_it_can_destroy(seed: int) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    combat = sc.add(0, "ST03-013", Zone.HAND)  # Close Combat: 2 damage to 1 enemy Unit
    aile = sc.add(1, "ST04-001", damage=2)  # Aile Strike Gundam 4/4 <Blocker>, 2 HP left
    sc.add(1, "ST04-005")  # Strike Dagger 3/2
    sc.add(1, "ST04-003")  # Moebius Zero 2/4
    st = sc.start()
    play(st, combat)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert choice(st, agents(seed)[0]) == Action(A.SELECT, aile)


@pytest.mark.parametrize("seed", SEEDS)
def test_excess_trashes_the_weakest_unit(seed: int) -> None:
    sc = Scenario()
    sc.resources(0, 2)
    for _ in range(5):
        sc.add(0, "GD01-013")
    weak = sc.add(0, "ST01-008", damage=0)
    strike = sc.add(0, "ST04-005", Zone.HAND)
    st = sc.start()
    play(st, strike)
    assert st.pending is not None and st.pending.kind is DecisionKind.EXCESS
    assert choice(st, agents(seed)[0]) == Action(A.SELECT, weak)


@pytest.mark.parametrize("seed", SEEDS)
def test_takes_a_free_card_from_burst(seed: int) -> None:
    st = burst()  # Amuro Ray: 【Burst】Add this card to your hand
    assert choice(st, agents(seed)[1]) == Action(A.YES)


@pytest.mark.parametrize("seed", SEEDS)
def test_chooses_to_play_first_with_proactive_decks(seed: int) -> None:
    """Both test decks develop the board early; Player One wins about 60% of self-play games,
    and the play/draw simulation agrees."""
    st = choose_first()
    assert st.pending is not None
    chooser = st.pending.player
    assert choice(st, agents(seed)[chooser]) == Action(A.GO_FIRST, chooser)


# A legal Blue/White deck: 40 Units of Lv.1-2 and 10 Units of Lv.6 or higher.
CHEAP = [
    "ST01-005",
    "ST01-008",
    "ST02-009",
    "GD01-008",
    "GD01-011",
    "GD01-021",
    "GD01-022",
    "EB01-051",
    "GD01-079",
    "GD01-085",
]
HEAVY = ["GD01-065", "EB01-001", "EB01-002", "EB01-004", "EB01-005"]
MULLIGAN_DECK = DeckList(
    tuple(n for n in CHEAP for _ in range(4)) + tuple(n for n in HEAVY for _ in range(2)),
    ("R-001",) * 10,
)


def _opening(seed: int) -> GameState:
    st = new_game((MULLIGAN_DECK, MULLIGAN_DECK), seed)
    assert st.pending is not None
    apply(st, st.pending.options[0])
    return st


def _heavy_cards(st: GameState, player: int) -> int:
    return sum(1 for u in st.zones[player][Zone.HAND] if V.cdef(st, u).card_number in HEAVY)


def _find_opening(heavy_at_least: int, heavy_at_most: int) -> GameState:
    for seed in range(2000):
        st = _opening(seed)
        assert st.pending is not None
        n = _heavy_cards(st, st.pending.player)
        if heavy_at_least <= n <= heavy_at_most:
            return st
    raise AssertionError("no opening hand found")


@pytest.mark.parametrize("seed", SEEDS)
def test_mulligans_a_hand_of_uncastable_cards(seed: int) -> None:
    st = _find_opening(4, 5)
    assert choice(st, agents(seed)[0]) == Action(A.REDRAW)


@pytest.mark.parametrize("seed", SEEDS)
def test_keeps_a_hand_with_early_plays(seed: int) -> None:
    st = _find_opening(0, 0)
    assert choice(st, agents(seed)[0]) == Action(A.KEEP)
