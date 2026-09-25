"""Attacking and battles (rules section 8, keyword effects 13-1)."""

from __future__ import annotations

from gcg_sim.effects import dsl as d
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import Battle, GameState
from gcg_sim.engine.types import NO_ARG, PLAYER_TARGET, EndReason, Step, Zone


def declare_attack(st: GameState, attacker: int, target: int) -> None:
    """Rule 8-2-1: rest the attacker and declare the target; then 【Attack】 effects trigger (8-2-2)."""
    a = st.cards[attacker]
    a.rested = True
    st.touch()
    defender = 1 - a.owner
    b = Battle(
        attacker=attacker,
        attacker_seq=a.zone_seq,
        target=target,
        target_seq=st.cards[target].zone_seq if target >= 0 else 0,
        defender=defender,
        battle_id=st.next_battle_id,
    )
    st.next_battle_id += 1
    st.battles.append(b)
    core.record(st, "attack", a.owner, a.owner, attacker)
    core.next_group(st)
    core.emit(st, d.Ev.ATTACKS, attacker, player=a.owner, target=target, defender=defender)
    st.step = Step.ATTACK_END_CHECK


def battle_broken(st: GameState, b: Battle) -> bool:
    """Rules 8-2-4, 8-3-5, 8-4-2: attacker or targeted Unit left its location."""
    a = st.cards[b.attacker]
    if a.zone is not Zone.BATTLE or a.zone_seq != b.attacker_seq:
        return True
    if b.target >= 0:
        t = st.cards[b.target]
        if t.zone not in (Zone.BATTLE, Zone.BASE) or t.zone_seq != b.target_seq:
            return True
    return False


def eligible_blockers(st: GameState, b: Battle) -> list[int]:
    """Rules 8-3-1..8-3-3, 13-1-4, 13-1-6."""
    dv = V.derived(st)
    if V.has_kw(dv, b.attacker, d.Kw.HIGH_MANEUVER) or V.rules_of(
        dv, b.attacker, d.RuleKind.CANT_BE_BLOCKED
    ):
        return []
    no_vs = V.rules_of(dv, b.attacker, d.RuleKind.NO_BLOCKER_VS)
    out = []
    for uid in st.zones[b.defender][Zone.BATTLE]:
        c = st.cards[uid]
        if c.rested or uid == b.target:
            continue
        if not V.has_kw(dv, uid, d.Kw.BLOCKER):
            continue
        if V.rules_of(dv, uid, d.RuleKind.CANT_BLOCK):
            continue
        blocked_by_rule = False
        for r in no_vs:
            if not r.rule.source_filters or V.matches(
                st, dv, V.Ctx(r.controller, r.source), uid, r.rule.source_filters
            ):
                blocked_by_rule = True
        if blocked_by_rule:
            continue
        out.append(uid)
    return out


def block(st: GameState, blocker: int) -> None:
    """Rule 13-1-4-1: rest the Blocker and change the attack target to it (5-22-2)."""
    b = st.battle
    assert b is not None
    c = st.cards[blocker]
    c.rested = True
    b.target = blocker
    b.target_seq = c.zone_seq
    b.blocked = True
    st.touch()
    core.record(st, "block", c.owner, c.owner, blocker)
    group = core.next_group(st)
    core.emit(st, d.Ev.BLOCKS, blocker, player=c.owner, attacker=b.attacker, group=group)
    core.emit(
        st, d.Ev.BLOCKED, b.attacker, player=st.cards[b.attacker].owner, blocker=blocker, group=group
    )


def _destroy_battle(st: GameState, victims: dict[int, int]) -> list[int]:
    """Destroy Units/Bases with lethal damage after battle damage; ``victims`` maps uid → destroyer uid."""
    dv = V.derived(st)
    doomed = [
        u
        for u in victims
        if st.cards[u].zone in (Zone.BATTLE, Zone.BASE) and st.cards[u].damage >= V.hp_of(st, dv, u)
    ]
    if not doomed:
        return []
    breach = {src: V.kw_amount(dv, src, d.Kw.BREACH) for src in set(victims.values())}
    destroyed = core.destroy(st, doomed, battle=True, by=NO_ARG)
    group = core.next_group(st)
    for u in destroyed:
        src = victims[u]
        if st.cards[src].owner == st.cards[u].owner:
            continue
        if V.reg().db.by_id(st.cards[u].def_id).card_type.is_base:
            core.emit(
                st,
                d.Ev.DESTROYS_SHIELD_CARD,
                src,
                player=st.cards[src].owner,
                target=u,
                battle=1,
                group=group,
            )
            continue
        core.emit(
            st,
            d.Ev.DESTROYS_BY_BATTLE,
            src,
            player=st.cards[src].owner,
            target=u,
            breach=breach.get(src, 0),
            group=group,
        )
    return destroyed


def _exchange(st: GameState, attacker: int, target: int, first_strike: bool) -> None:
    dv = V.derived(st)
    a_owner = st.cards[attacker].owner
    t_owner = st.cards[target].owner
    atk_ap = V.ap_of(st, dv, attacker)
    tgt_ap = V.ap_of(st, dv, target)
    if first_strike:
        core.damage_card(st, target, atk_ap, source=attacker, battle=True, by=a_owner)
        if _destroy_battle(st, {target: attacker}):
            return  # rule 13-1-5-2: no damage from a destroyed target
        if (
            st.cards[target].zone in (Zone.BATTLE, Zone.BASE)
            and st.cards[attacker].zone is Zone.BATTLE
        ):
            dv = V.derived(st)
            core.damage_card(
                st, attacker, V.ap_of(st, dv, target), source=target, battle=True, by=t_owner
            )
            _destroy_battle(st, {attacker: target})
        return
    core.damage_card(st, target, atk_ap, source=attacker, battle=True, by=a_owner)
    core.damage_card(st, attacker, tgt_ap, source=target, battle=True, by=t_owner)
    _destroy_battle(st, {target: attacker, attacker: target})  # rule 8-5-3-2-3: simultaneous


def damage_step(st: GameState) -> None:
    """Rule 8-5."""
    b = st.battle
    assert b is not None
    dv = V.derived(st)
    attacker = b.attacker
    a_owner = st.cards[attacker].owner
    first_strike = V.has_kw(dv, attacker, d.Kw.FIRST_STRIKE)
    if b.target == PLAYER_TARGET:
        defender = b.defender
        base = st.zones[defender][Zone.BASE]
        if (base or st.zones[defender][Zone.SHIELD]) and core.shield_area_protected(
            st, defender, attacker, True, a_owner
        ):
            return
        if base:
            _exchange(st, attacker, base[0], first_strike)  # rules 8-5-2-4, 8-5-2-4-2
        elif st.zones[defender][Zone.SHIELD]:
            ap = V.ap_of(st, dv, attacker)
            if ap >= 1:  # Q35: each Shield has 1 HP; 0 damage is not dealt (5-5-5)
                n = 2 if V.has_kw(dv, attacker, d.Kw.SUPPRESSION) else 1  # rule 13-1-7
                shields = list(st.zones[defender][Zone.SHIELD][:n])
                core.destroy_shields(
                    st, defender, shields, battle=True, source=attacker, by=a_owner
                )
        else:
            ap = V.ap_of(st, dv, attacker)
            if ap > 0:  # rules 8-5-2-2, 1-2-2-1
                core.record(st, "player_damage", defender, a_owner, attacker)
                core.set_winner(st, {defender}, EndReason.BATTLE_DAMAGE)
        return
    target = b.target
    if st.cards[target].zone is Zone.BASE:
        _exchange(st, attacker, target, first_strike)
        return
    _exchange(st, attacker, target, first_strike)


def end_battle(st: GameState) -> None:
    """Rule 8-6-1: "during this battle" effects end."""
    b = st.battle
    assert b is not None
    b.ended = True
    st.lasting = [
        le
        for le in st.lasting
        if not (le.duration == "this_battle" and le.battle_id == b.battle_id)
    ]
    st.delayed = [
        le
        for le in st.delayed
        if not (le.duration == "this_battle" and le.battle_id == b.battle_id)
    ]
    st.touch()
    core.next_group(st)
    core.emit(st, d.Ev.BATTLE_END, b.attacker, player=st.cards[b.attacker].owner, target=b.target)


def damage_only_battle(st: GameState, attacker: int, target: int) -> None:
    """Rules 5-22-3, 5-22-4, 13-1-5-4: begin a battle performing only the damage step."""
    a = st.cards[attacker]
    t = st.cards[target]
    b = Battle(
        attacker=attacker,
        attacker_seq=a.zone_seq,
        target=target,
        target_seq=t.zone_seq,
        defender=t.owner,
        battle_id=st.next_battle_id,
        damage_only=True,
        attack_triggers=False,
    )
    st.next_battle_id += 1
    st.battles.append(b)
    damage_step(st)
    if st.winner is None:
        end_battle(st)
    st.battles.pop()
    st.touch()
