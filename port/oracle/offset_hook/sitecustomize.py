"""Numbering-shift probe (docs/port/PLAN.md §8.9, §14 R1).

Activated by ``PYTHONPATH=port/oracle/offset_hook`` together with
``GCG_ORACLE_EFFECT_OFFSET=<n>`` (n >= 1). Python imports ``sitecustomize`` at start-up, and
spawned worker processes inherit both variables, so every process of a run is probed.

What it does, without changing any game rule:

- right after ``gcg_sim.effects.registry`` is imported, ``get_registry`` is wrapped so that the
  freshly built registry gets ``n`` distinct dummy continuous effects through
  ``Registry.cont_key`` before anything else uses it. Every real ``effect_key``
  (``effects/registry.py:124-130``) then starts at ``n`` instead of 0. Content ids are
  unaffected (``runner/digest.py:26-49``);
- right after ``gcg_sim.engine.core`` is imported, ``new_card`` is wrapped to track the largest
  uid any game allocates and to fail (``ProbeError``) as soon as a uid reaches ``n``: the probe is
  only meaningful while every uid stays below the offset, so a once-key number equal to a uid can
  only be a uid;
- with ``GCG_ORACLE_OFFSET_REPORT=<file>``, each process appends one JSON line
  ``{"pid", "offset", "max_uid", "effects"}`` to that file at exit.

Without ``GCG_ORACLE_EFFECT_OFFSET`` (or with 0) it installs nothing.
"""

from __future__ import annotations

import atexit
import functools
import importlib
import importlib.abc
import importlib.machinery
import json
import os
import sys
from collections.abc import Callable, Sequence
from types import ModuleType
from typing import Any

OFFSET_ENV = "GCG_ORACLE_EFFECT_OFFSET"
REPORT_ENV = "GCG_ORACLE_OFFSET_REPORT"
DUMMY_TRAIT_PREFIX = "__oracle_offset_probe_"
_TARGETS = ("gcg_sim.effects.registry", "gcg_sim.engine.core")


class ProbeError(RuntimeError):
    """A uid reached the effect-key offset, so the probe cannot separate uids from keys.

    It pickles as a plain ``RuntimeError``, so a parent process without the probe on its path
    (``traces.py`` runs its workers with it) still receives the message."""

    def __reduce__(self) -> tuple[type[RuntimeError], tuple[str]]:
        return RuntimeError, (f"ProbeError: {self}",)


_state: dict[str, int] = {"max_uid": -1, "effects": 0}


def _offset() -> int:
    text = os.environ.get(OFFSET_ENV, "0").strip() or "0"
    value = int(text)
    if value < 0:
        raise ValueError(f"{OFFSET_ENV} must be >= 0 (got {value})")
    return value


def _patch_registry(module: ModuleType, n: int) -> None:
    original: Callable[[], Any] = module.get_registry
    dsl = importlib.import_module("gcg_sim.effects.dsl")

    @functools.cache
    def get_registry() -> Any:
        reg = original()
        if not getattr(reg, "_oracle_offset_applied", False):
            if reg.continuous:
                raise ProbeError("the registry already holds continuous effects")
            for i in range(n):
                reg.cont_key(dsl.TraitGrant(traits=(f"{DUMMY_TRAIT_PREFIX}{i}",)))
            reg._oracle_offset_applied = True
            _state["effects"] = len(reg.continuous)
        return reg

    vars(module)["get_registry"] = get_registry


def _patch_core(module: ModuleType, n: int) -> None:
    original: Callable[..., int] = module.new_card

    def new_card(*args: Any, **kwargs: Any) -> int:
        uid = original(*args, **kwargs)
        _state["max_uid"] = max(_state["max_uid"], uid)
        if uid >= n:
            raise ProbeError(f"uid {uid} reached the effect-key offset {n} ({OFFSET_ENV})")
        return uid

    vars(module)["new_card"] = new_card


class _Loader(importlib.abc.Loader):
    def __init__(self, inner: importlib.abc.Loader, name: str, n: int) -> None:
        self._inner = inner
        self._name = name
        self._n = n

    def create_module(self, spec: importlib.machinery.ModuleSpec) -> ModuleType | None:
        return self._inner.create_module(spec)

    def exec_module(self, module: ModuleType) -> None:
        self._inner.exec_module(module)
        if self._name == "gcg_sim.effects.registry":
            _patch_registry(module, self._n)
        else:
            _patch_core(module, self._n)


class _Finder(importlib.abc.MetaPathFinder):
    def __init__(self, n: int) -> None:
        self._n = n

    def find_spec(
        self,
        fullname: str,
        path: Sequence[str] | None,
        target: ModuleType | None = None,
    ) -> importlib.machinery.ModuleSpec | None:
        if fullname not in _TARGETS:
            return None
        for finder in sys.meta_path:
            if finder is self or not hasattr(finder, "find_spec"):
                continue
            spec = finder.find_spec(fullname, path, target)
            if spec is not None and spec.loader is not None:
                spec.loader = _Loader(spec.loader, fullname, self._n)
                return spec
        return None


def _report() -> None:
    path = os.environ.get(REPORT_ENV)
    if not path:
        return
    line = {"pid": os.getpid(), "offset": _offset(), **_state}
    with open(path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(line, sort_keys=True) + "\n")


def install() -> bool:
    """Install the probe if ``GCG_ORACLE_EFFECT_OFFSET`` is set to n >= 1."""
    n = _offset()
    if n == 0:
        return False
    if any(name in sys.modules for name in _TARGETS):
        raise ProbeError("gcg_sim was imported before the probe could install")
    sys.meta_path.insert(0, _Finder(n))
    atexit.register(_report)
    return True


install()
