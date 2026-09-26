"""Agent construction for the runner.

The default factory builds the search AI from ``gcg_sim.ai`` (imported lazily, so the runner
has no import-time dependency on it). Tests inject another factory; for multi-process runs it
must be picklable, which :class:`FactoryRef` guarantees by naming the factory as
``"package.module:attr"`` or ``"/path/to/file.py:attr"`` and resolving it inside each worker.
"""

from __future__ import annotations

import hashlib
import importlib
import importlib.util
import sys
from abc import abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from functools import cache
from pathlib import Path
from typing import Any, Protocol, cast

from gcg_sim.engine.state import Action, GameState

AI_MODULE = "gcg_sim.ai"
DEFAULT_AGENT_KIND = "mcts"


@dataclass(frozen=True, slots=True)
class AgentSpec:
    """What a factory needs to build one seat's agent for one game."""

    role: str
    seat: int
    preset: str
    seed: int
    log: bool


class GameAgent(Protocol):
    name: str

    @abstractmethod
    def choose(self, st: GameState, player: int) -> Action: ...

    @abstractmethod
    def decision_log(self) -> list[dict[str, Any]]: ...


AgentFactory = Callable[[AgentSpec], GameAgent]


class AgentUnavailableError(RuntimeError):
    """The AI package needed by the default agent factory is not installed."""


def default_agent_factory(spec: AgentSpec) -> GameAgent:
    """The benchmark AI: ``gcg_sim.ai.make_agent("mcts", preset=..., seed=..., log=...)``."""
    try:
        ai = importlib.import_module(AI_MODULE)
    except ImportError as exc:
        raise AgentUnavailableError(f"the AI package {AI_MODULE!r} is not available") from exc
    agent = ai.make_agent(DEFAULT_AGENT_KIND, preset=spec.preset, seed=spec.seed, log=spec.log)
    return cast(GameAgent, agent)


def _load_file_module(path: Path) -> Any:
    digest = hashlib.sha256(str(path).encode()).hexdigest()[:12]
    name = f"_gcg_sim_agent_factory_{digest}"
    if name in sys.modules:
        return sys.modules[name]
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load agent factory module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@cache
def resolve_factory(ref: str) -> AgentFactory:
    """Resolve ``"package.module:attr"`` or ``"/path/to/file.py:attr"`` to a factory."""
    target, sep, attr = ref.rpartition(":")
    if not sep or not target or not attr:
        raise ValueError(f"agent factory must look like 'module:attr' or 'file.py:attr': {ref!r}")
    if target.endswith(".py"):
        module = _load_file_module(Path(target).resolve())
    else:
        module = importlib.import_module(target)
    factory = getattr(module, attr, None)
    if not callable(factory):
        raise ValueError(f"agent factory {ref!r} is not a callable attribute")
    return cast(AgentFactory, factory)


@dataclass(frozen=True, slots=True)
class FactoryRef:
    """A picklable reference to an agent factory, resolved (once per process) on first use."""

    ref: str

    def __call__(self, spec: AgentSpec) -> GameAgent:
        return resolve_factory(self.ref)(spec)


def describe_factory(factory: AgentFactory) -> str:
    if factory is default_agent_factory:
        return f"{AI_MODULE}.make_agent({DEFAULT_AGENT_KIND!r})"
    if isinstance(factory, FactoryRef):
        target, _, attr = factory.ref.rpartition(":")
        return f"{Path(target).name}:{attr}" if target.endswith(".py") else factory.ref
    module = getattr(factory, "__module__", "?")
    name = getattr(factory, "__qualname__", type(factory).__qualname__)
    return f"{module}.{name}"
