"""Head-to-head games between agents, serial or across processes, with per-game seeds.

Every game's seeds derive from (master seed, game index) only, so results are identical for
any number of worker processes.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from multiprocessing import get_context

from gcg_sim.ai.agents import make_agent
from gcg_sim.ai.base import Agent
from gcg_sim.engine.game import DeckList, apply, new_game
from gcg_sim.engine.state import GameState
from gcg_sim.rng import derive_seed

DECISION_LIMIT = 20000
MIRROR_BLOCK = 4


@dataclass(frozen=True, slots=True)
class AgentSpec:
    kind: str
    preset: str = "standard"


@dataclass(frozen=True, slots=True)
class GameSpec:
    """One game: ``agents[i]`` plays ``decks[i]`` in seat ``i``; ``challenger`` is the seat
    of the agent under test."""

    index: int
    game_seed: int
    agent_seeds: tuple[int, int]
    agents: tuple[AgentSpec, AgentSpec]
    decks: tuple[DeckList, DeckList]
    challenger: int


@dataclass(frozen=True, slots=True)
class GameOutcome:
    index: int
    challenger: int
    winner: int
    turns: int
    first_player: int
    decisions: int


def run_game(
    agents: tuple[Agent, Agent],
    decks: tuple[DeckList, DeckList],
    seed: int,
    observer: Callable[[GameState], None] | None = None,
) -> tuple[GameState, int]:
    """Play one game to the end; ``observer`` sees the state before every decision."""
    st = new_game(decks, seed)
    decisions = 0
    while st.pending is not None and decisions < DECISION_LIMIT:
        if observer is not None:
            observer(st)
        p = st.pending.player
        apply(st, agents[p].choose(st, p))
        decisions += 1
    return st, decisions


def play(spec: GameSpec) -> GameOutcome:
    a0, a1 = (
        make_agent(a.kind, preset=a.preset, seed=s)
        for a, s in zip(spec.agents, spec.agent_seeds, strict=True)
    )
    st, decisions = run_game((a0, a1), spec.decks, spec.game_seed)
    winner = -1 if st.winner is None else st.winner
    return GameOutcome(spec.index, spec.challenger, winner, st.turn, st.first_player, decisions)


def mirrored_specs(
    games: int,
    master_seed: int,
    challenger: AgentSpec,
    opponent: AgentSpec,
    decks: tuple[DeckList, DeckList],
) -> list[GameSpec]:
    """Blocks of four games on one game seed covering every (seat, deck) assignment: the
    challenger plays deck 0 and deck 1 from seat 0 and from seat 1, and the opponent gets
    the mirror image of each deal."""
    specs = []
    for i in range(games):
        block, variant = divmod(i, MIRROR_BLOCK)
        swap_decks, challenger_seat = divmod(variant, 2)
        deck_pair = (decks[1], decks[0]) if swap_decks else decks
        agents = (challenger, opponent) if challenger_seat == 0 else (opponent, challenger)
        game_seed = derive_seed(master_seed, "block", block, "decks", swap_decks)
        agent_seeds = (
            derive_seed(master_seed, "agent", i, 0),
            derive_seed(master_seed, "agent", i, 1),
        )
        specs.append(GameSpec(i, game_seed, agent_seeds, agents, deck_pair, challenger_seat))
    return specs


def play_all(specs: Sequence[GameSpec], workers: int) -> list[GameOutcome]:
    """Play every game, in index order, using ``workers`` processes (1 = in this process)."""
    if workers <= 1:
        return [play(s) for s in specs]
    with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn")) as pool:
        return list(pool.map(play, specs, chunksize=1))
