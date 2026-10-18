"""Detect spending moves that change nothing.

A Command play or an ability activation is "without effect" when it changes nothing but what it
spent. In one determinization of the player's information set, the move is played and
resolved, and the deterministic policy continues to the next turn while its actions are
recorded. The same actions are then replayed in the same world without the move. The move is
without effect when every recorded action was also legal without it, and both lines end in the
same position apart from the card and Resources spent and the player's own cards rested to pay
for it (the line without the move passes the extra action steps it gets from still holding
the card). Examples: AP-3 "during this turn" on an enemy Unit that battles no more this turn,
"rest it" on a Unit that is already rested, or a cost paid for an effect that does nothing.

The deterministic policy makes the choices inside the move's resolution, so a move can look
idle only because the policy picked an idle target (resting a rested Unit while an active one
is legal). With ``branch_choices`` a flagged move is re-checked once per option of the
player's first choice inside its resolution, and kept when some option leads to a different
position that the evaluation scores above idling.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from gcg_sim.ai import policy
from gcg_sim.ai.actions import Key
from gcg_sim.ai.evaluation import DEFAULT_WEIGHTS, Weights, burst_density, evaluate
from gcg_sim.ai.policy import BASELINE_KNOBS, PolicyKnobs
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone

A = ActionKind
SPENDING = frozenset({A.PLAY_COMMAND, A.ACTIVATE})
TOP_LEVEL = frozenset({DecisionKind.MAIN, DecisionKind.ACTION_STEP})
MAX_DECISIONS = 400
PASS = Action(ActionKind.PASS)
_SPENT_ZONES = frozenset({Zone.RESOURCE_AREA, Zone.OUTSIDE})


def _signature(st: GameState, spent: int) -> tuple[Any, ...]:
    cards = tuple(
        (c.uid, c.zone, c.rested, c.damage, c.pair, c.known)
        for c in st.cards
        if c.uid != spent and c.zone not in _SPENT_ZONES
    )
    lasting = tuple(
        (le.effect_key, le.targets, le.duration, le.player) for le in (*st.lasting, *st.delayed)
    )
    decks = tuple(tuple(st.zones[p][Zone.DECK]) for p in (0, 1))
    return (st.winner, st.turn, st.active, cards, lasting, decks)


def _policy_step(st: GameState, knobs: PolicyKnobs) -> Action:
    return policy.choose(st, None, 0.0, knobs)


def _resolving(st: GameState) -> bool:
    return (
        st.winner is None
        and st.pending is not None
        and (bool(st.frames) or st.pending.kind not in TOP_LEVEL)
    )


def _resolve(st: GameState, action: Action, knobs: PolicyKnobs) -> None:
    """Apply ``action`` and let the policy make the choices of its resolution (targets,
    optional parts, triggered effects) until play is back at a top-level decision."""
    apply(st, action, check=False)
    n = 0
    while _resolving(st) and n < MAX_DECISIONS:
        apply(st, _policy_step(st, knobs), check=False)
        n += 1


def _branches(world: GameState, action: Action, player: int, knobs: PolicyKnobs) -> list[GameState]:
    """``action`` resolved once per option of the player's first choice between several
    targets or modes inside its resolution (the policy makes every other choice); empty when
    the resolution asks no such choice."""
    st = world.clone()
    apply(st, action, check=False)
    n = 0
    while _resolving(st) and n < MAX_DECISIONS:
        dec = st.pending
        assert dec is not None
        if dec.kind is DecisionKind.SELECT and dec.player == player and len(dec.options) > 1:
            out = []
            for option in dec.options:
                branch = st.clone()
                _resolve(branch, option, knobs)
                out.append(branch)
            return out
        apply(st, _policy_step(st, knobs), check=False)
        n += 1
    return []


def _continue(st: GameState, turn: int, knobs: PolicyKnobs) -> list[Action]:
    actions: list[Action] = []
    while (st.winner is None and st.turn < turn and st.pending is not None) and len(
        actions
    ) < MAX_DECISIONS:
        action = _policy_step(st, knobs)
        apply(st, action, check=False)
        actions.append(action)
    return actions


def _replay(st: GameState, actions: list[Action], turn: int, knobs: PolicyKnobs) -> bool:
    """Apply ``actions`` in order, passing extra action steps; False as soon as a recorded
    action cannot be taken (the lines diverged)."""
    queue = list(actions)
    n = 0
    while st.winner is None and st.turn < turn and st.pending is not None and n < MAX_DECISIONS:
        n += 1
        if queue and queue[0] in st.pending.options:
            apply(st, queue.pop(0), check=False)
        elif st.pending.kind is DecisionKind.ACTION_STEP and PASS in st.pending.options:
            apply(st, PASS, check=False)
        elif queue:
            return False
        else:
            apply(st, _policy_step(st, knobs), check=False)
    return not queue


def without_effect(
    st: GameState,
    player: int,
    dec: Decision,
    moves: list[tuple[Key, Action]],
    seed: int,
    knobs: PolicyKnobs = BASELINE_KNOBS,
    *,
    branch_choices: bool = False,
    resource_aware: bool = False,
    weights: Weights = DEFAULT_WEIGHTS,
) -> set[Key]:
    """Keys of the spending moves in ``moves`` that change nothing but what they spend.

    ``branch_choices`` keeps a move when some option of its first choice reaches a different
    position that ``weights`` score above idling (see the module docstring).
    ``resource_aware`` also compares the resource areas: the move's line may only have paid
    (own Resources gone or rested), so placing an EX Resource or setting Resources active,
    or touching the opponent's Resources, counts as an effect."""
    spending = [(key, a) for key, a in moves if a.kind in SPENDING]
    if dec.kind not in TOP_LEVEL or not spending:
        return set()
    world = determinize(st, player, seed)
    lines = _Lines(world, player, world.turn + 1, knobs, resource_aware)
    out: set[Key] = set()
    density: tuple[float, float] | None = None
    for key, action in spending:
        played = world.clone()
        _resolve(played, action, knobs)
        spent = action.a if action.kind is A.PLAY_COMMAND else -1
        if lines.idle_line(played, spent) is None:
            continue
        if branch_choices:
            if density is None:
                density = burst_density(st, player)
            if _a_branch_beats_idling(lines, action, spent, weights, density):
                continue
        out.add(key)
    return out


@dataclass(frozen=True, slots=True)
class _Lines:
    """Compares a line in which the move was played with idling in the same world."""

    world: GameState
    player: int
    turn: int
    knobs: PolicyKnobs
    resource_aware: bool

    def continue_and_replay(self, line: GameState) -> GameState | None:
        """Continue ``line`` to the next turn; the world without the move with the same
        actions replayed, or None when they cannot all be taken (the lines diverged)."""
        actions = _continue(line, self.turn, self.knobs)
        idled = self.world.clone()
        return idled if _replay(idled, actions, self.turn, self.knobs) else None

    def same(self, line: GameState, idled: GameState, spent: int) -> bool:
        return _only_costs_differ(line, idled, spent, self.player) and (
            not self.resource_aware or _resources_only_paid(line, idled, self.player)
        )

    def idle_line(self, line: GameState, spent: int) -> GameState | None:
        """The idle line when ``line`` ends in the same position apart from costs."""
        idled = self.continue_and_replay(line)
        return idled if idled is not None and self.same(line, idled, spent) else None


def _a_branch_beats_idling(
    lines: _Lines, action: Action, spent: int, weights: Weights, density: tuple[float, float]
) -> bool:
    player = lines.player
    for line in _branches(lines.world, action, player, lines.knobs):
        idled = lines.continue_and_replay(line)
        if idled is None:
            idled = lines.world.clone()
            _continue(idled, lines.turn, lines.knobs)
        elif lines.same(line, idled, spent):
            continue
        if evaluate(line, player, weights, density) > evaluate(idled, player, weights, density):
            return True
    return False


def _only_costs_differ(played: GameState, idled: GameState, spent: int, player: int) -> bool:
    """The lines differ at most in the player's own cards being rested: the price of an
    activation (e.g. "Rest 1 of your Units:") whose effect then did nothing."""
    a, b = _signature(played, spent), _signature(idled, spent)
    if a[:3] != b[:3] or a[4:] != b[4:] or len(a[3]) != len(b[3]):
        return False
    for x, y in zip(a[3], b[3], strict=True):
        if x == y:
            continue
        uid, zone, rested, *rest = x
        if uid != y[0] or zone != y[1] or rest != list(y[3:]):
            return False
        if played.cards[uid].owner != player or not rested or y[2]:
            return False
    return True


def _resources(st: GameState, player: int) -> tuple[frozenset[int], int]:
    area = st.zones[player][Zone.RESOURCE_AREA]
    return frozenset(area), sum(1 for uid in area if not st.cards[uid].rested)


def _resources_only_paid(played: GameState, idled: GameState, player: int) -> bool:
    """The move's line has no Resource the idle line lacks and no more active ones, and the
    opponent's resource area is unchanged."""
    mine, mine_idle = _resources(played, player), _resources(idled, player)
    theirs, theirs_idle = _resources(played, 1 - player), _resources(idled, 1 - player)
    return mine[0] <= mine_idle[0] and mine[1] <= mine_idle[1] and theirs == theirs_idle
