from __future__ import annotations

import os
import sys
from pathlib import Path

import pytest

from gcg_sim.cli import AGENT_FACTORY_ENV

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "examples" / "decks"
RANDOM_AGENT = f"{ROOT / 'tests' / 'runner' / 'random_agent.py'}:make_random_agent"
FEDERATION = EXAMPLES / "blue-white-federation.txt"
ZEON = EXAMPLES / "red-green-zeon.txt"
TEKKADAN = EXAMPLES / "purple-white-tekkadan.txt"


@pytest.fixture
def random_agents(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(AGENT_FACTORY_ENV, RANDOM_AGENT)


@pytest.fixture(scope="session")
def console_script() -> Path:
    script = Path(sys.executable).parent / "gcg-sim"
    assert script.is_file(), f"console script not installed next to {sys.executable}"
    return script


@pytest.fixture(scope="session")
def script_env() -> dict[str, str]:
    return {**os.environ, AGENT_FACTORY_ENV: RANDOM_AGENT}
