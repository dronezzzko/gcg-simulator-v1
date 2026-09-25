"""Process-independent digests of game states.

``GameState.to_json()`` stores some registry indices that are assigned in first-use order
within a process: continuous effects and filter sets of lasting effects (also inside the
once-per-turn keys of damage prevention/reduction), and lazily registered programs. Two
processes that reach the same game state can therefore serialize it differently. The
canonical form replaces each such index with an id derived from the registry entry's content.
"""

from __future__ import annotations

import hashlib
import json
from typing import Any

from gcg_sim.effects.registry import Registry, get_registry
from gcg_sim.engine.core import PREVENT_ONCE_TAG, REDUCE_ONCE_TAG
from gcg_sim.engine.state import GameState

LASTING_EFFECT, LASTING_FILTERS = 0, 9  # positions in Lasting.to_json()
TRIGGER_PROGRAM = 0  # position in TriggerInst.to_json()
LASTING_SOURCE = -1  # rule-key marker of a lasting effect: (-1, effect_key)
ONCE_TAGS = (REDUCE_ONCE_TAG, PREVENT_ONCE_TAG)


def content_id(obj: object) -> str:
    return hashlib.sha256(repr(obj).encode()).hexdigest()[:16]


def _once_key(reg: Registry, key: list[Any]) -> list[Any]:
    if len(key) >= 3 and key[0] in ONCE_TAGS and key[1] == LASTING_SOURCE:
        return [key[0], key[1], content_id(reg.continuous[key[2]]), *key[3:]]
    return key


def canonical_state(st: GameState, reg: Registry | None = None) -> dict[str, Any]:
    """``st.to_json()`` with every process-local registry index replaced by a content id."""
    r = reg or get_registry()
    d = st.to_json()
    for le in (*d["lasting"], *d["delayed"]):
        le[LASTING_EFFECT] = content_id(r.continuous[le[LASTING_EFFECT]])
        if le[LASTING_FILTERS] >= 0:
            le[LASTING_FILTERS] = content_id(r.filter_sets[le[LASTING_FILTERS]])
    for f in d["frames"]:
        f["program_id"] = content_id(r.programs[f["program_id"]])
    for t in (*d["pending_triggers"], *(t for batch in d["batches"] for t in batch)):
        t[TRIGGER_PROGRAM] = content_id(r.programs[t[TRIGGER_PROGRAM]])
    d["once_used"] = sorted((_once_key(r, k) for k in d["once_used"]), key=json.dumps)
    return d


def state_digest(st: GameState, reg: Registry | None = None) -> str:
    """sha256 of the canonical state: equal in every process for equal game states."""
    canonical = canonical_state(st, reg)
    blob = json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(blob).hexdigest()
