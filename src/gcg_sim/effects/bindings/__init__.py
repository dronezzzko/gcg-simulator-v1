"""Per-card bindings for effects the text compiler cannot handle.

Every module in this package is imported automatically (sorted by name); no registry file
needs editing. A module registers cards with :func:`card` and deterministic custom hooks
with :func:`custom_step`, :func:`custom_cond`, :func:`custom_filter`, :func:`custom_value`.

    @card("GD01-001")
    def gd01_001(c: CardDef) -> CardScript:
        return CardScript(c.card_number, abilities=(...,), source="binding")
"""

from __future__ import annotations

import importlib
import pkgutil
from collections.abc import Callable
from functools import cache

from gcg_sim.cards.model import CardDef
from gcg_sim.effects.dsl import CardScript

BindingFn = Callable[[CardDef], CardScript]

_BINDINGS: dict[str, tuple[str, BindingFn]] = {}


class DuplicateBindingError(Exception):
    pass


def card(*card_numbers: str) -> Callable[[BindingFn], BindingFn]:
    def deco(fn: BindingFn) -> BindingFn:
        for n in card_numbers:
            if n in _BINDINGS and _BINDINGS[n][1] is not fn:
                raise DuplicateBindingError(f"{n} bound in {_BINDINGS[n][0]} and {fn.__module__}")
            _BINDINGS[n] = (fn.__module__, fn)
        return fn

    return deco


def custom_step(name: str) -> Callable[[Callable[..., bool]], Callable[..., bool]]:
    from gcg_sim.engine.interp import CUSTOM_STEPS

    def deco(fn: Callable[..., bool]) -> Callable[..., bool]:
        if name in CUSTOM_STEPS and CUSTOM_STEPS[name] is not fn:
            raise DuplicateBindingError(f"custom step {name} defined twice")
        CUSTOM_STEPS[name] = fn
        return fn

    return deco


def custom_cond(name: str) -> Callable[[Callable[..., bool]], Callable[..., bool]]:
    from gcg_sim.engine.view import CUSTOM_CONDS

    def deco(fn: Callable[..., bool]) -> Callable[..., bool]:
        if name in CUSTOM_CONDS and CUSTOM_CONDS[name] is not fn:
            raise DuplicateBindingError(f"custom cond {name} defined twice")
        CUSTOM_CONDS[name] = fn
        return fn

    return deco


def custom_filter(name: str) -> Callable[[Callable[..., bool]], Callable[..., bool]]:
    from gcg_sim.engine.view import CUSTOM_FILTERS

    def deco(fn: Callable[..., bool]) -> Callable[..., bool]:
        if name in CUSTOM_FILTERS and CUSTOM_FILTERS[name] is not fn:
            raise DuplicateBindingError(f"custom filter {name} defined twice")
        CUSTOM_FILTERS[name] = fn
        return fn

    return deco


def custom_value(name: str) -> Callable[[Callable[..., int]], Callable[..., int]]:
    from gcg_sim.engine.view import CUSTOM_VALUES

    def deco(fn: Callable[..., int]) -> Callable[..., int]:
        if name in CUSTOM_VALUES and CUSTOM_VALUES[name] is not fn:
            raise DuplicateBindingError(f"custom value {name} defined twice")
        CUSTOM_VALUES[name] = fn
        return fn

    return deco


@cache
def load_bindings() -> dict[str, tuple[str, BindingFn]]:
    for mod in sorted(pkgutil.iter_modules(__path__), key=lambda m: m.name):
        if mod.name.startswith("_"):
            continue
        importlib.import_module(f"{__name__}.{mod.name}")
    return dict(_BINDINGS)
