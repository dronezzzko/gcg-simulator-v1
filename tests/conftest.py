"""Opt-in paranoid mode: ``GCG_SIM_PARANOID_VIEW=1 uv run pytest`` checks every cached derived
view against a fresh computation in every test."""

from __future__ import annotations

import os
from collections.abc import Iterator

import pytest
from tests.paranoid import ViewChecker


@pytest.fixture(autouse=True)
def paranoid_view(monkeypatch: pytest.MonkeyPatch) -> Iterator[None]:
    if not os.environ.get("GCG_SIM_PARANOID_VIEW"):
        yield
        return
    checker = ViewChecker()
    checker.install(monkeypatch)
    yield
    assert not checker.stale, checker.stale[:5]
