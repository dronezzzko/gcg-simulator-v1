"""Information sets: what a player legally knows, and determinization for search.

Both decklists are known to both players (a benchmarking assumption, docs/ASSUMPTIONS.md);
hands, deck order, and Shields are hidden. A player knows the identity of a card instance
when its ``known`` bit is set (public zones, own hand, cards they looked at). Everything else is drawn from the multiset of that owner's decklist minus
the cards whose identity the player knows.
"""

from __future__ import annotations

from collections import Counter

from gcg_sim.engine import view as V
from gcg_sim.engine.state import GameState
from gcg_sim.engine.types import Zone
from gcg_sim.rng import SplitMix64, derive_seed

HIDDEN_ZONES = frozenset({Zone.DECK, Zone.HAND, Zone.SHIELD, Zone.RESOURCE_DECK})


def is_hidden_from(st: GameState, uid: int, observer: int) -> bool:
    c = st.cards[uid]
    return c.zone in HIDDEN_ZONES and not c.known & (1 << observer)


def unknown_instances(st: GameState, observer: int, owner: int) -> list[int]:
    """Uids of ``owner``'s cards whose identity ``observer`` does not know, in uid order."""
    out = [
        uid
        for z in HIDDEN_ZONES
        for uid in st.zones[owner][z]
        if not st.cards[uid].known & (1 << observer)
    ]
    out.sort()
    return out


def unknown_pools(st: GameState, observer: int, owner: int) -> tuple[list[int], list[int]]:
    """(main-deck pool, resource pool): def ids the hidden instances of ``owner`` can be."""
    main = Counter(st.decklists[owner])
    res = Counter(st.resource_decklists[owner])
    db = V.reg().db
    for c in st.cards:
        if c.owner != owner or is_hidden_from(st, c.uid, observer):
            continue
        def_id = db.base_def_id(c.def_id)
        if main[def_id] > 0:
            main[def_id] -= 1
        elif res[def_id] > 0:
            res[def_id] -= 1
    return sorted(main.elements()), sorted(res.elements())


def determinize(st: GameState, observer: int, seed: int) -> GameState:
    """Return a full game state consistent with ``observer``'s information set.

    Hidden identities are resampled from the known decklists and the game RNG (which governs
    future shuffles) is replaced, so the result depends only on what ``observer`` knows plus
    ``seed``.
    """
    s = st.clone()
    rng = SplitMix64(seed)
    for owner in (0, 1):
        slots = unknown_instances(s, observer, owner)
        if not slots:
            continue
        res_slots = [u for u in slots if s.cards[u].zone is Zone.RESOURCE_DECK]
        main_slots = [u for u in slots if s.cards[u].zone is not Zone.RESOURCE_DECK]
        main_pool, res_pool = unknown_pools(s, observer, owner)
        main_pool = _fit(main_pool, len(main_slots))
        res_pool = _fit(res_pool, len(res_slots))
        rng.shuffle(main_pool)
        rng.shuffle(res_pool)
        for uid, def_id in zip(main_slots, main_pool, strict=True):
            s.cards[uid].def_id = def_id
        for uid, def_id in zip(res_slots, res_pool, strict=True):
            s.cards[uid].def_id = def_id
    for h in s.history:
        if h.uid >= 0:
            h.def_id = s.cards[h.uid].def_id
    s.rng = SplitMix64(derive_seed(seed, "hidden-rng"))
    s.touch()
    return s


def _fit(pool: list[int], n: int) -> list[int]:
    """Pools match the slot count unless effects moved cards between players' decks; pad by
    repetition or trim deterministically so the determinized state stays well-formed."""
    if len(pool) >= n:
        return pool[:n]
    if not pool:
        raise ValueError("cannot determinize: empty pool for hidden cards")
    out = list(pool)
    i = 0
    while len(out) < n:
        out.append(pool[i % len(pool)])
        i += 1
    return out


def permute_hidden(st: GameState, observer: int, seed: int) -> GameState:
    """Shuffle the true identities of main-deck cards hidden from ``observer`` and reseed the
    game RNG — a different true state inside the same information set."""
    s = st.clone()
    rng = SplitMix64(seed)
    for owner in (0, 1):
        slots = [
            u
            for u in unknown_instances(s, observer, owner)
            if s.cards[u].zone is not Zone.RESOURCE_DECK
        ]
        ids = [s.cards[u].def_id for u in slots]
        rng.shuffle(ids)
        for uid, def_id in zip(slots, ids, strict=True):
            s.cards[uid].def_id = def_id
    for h in s.history:
        if h.uid >= 0:
            h.def_id = s.cards[h.uid].def_id
    s.rng = SplitMix64(derive_seed(seed, "permuted-rng"))
    s.touch()
    return s


def information_set_key(st: GameState, observer: int) -> tuple[object, ...]:
    """Hashable summary of everything ``observer`` may know (hidden identities masked)."""
    cards = tuple(
        (
            c.uid,
            -1 if is_hidden_from(st, c.uid, observer) else c.def_id,
            c.owner,
            int(c.zone),
            c.rested,
            c.damage,
            c.pair,
        )
        for c in st.cards
    )
    zones = tuple(tuple(tuple(z) for z in pz) for pz in st.zones)
    pending = st.pending.to_json() if st.pending else None
    return (cards, zones, st.turn, st.active, int(st.step), repr(pending))
