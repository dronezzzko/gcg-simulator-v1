"""Effect interpreter: executes flat programs one instruction at a time.

Instructions that need a player's choice set ``st.pending`` and mark the frame as waiting;
:func:`resume` consumes the chosen action and continues. Custom steps registered by binding
modules are deterministic and never request choices.
"""

from __future__ import annotations

from collections.abc import Callable
from enum import Enum

from gcg_sim.cards.model import CardType
from gcg_sim.cards.tokens import TokenSpec
from gcg_sim.effects import dsl as d
from gcg_sim.effects import program as pr
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Action, Battle, Decision, Frame, GameState, Lasting
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, ActionKind, DecisionKind, Duration, Zone

A = ActionKind


BURST_PROMPT = "activate Burst?"


class Status(Enum):
    NEXT = 0
    JUMPED = 1
    WAIT = 2
    PUSHED = 3


CustomStepFn = Callable[[GameState, Frame, V.Ctx, dict[str, object]], bool]
CUSTOM_STEPS: dict[str, CustomStepFn] = {}


def ctx_of(f: Frame) -> V.Ctx:
    return V.Ctx(f.controller, f.host, f.card_uid, f.vars, f.event)


def push_frame(
    st: GameState,
    program_id: int,
    *,
    controller: int,
    host: int,
    card_uid: int = NO_ARG,
    kind: str,
    event: tuple[tuple[str, int], ...] = (),
    vars: dict[str, tuple[int, ...]] | None = None,
) -> Frame:
    host_seq = st.cards[host].zone_seq if host >= 0 else 0
    f = Frame(
        program_id=program_id,
        controller=controller,
        host=host,
        host_seq=host_seq,
        card_uid=card_uid if card_uid != NO_ARG else host,
        kind=kind,
        event=event,
        vars=dict(vars or {}),
    )
    for k, v in event:
        if k.startswith("it") and k[2:].isdigit():
            f.vars.setdefault("it", ())
            f.vars["it"] = (*f.vars["it"], v)
    st.frames.append(f)
    return f


def _decide(
    st: GameState,
    f: Frame,
    player: int,
    kind: DecisionKind,
    options: list[Action],
    prompt: str,
    context: tuple[tuple[str, int], ...] = (),
) -> Status:
    f.waiting = True
    st.pending = Decision(player, kind, tuple(options), prompt, context)
    return Status.WAIT


# ---------------------------------------------------------------------------------------------
# main execution loop


def run_top_frame(st: GameState) -> None:
    """Execute the top frame until it finishes, waits for a decision, or pushes a sub-frame."""
    R = V.reg()
    f = st.frames[-1]
    prog = R.programs[f.program_id]
    while f.pc < len(prog.instrs):
        status = execute(st, f, prog.instrs[f.pc])
        if status is Status.NEXT:
            f.pc += 1
        if core.over(st):
            return
        core.rules_management(st)
        if core.over(st):
            return
        if status is Status.WAIT or status is Status.PUSHED:
            return
    st.frames.pop()
    finish_frame(st, f)


def finish_frame(st: GameState, f: Frame) -> None:
    if f.kind in ("command", "burst"):
        card = st.cards[f.card_uid]
        if card.zone is Zone.RESOLVING:
            core.move(st, f.card_uid, Zone.TRASH)
            if f.kind == "command":
                core.emit(st, d.Ev.COMMAND_RESOLVED, f.card_uid, player=f.controller)
    elif f.kind == "activate_main_sub" and st.frames:
        parent = st.frames[-1]
        parent.pc += 1
        parent.did = f.did
    elif f.kind == "play_sub" and st.frames:
        parent = st.frames[-1]
        parent.pc += 1


def execute(st: GameState, f: Frame, ins: pr.Instr) -> Status:
    h = _HANDLERS.get(type(ins))
    if h is None:
        raise core.EngineError(f"no handler for {type(ins).__name__}")
    return h(st, f, ins)


def resume(st: GameState, action: Action) -> None:
    f = st.frames[-1]
    f.waiting = False
    ins = V.reg().programs[f.program_id].instrs[f.pc]
    r = _RESUMERS.get(type(ins))
    if r is None:
        raise core.EngineError(f"no resumer for {type(ins).__name__}")
    status = r(st, f, ins, action)
    if status is Status.NEXT:
        f.pc += 1
    core.rules_management(st)


# ---------------------------------------------------------------------------------------------
# control flow


def _h_jump(st: GameState, f: Frame, ins: pr.Jump) -> Status:
    f.pc = ins.target
    return Status.JUMPED


def _h_jump_if_not(st: GameState, f: Frame, ins: pr.JumpIfNot) -> Status:
    if V.cond(st, V.derived(st), ctx_of(f), ins.cond, f.did):
        return Status.NEXT
    f.pc = ins.target
    return Status.JUMPED


def _h_jump_if_not_did(st: GameState, f: Frame, ins: pr.JumpIfNotDid) -> Status:
    if f.did:
        return Status.NEXT
    f.pc = ins.target
    return Status.JUMPED


def _h_set_did(st: GameState, f: Frame, ins: pr.SetDid) -> Status:
    f.did = ins.value
    return Status.NEXT


def _h_ask_may(st: GameState, f: Frame, ins: pr.AskMay) -> Status:
    player = V.player_of(st, ctx_of(f), ins.player)
    kind = DecisionKind.BURST if ins.prompt == BURST_PROMPT else DecisionKind.YES_NO
    return _decide(st, f, player, kind, [Action(A.YES), Action(A.NO)], ins.prompt or "you may")


def _r_ask_may(st: GameState, f: Frame, ins: pr.AskMay, action: Action) -> Status:
    if action.kind is A.YES:
        f.answer = True
        return Status.NEXT
    f.answer = False
    f.did = False
    f.pc = ins.target_if_no
    return Status.JUMPED


def _h_mode(st: GameState, f: Frame, ins: pr.ModeSelect) -> Status:
    player = V.player_of(st, ctx_of(f), ins.chooser)
    opts = [Action(A.SELECT, i) for i in range(len(ins.labels))]
    return _decide(st, f, player, DecisionKind.SELECT, opts, "choose mode", (("mode", 1),))


def _r_mode(st: GameState, f: Frame, ins: pr.ModeSelect, action: Action) -> Status:
    f.pc = ins.targets[action.a]
    return Status.JUMPED


def _h_loop_init(st: GameState, f: Frame, ins: pr.LoopInit) -> Status:
    if ins.ref is not None:
        items = V.resolve(st, V.derived(st), ctx_of(f), ins.ref)
        f.vars[ins.key] = tuple(items)
        f.ints[ins.key] = 0
    else:
        assert ins.times is not None
        f.ints[ins.key] = -max(0, V.value(st, V.derived(st), ctx_of(f), ins.times))
    return Status.NEXT


def _h_loop_next(st: GameState, f: Frame, ins: pr.LoopNext) -> Status:
    i = f.ints.get(ins.key, 0)
    if ins.var:
        items = f.vars.get(ins.key, ())
        if i >= len(items):
            f.pc = ins.end
            return Status.JUMPED
        f.vars[ins.var] = (items[i],)
        f.ints[ins.key] = i + 1
        return Status.NEXT
    if i >= 0:
        f.pc = ins.end
        return Status.JUMPED
    f.ints[ins.key] = i + 1
    return Status.NEXT


# ---------------------------------------------------------------------------------------------
# choosing


def _choose_candidates(st: GameState, f: Frame, ins: d.Choose) -> list[int]:
    dv = V.derived(st)
    ctx = ctx_of(f)
    cands = V.select(st, dv, ctx, ins.sel)
    if ins.distinct_from:
        excl = {u for v in ins.distinct_from for u in f.vars.get(v, ())}
        cands = [u for u in cands if u not in excl]
    if ins.targeting:
        cands = [u for u in cands if not _cant_be_chosen(st, dv, u, f.controller)]
    return cands


def _cant_be_chosen(st: GameState, dv: V.Derived, uid: int, chooser_side: int) -> bool:
    owner = st.cards[uid].owner
    for r in V.rules_of(dv, uid, d.RuleKind.CANT_BE_CHOSEN):
        if r.rule.source_side is d.Side.ENEMY and chooser_side == owner:
            continue
        return True
    return False


def choose_bounds(st: GameState, f: Frame, ins: d.Choose) -> tuple[int, int]:
    dv = V.derived(st)
    ctx = ctx_of(f)
    n = V.value(st, dv, ctx, ins.count)
    mn = n if ins.min_count is None else V.value(st, dv, ctx, ins.min_count)
    if ins.optional:
        mn = 0
    return max(0, n), max(0, min(mn, n))


def _choose_options(st: GameState, f: Frame, cands: list[int], n: int, mn: int) -> list[Action]:
    picked = f.buf
    last = cands.index(picked[-1]) if picked else -1
    opts = [Action(A.SELECT, u) for i, u in enumerate(cands) if i > last and u not in picked]
    if len(picked) >= mn:
        opts.append(Action(A.DONE))
    return opts


def _h_choose(st: GameState, f: Frame, ins: d.Choose) -> Status:
    cands = _choose_candidates(st, f, ins)
    n, mn = choose_bounds(st, f, ins)
    f.buf = []
    if n == 0 or not cands:
        return _finish_choose(f, ins)
    if ins.random:
        pool = list(cands)
        chosen = []
        for _ in range(min(n, len(pool))):
            chosen.append(pool.pop(st.rng.randrange(len(pool))))
        f.buf = chosen
        return _finish_choose(f, ins)
    if len(cands) <= mn or (len(cands) <= n and mn == n):
        f.buf = list(cands)
        return _finish_choose(f, ins)
    chooser = V.player_of(st, ctx_of(f), ins.chooser)
    return _decide(
        st,
        f,
        chooser,
        DecisionKind.SELECT,
        _choose_options(st, f, cands, n, mn),
        f"choose {ins.var}",
        (("min", mn), ("max", n)),
    )


def _finish_choose(f: Frame, ins: d.Choose) -> Status:
    f.vars[ins.var] = tuple(f.buf)
    f.did = bool(f.buf)
    f.buf = []
    return Status.NEXT


def _r_choose(st: GameState, f: Frame, ins: d.Choose, action: Action) -> Status:
    if action.kind is A.DONE:
        return _finish_choose(f, ins)
    f.buf.append(action.a)
    cands = _choose_candidates(st, f, ins)
    n, mn = choose_bounds(st, f, ins)
    remaining = [u for u in cands if u not in f.buf and cands.index(u) > cands.index(f.buf[-1])]
    if len(f.buf) >= n or not remaining:
        return _finish_choose(f, ins)
    chooser = V.player_of(st, ctx_of(f), ins.chooser)
    _decide(
        st,
        f,
        chooser,
        DecisionKind.SELECT,
        _choose_options(st, f, cands, n, mn),
        f"choose {ins.var}",
        (("min", mn), ("max", n)),
    )
    return Status.WAIT


# ---------------------------------------------------------------------------------------------
# card and zone actions


def _in_play(st: GameState, uids: tuple[int, ...]) -> list[int]:
    return [u for u in uids if st.cards[u].zone in (Zone.BATTLE, Zone.BASE)]


def _refs(st: GameState, f: Frame, ref: d.Ref) -> tuple[int, ...]:
    return V.resolve(st, V.derived(st), ctx_of(f), ref)


def _val(st: GameState, f: Frame, v: d.Value) -> int:
    return V.value(st, V.derived(st), ctx_of(f), v)


def _h_draw(st: GameState, f: Frame, ins: d.Draw) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    f.did = draw(st, p, n) > 0
    return Status.NEXT


def draw(st: GameState, p: int, n: int) -> int:
    drawn = 0
    group = core.next_group(st)
    for _ in range(max(0, n)):
        deck = st.zones[p][Zone.DECK]
        if not deck:
            break
        uid = deck[0]
        core.move(st, uid, Zone.HAND)
        drawn += 1
        core.record(st, "draw", p, p, uid)
        core.emit(st, d.Ev.DRAWN, uid, player=p, group=group)
    return drawn


def _discard_candidates(st: GameState, f: Frame, ins: d.Discard, p: int) -> list[int]:
    hand = st.zones[p][Zone.HAND]
    if not ins.filters:
        return list(hand)
    dv = V.derived(st)
    return [u for u in hand if V.matches(st, dv, ctx_of(f), u, ins.filters)]


def _h_discard(st: GameState, f: Frame, ins: d.Discard) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    cands = _discard_candidates(st, f, ins, p)
    f.buf = []
    if n <= 0 or not cands:
        f.vars[ins.var] = ()
        f.did = False
        return Status.NEXT
    if ins.random:
        pool = list(cands)
        chosen = [pool.pop(st.rng.randrange(len(pool))) for _ in range(min(n, len(pool)))]
        return _do_discard(st, f, ins, p, chosen)
    if len(cands) <= n:
        return _do_discard(st, f, ins, p, list(cands))
    chooser = p if ins.chooser is None else V.player_of(st, ctx_of(f), ins.chooser)
    opts = [Action(A.SELECT, u) for u in cands]
    return _decide(st, f, chooser, DecisionKind.DISCARD, opts, "discard", (("count", n),))


def _r_discard(st: GameState, f: Frame, ins: d.Discard, action: Action) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    f.buf.append(action.a)
    cands = [u for u in _discard_candidates(st, f, ins, p) if u not in f.buf]
    if len(f.buf) >= n or not cands:
        return _do_discard(st, f, ins, p, list(f.buf))
    last = sorted(f.buf)
    opts = [Action(A.SELECT, u) for u in cands if u > last[-1]] or [
        Action(A.SELECT, u) for u in cands
    ]
    chooser = p if ins.chooser is None else V.player_of(st, ctx_of(f), ins.chooser)
    _decide(st, f, chooser, DecisionKind.DISCARD, opts, "discard", (("count", n - len(f.buf)),))
    return Status.WAIT


def _do_discard(st: GameState, f: Frame, ins: d.Discard, p: int, uids: list[int]) -> Status:
    discard(st, p, uids, by=f.controller)
    f.vars[ins.var] = tuple(uids)
    f.buf = []
    f.did = bool(uids)
    return Status.NEXT


def discard(st: GameState, p: int, uids: list[int], by: int) -> None:
    group = core.next_group(st)
    for u in uids:
        core.move(st, u, Zone.TRASH)
        core.record(st, "discard", p, by, u)
    for u in uids:
        core.emit(st, d.Ev.DISCARDED, u, player=p, by=by, group=group)


def _h_damage(st: GameState, f: Frame, ins: d.Damage) -> Status:
    targets = _refs(st, f, ins.ref)
    amount = _val(st, f, ins.amount)
    dealt = False
    core.next_group(st)
    for u in targets:
        c = st.cards[u]
        if c.zone is Zone.SHIELD:
            if amount > 0:
                core.destroy_shields(st, c.owner, [u], battle=False, source=f.host, by=f.controller)
                dealt = True
            continue
        if core.damage_card(st, u, amount, source=f.host, battle=False, by=f.controller) > 0:
            dealt = True
    f.did = dealt
    return Status.NEXT


def _h_damage_shield_area(st: GameState, f: Frame, ins: d.DamageShieldArea) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    amount = _val(st, f, ins.amount)
    had = bool(st.zones[p][Zone.BASE] or st.zones[p][Zone.SHIELD])
    core.damage_shield_area(
        st, p, amount, cards=ins.cards, source=f.host, battle=False, by=f.controller
    )
    f.did = had and amount > 0
    return Status.NEXT


def _h_damage_player(st: GameState, f: Frame, ins: d.DamagePlayer) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    amount = _val(st, f, ins.amount)
    had = bool(st.zones[p][Zone.BASE] or st.zones[p][Zone.SHIELD])
    core.damage_shield_area(st, p, amount, cards=1, source=f.host, battle=False, by=f.controller)
    f.did = had and amount > 0
    return Status.NEXT


def _h_destroy(st: GameState, f: Frame, ins: d.Destroy) -> Status:
    targets = _in_play(st, _refs(st, f, ins.ref))
    f.did = bool(core.destroy(st, targets, battle=False, by=f.controller, source=f.host))
    return Status.NEXT


def _h_rest(st: GameState, f: Frame, ins: d.Rest) -> Status:
    did = False
    group = core.next_group(st)
    for u in _refs(st, f, ins.ref):
        c = st.cards[u]
        if c.zone in (Zone.BATTLE, Zone.BASE, Zone.RESOURCE_AREA) and not c.rested:
            c.rested = True
            st.touch()
            did = True
            core.emit(st, d.Ev.RESTED, u, player=c.owner, by=f.controller, group=group)
    f.did = did
    return Status.NEXT


def _h_set_active(st: GameState, f: Frame, ins: d.SetActive) -> Status:
    did = False
    dv = V.derived(st)
    group = core.next_group(st)
    for u in _refs(st, f, ins.ref):
        c = st.cards[u]
        if c.zone in (Zone.BATTLE, Zone.BASE, Zone.RESOURCE_AREA) and c.rested:
            if V.rules_of(dv, u, d.RuleKind.CANT_BE_SET_ACTIVE):
                continue
            c.rested = False
            st.touch()
            did = True
            core.emit(st, d.Ev.SET_ACTIVE, u, player=c.owner, by=f.controller, group=group)
    f.did = did
    return Status.NEXT


def _returnable(st: GameState, dv: V.Derived, u: int, by: int) -> bool:
    for r in V.rules_of(dv, u, d.RuleKind.CANT_BE_RETURNED):
        if r.rule.source_side is d.Side.ENEMY and by == st.cards[u].owner:
            continue
        return False
    return True


def _h_return_to_hand(st: GameState, f: Frame, ins: d.ReturnToHand) -> Status:
    dv = V.derived(st)
    targets = [
        u
        for u in _refs(st, f, ins.ref)
        if st.cards[u].zone in (Zone.BATTLE, Zone.BASE, Zone.PAIRED, Zone.TRASH, Zone.RESOLVING)
        and _returnable(st, dv, u, f.controller)
    ]
    f.did = return_to_hand(st, targets, by=f.controller)
    return Status.NEXT


def return_to_hand(st: GameState, targets: list[int], by: int) -> bool:
    if not targets:
        return False
    in_play = [u for u in targets if st.cards[u].zone in (Zone.BATTLE, Zone.BASE)]
    lki = core.ability_snapshot(st, in_play)
    group = core.next_group(st)
    moved = []
    for u in targets:
        c = st.cards[u]
        if c.zone is Zone.OUTSIDE:
            continue
        core.move(st, u, Zone.HAND)
        moved.append(u)
    for u in moved:
        core.emit(
            st,
            d.Ev.RETURNED_TO_HAND,
            u,
            player=st.cards[u].owner,
            by=by,
            group=group,
            lki={k: v for k, v in lki.items() if k == u},
        )
    return bool(moved)


def _h_to_deck(st: GameState, f: Frame, ins: d.ToDeck) -> Status:
    targets = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is not Zone.OUTSIDE]
    owners: set[int] = set()
    for u in targets:
        owners.add(st.cards[u].owner)
        core.move(st, u, Zone.DECK, bottom=ins.bottom)
    if ins.shuffle:
        for p in sorted(owners):
            core.shuffle_deck(st, p)
    f.did = bool(targets)
    return Status.NEXT


def _h_exile(st: GameState, f: Frame, ins: d.Exile) -> Status:
    targets = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is not Zone.OUTSIDE]
    group = core.next_group(st)
    for u in targets:
        core.move(st, u, Zone.REMOVAL)
    for u in targets:
        core.emit(st, d.Ev.EXILED, u, player=st.cards[u].owner, by=f.controller, group=group)
    f.did = bool(targets)
    return Status.NEXT


def _h_to_trash(st: GameState, f: Frame, ins: d.ToTrash) -> Status:
    targets = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is not Zone.OUTSIDE]
    for u in targets:
        core.move(st, u, Zone.TRASH)
    f.did = bool(targets)
    return Status.NEXT


def _h_add_to_hand(st: GameState, f: Frame, ins: d.AddToHand) -> Status:
    targets = [
        u for u in _refs(st, f, ins.ref) if st.cards[u].zone not in (Zone.HAND, Zone.OUTSIDE)
    ]
    group = core.next_group(st)
    for u in targets:
        core.move(st, u, Zone.HAND, reveal=ins.reveal)
    for u in targets:
        core.emit(st, d.Ev.ADDED_TO_HAND, u, player=st.cards[u].owner, by=f.controller, group=group)
    f.did = bool(targets)
    return Status.NEXT


# -- deploying ------------------------------------------------------------------------------


def _excess_needed(
    st: GameState, player: int, incoming_units: int, incoming_bases: int
) -> tuple[int, int]:
    units = len(st.zones[player][Zone.BATTLE])
    bases = len(st.zones[player][Zone.BASE])
    return max(0, units + incoming_units - core.BATTLE_LIMIT), max(
        0, bases + incoming_bases - core.BASE_LIMIT
    )


def _ask_excess(
    st: GameState, f: Frame, player: int, zone: Zone, protect: tuple[int, ...]
) -> Status:
    cands = [u for u in st.zones[player][zone] if u not in protect]
    opts = [Action(A.SELECT, u) for u in cands]
    return _decide(
        st, f, player, DecisionKind.EXCESS, opts, "battle area excess", (("zone", int(zone)),)
    )


def _r_excess(st: GameState, f: Frame, ins: d.Step, action: Action) -> Status:
    trash_excess(st, action.a)
    return execute(st, f, ins)


def trash_excess(st: GameState, uid: int) -> None:
    """Rules 11-4-2 / 11-5-2: place into the trash; not destroyed (5-10-4)."""
    c = st.cards[uid]
    core.move(st, uid, Zone.TRASH)
    core.record(st, "excess", c.owner, c.owner, uid)
    if V.reg().db.by_id(c.def_id).is_token and st.cards[uid].zone is Zone.TRASH:
        core.remove_from_game(st, uid)


def _deployable(st: GameState, uids: tuple[int, ...]) -> list[int]:
    out = []
    for u in uids:
        c = st.cards[u]
        t = V.reg().db.by_id(c.def_id).card_type
        if (t.is_unit or t.is_base) and c.zone not in (Zone.BATTLE, Zone.BASE, Zone.OUTSIDE):
            out.append(u)
    return out


def _h_deploy_card(st: GameState, f: Frame, ins: d.DeployCard) -> Status:
    cards = _deployable(st, _refs(st, f, ins.ref))
    if not cards:
        f.did = False
        return Status.NEXT
    by_player: dict[int, list[int]] = {}
    for u in cards:
        by_player.setdefault(st.cards[u].owner, []).append(u)
    for p, us in sorted(by_player.items()):
        n_units = sum(1 for u in us if core.card_type(st, u).is_unit)
        n_bases = len(us) - n_units
        ex_u, ex_b = _excess_needed(st, p, n_units, n_bases)
        if ex_u:
            return _ask_excess(st, f, p, Zone.BATTLE, tuple(us))
        if ex_b:
            base = st.zones[p][Zone.BASE]
            if len(base) == 1:
                trash_excess(st, base[0])
            else:
                return _ask_excess(st, f, p, Zone.BASE, tuple(us))
    deploy_cards(st, cards, rested=ins.rested, by=f.controller, ex_used=f.ints.get("ex_used", 0))
    f.did = True
    return Status.NEXT


def deploy_cards(st: GameState, cards: list[int], *, rested: bool, by: int, ex_used: int = 0) -> None:
    group = core.next_group(st)
    froms = {u: st.cards[u].zone for u in cards}
    for u in cards:
        t = core.card_type(st, u)
        core.move(st, u, Zone.BATTLE if t.is_unit else Zone.BASE, rested=rested)
        core.record(st, "deployed", st.cards[u].owner, by, u)
    for u in cards:
        core.emit(
            st,
            d.Ev.DEPLOYED,
            u,
            player=st.cards[u].owner,
            by=by,
            group=group,
            from_loc=V.zone_loc_code(froms[u]),
            ex_used=ex_used,
        )


def _h_deploy_token(st: GameState, f: Frame, ins: d.DeployToken) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    if n <= 0:
        f.did = False
        return Status.NEXT
    ex_u, _ = _excess_needed(st, p, n, 0)
    if ex_u and len(st.zones[p][Zone.BATTLE]) > 0:
        return _ask_excess(st, f, p, Zone.BATTLE, ())
    spec = token_spec(ins.token_key)
    tdef = V.reg().db.token_for(spec)
    group = core.next_group(st)
    made = []
    for _ in range(min(n, core.BATTLE_LIMIT)):
        uid = core.new_card(st, tdef.def_id, p, Zone.BATTLE, rested=ins.rested)
        made.append(uid)
        core.record(st, "deployed", p, f.controller, uid)
    for uid in made:
        core.emit(st, d.Ev.DEPLOYED, uid, player=p, by=f.controller, group=group, token=1)
    f.vars[ins.var] = tuple(made)
    f.did = bool(made)
    return Status.NEXT


_TOKEN_SPECS: dict[str, TokenSpec] = {}


def token_spec(key: str) -> TokenSpec:
    spec = _TOKEN_SPECS.get(key)
    if spec is None:
        name, traits, ap, hp, text = key.split("|", 4)
        spec = TokenSpec(name, tuple(t for t in traits.split("/") if t), int(ap), int(hp), text)
        _TOKEN_SPECS[key] = spec
    return spec


def _h_place_ex(st: GameState, f: Frame, ins: d.PlaceExResource) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    f.did = place_ex_resource(st, p, rested=ins.rested, by=f.controller)
    return Status.NEXT


def place_ex_resource(st: GameState, p: int, *, rested: bool, by: int) -> bool:
    area = st.zones[p][Zone.RESOURCE_AREA]
    ex_def = V.reg().db.ex_resource.def_id
    n_ex = sum(1 for u in area if core.card_type(st, u) is CardType.EX_RESOURCE)
    if len(area) >= core.RESOURCE_LIMIT or n_ex >= core.EX_RESOURCE_LIMIT:
        return False  # rules 4-4-2, 4-4-2-1
    uid = core.new_card(st, ex_def, p, Zone.RESOURCE_AREA, rested=rested)
    core.record(st, "ex_resource", p, by, uid)
    core.emit(st, d.Ev.EX_RESOURCE_PLACED, uid, player=p, by=by)
    return True


def _h_place_resource(st: GameState, f: Frame, ins: d.PlaceResource) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    f.did = place_resource(st, p, rested=ins.rested, by=f.controller)
    return Status.NEXT


def place_resource(st: GameState, p: int, *, rested: bool, by: int) -> bool:
    rdeck = st.zones[p][Zone.RESOURCE_DECK]
    area = st.zones[p][Zone.RESOURCE_AREA]
    if not rdeck or len(area) >= core.RESOURCE_LIMIT:
        return False
    uid = rdeck[0]
    core.move(st, uid, Zone.RESOURCE_AREA, rested=rested)
    core.emit(st, d.Ev.RESOURCE_PLACED, uid, player=p, by=by)
    return True


def _h_set_resources_active(st: GameState, f: Frame, ins: d.SetResourcesActive) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    did = False
    area = st.zones[p][Zone.RESOURCE_AREA]
    ordered = sorted(area, key=lambda u: core.card_type(st, u) is CardType.EX_RESOURCE)
    for u in ordered:
        if n <= 0:
            break
        c = st.cards[u]
        if c.rested:
            c.rested = False
            n -= 1
            did = True
    if did:
        st.touch()
    f.did = did
    return Status.NEXT


def _h_rest_resources(st: GameState, f: Frame, ins: d.RestResources) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    did = False
    area = st.zones[p][Zone.RESOURCE_AREA]
    ordered = sorted(area, key=lambda u: core.card_type(st, u) is CardType.EX_RESOURCE)
    for u in ordered:
        if n <= 0:
            break
        c = st.cards[u]
        if not c.rested:
            c.rested = True
            n -= 1
            did = True
    if did:
        st.touch()
    f.did = did
    return Status.NEXT


def _h_shield_to_hand(st: GameState, f: Frame, ins: d.ShieldToHand) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    shields = st.zones[p][Zone.SHIELD][: max(0, n)]
    group = core.next_group(st)
    for u in list(shields):
        core.move(st, u, Zone.HAND)
        core.record(st, "shield_to_hand", p, f.controller, u)
    for u in shields:
        core.emit(st, d.Ev.SHIELD_TO_HAND, u, player=p, by=f.controller, group=group)
    f.did = bool(shields)
    return Status.NEXT


def _h_add_to_shields(st: GameState, f: Frame, ins: d.AddToShields) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    if ins.ref is None:
        n = _val(st, f, ins.count)
        cards = st.zones[p][Zone.DECK][: max(0, n)]
    else:
        cards = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is not Zone.OUTSIDE]
    for u in list(cards):
        core.move(st, u, Zone.SHIELD, bottom=not ins.top)
    f.did = bool(cards)
    return Status.NEXT


def _h_mill(st: GameState, f: Frame, ins: d.Mill) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    cards = list(st.zones[p][Zone.DECK][: max(0, n)])
    for u in cards:
        core.move(st, u, Zone.TRASH)
    f.vars[ins.var] = tuple(cards)
    f.did = bool(cards)
    return Status.NEXT


def _h_look_top(st: GameState, f: Frame, ins: d.LookTop) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    n = _val(st, f, ins.count)
    cards = tuple(st.zones[p][Zone.DECK][: max(0, n)])
    bits = core.BOTH_KNOW if ins.reveal else (1 << f.controller)
    for u in cards:
        st.cards[u].known |= bits
    f.vars[ins.var] = cards
    f.did = bool(cards)
    return Status.NEXT


def _h_arrange(st: GameState, f: Frame, ins: d.Arrange) -> Status:
    cards = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is Zone.DECK]
    if not cards:
        f.did = False
        return Status.NEXT
    p = st.cards[cards[0]].owner
    chooser = V.player_of(st, ctx_of(f), ins.player)
    if ins.mode in ("top_or_bottom", "top_or_bottom_each"):
        remaining = [u for u in cards if u not in f.buf]
        if ins.mode == "top_or_bottom" and len(cards) > 1:
            opts = [Action(A.SELECT, 0), Action(A.SELECT, 1)]
            return _decide(
                st, f, chooser, DecisionKind.ARRANGE, opts, "top(0) or bottom(1)", (("all", 1),)
            )
        if not remaining:
            f.buf = []
            f.did = True
            return Status.NEXT
        opts = [Action(A.SELECT, 0, remaining[0]), Action(A.SELECT, 1, remaining[0])]
        return _decide(
            st,
            f,
            chooser,
            DecisionKind.ARRANGE,
            opts,
            "top(0) or bottom(1)",
            (("card", remaining[0]),),
        )
    remaining = [u for u in cards if u not in f.buf]
    if len(remaining) <= 1:
        order = [*f.buf, *remaining]
        _place_ordered(st, p, order, bottom=ins.mode == "bottom_any_order")
        f.buf = []
        f.did = True
        return Status.NEXT
    opts = [Action(A.SELECT, u) for u in remaining]
    return _decide(
        st, f, chooser, DecisionKind.ARRANGE, opts, "next card (top first)", (("order", 1),)
    )


def _place_ordered(st: GameState, p: int, order: list[int], *, bottom: bool) -> None:
    deck = st.zones[p][Zone.DECK]
    for u in order:
        deck.remove(u)
    if bottom:
        deck.extend(order)
    else:
        deck[0:0] = order
    st.touch()


def _r_arrange(st: GameState, f: Frame, ins: d.Arrange, action: Action) -> Status:
    cards = [u for u in _refs(st, f, ins.ref) if st.cards[u].zone is Zone.DECK]
    p = st.cards[cards[0]].owner
    if ins.mode == "top_or_bottom" and len(cards) > 1:
        _place_ordered(st, p, cards, bottom=action.a == 1)
        f.did = True
        return Status.NEXT
    if ins.mode in ("top_or_bottom", "top_or_bottom_each"):
        u = action.b if action.b != NO_ARG else cards[0]
        _place_ordered(st, p, [u], bottom=action.a == 1)
        f.buf.append(u)
        return _h_arrange(st, f, ins)
    f.buf.append(action.a)
    return _h_arrange(st, f, ins)


def _h_reveal(st: GameState, f: Frame, ins: d.Reveal) -> Status:
    for u in _refs(st, f, ins.ref):
        st.cards[u].known = core.BOTH_KNOW
    f.did = True
    return Status.NEXT


def _h_shuffle(st: GameState, f: Frame, ins: d.Shuffle) -> Status:
    core.shuffle_deck(st, V.player_of(st, ctx_of(f), ins.player))
    f.did = True
    return Status.NEXT


def _h_recover(st: GameState, f: Frame, ins: d.Recover) -> Status:
    amount = _val(st, f, ins.amount)
    did = False
    for u in _in_play(st, _refs(st, f, ins.ref)):
        c = st.cards[u]
        if c.damage > 0 and amount > 0:  # rule 5-6-3
            healed = min(c.damage, amount)
            c.damage -= healed
            st.touch()
            did = True
            core.emit(st, d.Ev.RECOVERED, u, player=c.owner, amount=healed)
    f.did = did
    return Status.NEXT


def _h_apply(st: GameState, f: Frame, ins: d.Apply) -> Status:
    targets = _refs(st, f, ins.ref)
    live = tuple((u, st.cards[u].zone_seq) for u in targets if st.cards[u].zone is not Zone.OUTSIDE)
    if not live:
        f.did = False
        return Status.NEXT
    add_lasting(st, ins.effect, f.controller, f.host, live, ins.duration)
    f.did = True
    return Status.NEXT


def add_lasting(
    st: GameState,
    effect: d.Continuous,
    controller: int,
    source: int,
    targets: tuple[tuple[int, int], ...],
    duration: Duration,
    *,
    player: int = NO_ARG,
    uses: int = 0,
    filters: tuple[d.Filter, ...] = (),
) -> None:
    R = V.reg()
    b = st.battle
    st.lasting.append(
        Lasting(
            effect_key=R.cont_key(effect),
            controller=controller,
            source_uid=source,
            targets=targets,
            duration=duration.value,
            created_turn=st.turn,
            battle_id=b.battle_id if b is not None else NO_ARG,
            player=player,
            uses=uses,
            filters_key=R.filters_key(filters) if filters else NO_ARG,
        )
    )
    st.touch()


def _h_apply_player(st: GameState, f: Frame, ins: d.ApplyPlayer) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    add_lasting(
        st,
        ins.effect,
        f.controller,
        f.host,
        (),
        ins.duration,
        player=p,
        uses=ins.uses,
        filters=ins.filters,
    )
    f.did = True
    return Status.NEXT


def can_pair(st: GameState, dv: V.Derived, unit: int) -> bool:
    c = st.cards[unit]
    return (
        c.zone is Zone.BATTLE and c.pair < 0 and not V.rules_of(dv, unit, d.RuleKind.CANT_BE_PAIRED)
    )


def _h_pair(st: GameState, f: Frame, ins: d.Pair) -> Status:
    pilots = _refs(st, f, ins.pilot)
    units = _refs(st, f, ins.unit)
    dv = V.derived(st)
    if not pilots or not units:
        f.did = False
        return Status.NEXT
    pilot, unit = pilots[0], units[0]
    pd = V.reg().db.by_id(st.cards[pilot].def_id)
    if (
        not pd.is_pilot_capable
        or not can_pair(st, dv, unit)
        or st.cards[pilot].zone is Zone.OUTSIDE
    ):
        f.did = False
        return Status.NEXT
    pair(st, pilot, unit, by=f.controller)
    f.did = True
    return Status.NEXT


def pair(st: GameState, pilot: int, unit: int, *, by: int) -> None:
    """Rule 5-9: place the Pilot beneath the Unit; 【When Paired】 and 【When Linked】 trigger."""
    from_zone = st.cards[pilot].zone
    core.move(st, pilot, Zone.PAIRED)
    st.cards[pilot].pair = unit
    st.cards[unit].pair = pilot
    st.touch()
    core.record(st, "paired", st.cards[unit].owner, by, pilot)
    dv = V.derived(st)
    group = core.next_group(st)
    linked = unit in dv.linked
    core.emit(
        st,
        d.Ev.PAIRED,
        unit,
        player=st.cards[unit].owner,
        pilot=pilot,
        by=by,
        group=group,
        from_loc=V.zone_loc_code(from_zone),
        linked=int(linked),
    )
    if linked:
        core.emit(
            st, d.Ev.LINKED, unit, player=st.cards[unit].owner, pilot=pilot, by=by, group=group
        )


def _h_play_card(st: GameState, f: Frame, ins: d.PlayCard) -> Status:
    cards = _refs(st, f, ins.ref)
    if not cards:
        f.did = False
        return Status.NEXT
    uid = cards[0]
    t = core.card_type(st, uid)
    if not ins.free:
        cost = max(0, V.play_cost(st, V.derived(st), uid) + _val(st, f, ins.cost_delta))
        if not pay_generic(st, st.cards[uid].owner, cost):
            f.did = False
            return Status.NEXT
    if t.is_unit or t.is_base:
        f.vars["__play"] = (uid,)
        push_frame(
            st,
            system_program("deploy"),
            controller=f.controller,
            host=uid,
            kind="play_sub",
            vars={"card": (uid,)},
        )
        f.did = True
        return Status.PUSHED
    if t is CardType.COMMAND:
        entry = V.reg().cards[st.cards[uid].def_id]
        if entry.command_aid < 0:
            f.did = False
            return Status.NEXT
        core.move(st, uid, Zone.RESOLVING, reveal=True)
        core.emit(st, d.Ev.COMMAND_PLAYED, uid, player=f.controller)
        push_frame(
            st,
            V.reg().abilities[entry.command_aid].program_id,
            controller=f.controller,
            host=uid,
            kind="command",
        )
        f.pc += 1
        f.did = True
        return Status.PUSHED
    f.did = False
    return Status.NEXT


def pay_generic(st: GameState, p: int, cost: int, ex_used: int = -1) -> bool:
    """Rest ``cost`` active Resources (rule 2-10-1); EX Resources used are removed (5-17-3-2-3).

    ``ex_used`` < 0 means prefer normal Resources first.
    """
    if cost <= 0:
        return True
    area = st.zones[p][Zone.RESOURCE_AREA]
    normal = [
        u
        for u in area
        if not st.cards[u].rested and core.card_type(st, u) is not CardType.EX_RESOURCE
    ]
    ex = [
        u for u in area if not st.cards[u].rested and core.card_type(st, u) is CardType.EX_RESOURCE
    ]
    if ex_used < 0:
        ex_used = max(0, cost - len(normal))
    if ex_used > len(ex) or cost - ex_used > len(normal) or ex_used > cost:
        return False
    for u in normal[: cost - ex_used]:
        st.cards[u].rested = True
    for u in ex[:ex_used]:
        st.cards[u].rested = True
        core.remove_from_game(st, u)
    st.touch()
    return True


def _h_activate_main(st: GameState, f: Frame, ins: d.ActivateMain) -> Status:
    cards = _refs(st, f, ins.ref)
    if not cards:
        f.did = False
        return Status.NEXT
    uid = cards[0]
    entry = V.reg().cards[st.cards[uid].def_id]
    if entry.command_aid < 0:
        f.did = False
        return Status.NEXT
    push_frame(
        st,
        V.reg().abilities[entry.command_aid].program_id,
        controller=f.controller,
        host=uid,
        kind="activate_main_sub",
    )
    return Status.PUSHED


def _h_start_battle(st: GameState, f: Frame, ins: d.StartBattle) -> Status:
    attackers = _in_play(st, _refs(st, f, ins.attacker))
    targets = _in_play(st, _refs(st, f, ins.target))
    if not attackers or not targets:
        f.did = False
        return Status.NEXT
    from gcg_sim.engine.battle import damage_only_battle

    damage_only_battle(st, attackers[0], targets[0])
    f.did = True
    return Status.NEXT


def _h_change_target(st: GameState, f: Frame, ins: d.ChangeAttackTarget) -> Status:
    b = st.battle
    targets = _in_play(st, _refs(st, f, ins.target))
    if b is None or b.ended or not targets:
        f.did = False
        return Status.NEXT
    b.target = targets[0]
    b.target_seq = st.cards[targets[0]].zone_seq
    st.touch()
    f.did = True
    return Status.NEXT


def _h_bind(st: GameState, f: Frame, ins: d.BindVar) -> Status:
    f.vars[ins.var] = _refs(st, f, ins.ref)
    return Status.NEXT


def _h_delayed(st: GameState, f: Frame, ins: d.DelayedTrigger) -> Status:
    R = V.reg()
    ab_owner = R.cards[st.cards[f.card_uid].def_id] if f.card_uid >= 0 else None
    index = -1
    card_number = ""
    if ab_owner is not None:
        card_number = ab_owner.card_number
        n = 0
        for aid in (*ab_owner.own, *ab_owner.unit):
            for dt in core._walk_delayed(getattr(R.abilities[aid].ability, "steps", ())):
                if dt == ins and index < 0:
                    index = n
                n += 1
    if index < 0:
        raise core.EngineError("delayed trigger not registered on its card")
    bound: list[tuple[int, int]] = []
    for v in ins.bind_vars:
        for u in f.vars.get(v, ()):
            bound.append((u, st.cards[u].zone_seq))
    le = Lasting(
        effect_key=R.cont_key(d.AbilityGrant(card_number, index)),
        controller=f.controller,
        source_uid=f.host,
        targets=tuple(bound),
        duration=ins.duration.value,
        created_turn=st.turn,
        battle_id=st.battle.battle_id if st.battle else NO_ARG,
    )
    st.delayed.append(le)
    f.did = True
    return Status.NEXT


def _h_pay_cost(st: GameState, f: Frame, ins: d.PayCost) -> Status:
    p = V.player_of(st, ctx_of(f), ins.player)
    f.did = pay_generic(st, p, ins.amount)
    return Status.NEXT


def _h_custom(st: GameState, f: Frame, ins: d.CustomStep) -> Status:
    fn = CUSTOM_STEPS[ins.name]
    f.did = fn(st, f, ctx_of(f), dict(ins.params))
    st.touch()
    return Status.NEXT


# ---------------------------------------------------------------------------------------------
# system programs (engine procedures expressed as DSL so they share decision machinery)

_SYSTEM: dict[str, tuple[d.Step, ...]] = {
    "deploy": (d.DeployCard(d.Var("card")),),
    "pair": (d.Pair(d.Var("pilot"), d.Var("unit")),),
    "hand_limit": (d.Discard(count=d.Sum((d.HandSize(), -core.HAND_LIMIT))),),
}
_SYSTEM_IDS: dict[str, int] = {}


def system_program(name: str) -> int:
    pid = _SYSTEM_IDS.get(name)
    if pid is None:
        pid = V.reg().program(_SYSTEM[name], f"system:{name}")
        _SYSTEM_IDS[name] = pid
    return pid


_HANDLERS: dict[type, Callable[..., Status]] = {
    pr.Jump: _h_jump,
    pr.JumpIfNot: _h_jump_if_not,
    pr.JumpIfNotDid: _h_jump_if_not_did,
    pr.SetDid: _h_set_did,
    pr.AskMay: _h_ask_may,
    pr.ModeSelect: _h_mode,
    pr.LoopInit: _h_loop_init,
    pr.LoopNext: _h_loop_next,
    d.Choose: _h_choose,
    d.Draw: _h_draw,
    d.Discard: _h_discard,
    d.Damage: _h_damage,
    d.DamageShieldArea: _h_damage_shield_area,
    d.DamagePlayer: _h_damage_player,
    d.Destroy: _h_destroy,
    d.Rest: _h_rest,
    d.SetActive: _h_set_active,
    d.ReturnToHand: _h_return_to_hand,
    d.ToDeck: _h_to_deck,
    d.Exile: _h_exile,
    d.ToTrash: _h_to_trash,
    d.AddToHand: _h_add_to_hand,
    d.DeployCard: _h_deploy_card,
    d.DeployToken: _h_deploy_token,
    d.PlaceExResource: _h_place_ex,
    d.PlaceResource: _h_place_resource,
    d.SetResourcesActive: _h_set_resources_active,
    d.RestResources: _h_rest_resources,
    d.ShieldToHand: _h_shield_to_hand,
    d.AddToShields: _h_add_to_shields,
    d.Mill: _h_mill,
    d.LookTop: _h_look_top,
    d.Arrange: _h_arrange,
    d.Reveal: _h_reveal,
    d.Shuffle: _h_shuffle,
    d.Recover: _h_recover,
    d.Apply: _h_apply,
    d.ApplyPlayer: _h_apply_player,
    d.Pair: _h_pair,
    d.PlayCard: _h_play_card,
    d.ActivateMain: _h_activate_main,
    d.StartBattle: _h_start_battle,
    d.ChangeAttackTarget: _h_change_target,
    d.BindVar: _h_bind,
    d.DelayedTrigger: _h_delayed,
    d.PayCost: _h_pay_cost,
    d.CustomStep: _h_custom,
}

_RESUMERS: dict[type, Callable[..., Status]] = {
    pr.AskMay: _r_ask_may,
    pr.ModeSelect: _r_mode,
    d.Choose: _r_choose,
    d.Discard: _r_discard,
    d.DeployCard: _r_excess,
    d.DeployToken: _r_excess,
    d.Arrange: _r_arrange,
}

__all__ = ["CUSTOM_STEPS", "PLAYER_TARGET", "Battle", "run_top_frame"]
