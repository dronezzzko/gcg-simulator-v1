"""Game states pending each kind of decision, built with the testkit (for the AI tests)."""

from __future__ import annotations

from collections.abc import Callable

from tests.ai.decks import DECKS

from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import DecisionKind, Step, Zone
from gcg_sim.testkit import Scenario, attack, pass_all, play

K = DecisionKind


def choose_first() -> GameState:
    return new_game(DECKS, 5)


def redraw() -> GameState:
    st = new_game(DECKS, 6)
    assert st.pending is not None
    apply(st, st.pending.options[0])
    return st


def main_phase() -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "ST01-001")
    sc.add(1, "ST04-005", rested=True)
    sc.hand(0, "GD01-018", "ST01-010", "ST01-014")
    return sc.start()


def block_step() -> GameState:
    sc = Scenario()
    gm = sc.add(0, "ST01-005")
    sc.add(1, "ST01-008")
    sc.add(1, "ST01-009")
    st = sc.start()
    attack(st, gm)
    return st


def action_step() -> GameState:
    sc = Scenario()
    gundam = sc.add(0, "GD01-013")
    sc.resources(1, 3)
    sc.hand(1, "ST01-014")
    st = sc.start()
    attack(st, gundam)
    return st


def order_trigger() -> GameState:
    sc = Scenario()
    sc.add(0, "GD01-004", damage=1)
    sc.add(0, "GD01-004", damage=1)
    return sc.start(Step.END_STEP)


def burst() -> GameState:
    sc = Scenario()
    gm = sc.add(0, "ST01-005")
    sc.shields(1, "ST01-010")
    st = sc.start()
    attack(st, gm)
    pass_all(st)
    return st


def yes_no() -> GameState:
    sc = Scenario()
    sc.resources(0, 3)
    age = sc.add(0, "GD02-021", Zone.HAND)
    sc.hand(0, "GD02-030")
    st = sc.start()
    play(st, age)
    return st


def select_target() -> GameState:
    sc = Scenario()
    sc.resources(0, 3)
    guntank = sc.add(0, "ST01-004", Zone.HAND)
    sc.add(1, "ST01-008")
    sc.add(1, "ST02-009")
    st = sc.start()
    play(st, guntank)
    return st


def select_up_to_two() -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    orders = sc.add(0, "GD01-099", Zone.HAND)
    sc.add(1, "ST01-008")
    sc.add(1, "ST02-009")
    sc.add(1, "ST04-005")
    st = sc.start()
    play(st, orders)
    return st


def select_mode() -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    card = sc.add(0, "GD05-106", Zone.HAND)
    sc.trash(0, "GD01-088")
    st = sc.start()
    play(st, card)
    return st


def excess() -> GameState:
    sc = Scenario()
    sc.resources(0, 2)
    for _ in range(6):
        sc.add(0, "ST01-005")
    gm = sc.add(0, "ST04-005", Zone.HAND)
    st = sc.start()
    play(st, gm)
    return st


def discard() -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    strike = sc.add(0, "ST04-002", Zone.HAND)
    sc.hand(0, "ST04-005", "ST04-013")
    st = sc.start()
    play(st, strike)
    return st


def arrange() -> GameState:
    sc = Scenario()
    sc.resources(0, 1)
    dopp = sc.add(0, "GD01-039", Zone.HAND)
    st = sc.start()
    play(st, dopp)
    return st


SITUATIONS: dict[DecisionKind, Callable[[], GameState]] = {
    K.CHOOSE_FIRST: choose_first,
    K.REDRAW: redraw,
    K.MAIN: main_phase,
    K.BLOCK: block_step,
    K.ACTION_STEP: action_step,
    K.ORDER_TRIGGER: order_trigger,
    K.BURST: burst,
    K.YES_NO: yes_no,
    K.SELECT: select_target,
    K.EXCESS: excess,
    K.DISCARD: discard,
    K.ARRANGE: arrange,
}

EXTRA_SELECTS: dict[str, Callable[[], GameState]] = {
    "select_up_to_two": select_up_to_two,
    "select_mode": select_mode,
}
