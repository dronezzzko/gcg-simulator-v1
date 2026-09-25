"""Token definitions embedded in effect text (rule 5-17).

Effects create tokens with inline definitions such as ``[Zaku Ⅱ]((Zeon)･AP1･HP1)`` or
``[Bit / Funnel]((Long-Range Weapon)･AP2･HP2･This Unit can't be paired with a Pilot or attack)``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

TOKEN_DEF = re.compile(
    r"\[(?P<name>[^\[\]]+)\]\(\((?P<traits>[^)]*(?:\)\s*\([^)]*)*)\)\s*[･・]\s*AP(?P<ap>\d+)\s*[･・]\s*HP(?P<hp>\d+)"
    r"(?:\s*[･・]\s*(?P<text>[^()]*(?:\([^()]*\)[^()]*)*))?\)"
)


@dataclass(frozen=True, slots=True)
class TokenSpec:
    name: str
    traits: tuple[str, ...]
    ap: int
    hp: int
    text: str

    @property
    def key(self) -> str:
        traits = "/".join(self.traits)
        return f"{self.name}|{traits}|{self.ap}|{self.hp}|{self.text}"


def normalize_token_text(text: str) -> str:
    return " ".join(text.replace("・", "･").split()).strip().rstrip(".") + (
        "." if text.strip() else ""
    )


def parse_token_specs(effect: str) -> list[TokenSpec]:
    specs: list[TokenSpec] = []
    for m in TOKEN_DEF.finditer(effect):
        traits = tuple(t.strip() for t in re.findall(r"\(([^)]*)\)", "(" + m.group("traits") + ")"))
        text = (m.group("text") or "").strip()
        specs.append(
            TokenSpec(
                name=m.group("name").strip(),
                traits=tuple(t for t in traits if t),
                ap=int(m.group("ap")),
                hp=int(m.group("hp")),
                text=normalize_token_text(text) if text else "",
            )
        )
    return specs
