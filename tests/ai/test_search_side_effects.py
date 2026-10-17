"""Searching never changes the true game: determinized and permuted copies, and a whole MCTS
decision, leave the state they start from (and so its digest) exactly as it was."""

from __future__ import annotations

from collections.abc import Callable

from tests.ai.decks import DECKS

from gcg_sim.ai import SearchConfig
from gcg_sim.ai.agents import MctsAgent
from gcg_sim.engine.game import apply, new_game
from gcg_sim.engine.observe import determinize, is_hidden_from, permute_hidden
from gcg_sim.engine.state import GameState
from gcg_sim.rng import SplitMix64
from gcg_sim.runner.digest import state_digest

SMALL_SEARCH = SearchConfig(
    name="small", max_iterations=16, min_iterations=16, iterations_per_option=1
)


def _first_state(game_seed: int, wanted: Callable[[GameState], bool]) -> GameState:
    """The first decision of a random-move game that satisfies ``wanted``."""
    st = new_game(DECKS, game_seed)
    rng = SplitMix64(game_seed)
    while st.pending is not None and not wanted(st):
        apply(st, st.pending.options[rng.randrange(len(st.pending.options))])
    assert st.pending is not None
    return st


def _hidden_history(st: GameState, observer: int) -> list[int]:
    """Positions of this turn's history entries about cards hidden from ``observer``."""
    return [
        i for i, h in enumerate(st.history) if h.uid >= 0 and is_hidden_from(st, h.uid, observer)
    ]


def test_search_copies_leave_the_true_history_untouched() -> None:
    """A determinized or permuted copy gives this turn's history entries its own identities
    without writing them into the entries it shares with the true state."""
    st = _first_state(4, lambda s: bool(_hidden_history(s, 1 - s.active)))
    before = state_digest(st)
    rewritten = 0
    for observer in (0, 1):
        for seed in range(6):
            for copy in (determinize(st, observer, seed), permute_hidden(st, observer, seed)):
                for i in _hidden_history(st, observer):
                    h = copy.history[i]
                    assert h.def_id == copy.cards[h.uid].def_id
                    rewritten += h.def_id != st.history[i].def_id
    assert rewritten > 0
    assert state_digest(st) == before


def test_mcts_choice_leaves_the_true_state_unchanged() -> None:
    """The searching player's determinizations never change the state it decides on (its
    digest is recorded with every game and checked when a replay is verified)."""
    st = _first_state(
        4,
        lambda s: (
            s.pending is not None
            and len(s.pending.options) > 1
            and bool(_hidden_history(s, s.pending.player))
        ),
    )
    assert st.pending is not None
    before = state_digest(st)
    MctsAgent(5, config=SMALL_SEARCH).choose(st, st.pending.player)
    assert state_digest(st) == before
