"""State invariants checked by the robustness suite (acceptance criterion 6).

- Card conservation: every non-token card is in exactly one location list, matching its
  ``zone`` field, and each player keeps exactly the cards they started with.
- Legal states: location limits, pairing consistency, non-negative damage only where damage
  can exist, no lethal damage left after rules management, well-formed pending decisions.
- Termination is checked by the caller (games must end by a rules-defined condition).
"""

from __future__ import annotations

from collections import Counter

from gcg_sim.cards.model import CardType
from gcg_sim.effects.dsl import RuleKind
from gcg_sim.engine import core
from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Phase, Zone


class InvariantViolation(AssertionError):
    pass


LISTED_ZONES = tuple(z for z in Zone if z is not Zone.OUTSIDE)


def check(st: GameState, initial: Counter[tuple[int, int]] | None = None) -> None:
    errors: list[str] = []
    db = V.reg().db
    seen: Counter[int] = Counter()
    for p in (0, 1):
        for z in LISTED_ZONES:
            for uid in st.zones[p][z]:
                seen[uid] += 1
                c = st.cards[uid]
                if c.zone is not z:
                    errors.append(f"uid {uid} listed in {z.name} but zone={c.zone.name}")
                if c.owner != p:
                    errors.append(f"uid {uid} owned by {c.owner} listed under player {p}")
    for c in st.cards:
        cd = db.by_id(c.def_id)
        if c.zone is Zone.OUTSIDE:
            if seen[c.uid]:
                errors.append(f"uid {c.uid} is OUTSIDE but listed")
            if not cd.is_token:
                errors.append(f"non-token uid {c.uid} ({cd.card_number}) left the game")
            continue
        if seen[c.uid] != 1:
            errors.append(f"uid {c.uid} ({cd.card_number}) listed {seen[c.uid]} times")
        if cd.is_token and c.zone not in core.TOKEN_ZONES:
            errors.append(f"token uid {c.uid} in {c.zone.name}")
        if c.damage < 0:
            errors.append(f"uid {c.uid} has negative damage")
        if c.damage and c.zone not in (Zone.BATTLE, Zone.BASE):
            errors.append(f"uid {c.uid} has damage {c.damage} in {c.zone.name}")
        if c.zone is Zone.PAIRED:
            u = c.pair
            if u < 0 or st.cards[u].zone is not Zone.BATTLE or st.cards[u].pair != c.uid:
                errors.append(f"pilot uid {c.uid} paired inconsistently with {u}")
        elif c.zone is Zone.BATTLE:
            if c.pair >= 0 and (
                st.cards[c.pair].zone is not Zone.PAIRED or st.cards[c.pair].pair != c.uid
            ):
                errors.append(f"unit uid {c.uid} has inconsistent pilot {c.pair}")
            if not cd.card_type.is_unit:
                errors.append(f"non-unit {cd.card_number} in battle area")
        elif c.pair >= 0:
            errors.append(f"uid {c.uid} in {c.zone.name} still paired with {c.pair}")
        if c.zone is Zone.BASE and not cd.card_type.is_base:
            errors.append(f"non-base {cd.card_number} in base section")
        if c.zone in (Zone.RESOURCE_AREA, Zone.RESOURCE_DECK) and not cd.card_type.is_resource:
            errors.append(f"non-resource {cd.card_number} in {c.zone.name}")
    for p in (0, 1):
        pz = st.zones[p]
        if len(pz[Zone.BATTLE]) > core.BATTLE_LIMIT:
            errors.append(f"player {p} has {len(pz[Zone.BATTLE])} Units (limit 6)")
        if len(pz[Zone.BASE]) > core.BASE_LIMIT:
            errors.append(f"player {p} has {len(pz[Zone.BASE])} Bases")
        n_ex = sum(
            1
            for u in pz[Zone.RESOURCE_AREA]
            if db.by_id(st.cards[u].def_id).card_type is CardType.EX_RESOURCE
        )
        if len(pz[Zone.RESOURCE_AREA]) > core.RESOURCE_LIMIT or n_ex > core.EX_RESOURCE_LIMIT:
            errors.append(f"player {p} resource area over limit")
    if initial is not None:
        now: Counter[tuple[int, int]] = Counter(
            (c.owner, db.base_def_id(c.def_id)) for c in st.cards if not db.by_id(c.def_id).is_token
        )
        if now != initial:
            errors.append(f"card multiset changed: {now - initial} / {initial - now}")
    if st.winner is None:
        if st.pending is None:
            errors.append("no pending decision in an unfinished game")
        elif not st.pending.options:
            errors.append(f"decision {st.pending.kind.value} has no options")
        if st.phase is not Phase.SETUP and not st.frames:
            dv = V.derived(st)
            for p in (0, 1):
                for zz in (Zone.BATTLE, Zone.BASE):
                    for uid in st.zones[p][zz]:
                        if st.cards[uid].damage >= V.hp_of(st, dv, uid) and not V.rules_of(
                            dv, uid, RuleKind.CANT_BE_DESTROYED
                        ):
                            errors.append(f"uid {uid} survives with lethal damage")
    if errors:
        raise InvariantViolation("; ".join(errors[:10]))


def initial_multiset(st: GameState) -> Counter[tuple[int, int]]:
    db = V.reg().db
    return Counter(
        (c.owner, db.base_def_id(c.def_id)) for c in st.cards if not db.by_id(c.def_id).is_token
    )
