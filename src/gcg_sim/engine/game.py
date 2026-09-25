"""Game procedure: setup (rule 6), turn structure (7), battles (8), action steps (9),
effect activation (10), and the public engine API: :func:`new_game`, :func:`legal_actions`,
:func:`apply`.
"""

from __future__ import annotations

from dataclasses import dataclass

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import AbilityEntry
from gcg_sim.engine import battle as B
from gcg_sim.engine import core
from gcg_sim.engine import interp as I
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Action, Decision, GameState, TriggerInst
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

A = ActionKind
SUPPORT_AID = -10
ALT_PLAY_BASE = 100  # Action.c = EX used + ALT_PLAY_BASE * (modifier index + 1)
MAX_ACTIONS_DEFAULT = 20000


class IllegalActionError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class DeckList:
    """A deck (50 card numbers) and a resource deck (10 card numbers), rule 6-1-1."""

    main: tuple[str, ...]
    resources: tuple[str, ...]


class DeckConstructionError(ValueError):
    """A deck or resource deck breaks the construction rules (rule 6-1)."""


MAIN_DECK_SIZE = 50
RESOURCE_DECK_SIZE = 10
MAX_COPIES = 4
MAX_COLORS = 2


def deck_construction_problems(deck: DeckList) -> list[str]:
    """Rules 6-1-1..6-1-1-5 (engine-level check; B&R lists are enforced by gcg_sim.deck)."""
    from collections import Counter

    from gcg_sim.cards.model import MAIN_DECK_TYPES

    db = V.reg().db
    problems: list[str] = []
    if len(deck.main) != MAIN_DECK_SIZE:
        problems.append(
            f"deck has {len(deck.main)} cards; exactly {MAIN_DECK_SIZE} required (6-1-1)"
        )
    if len(deck.resources) != RESOURCE_DECK_SIZE:
        problems.append(
            f"resource deck has {len(deck.resources)} cards; exactly {RESOURCE_DECK_SIZE} required (6-1-1)"
        )
    colors = set()
    for number, n in sorted(Counter(deck.main).items()):
        cd = db.get(number)
        if cd is None:
            problems.append(f"unknown card {number}")
            continue
        if cd.card_type not in MAIN_DECK_TYPES:
            problems.append(
                f"{number} is a {cd.card_type.value} card and cannot be in the deck (6-1-1-1)"
            )
        if n > MAX_COPIES:
            problems.append(f"{n} copies of {number}; at most {MAX_COPIES} allowed (6-1-1-3)")
        if cd.color is not None:
            colors.add(cd.color)
    if len(colors) > MAX_COLORS:
        names = ", ".join(sorted(c.value for c in colors))
        problems.append(
            f"deck uses {len(colors)} colours ({names}); at most {MAX_COLORS} (6-1-1-2)"
        )
    for number in sorted(set(deck.resources)):
        cd = db.get(number)
        if cd is None or cd.card_type is not CardType.RESOURCE:
            problems.append(
                f"{number} is not a Resource card and cannot be in the resource deck (6-1-1-4)"
            )
    return problems


# ---------------------------------------------------------------------------------------------
# setup


def new_game(
    decks: tuple[DeckList, DeckList],
    seed: int,
    *,
    chooser: int | None = None,
    turn_limit: int = 200,
    max_actions: int = MAX_ACTIONS_DEFAULT,
    validate: bool = True,
) -> GameState:
    """Create a game. Each deck is shuffled with the seeded RNG and then cut at a random point
    (Floor Rules); card uids are assigned after shuffling so they carry no identity information.

    ``chooser`` is the player who decides who goes first (BO3: loser of the previous game);
    ``None`` means a seeded die roll decides (rule 6-2-1-4).
    """
    R = V.reg()
    if validate:
        for p, deck in enumerate(decks):
            problems = deck_construction_problems(deck)
            if problems:
                raise DeckConstructionError(f"player {p}: " + "; ".join(problems))
    st = GameState(seed)
    st.turn_limit = turn_limit
    rng: SplitMix64 = st.rng
    main_ids: list[list[int]] = []
    res_ids: list[list[int]] = []
    for p in (0, 1):
        ids = [R.db[n].def_id for n in decks[p].main]
        for i in ids:
            R.require(i)
        rng.shuffle(ids)
        if len(ids) > 1:
            k = 1 + rng.randrange(len(ids) - 1)
            ids = ids[k:] + ids[:k]
        main_ids.append(ids)
        res_ids.append([R.db[n].def_id for n in decks[p].resources])
    for p in (0, 1):
        for def_id in main_ids[p]:
            uid = core.new_card(st, def_id, p, Zone.DECK)
            st.cards[uid].known = 0
    for p in (0, 1):
        for def_id in res_ids[p]:
            uid = core.new_card(st, def_id, p, Zone.RESOURCE_DECK)
            st.cards[uid].known = 1 << p
    st.decklists = (tuple(sorted(main_ids[0])), tuple(sorted(main_ids[1])))
    st.resource_decklists = (tuple(sorted(res_ids[0])), tuple(sorted(res_ids[1])))
    st.setup_chooser = chooser if chooser is not None else rng.randrange(2)
    st.max_actions = max_actions
    st.phase = Phase.SETUP
    st.step = Step.SETUP_CHOOSE_FIRST
    advance(st)
    return st


def _setup_step(st: GameState) -> None:
    if st.step is Step.SETUP_CHOOSE_FIRST:
        st.pending = Decision(
            st.setup_chooser,
            DecisionKind.CHOOSE_FIRST,
            (Action(A.GO_FIRST, st.setup_chooser), Action(A.GO_FIRST, 1 - st.setup_chooser)),
            "choose Player One",
        )
    elif st.step is Step.SETUP_DRAW:
        for p in (st.first_player, 1 - st.first_player):
            for _ in range(core.START_HAND):
                uid = st.zones[p][Zone.DECK][0]
                core.move(st, uid, Zone.HAND)
        st.step = Step.SETUP_REDRAW_P1
    elif st.step in (Step.SETUP_REDRAW_P1, Step.SETUP_REDRAW_P2):
        p = st.first_player if st.step is Step.SETUP_REDRAW_P1 else 1 - st.first_player
        st.pending = Decision(p, DecisionKind.REDRAW, (Action(A.KEEP), Action(A.REDRAW)), "redraw?")
    elif st.step is Step.SETUP_SHIELDS:
        R = V.reg()
        for p in (st.first_player, 1 - st.first_player):
            for _ in range(core.SHIELD_COUNT):  # rule 6-2-2
                uid = st.zones[p][Zone.DECK][0]
                core.move(st, uid, Zone.SHIELD)
            core.new_card(st, R.db.ex_base.def_id, p, Zone.BASE)  # rule 6-2-3
        core.new_card(st, R.db.ex_resource.def_id, 1 - st.first_player, Zone.RESOURCE_AREA)  # 6-2-4
        st.turn = 1
        st.active = st.first_player  # rule 6-2-5
        st.phase = Phase.START
        st.step = Step.ACTIVE_STEP
        st.history = []
        st.touch()


def _redraw(st: GameState, p: int) -> None:
    """Rule 6-2-1-6-1: hand to bottom of deck, draw five, then shuffle."""
    for uid in list(st.zones[p][Zone.HAND]):
        core.move(st, uid, Zone.DECK, bottom=True)
    for _ in range(core.START_HAND):
        core.move(st, st.zones[p][Zone.DECK][0], Zone.HAND)
    core.shuffle_deck(st, p)
    st.redraws[p] = True


# ---------------------------------------------------------------------------------------------
# turn procedure


def _expire(st: GameState, *, battle_id: int = NO_ARG) -> None:
    def keep(le_duration: str, created: int, controller: int) -> bool:
        if le_duration in ("this_turn", "this_battle"):
            return False
        if le_duration == "opponent_next_turn":
            return not (st.turn > created and st.active != controller)
        if le_duration == "your_next_turn":
            return not (st.turn > created and st.active == controller)
        return True

    st.lasting = [le for le in st.lasting if keep(le.duration, le.created_turn, le.controller)]
    st.delayed = [le for le in st.delayed if keep(le.duration, le.created_turn, le.controller)]
    st.touch()


def _turn_step(st: GameState) -> None:
    s = st.step
    p = st.active
    if s is Step.ACTIVE_STEP:
        dv = V.derived(st)
        for z in (Zone.BATTLE, Zone.RESOURCE_AREA, Zone.BASE):  # rule 7-2-3-1
            for uid in st.zones[p][z]:
                c = st.cards[uid]
                if (
                    c.rested
                    and not V.rules_of(dv, uid, d.RuleKind.CANT_BE_SET_ACTIVE)
                    and not V.rules_of(dv, uid, d.RuleKind.STAYS_RESTED_IN_START_PHASE)
                ):
                    c.rested = False
        st.touch()
        st.step = Step.START_STEP
    elif s is Step.START_STEP:
        core.next_group(st)
        core.emit(st, d.Ev.TURN_START, NO_ARG, player=p)  # rule 7-2-4-1
        st.step = Step.START_STEP_DONE  # rule 7-2-2: its effects resolve before moving on
    elif s is Step.START_STEP_DONE:
        st.phase = Phase.DRAW  # rule 7-2-5
        st.step = Step.DRAW_STEP
    elif s is Step.DRAW_STEP:
        I.draw(st, p, 1)  # rule 7-3-1
        st.step = Step.DRAW_STEP_DONE
    elif s is Step.DRAW_STEP_DONE:
        st.phase = Phase.RESOURCE
        st.step = Step.RESOURCE_STEP
    elif s is Step.RESOURCE_STEP:
        I.place_resource(st, p, rested=False, by=p)  # rule 7-4-1
        st.step = Step.RESOURCE_STEP_DONE
    elif s is Step.RESOURCE_STEP_DONE:
        st.phase = Phase.MAIN
        st.step = Step.MAIN
    elif s is Step.MAIN:
        st.pending = Decision(p, DecisionKind.MAIN, tuple(main_options(st)), "main phase")
    elif s is Step.ATTACK_END_CHECK:
        b = st.battle
        assert b is not None
        st.step = Step.BATTLE_END if B.battle_broken(st, b) else Step.BLOCK
    elif s is Step.BLOCK:
        b = st.battle
        assert b is not None
        blockers = B.eligible_blockers(st, b)
        if not blockers:
            st.step = Step.BLOCK_END_CHECK
        else:
            opts = [Action(A.BLOCK, u) for u in blockers] + [Action(A.NO_BLOCK)]
            st.pending = Decision(b.defender, DecisionKind.BLOCK, tuple(opts), "block?")
    elif s is Step.BLOCK_END_CHECK:
        b = st.battle
        assert b is not None
        if B.battle_broken(st, b):
            st.step = Step.BATTLE_END
        else:
            st.step = Step.BATTLE_ACTION
            st.priority = st.standby
            st.passes = 0
    elif s is Step.BATTLE_ACTION or s is Step.END_ACTION:
        _action_step(st)
    elif s is Step.BATTLE_ACTION_END_CHECK:
        b = st.battle
        assert b is not None
        st.step = Step.BATTLE_END if B.battle_broken(st, b) else Step.DAMAGE
    elif s is Step.DAMAGE:
        st.step = Step.DAMAGE_DONE
        B.damage_step(st)
    elif s is Step.DAMAGE_DONE:
        st.step = Step.BATTLE_END
    elif s is Step.BATTLE_END:
        st.step = Step.BATTLE_END_DONE
        B.end_battle(st)
    elif s is Step.BATTLE_END_DONE:
        st.battles.pop()
        st.touch()
        st.step = Step.MAIN  # rule 8-6-2
    elif s is Step.END_STEP:
        core.next_group(st)
        core.emit(st, d.Ev.TURN_END, NO_ARG, player=p)  # rules 7-6-4-1, 13-1-1-1
        st.step = Step.END_STEP_DONE  # rule 7-6-2
    elif s is Step.END_STEP_DONE:
        st.step = Step.HAND_STEP
    elif s is Step.HAND_STEP:
        if len(st.zones[p][Zone.HAND]) > core.HAND_LIMIT:  # rule 7-6-5-1
            I.push_frame(
                st, I.system_program("hand_limit"), controller=p, host=NO_ARG, kind="system"
            )
        st.step = Step.HAND_STEP_DONE
    elif s is Step.HAND_STEP_DONE:
        st.step = Step.CLEANUP_STEP
    elif s is Step.CLEANUP_STEP:
        _expire(st)  # rule 7-6-6-1
        st.step = Step.CLEANUP_STEP_DONE
    elif s is Step.CLEANUP_STEP_DONE:
        st.step = Step.TURN_END
    elif s is Step.TURN_END:
        _next_turn(st)
    else:
        raise core.EngineError(f"unexpected step {s}")


def _next_turn(st: GameState) -> None:
    """Rule 7-6-7: the turn passes to the opponent."""
    if st.turn >= st.turn_limit:
        st.winner = -1
        st.end_reason = EndReason.TURN_LIMIT
        st.phase = Phase.GAME_OVER
        st.step = Step.GAME_OVER
        return
    st.turn += 1
    st.active = 1 - st.active
    st.once_used = set()
    st.history = []
    st.phase = Phase.START
    st.step = Step.ACTIVE_STEP
    st.touch()


def _action_step(st: GameState) -> None:
    """Rules 9-2..9-5: alternate from the standby player until both pass consecutively."""
    if st.passes >= 2:
        st.step = Step.BATTLE_ACTION_END_CHECK if st.step is Step.BATTLE_ACTION else Step.END_STEP
        return
    p = st.priority
    opts = action_step_options(st, p)
    if not opts:
        st.passes += 1
        st.priority = 1 - p
        return
    st.pending = Decision(p, DecisionKind.ACTION_STEP, (*opts, Action(A.PASS)), "action step")


# ---------------------------------------------------------------------------------------------
# legal options


def _resources(st: GameState, p: int) -> tuple[int, int]:
    normal = ex = 0
    for uid in st.zones[p][Zone.RESOURCE_AREA]:
        if st.cards[uid].rested:
            continue
        if core.card_type(st, uid) is CardType.EX_RESOURCE:
            ex += 1
        else:
            normal += 1
    return normal, ex


def payment_choices(st: GameState, p: int, cost: int) -> list[int]:
    """Numbers of EX Resources that can be used to pay ``cost`` (rules 2-10-1, 5-17-3-2-3)."""
    if cost <= 0:
        return [0]
    normal, ex = _resources(st, p)
    lo = max(0, cost - normal)
    hi = min(cost, ex)
    return list(range(lo, hi + 1)) if lo <= hi else []


def _level_ok(st: GameState, dv: V.Derived, p: int, uid: int) -> bool:
    return len(st.zones[p][Zone.RESOURCE_AREA]) >= V.play_level(st, dv, uid)  # rule 2-9-1


def _required_targets_ok(st: GameState, program_id: int, ctx: V.Ctx) -> bool:
    """Rules 10-1-8-1-1, 10-1-8-1-2, 10-2-2, Q100: mandatory targeted choices before "Then" must
    be possible in full."""
    return I.targets_available(V.reg().programs[program_id].instrs, 0, st, ctx)


def _command_playable(st: GameState, p: int, uid: int, timing: d.Timing) -> bool:
    R = V.reg()
    entry = R.cards[st.cards[uid].def_id]
    if entry.command_aid < 0:
        return False
    ab = R.abilities[entry.command_aid].ability
    assert isinstance(ab, d.Command)
    if ab.timing is not d.Timing.MAIN_OR_ACTION and ab.timing is not timing:
        return False
    ctx = V.Ctx(p, uid)
    if ab.cond is not d.TRUE and not V.cond(st, V.derived(st), ctx, ab.cond):
        return False
    return _required_targets_ok(st, R.abilities[entry.command_aid].program_id, ctx)


def _activated_list(st: GameState, p: int, timing: d.Timing) -> list[tuple[int, int, int]]:
    """(host uid, aid or SUPPORT_AID, resource cost) for activatable abilities."""
    dv = V.derived(st)
    out: list[tuple[int, int, int]] = []
    for z in (Zone.BATTLE, Zone.BASE, Zone.HAND, Zone.TRASH, Zone.PAIRED):
        for host in st.zones[p][z]:
            for a in dv.abilities.get(host, ()):
                ab = a.ability
                if not isinstance(ab, d.Activated):
                    continue
                if ab.timing is not d.Timing.MAIN_OR_ACTION and ab.timing is not timing:
                    continue
                cost = _activation_ok(st, dv, p, host, a)
                if cost >= 0:
                    out.append((host, a.aid, cost))
            if (
                timing is d.Timing.MAIN
                and z is Zone.BATTLE
                and V.kw_amount(dv, host, d.Kw.SUPPORT) > 0
                and not st.cards[host].rested
                and any(u != host for u in st.zones[p][Zone.BATTLE])
            ):
                out.append((host, SUPPORT_AID, 0))
    return out


def _activation_ok(st: GameState, dv: V.Derived, p: int, host: int, a: AbilityEntry) -> int:
    ab = a.ability
    assert isinstance(ab, d.Activated)
    if ab.once_per_turn and (a.aid, host, st.cards[host].zone_seq) in st.once_used:
        return -1
    card_uid = core._card_uid_for(st, a, host)
    ctx = V.Ctx(p, host, card_uid)
    if ab.cond is not d.TRUE and not V.cond(st, dv, ctx, ab.cond):
        return -1
    res = 0
    for c in ab.costs:
        if isinstance(c, d.RestSelf):
            hc = st.cards[host]
            if hc.rested or hc.zone not in (Zone.BATTLE, Zone.BASE):
                return -1
        elif isinstance(c, d.PayResources):
            res += c.amount
        elif isinstance(c, (d.RestCards, d.DestroyCards, d.DiscardCards, d.ExileCards)):
            sel = c.sel
            if isinstance(c, d.RestCards):
                sel = d.Sel(sel.side, sel.loc, (*sel.filters, d.IsRested(False)), sel.top_n)
            if len(V.select(st, dv, ctx, sel)) < c.count:
                return -1
        elif isinstance(c, (d.ReturnSelf, d.DestroySelf, d.TrashSelf)):
            if st.cards[host].zone not in (
                Zone.BATTLE,
                Zone.BASE,
                Zone.PAIRED,
                Zone.HAND,
                Zone.TRASH,
            ):
                return -1
    if res and not payment_choices(st, p, res):
        return -1
    if not _required_targets_ok(st, a.program_id, ctx):
        return -1
    return res


def _attack_options(st: GameState, p: int) -> list[Action]:
    """Rules 3-2-4, 3-2-6-3, 7-5-4-1, 8-2-1, plus attack restrictions/permissions from effects."""
    dv = V.derived(st)
    out: list[Action] = []
    enemy = 1 - p
    enemy_units = st.zones[enemy][Zone.BATTLE]
    attractors = [t for t in enemy_units if V.rules_of(dv, t, d.RuleKind.FORCE_ATTACK_TARGET)]
    for uid in st.zones[p][Zone.BATTLE]:
        c = st.cards[uid]
        if c.rested or V.rules_of(dv, uid, d.RuleKind.CANT_ATTACK):
            continue
        deploy_limits: tuple[d.Filter, ...] | None = None
        if c.entered_turn == st.turn and uid not in dv.linked:
            perms = V.rules_of(dv, uid, d.RuleKind.ATTACK_ON_DEPLOY_TURN)
            if not perms:
                continue
            if all(r.rule.name == "units_only" for r in perms):
                deploy_limits = perms[0].rule.source_filters
        unit_targets: list[int] = []
        if not V.rules_of(dv, uid, d.RuleKind.CANT_ATTACK_UNITS):
            active_perms = V.rules_of(dv, uid, d.RuleKind.MAY_ATTACK_ACTIVE)
            for t in enemy_units:
                if V.rules_of(dv, t, d.RuleKind.CANT_BE_ATTACKED):
                    continue
                tc = st.cards[t]
                ok = tc.rested or any(
                    not r.rule.source_filters
                    or V.matches(st, dv, V.Ctx(p, uid), t, r.rule.source_filters)
                    for r in active_perms
                )
                if ok and deploy_limits is not None and deploy_limits:
                    ok = V.matches(st, dv, V.Ctx(p, uid), t, deploy_limits)
                if ok:
                    unit_targets.append(t)
        forced = _forced_targets(st, dv, uid, attractors, unit_targets)
        if forced:
            out.extend(Action(A.ATTACK, uid, t) for t in forced)
            continue
        if deploy_limits is None and not V.rules_of(dv, uid, d.RuleKind.CANT_ATTACK_PLAYER):
            out.append(Action(A.ATTACK, uid, PLAYER_TARGET))
        out.extend(Action(A.ATTACK, uid, t) for t in unit_targets)
    return out


def _forced_targets(
    st: GameState, dv: V.Derived, attacker: int, attractors: list[int], legal: list[int]
) -> list[int]:
    """Attack targets ``attacker`` is limited to: Units it "must choose" (GD04-107) outrank
    rested Units it chooses "if possible" (Q289); the attacker picks among several (Q290,
    Q388); a forced Unit that is not a legal target imposes nothing (Q300, Q301)."""
    must: list[int] = []
    if_possible: list[int] = []
    for t in attractors:
        if t not in legal:
            continue
        for r in V.rules_of(dv, t, d.RuleKind.FORCE_ATTACK_TARGET):
            filters = r.rule.source_filters
            owner = st.cards[t].owner
            if filters and not V.matches(st, dv, V.Ctx(owner, t), attacker, filters):
                continue
            if r.rule.name == "must":
                must.append(t)
                break
            if st.cards[t].rested and t not in if_possible:
                if_possible.append(t)
    return must or if_possible


def main_options(st: GameState) -> list[Action]:
    """Rule 7-5-1: play a card, activate 【Activate･Main】, attack, or end the main phase."""
    p = st.active
    dv = V.derived(st)
    R = V.reg()
    out: list[Action] = []
    my_units = [u for u in st.zones[p][Zone.BATTLE] if I.can_pair(st, dv, u)]
    for uid in st.zones[p][Zone.HAND]:
        cd = R.db.by_id(st.cards[uid].def_id)
        alternatives = _alt_play_options(st, dv, p, uid, my_units)
        pays = payment_choices(st, p, V.play_cost(st, dv, uid)) if _level_ok(st, dv, p, uid) else []
        if not pays:
            out.extend(alternatives)
            continue
        t = cd.card_type
        if t is CardType.UNIT:
            out.extend(Action(A.PLAY_UNIT, uid, NO_ARG, c) for c in pays)
        elif t is CardType.BASE:
            out.extend(Action(A.PLAY_BASE, uid, NO_ARG, c) for c in pays)
        elif t is CardType.PILOT:
            for u in my_units:
                out.extend(Action(A.PAIR, uid, u, c) for c in pays)
        elif t is CardType.COMMAND:
            if _command_playable(st, p, uid, d.Timing.MAIN):
                out.extend(Action(A.PLAY_COMMAND, uid, NO_ARG, c) for c in pays)
            if cd.pilot_name is not None:  # rule 3-4-6-2
                for u in my_units:
                    out.extend(Action(A.PAIR, uid, u, c) for c in pays)
        out.extend(alternatives)
    for host, aid, cost in _activated_list(st, p, d.Timing.MAIN):
        out.extend(Action(A.ACTIVATE, host, aid, c) for c in payment_choices(st, p, cost))
    out.extend(_attack_options(st, p))
    out.append(Action(A.END_MAIN))
    return out


def play_modifiers(st: GameState, uid: int) -> list[AbilityEntry]:
    R = V.reg()
    entry = R.cards[st.cards[uid].def_id]
    return [R.abilities[a] for a in entry.own if isinstance(R.abilities[a].ability, d.PlayModifier)]


def _costs_payable(
    st: GameState, dv: V.Derived, p: int, host: int, costs: tuple[d.Cost, ...]
) -> bool:
    ctx = V.Ctx(p, host)
    for c in costs:
        if isinstance(c, (d.RestCards, d.DestroyCards, d.DiscardCards, d.ExileCards)):
            sel = c.sel
            if isinstance(c, d.RestCards):
                sel = d.Sel(sel.side, sel.loc, (*sel.filters, d.IsRested(False)), sel.top_n)
            cands = [u for u in V.select(st, dv, ctx, sel) if u != host]
            if len(cands) < c.count:
                return False
    return True


def _alt_play_options(
    st: GameState, dv: V.Derived, p: int, uid: int, my_units: list[int]
) -> list[Action]:
    """Play modifiers ("When playing this card from your hand, ... play it as if it has N Lv.
    and cost"): each offers the card at its modified Lv./cost when its condition and extra
    costs can be met."""
    mods = play_modifiers(st, uid)
    if not mods:
        return []
    R = V.reg()
    cd = R.db.by_id(st.cards[uid].def_id)
    out: list[Action] = []
    for i, entry in enumerate(mods):
        pm = entry.ability
        assert isinstance(pm, d.PlayModifier)
        ctx = V.Ctx(p, uid)
        if pm.cond is not d.TRUE and not V.cond(st, dv, ctx, pm.cond):
            continue
        if not _costs_payable(st, dv, p, uid, pm.costs):
            continue
        level = pm.level if pm.level is not None else V.play_level(st, dv, uid)
        if len(st.zones[p][Zone.RESOURCE_AREA]) < level:
            continue
        cost = pm.cost if pm.cost is not None else V.play_cost(st, dv, uid)
        enc = [c + ALT_PLAY_BASE * (i + 1) for c in payment_choices(st, p, cost)]
        t = cd.card_type
        if t is CardType.UNIT:
            out.extend(Action(A.PLAY_UNIT, uid, NO_ARG, c) for c in enc)
        elif t is CardType.BASE:
            out.extend(Action(A.PLAY_BASE, uid, NO_ARG, c) for c in enc)
        elif t is CardType.COMMAND and _command_playable(st, p, uid, d.Timing.MAIN):
            out.extend(Action(A.PLAY_COMMAND, uid, NO_ARG, c) for c in enc)
        if cd.is_pilot_capable:
            for u in my_units:
                if pm.pair_filters and not V.matches(st, dv, ctx, u, pm.pair_filters):
                    continue
                out.extend(Action(A.PAIR, uid, u, c) for c in enc)
    return out


def action_step_options(st: GameState, p: int) -> list[Action]:
    """Rule 9-3: 【Action】 Command cards and 【Activate･Action】 effects (13-2-4-2: no pairing)."""
    dv = V.derived(st)
    out: list[Action] = []
    for uid in st.zones[p][Zone.HAND]:
        cd = V.reg().db.by_id(st.cards[uid].def_id)
        if cd.card_type is not CardType.COMMAND or not _level_ok(st, dv, p, uid):
            continue
        pays = payment_choices(st, p, V.play_cost(st, dv, uid))
        if pays and _command_playable(st, p, uid, d.Timing.ACTION):
            out.extend(Action(A.PLAY_COMMAND, uid, NO_ARG, c) for c in pays)
    for host, aid, cost in _activated_list(st, p, d.Timing.ACTION):
        out.extend(Action(A.ACTIVATE, host, aid, c) for c in payment_choices(st, p, cost))
    return out


# ---------------------------------------------------------------------------------------------
# triggers


def _form_batch(st: GameState) -> None:
    batch = st.pending_triggers
    st.pending_triggers = []
    st.batches.append(batch)


def _trigger_candidates(st: GameState, batch: list[TriggerInst]) -> list[int]:
    """Rules 10-1-6-5, 10-1-6-6, 10-1-6-8: Bursts first, then the active player's, then standby's."""
    bursts = [i for i, t in enumerate(batch) if t.burst]
    if bursts:
        return bursts
    act = [i for i, t in enumerate(batch) if t.controller == st.active]
    if act:
        return act
    return list(range(len(batch)))


def _start_next_trigger(st: GameState) -> None:
    batch = st.batches[-1]
    if not batch:
        st.batches.pop()
        return
    cands = _trigger_candidates(st, batch)
    if len(cands) > 1:
        first = batch[cands[0]]
        if all(
            batch[i].program_id == first.program_id
            and batch[i].ability_key == first.ability_key
            and batch[i].burst == first.burst
            and batch[i].host == first.host
            for i in cands
        ):
            cands = cands[:1]
    if len(cands) == 1:
        _resolve_trigger(st, batch.pop(cands[0]))
        return
    player = batch[cands[0]].controller
    st.pending = Decision(
        player,
        DecisionKind.ORDER_TRIGGER,
        tuple(Action(A.ORDER, i) for i in cands),
        "order triggers",
    )


def _resolve_trigger(st: GameState, t: TriggerInst) -> None:
    if t.once_key and t.once_key in st.once_used:
        return
    kind = "burst" if t.burst else "trigger"
    if t.burst:
        core.record(st, "burst_revealed", t.controller, t.controller, t.card_uid)
    f = I.push_frame(
        st,
        t.program_id,
        controller=t.controller,
        host=t.host,
        card_uid=t.card_uid,
        kind=kind,
        event=t.event,
    )
    f.once_key = t.once_key


# ---------------------------------------------------------------------------------------------
# main loop


def advance(st: GameState) -> None:
    """Run the game forward until a decision is required or the game ends."""
    while st.pending is None and st.winner is None:
        if st.action_count > st.max_actions:
            st.winner = -1
            st.end_reason = EndReason.TURN_LIMIT
            st.phase = Phase.GAME_OVER
            st.step = Step.GAME_OVER
            return
        if st.frames:
            I.run_top_frame(st)
            continue
        if st.pending_triggers:
            _form_batch(st)
            continue
        if st.batches:
            _start_next_trigger(st)
            continue
        if st.phase is Phase.SETUP:
            _setup_step(st)
        else:
            core.rules_management(st)
            if core.over(st):
                return
            if st.frames or st.pending_triggers:
                continue
            _turn_step(st)


def concede(st: GameState, player: int) -> None:
    """Rule 1-2-4: a player may concede at any time and loses immediately (1-2-5: never a
    substitution)."""
    if st.winner is not None:
        return
    st.frames = []
    st.batches = []
    st.pending_triggers = []
    core.set_winner(st, {player}, EndReason.CONCEDE)


def legal_actions(st: GameState) -> tuple[Action, ...]:
    if st.pending is None:
        return ()
    return st.pending.options


def apply(st: GameState, action: Action, *, check: bool = True) -> None:
    """Apply ``action`` for the deciding player and advance to the next decision (in place)."""
    dec = st.pending
    if dec is None:
        raise IllegalActionError("no decision pending")
    if check and action not in dec.options:
        raise IllegalActionError(f"illegal action {action} for {dec.kind.value}")
    st.pending = None
    st.action_count += 1
    k = dec.kind
    if st.frames and st.frames[-1].waiting:
        I.resume(st, action)
    elif k is DecisionKind.CHOOSE_FIRST:
        st.first_player = action.a
        st.active = action.a
        st.step = Step.SETUP_DRAW
    elif k is DecisionKind.REDRAW:
        if action.kind is A.REDRAW:
            _redraw(st, dec.player)
        st.step = Step.SETUP_REDRAW_P2 if st.step is Step.SETUP_REDRAW_P1 else Step.SETUP_SHIELDS
    elif k is DecisionKind.MAIN:
        _apply_main(st, action)
    elif k is DecisionKind.BLOCK:
        if action.kind is A.BLOCK:
            B.block(st, action.a)
        st.step = Step.BLOCK_END_CHECK
    elif k is DecisionKind.ACTION_STEP:
        if action.kind is A.PASS:
            st.passes += 1
        else:
            st.passes = 0
            _play_or_activate(st, dec.player, action)
        st.priority = 1 - dec.player
    elif k is DecisionKind.ORDER_TRIGGER:
        batch = st.batches[-1]
        _resolve_trigger(st, batch.pop(action.a))
    else:
        raise core.EngineError(f"unhandled decision {k}")
    advance(st)


def _apply_main(st: GameState, action: Action) -> None:
    p = st.active
    if action.kind is A.END_MAIN:
        st.phase = Phase.END  # rule 7-5-5-2
        st.step = Step.END_ACTION
        st.priority = st.standby
        st.passes = 0
    elif action.kind is A.ATTACK:
        B.declare_attack(st, action.a, action.b)
    else:
        _play_or_activate(st, p, action)


def _pay(st: GameState, p: int, uid: int, ex_used: int, cost: int) -> None:
    if not I.pay_generic(st, p, cost, ex_used):
        raise core.EngineError("payment failed for a legal action")
    _consume_player_cost_mods(st, uid)


def _consume_player_cost_mods(st: GameState, uid: int) -> None:
    if not st.lasting:
        return
    R = V.reg()
    card = st.cards[uid]
    changed = False
    keep = []
    for le in st.lasting:
        applies = (
            le.player == card.owner
            and le.uses > 0
            and isinstance(R.continuous[le.effect_key], d.CostMod)
            and (
                le.filters_key == NO_ARG
                or V.matches(
                    st,
                    V.derived(st),
                    V.Ctx(le.controller, le.source_uid),
                    uid,
                    R.filter_sets[le.filters_key],
                )
            )
        )
        if applies:
            le.uses -= 1
            changed = True
            if le.uses == 0:
                continue
        keep.append(le)
    if changed:
        st.lasting = keep
        st.touch()


def _play_or_activate(st: GameState, p: int, action: Action) -> None:
    """Rules 7-5-2-2, 9-3-1, 10-3-1."""
    R = V.reg()
    dv = V.derived(st)
    k = action.kind
    if k is A.ACTIVATE:
        host, aid, ex = action.a, action.b, action.c
        if aid == SUPPORT_AID:
            core.record(st, "activate", p, p, host)
            I.push_frame(st, R.support_program, controller=p, host=host, kind="activated")
            return
        a = R.abilities[aid]
        ab = a.ability
        assert isinstance(ab, d.Activated)
        res = sum(c.amount for c in ab.costs if isinstance(c, d.PayResources))
        if res and not I.pay_generic(st, p, res, ex):
            raise core.EngineError("activation payment failed")
        if ab.once_per_turn:
            st.once_used.add((a.aid, host, st.cards[host].zone_seq))
        core.record(st, "activate", p, p, host)
        if res:
            core.record(st, "cost_paid", p, p, host)
            core.emit(st, d.Ev.COST_PAID, host, player=p, amount=res)
        I.push_frame(
            st,
            a.program_id,
            controller=p,
            host=host,
            card_uid=core._card_uid_for(st, a, host),
            kind="activated",
        )
        return
    uid = action.a
    alt_cost_program = -1
    ex = action.c
    if action.c >= ALT_PLAY_BASE:
        alt = action.c // ALT_PLAY_BASE - 1
        ex = action.c % ALT_PLAY_BASE
        entry_pm = play_modifiers(st, uid)[alt]
        pm = entry_pm.ability
        assert isinstance(pm, d.PlayModifier)
        cost = pm.cost if pm.cost is not None else V.play_cost(st, dv, uid)
        alt_cost_program = entry_pm.cost_program_id
    else:
        cost = V.play_cost(st, dv, uid)
    st.cards[uid].known = core.BOTH_KNOW  # rule 7-5-2-2-1: reveal
    _pay(st, p, uid, ex, cost)
    core.record(st, "play", p, p, uid)
    if ex > 0:
        core.record(st, "played_with_ex", p, p, uid)
    if k is A.PLAY_UNIT or k is A.PLAY_BASE:
        fr = I.push_frame(
            st,
            I.system_program("deploy"),
            controller=p,
            host=uid,
            kind="system",
            vars={"card": (uid,)},
        )
        fr.ints["ex_used"] = ex
    elif k is A.PAIR:
        I.push_frame(
            st,
            I.system_program("pair"),
            controller=p,
            host=uid,
            kind="system",
            vars={"pilot": (uid,), "unit": (action.b,)},
        )
    elif k is A.PLAY_COMMAND:
        entry = R.cards[st.cards[uid].def_id]
        core.move(st, uid, Zone.RESOLVING, reveal=True)  # rule 3-4-3
        core.record(st, "command_activated", p, p, uid)
        core.emit(st, d.Ev.COMMAND_PLAYED, uid, player=p, ex_used=ex)
        I.push_frame(
            st, R.abilities[entry.command_aid].program_id, controller=p, host=uid, kind="command"
        )
    else:
        raise core.EngineError(f"unexpected play action {action}")
    if alt_cost_program >= 0:
        I.push_frame(st, alt_cost_program, controller=p, host=uid, kind="system")


# ---------------------------------------------------------------------------------------------
# convenience


def step(st: GameState, action: Action) -> GameState:
    """Return a new state with ``action`` applied (the input is unchanged)."""
    s = st.clone()
    apply(s, action)
    return s
