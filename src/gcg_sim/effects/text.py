"""Effect text normalization and ability splitting.

Rule 2-11-4: parenthesised explanatory notes have no influence on the game, so reminder
text is stripped. Parenthesised traits such as ``(Zeon)`` and inline token definitions such as
``[Zaku Ⅱ]((Zeon)･AP1･HP1)`` are kept.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass

_WS = re.compile(r"[ \t　]+")


def _is_reminder(inner: str) -> bool:
    s = inner.strip()
    if not s:
        return False
    if s.startswith("(") and s.endswith(")") and s.count("(") == 1:
        return False
    if re.fullmatch(r"[^.()]*", s) and not re.search(
        r"\b(this|the|a|an|when|while|to|of|your)\b", s, re.I
    ):
        return False  # a trait such as "(Earth Federation)"
    return s.endswith((".", ".)")) or len(s.split()) >= 4


def strip_reminders(text: str) -> str:
    """Remove top-level reminder parentheticals that are not part of token definitions."""
    out: list[str] = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "(" and not (i > 0 and text[i - 1] == "]"):
            depth = 0
            j = i
            while j < n:
                if text[j] == "(":
                    depth += 1
                elif text[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            inner = text[i + 1 : j]
            if j < n and _is_reminder(inner):
                i = j + 1
                continue
            out.append(text[i : j + 1])
            i = j + 1
            continue
        if ch == "[" and i + 1 < n:
            # copy a token definition "[Name]((...)･AP1･HP1)" verbatim
            m = re.match(r"\[[^\[\]]+\]\(\((?:[^()]|\([^()]*\))*\)", text[i:])
            if m:
                out.append(m.group(0))
                i += m.end()
                continue
        out.append(ch)
        i += 1
    return "".join(out)


def normalize(text: str) -> str:
    """Canonical text used for compilation and golden hashing."""
    t = (
        text.replace("・", "･")
        .replace("\r\n", "\n")
        .replace("’", "'")
        .replace("“", '"')
        .replace("”", '"')
    )
    t = t.replace("【Activate ･Main】", "【Activate･Main】").replace(
        "【Activate ･Action】", "【Activate･Action】"
    )
    t = t.replace("【Activate･ Main】", "【Activate･Main】").replace(
        "【Activate･ Action】", "【Activate･Action】"
    )
    t = t.replace("：", ":")
    t = strip_reminders(t)
    lines = []
    for raw in t.split("\n"):
        line = _WS.sub(" ", raw).strip()
        if line:
            lines.append(line)
    return "\n".join(lines)


def text_hash(text: str) -> str:
    return hashlib.sha256(normalize(text).encode()).hexdigest()[:16]


@dataclass(frozen=True, slots=True)
class RawAbility:
    """One ability line: its leading bracket markers and the body text."""

    markers: tuple[str, ...]
    body: str
    line: str


_MARKER = re.compile(r"^【([^】]+)】")
_KEYWORD_ONLY = re.compile(r"^(<[^>]+>\s*)+$")


def split_abilities(normalized: str) -> list[RawAbility]:
    if normalized.strip() in ("", "-"):
        return []
    out: list[RawAbility] = []
    for line in normalized.split("\n"):
        if line in ("-",):
            continue
        rest = line
        markers: list[str] = []
        while True:
            m = _MARKER.match(rest)
            if not m:
                break
            markers.append(m.group(1).strip())
            rest = rest[m.end() :].lstrip()
            if rest.startswith("/"):
                rest = rest[1:].lstrip()
        out.append(RawAbility(tuple(markers), rest.strip(), line))
    return out
