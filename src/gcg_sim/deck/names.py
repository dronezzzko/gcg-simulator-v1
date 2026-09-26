"""Card-name normalization used to compare names written in deck files with the card data."""

from __future__ import annotations

import unicodedata

_ZERO_WIDTH = dict.fromkeys(map(ord, "\u200b\u200c\u200d\u2060\ufeff\u00ad"), None)
_QUOTES = str.maketrans(
    {
        "\u2018": "'",
        "\u2019": "'",
        "\u201a": "'",
        "\u201b": "'",
        "\u2032": "'",
        "\u00b4": "'",
        "`": "'",
        "\u201c": '"',
        "\u201d": '"',
        "\u201e": '"',
        "\u2033": '"',
    }
)


def strip_invisible(text: str) -> str:
    return text.translate(_ZERO_WIDTH)


def normalize_name(name: str) -> str:
    """NFKC, zero-width characters removed, curly quotes straightened, whitespace collapsed,
    case folded: ``"Zaku Ⅱ"`` and ``"zaku ii"`` compare equal, as do ``Z’Gok`` and ``Z'Gok``."""
    text = unicodedata.normalize("NFKC", name).translate(_ZERO_WIDTH).translate(_QUOTES)
    return " ".join(text.split()).casefold()


def names_match(written: str, card_names: tuple[str, ...]) -> bool:
    wanted = normalize_name(written)
    return any(normalize_name(n) == wanted for n in card_names)
