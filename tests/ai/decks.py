"""Two legal decks built only from implemented cards, used by the AI tests.

Each list is 50 main-deck card numbers (1-2 colours, at most 4 copies, no banned or
restricted cards, at most one vanilla Lv.2/cost 1/2 AP/2 HP Unit number) plus 10 ``R-001``
Resources.
"""

from __future__ import annotations

from gcg_sim.engine.game import DeckList

RESOURCES: tuple[str, ...] = ("R-001",) * 10

# Blue/White Earth Federation: Gundam + Amuro links, Guncannon/Sayla, Blockers, rest/AP- tricks.
FEDERATION_COUNTS: dict[str, int] = {
    "ST01-001": 3,  # Gundam (Lv4, link Amuro Ray, <Repair 2>)
    "GD01-013": 2,  # Gundam (Lv4, link Amuro Ray)
    "ST01-002": 2,  # Gundam (MA Form)
    "GD01-004": 3,  # Guncannon (<Repair 1>, link White Base Team)
    "GD01-008": 3,  # Guntank (Deploy: 1 damage to a rested enemy)
    "ST01-005": 4,  # GM (vanilla Lv2)
    "ST01-008": 3,  # Demi Trainer (<Blocker>)
    "ST01-009": 2,  # Zowort (<Blocker>)
    "GD01-018": 3,  # ReZEL
    "GD01-016": 2,  # Jegan
    "GD01-017": 2,  # Stark Jegan
    "ST01-006": 2,  # Gundam Aerial (Permet Score Six)
    "ST01-010": 4,  # Amuro Ray (Pilot)
    "GD01-087": 2,  # Sayla Mass (Pilot, White Base Team)
    "ST01-011": 2,  # Suletta Mercury (Pilot)
    "ST01-012": 2,  # Thoroughly Damaged
    "ST01-014": 3,  # Unforeseen Incident (Burst)
    "GD01-099": 2,  # Intercept Orders (Burst)
    "ST01-016": 2,  # Asticassia School of Technology, Earth House (Base)
    "GD01-124": 2,  # Side 7 (Base)
}

# Red/White SEED: Strike/Aegis links with Kira/Athrun, Moebius Blockers, bounce and burn.
SEED_COUNTS: dict[str, int] = {
    "ST04-006": 2,  # Aegis Gundam (link Athrun Zala)
    "ST04-007": 2,  # Aegis Gundam (MA Mode) (<Breach 3>)
    "ST04-001": 2,  # Aile Strike Gundam (<Blocker>)
    "ST04-002": 3,  # Strike Gundam (Deploy: draw 1, discard 1)
    "GD01-077": 2,  # Strike Gundam (link Kira Yamato)
    "ST04-003": 2,  # Moebius Zero
    "ST04-004": 3,  # Moebius (<Blocker>)
    "ST04-005": 3,  # Strike Dagger
    "ST04-008": 4,  # Ginn (vanilla Lv2)
    "ST04-009": 2,  # Miguel's Ginn
    "GD01-054": 3,  # Duel Gundam
    "GD01-064": 2,  # DINN
    "ST04-010": 4,  # Kira Yamato (Pilot)
    "ST04-011": 3,  # Athrun Zala (Pilot)
    "ST04-013": 3,  # Hawk of Endymion (bounce)
    "ST04-014": 2,  # The Magic Bullet of Dusk
    "GD01-115": 2,  # Zeon Remnant Forces
    "GD01-116": 2,  # Stealth Stratagem
    "ST04-016": 2,  # Vesalius (Base)
    "ST04-015": 2,  # Archangel (Base)
}


def expand(counts: dict[str, int]) -> tuple[str, ...]:
    return tuple(number for number, n in sorted(counts.items()) for _ in range(n))


FEDERATION: tuple[str, ...] = expand(FEDERATION_COUNTS)
SEED: tuple[str, ...] = expand(SEED_COUNTS)
FEDERATION_DECK = DeckList(FEDERATION, RESOURCES)
SEED_DECK = DeckList(SEED, RESOURCES)
DECKS: tuple[DeckList, DeckList] = (FEDERATION_DECK, SEED_DECK)
