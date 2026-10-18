"""Opt-in paranoid mode: ``GCG_SIM_PARANOID_VIEW=1 uv run pytest`` runs the engine-cache checks
of ``tests/paranoid.py`` in every test."""

from __future__ import annotations

import os
from collections.abc import Iterator

import pytest
from tests.paranoid import CacheChecker


@pytest.fixture(autouse=True)
def paranoid_view(monkeypatch: pytest.MonkeyPatch) -> Iterator[None]:
    if not os.environ.get("GCG_SIM_PARANOID_VIEW"):
        yield
        return
    checker = CacheChecker()
    checker.install(monkeypatch)
    yield
    assert not checker.stale, checker.stale[:5]
