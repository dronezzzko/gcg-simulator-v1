"""Platform-independent deterministic randomness.

All randomness in the simulator flows through :class:`SplitMix64` so that results are
identical across Python versions, operating systems, and process layouts.
"""

from __future__ import annotations

import hashlib
from collections.abc import MutableSequence

_MASK64 = (1 << 64) - 1


class SplitMix64:
    """SplitMix64 generator whose entire state is one 64-bit integer (cheap to clone)."""

    __slots__ = ("state",)

    def __init__(self, seed: int) -> None:
        self.state = seed & _MASK64

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & _MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & _MASK64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & _MASK64
        return z ^ (z >> 31)

    def randrange(self, n: int) -> int:
        """Uniform integer in ``[0, n)`` using unbiased rejection sampling."""
        if n <= 0:
            raise ValueError("randrange() requires n > 0")
        limit = (1 << 64) - ((1 << 64) % n)
        while True:
            x = self.next_u64()
            if x < limit:
                return x % n

    def random(self) -> float:
        return (self.next_u64() >> 11) * (1.0 / (1 << 53))

    def shuffle[T](self, items: MutableSequence[T]) -> None:
        for i in range(len(items) - 1, 0, -1):
            j = self.randrange(i + 1)
            items[i], items[j] = items[j], items[i]

    def copy(self) -> SplitMix64:
        return SplitMix64(self.state)


def derive_seed(master: int, *path: object) -> int:
    """Derive an independent 64-bit seed from a master seed and a label path.

    Used so that per-match and per-game seeds depend only on (master seed, indices), never on
    worker scheduling.
    """
    material = repr((master & _MASK64, *path)).encode()
    return int.from_bytes(hashlib.sha256(material).digest()[:8], "big")
