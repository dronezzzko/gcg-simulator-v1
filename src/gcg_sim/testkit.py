"""Scenario builder and decision helpers for rule, card, and AI tests.

A :class:`Scenario` places specific cards in specific zones and starts the game in the
active player's main phase, so a test can exercise one rule or card effect directly::

    sc = Scenario(active=0)
    gundam = sc.add(0, "GD01-001", Zone.HAND)
    sc.resources(0, 5)
    st = sc.start()
    play(st, gundam)

Helpers find the matching legal action and apply it, raising ``AssertionError`` with the list
of legal options when no action matches, so a failing test explains itself.
"""

from __future__ import annotations

from collections.abc import Iterable, Sequence

from gcg_sim.cards.model import CardType
from gcg_sim.effects.registry import get_registry
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.game import IllegalActionError, advance, apply
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, ActionKind, DecisionKind, Phase, Step, Zone

A = ActionKind
FILLER_UNIT = "GD01-060"  # vanilla Lv2 Unit used to fill decks
FILLER_RESOURCE = "R-001"


class Scenario:
    """Build a mid-game state directly (bypassing setup) for focused tests."""

    def __init__(
        self, *, active: int = 0, turn: int = 3, seed: int = 0, deck_size: int = 20
    ) -> None:
        self.st = GameState(seed)
        self.st.turn = turn
        self.st.active = active
        self.st.first_player = active if turn % 2 == 1 else 1 - active
        self.db = get_registry().db
        self._deck_size = deck_size
        self._decks: list[list[str]] = [[], []]
        self._custom_deck = [False, False]

    # -- building ---------------------------------------------------------------------------
    def add(
        self,
        player: int,
        number: str,
        zone: Zone = Zone.BATTLE,
        *,
        rested: bool = False,
        damage: int = 0,
        pilot: str | None = None,
        deployed_this_turn: bool = False,
        known: bool = True,
    ) -> int:
        """Put a card into a zone. ``pilot`` pairs a Pilot card (by number) with a Unit."""
        cdef = self.db[number]
        get_registry().require(cdef.def_id)
        uid = core.new_card(self.st, cdef.def_id, player, zone, rested=rested)
        c = self.st.cards[uid]
        c.damage = damage
        c.entered_turn = self.st.turn if deployed_this_turn else self.st.turn - 2
        c.known = core.BOTH_KNOW if known or zone.is_public else (1 << player)
        if zone is Zone.HAND:
            c.known = core.BOTH_KNOW if known else (1 << player)
        if pilot is not None:
            p = self.add(player, pilot, Zone.PAIRED)
            self.st.cards[p].pair = uid
            c.pair = p
        self.st.touch()
        return uid

    def hand(self, player: int, *numbers: str) -> list[int]:
        return [self.add(player, n, Zone.HAND) for n in numbers]

    def trash(self, player: int, *numbers: str) -> list[int]:
        return [self.add(player, n, Zone.TRASH) for n in numbers]

    def shields(self, player: int, *numbers: str) -> list[int]:
        """Add Shields (first number = top Shield)."""
        out = []
        for n in numbers:
            uid = self.add(player, n, Zone.SHIELD, known=False)
            self.st.cards[uid].known = 0
            out.append(uid)
        return out

    def deck(self, player: int, *numbers: str) -> None:
        """Set the deck contents (first number = top card); filler is appended below."""
        self._decks[player] = list(numbers)
        self._custom_deck[player] = True

    def resources(self, player: int, n: int, *, rested: int = 0, ex: int = 0) -> list[int]:
        out = []
        for i in range(n):
            out.append(self.add(player, FILLER_RESOURCE, Zone.RESOURCE_AREA, rested=i < rested))
        for _ in range(ex):
            out.append(self.add(player, self.db.ex_resource.card_number, Zone.RESOURCE_AREA))
        return out

    def base(self, player: int, number: str | None = None, *, damage: int = 0) -> int:
        n = number or self.db.ex_base.card_number
        return self.add(player, n, Zone.BASE, damage=damage)

    def resource_deck(self, player: int, n: int) -> None:
        for _ in range(n):
            self.add(player, FILLER_RESOURCE, Zone.RESOURCE_DECK, known=False)

    # -- starting ---------------------------------------------------------------------------
    def _build_decks(self) -> None:
        for p in (0, 1):
            numbers = list(self._decks[p])
            filler = (
                max(0, self._deck_size - len(numbers))
                if not self._custom_deck[p]
                else self._deck_size
            )
            numbers += [FILLER_UNIT] * filler
            for n in numbers:
                uid = core.new_card(self.st, self.db[n].def_id, p, Zone.DECK)
                self.st.cards[uid].known = 0
        decks: list[list[int]] = [[], []]
        res: list[list[int]] = [[], []]
        for c in self.st.cards:
            cd = self.db.by_id(c.def_id)
            if cd.is_token:
                continue
            if cd.card_type is CardType.RESOURCE:
                res[c.owner].append(c.def_id)
            else:
                decks[c.owner].append(c.def_id)
        self.st.decklists = (tuple(sorted(decks[0])), tuple(sorted(decks[1])))
        self.st.resource_decklists = (tuple(sorted(res[0])), tuple(sorted(res[1])))

    def start(self, step: Step = Step.MAIN) -> GameState:
        """Begin at ``step`` of the active player's turn (default: main phase) and advance to
        the first decision."""
        self._build_decks()
        st = self.st
        st.phase = {
            Step.ACTIVE_STEP: Phase.START,
            Step.START_STEP: Phase.START,
            Step.DRAW_STEP: Phase.DRAW,
            Step.RESOURCE_STEP: Phase.RESOURCE,
            Step.MAIN: Phase.MAIN,
            Step.END_ACTION: Phase.END,
            Step.END_STEP: Phase.END,
            Step.HAND_STEP: Phase.END,
        }[step]
        st.step = step
        if step is Step.END_ACTION:
            st.priority = st.standby
            st.passes = 0
        st.touch()
        advance(st)
        return st


# ---------------------------------------------------------------------------------------------
# decision helpers


def options(st: GameState) -> tuple[Action, ...]:
    return st.pending.options if st.pending else ()


def _fail(st: GameState, want: str) -> AssertionError:
    dec = st.pending
    opts = ", ".join(str(o) for o in (dec.options if dec else ()))
    kind = dec.kind.value if dec else "none"
    return AssertionError(
        f"no legal action {want}; pending={kind} options=[{opts}] winner={st.winner}"
    )


def act(
    st: GameState,
    kind: ActionKind,
    a: int | None = None,
    b: int | None = None,
    c: int | None = None,
) -> Action:
    """Apply the first legal action matching the given fields (``None`` = any)."""
    for o in options(st):
        if (
            o.kind is kind
            and (a is None or o.a == a)
            and (b is None or o.b == b)
            and (c is None or o.c == c)
        ):
            apply(st, o)
            return o
    raise _fail(st, f"{kind.value}(a={a}, b={b}, c={c})")


def has_action(st: GameState, kind: ActionKind, a: int | None = None, b: int | None = None) -> bool:
    return any(
        o.kind is kind and (a is None or o.a == a) and (b is None or o.b == b) for o in options(st)
    )


def play(st: GameState, uid: int, *, onto: int | None = None, ex: int = 0) -> Action:
    """Play a card from hand in the main phase (Unit/Base/Command, or pair a Pilot ``onto`` a Unit)."""
    cd = V.cdef(st, uid)
    if onto is not None:
        return act(st, A.PAIR, uid, onto, ex)
    if cd.card_type is CardType.UNIT:
        return act(st, A.PLAY_UNIT, uid, None, ex)
    if cd.card_type is CardType.BASE:
        return act(st, A.PLAY_BASE, uid, None, ex)
    return act(st, A.PLAY_COMMAND, uid, None, ex)


def activate(st: GameState, host: int, ability: int | None = None, ex: int = 0) -> Action:
    return act(st, A.ACTIVATE, host, ability, ex)


def attack(st: GameState, attacker: int, target: int = PLAYER_TARGET) -> Action:
    return act(st, A.ATTACK, attacker, target)


def end_main(st: GameState) -> Action:
    return act(st, A.END_MAIN)


def block(st: GameState, blocker: int | None) -> Action:
    if blocker is None:
        return act(st, A.NO_BLOCK)
    return act(st, A.BLOCK, blocker)


def pass_(st: GameState) -> Action:
    return act(st, A.PASS)


def yes(st: GameState) -> Action:
    return act(st, A.YES)


def no(st: GameState) -> Action:
    return act(st, A.NO)


def select(st: GameState, *uids: int, done: bool | None = None) -> None:
    """Pick cards for a pending SELECT/DISCARD/EXCESS decision; finish with DONE when offered
    and ``done`` is not False."""
    for u in uids:
        act(st, A.SELECT, u)
    if (
        done is not False
        and st.pending is not None
        and has_action(st, A.DONE)
        and (done or st.pending.kind is DecisionKind.SELECT)
    ):
        act(st, A.DONE)


def choose_option(st: GameState, index: int) -> Action:
    return act(st, A.SELECT, index)


def order(st: GameState, index: int = 0) -> Action:
    return act(st, A.ORDER, options(st)[index].a)


def pass_all(st: GameState, *, max_steps: int = 50) -> None:
    """Pass every action-step priority and decline optional blocks until a non-action-step
    decision (or the end of the game) is reached."""
    for _ in range(max_steps):
        dec = st.pending
        if dec is None:
            return
        if dec.kind is DecisionKind.ACTION_STEP:
            act(st, A.PASS)
        elif dec.kind is DecisionKind.BLOCK:
            act(st, A.NO_BLOCK)
        else:
            return


def to_next_turn(st: GameState) -> None:
    """End the current main phase and pass through the end phase to the next turn's main phase."""
    if st.pending is not None and st.pending.kind is DecisionKind.MAIN:
        end_main(st)
    for _ in range(200):
        dec = st.pending
        if dec is None:
            return
        if dec.kind is DecisionKind.MAIN:
            return
        if dec.kind in (DecisionKind.ACTION_STEP,):
            act(st, A.PASS)
        elif dec.kind is DecisionKind.BLOCK:
            act(st, A.NO_BLOCK)
        elif dec.kind is DecisionKind.DISCARD:
            act(st, A.SELECT, dec.options[0].a)
        else:
            raise _fail(st, "to reach the next main phase")


def zone_of(st: GameState, uid: int) -> Zone:
    return st.cards[uid].zone


def ap(st: GameState, uid: int) -> int:
    return V.ap_of(st, V.derived(st), uid)


def hp(st: GameState, uid: int) -> int:
    return V.hp_of(st, V.derived(st), uid)


def keywords(st: GameState, uid: int) -> dict[str, int]:
    return {k.value: v for k, v in V.keywords(V.derived(st), uid).items()}


def uids_in(st: GameState, player: int, zone: Zone) -> list[int]:
    return list(st.zones[player][zone])


def numbers_in(st: GameState, player: int, zone: Zone) -> list[str]:
    return [V.cdef(st, u).card_number for u in st.zones[player][zone]]


def legal_kinds(st: GameState) -> set[ActionKind]:
    return {o.kind for o in options(st)}


def expect_illegal(st: GameState, action: Action) -> None:
    try:
        apply(st, action)
    except IllegalActionError:
        return
    raise AssertionError(f"{action} was accepted but should be illegal")


def run_random(st: GameState, seed: int, *, max_actions: int = 5000) -> None:
    """Play uniformly random legal actions to the end of the game (for smoke checks)."""
    from gcg_sim.rng import SplitMix64

    rng = SplitMix64(seed)
    for _ in range(max_actions):
        if st.pending is None:
            return
        opts = st.pending.options
        apply(st, opts[rng.randrange(len(opts))])


def names(st: GameState, uids: Iterable[int]) -> list[str]:
    return [V.cdef(st, u).name for u in uids]


def card_numbers(st: GameState, uids: Sequence[int]) -> list[str]:
    return [V.cdef(st, u).card_number for u in uids]


__all__ = ["NO_ARG", "PLAYER_TARGET", "Scenario", "Zone"]
