"""Canonical move keys for the search tree.

Two legal actions are the same move when they differ only in which copy of a card in a
private or public pile is used (two copies of one card in hand) or in how many EX Resources
pay for them. Keys name hand, trash, and other off-field cards by card definition (as known to
the deciding player) and field cards by instance, so a key means the same move in every
determinization of the decider's information set.
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable
from typing import Any

from gcg_sim.ai.base import uid_argument
from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.engine import view as V
from gcg_sim.engine.battle import declare_attack
from gcg_sim.engine.game import (
    ALT_PLAY_BASE,
    _activated_list,
    _command_playable,
    action_step_options,
)
from gcg_sim.engine.observe import determinize
from gcg_sim.engine.state import Action, Decision, GameState
from gcg_sim.engine.types import NO_ARG, ActionKind, DecisionKind, Zone

A = ActionKind
Key = tuple[str, int, int, int]

_BY_DEFINITION = frozenset(
    {Zone.HAND, Zone.TRASH, Zone.DECK, Zone.REMOVAL, Zone.SHIELD, Zone.RESOURCE_DECK}
)
_DEF_OFFSET = 1 << 24
_PAID_KINDS = frozenset({A.PLAY_UNIT, A.PLAY_BASE, A.PAIR, A.PLAY_COMMAND, A.ACTIVATE})


def ex_used(action: Action) -> int:
    """EX Resources an action spends (0 for actions without a payment)."""
    if action.kind not in _PAID_KINDS or action.c < 0:
        return 0
    return action.c % ALT_PLAY_BASE


def _ident(st: GameState, uid: int, decider: int) -> int:
    if not 0 <= uid < len(st.cards):
        return uid
    c = st.cards[uid]
    if c.zone in _BY_DEFINITION and c.known & (1 << decider):
        return _DEF_OFFSET + c.def_id
    return uid


def action_key(st: GameState, dec: Decision, action: Action) -> Key:
    k = action.kind
    p = dec.player
    if k is A.SELECT:
        a = _ident(st, action.a, p) if uid_argument(dec) else action.a
        return (k.value, a, action.b, NO_ARG)
    if k in _PAID_KINDS:
        variant = action.c // ALT_PLAY_BASE if action.c >= 0 else NO_ARG
        b = action.b if k is A.ACTIVATE else _ident(st, action.b, p)
        return (k.value, _ident(st, action.a, p), b, variant)
    if k in (A.ATTACK, A.BLOCK):
        return (k.value, action.a, action.b, NO_ARG)
    return (k.value, action.a, action.b, action.c)


def canonical(st: GameState, dec: Decision) -> list[tuple[Key, Action]]:
    """One representative action per distinct move, in option order; the representative
    spends the fewest EX Resources."""
    best: dict[Key, Action] = {}
    order: list[Key] = []
    for action in dec.options:
        key = action_key(st, dec, action)
        cur = best.get(key)
        if cur is None:
            best[key] = action
            order.append(key)
        elif ex_used(action) < ex_used(cur):
            best[key] = action
    return [(key, best[key]) for key in order]


_ADD_SELF_TO_HAND = (d.AddToHand(d.ThisCard()),)


def _free_card_burst(st: GameState) -> bool:
    """The resolving 【Burst】 only adds its own card to the hand: accepting costs nothing."""
    if not st.frames:
        return False
    uid = st.frames[-1].card_uid
    if uid < 0:
        return False
    R = V.reg()
    entry = R.cards[st.cards[uid].def_id]
    if entry.burst_aid < 0:
        return False
    burst = R.abilities[entry.burst_aid].ability
    return isinstance(burst, d.Burst) and burst.steps == _ADD_SELF_TO_HAND


def _attack_can_matter(st: GameState, player: int, attacker: int) -> bool:
    """A Unit with AP 0 or less deals no battle damage (5-5-5); its attack can still matter
    through attack-triggered effects or an AP boost played in the battle's action step."""
    dv = V.derived(st)
    if V.ap_of(st, dv, attacker) > 0:
        return True
    if any(
        st.cards[host].owner == player for host, _ in V.trigger_index(st, dv).get(d.Ev.ATTACKS, ())
    ):
        return True
    if _activated_list(st, player, d.Timing.ACTION):
        return True
    return any(
        _command_playable(st, player, uid, d.Timing.ACTION) for uid in st.zones[player][Zone.HAND]
    )


def _attack_matters(world: GameState, player: int, attacker: int, target: int) -> bool:
    """:func:`_attack_can_matter` judged by the engine itself, by declaring the attack in
    ``world``: an attack-triggered effect counts only if it triggers for the player (subject,
    target and condition), and a battle trick only if the player can play it in the battle's
    action step (level, cost and targets, e.g. "your Unit battling an enemy Unit")."""
    trial = world.clone()
    queued = len(trial.pending_triggers)
    declare_attack(trial, attacker, target)
    if any(t.controller == player for t in trial.pending_triggers[queued:]):
        return True
    return bool(action_step_options(trial, player))


_TRIAL_WORLD_SEED = 0


def _drop_idle_attacks(
    st: GameState, player: int, moves: list[tuple[Key, Action]]
) -> list[tuple[Key, Action]]:
    """``moves`` without the attacks by Units with AP 0 or less that :func:`_attack_matters`
    rejects, judged in one determinization so that the trial reads nothing hidden from the
    player."""
    dv = V.derived(st)
    zero = {key for key, a in moves if a.kind is A.ATTACK and V.ap_of(st, dv, a.a) <= 0}
    if not zero:
        return moves
    world = determinize(st, player, _TRIAL_WORLD_SEED)
    return [m for m in moves if m[0] not in zero or _attack_matters(world, player, m[1].a, m[1].b)]


def _contains(node: object, found: Callable[[object], bool]) -> bool:
    if found(node):
        return True
    if isinstance(node, tuple):
        return any(_contains(x, found) for x in node)
    if dataclasses.is_dataclass(node) and not isinstance(node, type):
        return any(_contains(getattr(node, f.name), found) for f in dataclasses.fields(node))
    return False


def _reads_did(node: object) -> bool:
    return isinstance(node, (pr.JumpIfNotDid, d.DidLast))


_rest_only: dict[tuple[int, int], bool] = {}


def _chosen_only_to_rest(st: GameState) -> bool:
    """The waiting choice is the player's pick of enemy cards for its own effect, the effect
    does nothing with them but rest them (no later "it gets ..." or "deal ... to it"), and
    whether it rested something changes nothing: no "If you do" follows, and a 【Once per
    Turn】 effect that has done nothing yet would not stay unused."""
    if not st.frames or not st.frames[-1].waiting:
        return False
    f = st.frames[-1]
    if f.once_key and not f.acted:
        return False
    key = (f.program_id, f.pc)
    cached = _rest_only.get(key)
    if cached is None:
        instrs = V.reg().programs[f.program_id].instrs
        ins = instrs[f.pc]
        cached = False
        if isinstance(ins, d.Choose) and ins.sel.side is d.Side.ENEMY and ins.chooser is d.P.YOU:
            var = ins.var
            uses = [
                pc
                for pc in range(f.pc + 1, len(instrs))
                if _contains(instrs[pc], lambda n: isinstance(n, d.Var) and n.name == var)
            ]
            cached = (
                len(uses) == 1
                and isinstance(instrs[uses[0]], d.Rest)
                and not _contains(instrs[uses[0] + 1 :], _reads_did)
            )
        _rest_only[key] = cached
    return cached


_IN_PLAY = frozenset({Zone.BATTLE, Zone.BASE, Zone.PAIRED})


def _resting_can_trigger(st: GameState) -> bool:
    """A card in play could react to a Unit being rested (e.g. "when one of your Units is
    rested by one of your opponent's effects"), or a delayed effect is waiting."""
    if st.delayed:
        return True
    hosts = V.trigger_index(st, V.derived(st)).get(d.Ev.RESTED, ())
    return any(st.cards[host].zone in _IN_PLAY for host, _ in hosts)


def _resting_changes_more_than_the_unit(st: GameState, dec: Decision) -> bool:
    """Resting one of the active enemy Units on offer switches something on ("while this Unit
    is rested, friendly Shields can't receive battle damage"): in a determinization (which reads
    nothing hidden), the derived characteristics of some card change beyond its rested state.
    Resting an already-rested Unit then is not the same as doing nothing."""
    world = determinize(st, dec.player, _TRIAL_WORLD_SEED)
    before = V.derived(world)
    for o in dec.options:
        if o.kind is not A.SELECT or not 0 <= o.a < len(world.cards):
            continue
        c = world.cards[o.a]
        if c.owner == dec.player or c.zone is not Zone.BATTLE or c.rested:
            continue
        rested = world.clone()
        rested.cards[o.a].rested = True
        rested.touch()
        if not V.derived(rested).same_values(before):
            return True
    return False


def _rests_a_rested_enemy(st: GameState, dec: Decision, action: Action) -> bool:
    if action.kind is not A.SELECT or not 0 <= action.a < len(st.cards):
        return False
    c = st.cards[action.a]
    return c.owner != dec.player and c.zone is Zone.BATTLE and c.rested


def prune_dominated(
    st: GameState,
    dec: Decision,
    moves: list[tuple[Key, Action]],
    *,
    strict_attacks: bool = False,
    rest_targets: bool = False,
) -> list[tuple[Key, Action]]:
    """Drop moves that can only be worse than another legal move: declining a free
    "【Burst】Add this card to your hand", and attacking with a Unit whose attack cannot deal
    damage or trigger anything (``strict_attacks`` asks the engine whether it can). With
    ``rest_targets``, an effect that only rests the enemy Units it chooses does not choose one
    that is already rested (resting it changes nothing) unless a card in play reacts to Units
    being rested or being rested switches on an effect. At least one move is always kept."""
    if dec.kind is DecisionKind.BURST and _free_card_burst(st):
        kept = [m for m in moves if m[1].kind is A.YES]
    elif (
        rest_targets
        and dec.kind is DecisionKind.SELECT
        and uid_argument(dec)
        and _chosen_only_to_rest(st)
        and not _resting_can_trigger(st)
        and not _resting_changes_more_than_the_unit(st, dec)
    ):
        kept = [m for m in moves if not _rests_a_rested_enemy(st, dec, m[1])]
    elif dec.kind is DecisionKind.MAIN and strict_attacks:
        kept = _drop_idle_attacks(st, dec.player, moves)
    elif dec.kind is DecisionKind.MAIN:
        kept = [
            m
            for m in moves
            if m[1].kind is not A.ATTACK or _attack_can_matter(st, dec.player, m[1].a)
        ]
    else:
        return moves
    return kept or moves


_FIELD = frozenset({Zone.BATTLE, Zone.BASE})
_SELF = -(1 << 40)


def _in_flight(st: GameState, uid: int) -> bool:
    """A battle, a resolving effect or a waiting trigger names the card."""
    if any(uid in (b.attacker, b.target) for b in st.battles):
        return True
    for f in st.frames:
        if uid in (f.host, f.card_uid) or uid in f.buf or any(v == uid for _, v in f.event):
            return True
        if any(uid in chosen for chosen in f.vars.values()):
            return True
    triggers = [*st.pending_triggers, *(t for batch in st.batches for t in batch)]
    return any(uid in (t.host, t.card_uid) or any(v == uid for _, v in t.event) for t in triggers)


def _references(st: GameState, uid: int) -> tuple[Any, ...]:
    """What this turn's history, once-per-turn records and lasting or delayed effects say
    about the card, with the card itself written as ``_SELF``."""
    history = tuple(
        (e.kind, e.player, e.by, e.uid == uid, e.source == uid)
        for e in st.history
        if uid in (e.uid, e.source)
    )
    once = tuple(
        sorted(tuple(_SELF if x == uid else x for x in key) for key in st.once_used if uid in key)
    )
    seq = st.cards[uid].zone_seq
    lasting = tuple(
        sorted(
            (
                le.effect_key,
                le.controller,
                le.duration,
                le.created_turn,
                le.battle_id,
                le.player,
                le.uses,
                le.filters_key,
                le.aux,
                (uid, seq) in le.targets,
                le.source_uid == uid,
            )
            for le in (*st.lasting, *st.delayed)
            if (uid, seq) in le.targets or le.source_uid == uid
        )
    )
    return history, once, lasting


def _fingerprint(st: GameState, dv: V.Derived, uid: int) -> tuple[Any, ...]:
    """The public state of a field card, without its instance number: two copies of a Unit
    with equal fingerprints are interchangeable. Any other card stays itself."""
    if not 0 <= uid < len(st.cards):
        return ("arg", uid)
    c = st.cards[uid]
    pilot = c.pair
    if c.zone not in _FIELD or _in_flight(st, uid) or (pilot >= 0 and _in_flight(st, pilot)):
        return ("card", uid)
    paired = (st.cards[pilot].def_id, _references(st, pilot)) if pilot >= 0 else ()
    return (
        "field",
        c.owner,
        c.def_id,
        c.zone,
        c.rested,
        c.damage,
        c.entered_turn == st.turn,
        V.ap_of(st, dv, uid),
        V.hp_of(st, dv, uid),
        tuple(sorted((k.value, n) for k, n in dv.kw.get(uid, {}).items())),
        tuple(sorted(dv.traits.get(uid, ()))),
        uid in dv.linked,
        tuple(sorted(a.aid for a in dv.abilities.get(uid, ()))),
        tuple(sorted((repr(r.rule), r.controller, r.source == uid) for r in dv.rules.get(uid, ()))),
        _references(st, uid),
        paired,
    )


def _public_move(st: GameState, dv: V.Derived, dec: Decision, key: Key, action: Action) -> Any:
    k = action.kind
    if k is A.ATTACK:
        return (k.value, _fingerprint(st, dv, action.a), _fingerprint(st, dv, action.b))
    if k is A.BLOCK:
        return (k.value, _fingerprint(st, dv, action.a), action.b)
    if k is A.SELECT and uid_argument(dec):
        return (k.value, _fingerprint(st, dv, action.a), action.b)
    if k is A.PAIR:
        return (k.value, key[1], _fingerprint(st, dv, action.b), key[3])
    return key


def merge_equivalent(
    st: GameState, dec: Decision, moves: list[tuple[Key, Action]]
) -> list[tuple[Key, Action]]:
    """Keep the first of every group of moves that differ only in which of two
    interchangeable field cards (equal :func:`_fingerprint`) they attack with, attack, block
    with, choose or pair onto, so the search does not split its visits between them."""
    dv = V.derived(st)
    seen: set[Any] = set()
    out: list[tuple[Key, Action]] = []
    for key, action in moves:
        public = _public_move(st, dv, dec, key, action)
        if public not in seen:
            seen.add(public)
            out.append((key, action))
    return out
