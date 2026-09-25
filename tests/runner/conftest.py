from __future__ import annotations

from pathlib import Path

import pytest

from gcg_sim.deck import Deck, load_deck
from gcg_sim.runner import BenchmarkConfig, FactoryRef

EXAMPLES = Path(__file__).resolve().parents[2] / "examples" / "decks"
RANDOM_AGENT = f"{Path(__file__).resolve().with_name('random_agent.py')}:make_random_agent"


@pytest.fixture(scope="session")
def random_factory() -> FactoryRef:
    return FactoryRef(RANDOM_AGENT)


@pytest.fixture(scope="session")
def federation() -> Deck:
    return load_deck(EXAMPLES / "blue-white-federation.txt")


@pytest.fixture(scope="session")
def zeon() -> Deck:
    return load_deck(EXAMPLES / "red-green-zeon.txt")


@pytest.fixture(scope="session")
def tekkadan() -> Deck:
    return load_deck(EXAMPLES / "purple-white-tekkadan.txt")


@pytest.fixture
def config(federation: Deck, zeon: Deck) -> BenchmarkConfig:
    return BenchmarkConfig(federation, zeon, matches=6, seed=1234)
