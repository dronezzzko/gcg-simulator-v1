"""State mutation primitives, events, trigger collection, damage, and rules management."""

from __future__ import annotations

from gcg_sim.cards.model import CardType
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import AbilityEntry
from gcg_sim.engine import view as V
from gcg_sim.engine.state import CardInstance, GameState, HistoryEvent, TriggerInst
from gcg_sim.engine.types import NO_ARG, EndReason, Phase, Step, Zone

BOTH_KNOW = 0b11
HAND_LIMIT = 10
BATTLE_LIMIT = 6
BASE_LIMIT = 1
RESOURCE_LIMIT = 15
EX_RESOURCE_LIMIT = 5
SHIELD_COUNT = 6
START_HAND = 5

REDUCE_ONCE_TAG = -7
PREVENT_ONCE_TAG = -8
REST_SUB_TAG = -9

TOKEN_ZONES = frozenset({Zone.BATTLE, Zone.RESOURCE_AREA, Zone.SHIELD, Zone.BASE})


class EngineError(Exception):
    """Internal invariant violation (a bug), never an expected game outcome."""


# ---------------------------------------------------------------------------------------------
# zone movement


def _remove_from_zone(st: GameState, c: CardInstance) -> None:
    if c.zone is Zone.OUTSIDE or (
        c.zone is Zone.RESOLVING and c.uid not in st.zones[c.owner][Zone.RESOLVING]
    ):
        return
    lst = st.zones[c.owner][c.zone]
    lst.remove(c.uid)


def _knowledge_on_enter(c: CardInstance, src: Zone, dst: Zone, reveal: bool) -> None:
    if dst.is_public or reveal:
        c.known = BOTH_KNOW
    elif dst is Zone.HAND:
        c.known = BOTH_KNOW if src.is_public else (c.known | (1 << c.owner))
    # deck/shield/resource deck keep whatever knowledge players already had of this card


def move(
    st: GameState,
    uid: int,
    dst: Zone,
    *,
    bottom: bool = False,
    reveal: bool = False,
    rested: bool = False,
) -> None:
    """Move a card to ``dst`` of its owner, applying new-card semantics (rule 4-1-5).

    A paired Pilot follows its Unit (rule 3-3-6); a Unit losing its Pilot is unpaired.
    Tokens leaving the field cease to exist after momentarily arriving (rule 5-17-2-5).
    """
    c = st.cards[uid]
    src = c.zone
    if c.pair >= 0:
        other = st.cards[c.pair]
        if src is Zone.BATTLE:
            other.pair = NO_ARG
            c.pair = NO_ARG
            _move_single(st, other, dst, bottom=bottom, reveal=reveal, rested=False)
        elif src is Zone.PAIRED:
            other.pair = NO_ARG
            c.pair = NO_ARG
    _move_single(st, c, dst, bottom=bottom, reveal=reveal, rested=rested)
    st.touch()


def _move_single(
    st: GameState, c: CardInstance, dst: Zone, *, bottom: bool, reveal: bool, rested: bool
) -> None:
    src = c.zone
    _remove_from_zone(st, c)
    c.zone_seq += 1
    c.damage = 0
    c.rested = rested
    c.entered_turn = st.turn
    _knowledge_on_enter(c, src, dst, reveal)
    cd = V.reg().db.by_id(c.def_id)
    if cd.is_token and dst not in TOKEN_ZONES:
        c.zone = Zone.OUTSIDE
        c.known = BOTH_KNOW
        return
    c.zone = dst
    lst = st.zones[c.owner][dst]
    if dst in (Zone.DECK, Zone.SHIELD, Zone.RESOURCE_DECK) and not bottom:
        lst.insert(0, c.uid)
    else:
        lst.append(c.uid)


def remove_from_game(st: GameState, uid: int) -> None:
    """Remove a token from the game entirely (rules 5-17-2-5, 5-17-3-2-3)."""
    c = st.cards[uid]
    _remove_from_zone(st, c)
    c.zone = Zone.OUTSIDE
    c.zone_seq += 1
    c.known = BOTH_KNOW
    st.touch()


def new_card(st: GameState, def_id: int, owner: int, zone: Zone, *, rested: bool = False) -> int:
    uid = len(st.cards)
    c = CardInstance(uid, def_id, owner, Zone.OUTSIDE)
    c.known = BOTH_KNOW
    st.cards.append(c)
    c.zone = zone
    c.rested = rested
    c.entered_turn = st.turn
    st.zones[owner][zone].append(uid)
    st.touch()
    return uid


def shuffle_deck(st: GameState, player: int) -> None:
    deck = st.zones[player][Zone.DECK]
    st.rng.shuffle(deck)
    for uid in deck:
        st.cards[uid].known = 0
    st.touch()


def record(st: GameState, kind: str, player: int, by: int = NO_ARG, uid: int = NO_ARG) -> None:
    def_id = st.cards[uid].def_id if uid >= 0 else NO_ARG
    st.history.append(HistoryEvent(kind, player, by, uid, def_id))


# ---------------------------------------------------------------------------------------------
# events and triggers


def next_group(st: GameState) -> int:
    st.event_group += 1
    return st.event_group


def _subject_matches(
    st: GameState,
    dv: V.Derived,
    host_owner: int,
    host: int,
    sel: d.Sel,
    subject: int,
    include_self: bool,
) -> bool:
    if subject < 0:
        return False
    if not include_self and subject == host:
        return False
    owner = st.cards[subject].owner
    if sel.side is d.Side.FRIENDLY and owner != host_owner:
        return False
    if sel.side is d.Side.ENEMY and owner == host_owner:
        return False
    if sel.filters:
        return V.matches(st, dv, V.Ctx(host_owner, host), subject, sel.filters)
    return True


def _payload(event: tuple[tuple[str, int], ...], key: str) -> int:
    for k, v in event:
        if k == key:
            return v
    return NO_ARG


def _trigger_ok(
    st: GameState,
    dv: V.Derived,
    trig: d.Trigger,
    host: int,
    host_owner: int,
    event: tuple[tuple[str, int], ...],
) -> bool:
    subject = _payload(event, "subject")
    if trig.whose_turn is not None:
        want = host_owner if trig.whose_turn is d.P.YOU else 1 - host_owner
        if st.active != want:
            return False
    if trig.self_only:
        if subject != host:
            return False
    elif trig.subject is not None:
        if not _subject_matches(st, dv, host_owner, host, trig.subject, subject, trig.include_self):
            return False
    elif not trig.include_self and subject == host:
        return False
    if trig.pilot_filters:
        pilot = _payload(event, "pilot")
        if pilot < 0 or not V.matches(st, dv, V.Ctx(host_owner, host), pilot, trig.pilot_filters):
            return False
    if trig.target_filters:
        target = _payload(event, "target")
        if target < 0 or not V.matches(
            st, dv, V.Ctx(host_owner, host), target, trig.target_filters
        ):
            return False
    if trig.battle_only is not None and bool(_payload(event, "battle") == 1) != trig.battle_only:
        return False
    if trig.by_enemy is not None:
        by = _payload(event, "by")
        if (by >= 0 and by != host_owner) != trig.by_enemy:
            return False
    return True


def _once_key(a: AbilityEntry, host: int, host_seq: int) -> tuple[int, ...]:
    return (a.aid, host, host_seq)


def _card_uid_for(st: GameState, a: AbilityEntry, host: int) -> int:
    if a.unit_text:
        c = st.cards[host]
        if c.pair >= 0 and st.cards[c.pair].def_id == a.def_id:
            return c.pair
    return host


def queue_trigger(
    st: GameState,
    a: AbilityEntry,
    host: int,
    controller: int,
    event: tuple[tuple[str, int], ...],
    card_uid: int = NO_ARG,
) -> None:
    ab = a.ability
    assert isinstance(ab, d.Triggered)
    host_seq = st.cards[host].zone_seq
    group = _payload(event, "group")
    for t in st.pending_triggers:
        if (
            t.ability_key[1] == a.aid
            and t.host == host
            and _payload(t.event, "group") == group
            and group >= 0
        ):
            return  # rule 10-1-6-3: simultaneous events trigger an effect only once
    once = _once_key(a, host, host_seq) if ab.once_per_turn else ()
    if once and once in st.once_used:
        return
    st.pending_triggers.append(
        TriggerInst(
            program_id=a.program_id,
            controller=controller,
            host=host,
            host_seq=host_seq,
            card_uid=card_uid if card_uid != NO_ARG else _card_uid_for(st, a, host),
            ability_key=(a.def_id, a.aid, int(a.unit_text)),
            event=event,
            once_key=once,
        )
    )


_LISTEN_CACHE: dict[tuple[tuple[int, ...], tuple[int, ...]], frozenset[d.Ev]] = {}


def clear_listen_cache() -> None:
    _LISTEN_CACHE.clear()


def listened_events(st: GameState) -> frozenset[d.Ev]:
    """Events any card in this game (both decklists, tokens, delayed triggers) can trigger on."""
    key = st.decklists
    got = _LISTEN_CACHE.get(key)
    if got is None:
        R = V.reg()
        evs: set[d.Ev] = set(R.always_events)
        for dl in key:
            for def_id in set(dl):
                evs.update(R.events_by_def.get(def_id, ()))
        got = frozenset(evs)
        if len(_LISTEN_CACHE) > 256:
            _LISTEN_CACHE.clear()
        _LISTEN_CACHE[key] = got
    return got


def emit(
    st: GameState,
    ev: d.Ev,
    subject: int = NO_ARG,
    *,
    player: int = NO_ARG,
    lki: dict[int, list[AbilityEntry]] | None = None,
    **extra: int,
) -> None:
    """Emit an event and queue every triggered effect it fulfils (rule 10-1-6)."""
    if st.winner is not None:
        return
    group = extra.pop("group", NO_ARG)
    if group == NO_ARG:
        group = st.event_group
    items: list[tuple[str, int]] = [("subject", subject), ("player", player), ("group", group)]
    items.extend(sorted(extra.items()))
    event = tuple(items)
    if ev not in listened_events(st) and not st.delayed and not lki:
        return
    dv = V.derived(st)
    lki = lki or {}
    candidates: list[tuple[int, AbilityEntry]] = [
        (host, a) for host, a in V.trigger_index(st, dv).get(ev, ()) if host not in lki
    ]
    for uid in sorted(lki, key=lambda u: (st.cards[u].owner != st.active, u)):
        candidates.extend((uid, a) for a in lki[uid] if a.trigger_event is ev)
    for host, a in candidates:
        ab = a.ability
        assert isinstance(ab, d.Triggered)
        owner = st.cards[host].owner
        if not _trigger_ok(st, dv, ab.trigger, host, owner, event):
            continue
        if ab.cond is not d.TRUE:
            ctx = V.Ctx(owner, host, _card_uid_for(st, a, host), event=event)
            if not V.cond(st, dv, ctx, ab.cond):
                continue
        queue_trigger(st, a, host, owner, event)
    _delayed_triggers(st, dv, ev, event)
    _keyword_triggers(st, dv, ev, event, subject)


def _delayed_triggers(
    st: GameState, dv: V.Derived, ev: d.Ev, event: tuple[tuple[str, int], ...]
) -> None:
    if not st.delayed:
        return
    R = V.reg()
    keep = []
    for le in st.delayed:
        spec = R.continuous[le.effect_key]
        assert isinstance(spec, d.AbilityGrant)
        entry = R.db.get(spec.card_number)
        fired = False
        if entry is not None:
            dt = _find_delayed(
                R.cards[entry.def_id].own + R.cards[entry.def_id].unit, spec.ability_index
            )
            if dt is not None and dt.trigger.event is ev:
                host = le.source_uid
                bound = [u for u, seq in le.targets if st.cards[u].zone_seq == seq]
                subject = _payload(event, "subject")
                if dt.trigger.self_only and le.targets:
                    ok = subject in bound and _trigger_ok(
                        st, dv, dt.trigger, subject, st.cards[subject].owner, event
                    )
                else:
                    ok = _trigger_ok(st, dv, dt.trigger, host, le.controller, event)
                if ok:
                    pid = R.program(dt.steps, f"{spec.card_number}#delayed")
                    st.pending_triggers.append(
                        TriggerInst(
                            program_id=pid,
                            controller=le.controller,
                            host=host,
                            host_seq=st.cards[host].zone_seq,
                            card_uid=host,
                            ability_key=(-1, -1, 0),
                            event=event,
                        )
                    )
                    fired = True
        if not fired:
            keep.append(le)
    st.delayed = keep


def _find_delayed(aids: tuple[int, ...], index: int) -> d.DelayedTrigger | None:
    R = V.reg()
    n = 0
    for aid in aids:
        ab = R.abilities[aid].ability
        steps = getattr(ab, "steps", ())
        for dt in _walk_delayed(steps):
            if n == index:
                return dt
            n += 1
    return None


def _walk_delayed(steps: tuple[d.Step, ...]) -> list[d.DelayedTrigger]:
    out: list[d.DelayedTrigger] = []
    for s in steps:
        if isinstance(s, d.DelayedTrigger):
            out.append(s)
            out.extend(_walk_delayed(s.steps))
        elif isinstance(s, d.If):
            out.extend(_walk_delayed(s.then))
            out.extend(_walk_delayed(s.otherwise))
        elif isinstance(s, (d.May, d.IfYouDo, d.ForEach, d.Repeat)):
            out.extend(_walk_delayed(s.steps))
        elif isinstance(s, d.ChooseMode):
            for _, body in s.options:
                out.extend(_walk_delayed(body))
    return out


def _keyword_triggers(
    st: GameState, dv: V.Derived, ev: d.Ev, event: tuple[tuple[str, int], ...], subject: int
) -> None:
    R = V.reg()
    if ev is d.Ev.TURN_END:
        # rule 13-1-1-1: <Repair> recovers HP at the end of your turn (Q51: only if damaged)
        for uid in st.zones[st.active][Zone.BATTLE]:
            if V.kw_amount(dv, uid, d.Kw.REPAIR) > 0 and st.cards[uid].damage > 0:
                st.pending_triggers.append(
                    TriggerInst(
                        R.repair_program,
                        st.active,
                        uid,
                        st.cards[uid].zone_seq,
                        uid,
                        (-2, 0, 0),
                        event,
                    )
                )
    elif ev is d.Ev.DESTROYS_BY_BATTLE and subject >= 0:
        # rule 13-1-2: <Breach> during your turn when this Unit destroys an enemy Unit with battle damage
        owner = st.cards[subject].owner
        amount = _payload(event, "breach")
        victim = _payload(event, "target")
        if amount <= 0 or owner != st.active or victim < 0:
            return
        if not V.reg().db.by_id(st.cards[victim].def_id).card_type.is_unit:
            return
        victim_owner = 1 - owner
        if not st.zones[victim_owner][Zone.BASE] and not st.zones[victim_owner][Zone.SHIELD]:
            return  # rule 13-1-2-4
        st.pending_triggers.append(
            TriggerInst(
                R.breach_program,
                owner,
                subject,
                st.cards[subject].zone_seq,
                subject,
                (-3, 0, 0),
                event,
            )
        )


def ability_snapshot(st: GameState, uids: list[int]) -> dict[int, list[AbilityEntry]]:
    """Last-known abilities of cards about to leave play (rules 13-2-8-2, 13-2-8-2-1)."""
    dv = V.derived(st)
    return {u: list(dv.abilities.get(u, ())) for u in uids}


# ---------------------------------------------------------------------------------------------
# damage


def _prevented(st: GameState, dv: V.Derived, uid: int, source: int, battle: bool, by: int) -> bool:
    for r in V.rules_of(dv, uid, d.RuleKind.CANT_RECEIVE_DAMAGE):
        if _damage_rule_applies(st, dv, r, uid, source, battle, by):
            if r.rule.once_per_turn:
                key = (PREVENT_ONCE_TAG, *r.key, uid, st.cards[uid].zone_seq)
                if key in st.once_used:
                    continue
                st.once_used.add(key)
            return True
    return False


def _damage_rule_applies(
    st: GameState, dv: V.Derived, r: V.RuleInst, uid: int, source: int, battle: bool, by: int
) -> bool:
    rule = r.rule
    if rule.damage_kind is d.DamageKind.BATTLE and not battle:
        return False
    if rule.damage_kind is d.DamageKind.EFFECT and battle:
        return False
    owner = st.cards[uid].owner
    if rule.source_side is d.Side.ENEMY and (by < 0 or by == owner):
        return False
    if rule.source_side is d.Side.FRIENDLY and by != owner:
        return False
    if rule.source_filters:
        if source < 0:
            return False
        if not V.matches(st, dv, V.Ctx(owner, uid), source, rule.source_filters):
            return False
    return True


def _reduce(
    st: GameState, dv: V.Derived, uid: int, amount: int, source: int, battle: bool, by: int
) -> int:
    for r in V.rules_of(dv, uid, d.RuleKind.REDUCE_DAMAGE):
        if amount <= 0:
            break
        if not _damage_rule_applies(st, dv, r, uid, source, battle, by):
            continue
        ikey = (REDUCE_ONCE_TAG, *r.key, uid, st.cards[uid].zone_seq)
        if r.rule.once_per_turn:
            if ikey in st.once_used:
                continue
            st.once_used.add(ikey)
        amount -= V.value(st, dv, V.Ctx(r.controller, r.source), r.rule.amount)
    return max(0, amount)


def damage_card(st: GameState, uid: int, amount: int, *, source: int, battle: bool, by: int) -> int:
    """Deal damage to a Unit or Base; returns damage actually dealt (rules 5-5, 5-21)."""
    dest, final = resolve_damage(st, uid, amount, source=source, battle=battle, by=by)
    return apply_damage(st, dest, final, source=source, battle=battle, by=by)


def resolve_damage(
    st: GameState, uid: int, amount: int, *, source: int, battle: bool, by: int
) -> tuple[int, int]:
    """Where damage lands and how much after redirection, prevention and reduction, evaluated
    on the current state (so simultaneous battle damage uses the pre-damage state, 8-5-3-2)."""
    if amount <= 0:
        return uid, 0
    c = st.cards[uid]
    if c.zone not in (Zone.BATTLE, Zone.BASE):
        return uid, 0
    dv = V.derived(st)
    if battle:
        for r in V.rules_of(dv, uid, d.RuleKind.REDIRECT_BATTLE_DAMAGE):
            dest = r.aux
            if dest >= 0 and dest != uid and st.cards[dest].zone is Zone.BATTLE:
                return resolve_damage(st, dest, amount, source=source, battle=battle, by=by)
    if _prevented(st, dv, uid, source, battle, by):
        return uid, 0
    return uid, _reduce(st, dv, uid, amount, source, battle, by)


def apply_damage(
    st: GameState, uid: int, amount: int, *, source: int, battle: bool, by: int
) -> int:
    c = st.cards[uid]
    if amount <= 0 or c.zone not in (Zone.BATTLE, Zone.BASE):
        return 0  # rule 5-5-5 / 5-21-2-1
    c.damage += amount
    st.touch()
    emit(
        st,
        d.Ev.DAMAGED,
        uid,
        player=c.owner,
        amount=amount,
        battle=int(battle),
        source=source,
        by=by,
    )
    if source >= 0:
        emit(
            st,
            d.Ev.DEALS_DAMAGE,
            source,
            player=st.cards[source].owner,
            target=uid,
            amount=amount,
            battle=int(battle),
            by=by,
        )
    return amount


def shield_area_protected(st: GameState, player: int, source: int, battle: bool, by: int) -> bool:
    """Player-level protection of a shield area (e.g. "your shield area cards can't receive
    damage from enemy Units that are Lv.3 or lower during this battle")."""
    if not st.lasting:
        return False
    R = V.reg()
    dv = V.derived(st)
    for le in st.lasting:
        if le.player != player:
            continue
        eff = R.continuous[le.effect_key]
        if (
            not isinstance(eff, d.RuleGrant)
            or eff.rule.kind is not d.RuleKind.SHIELD_AREA_PROTECTION
        ):
            continue
        rule = eff.rule
        if rule.damage_kind is d.DamageKind.BATTLE and not battle:
            continue
        if rule.damage_kind is d.DamageKind.EFFECT and battle:
            continue
        if rule.source_side is d.Side.ENEMY and (by < 0 or by == player):
            continue
        if rule.source_filters and (
            source < 0 or not V.matches(st, dv, V.Ctx(player, source), source, rule.source_filters)
        ):
            continue
        return True
    return False


def destroy_shields(
    st: GameState, player: int, uids: list[int], *, battle: bool, source: int, by: int
) -> None:
    """Destroy Shields simultaneously: reveal, offer 【Burst】, then trash (rules 5-10-3, 13-1-7-4)."""
    if not uids:
        return
    R = V.reg()
    group = next_group(st)
    for uid in uids:
        c = st.cards[uid]
        c.known = BOTH_KNOW
        entry = R.cards[c.def_id]
        if entry.has_burst:
            move(st, uid, Zone.RESOLVING, reveal=True)
            st.pending_triggers.append(
                TriggerInst(
                    program_id=R.abilities[entry.burst_aid].program_id,
                    controller=player,
                    host=uid,
                    host_seq=st.cards[uid].zone_seq,
                    card_uid=uid,
                    ability_key=(c.def_id, entry.burst_aid, 0),
                    event=(("subject", uid), ("player", player), ("group", group)),
                    burst=True,
                )
            )
        else:
            move(st, uid, Zone.TRASH, reveal=True)
        record(st, "shield_destroyed", player, by, uid)
    for uid in uids:
        emit(
            st,
            d.Ev.SHIELD_DESTROYED,
            uid,
            player=player,
            battle=int(battle),
            source=source,
            by=by,
            group=group,
        )
        if source >= 0:
            emit(
                st,
                d.Ev.DESTROYS_SHIELD_CARD,
                source,
                player=st.cards[source].owner,
                target=uid,
                battle=int(battle),
                group=group,
            )


def damage_shield_area(
    st: GameState, player: int, amount: int, *, cards: int, source: int, battle: bool, by: int
) -> None:
    """Damage the first card(s) of a shield area: the Base if present, else top Shield(s)."""
    if amount <= 0:
        return
    if shield_area_protected(st, player, source, battle, by):
        return
    base = st.zones[player][Zone.BASE]
    if base:
        damage_card(st, base[0], amount, source=source, battle=battle, by=by)
        return
    shields = st.zones[player][Zone.SHIELD][:cards]
    destroy_shields(st, player, list(shields), battle=battle, source=source, by=by)


# ---------------------------------------------------------------------------------------------
# destruction and rules management


def destroy(
    st: GameState, uids: list[int], *, battle: bool, by: int, source: int = NO_ARG
) -> list[int]:
    """Destroy Units/Bases simultaneously (rules 5-10, 13-2-8). Returns destroyed uids."""
    dv = V.derived(st)
    targets = []
    for u in uids:
        c = st.cards[u]
        if c.zone not in (Zone.BATTLE, Zone.BASE):
            continue
        if any(
            _destroy_rule_applies(st, dv, r, u, battle, by)
            for r in V.rules_of(dv, u, d.RuleKind.CANT_BE_DESTROYED)
        ):
            continue
        targets.append(u)
    if not targets:
        return []
    lki = ability_snapshot(st, targets)
    info: dict[int, dict[str, int]] = {}
    for u in targets:
        c = st.cards[u]
        info[u] = {
            "owner": c.owner,
            "pilot": c.pair,
            "linked": int(u in dv.linked),
            "ap": V.ap_of(st, dv, u),
            "was_zone": int(c.zone),
        }
    group = next_group(st)
    for u in targets:
        move(st, u, Zone.TRASH)
        record(st, "destroyed", info[u]["owner"], by, u)
    for u in targets:
        i = info[u]
        emit(
            st,
            d.Ev.DESTROYED,
            u,
            player=i["owner"],
            lki={k: v for k, v in lki.items() if k in targets},
            battle=int(battle),
            by=by,
            source=source,
            pilot=i["pilot"],
            linked=i["linked"],
            group=group,
        )
    for u in targets:
        if V.reg().db.by_id(st.cards[u].def_id).is_token and st.cards[u].zone is Zone.TRASH:
            remove_from_game(st, u)
    return targets


def _destroy_rule_applies(
    st: GameState, dv: V.Derived, r: V.RuleInst, uid: int, battle: bool, by: int
) -> bool:
    rule = r.rule
    if rule.damage_kind is d.DamageKind.BATTLE and not battle:
        return False
    if rule.damage_kind is d.DamageKind.EFFECT and battle:
        return False
    owner = st.cards[uid].owner
    return not (rule.source_side is d.Side.ENEMY and (by < 0 or by == owner))


def set_winner(st: GameState, losers: set[int], reason: EndReason) -> None:
    if st.winner is not None:
        return
    if len(losers) == 2:
        st.winner = -1
        st.end_reason = EndReason.BOTH_DEFEATED
    else:
        (loser,) = losers
        st.winner = 1 - loser
        st.end_reason = reason
    st.phase = Phase.GAME_OVER
    st.step = Step.GAME_OVER
    st.pending = None


def rules_management(st: GameState) -> None:
    """Rule 11: defeat check (11-2), then destruction of cards whose damage reached their HP
    (11-3), repeated until nothing changes."""
    if st.winner is not None or st.phase is Phase.SETUP:
        return
    while True:
        losers = {p for p in (0, 1) if not st.zones[p][Zone.DECK]}
        if losers:
            set_winner(st, losers, EndReason.DECK_OUT)
            return
        if not V.reg().hp_reduction_possible and not _any_damaged_or_zero_hp(st):
            return
        dv = V.derived(st)
        doomed = []
        for p in (st.active, 1 - st.active):
            for z in (Zone.BATTLE, Zone.BASE):
                for uid in st.zones[p][z]:
                    if st.cards[uid].damage >= V.hp_of(st, dv, uid):
                        doomed.append(uid)
        if not doomed:
            return
        if not destroy(st, doomed, battle=False, by=NO_ARG):
            return


def _any_damaged_or_zero_hp(st: GameState) -> bool:
    db = V.reg().db
    for p in (0, 1):
        for z in (Zone.BATTLE, Zone.BASE):
            for uid in st.zones[p][z]:
                c = st.cards[uid]
                if c.damage > 0 or db.by_id(c.def_id).hp <= 0:
                    return True
    return False


def over(st: GameState) -> bool:
    """Whether the game has ended (a function so type narrowing never assumes it is stable)."""
    return st.winner is not None


def card_type(st: GameState, uid: int) -> CardType:
    return V.reg().db.by_id(st.cards[uid].def_id).card_type
