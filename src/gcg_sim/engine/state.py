"""Game state: plain data, cheap to clone, and JSON-serializable."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from gcg_sim.engine.types import NO_ARG, ActionKind, DecisionKind, EndReason, Phase, Step, Zone
from gcg_sim.rng import SplitMix64

N_ZONES = len(Zone)


@dataclass(frozen=True, slots=True)
class Action:
    """A player's choice. ``a``/``b``/``c`` are integer parameters whose meaning depends on ``kind``."""

    kind: ActionKind
    a: int = NO_ARG
    b: int = NO_ARG
    c: int = NO_ARG

    def to_json(self) -> list[Any]:
        return [self.kind.value, self.a, self.b, self.c]

    @staticmethod
    def from_json(data: list[Any]) -> Action:
        return Action(ActionKind(data[0]), int(data[1]), int(data[2]), int(data[3]))

    def __str__(self) -> str:
        args = [str(x) for x in (self.a, self.b, self.c) if x != NO_ARG]
        return f"{self.kind.value}({','.join(args)})"


@dataclass(frozen=True, slots=True)
class Decision:
    """A pending choice for ``player`` with its complete list of legal options."""

    player: int
    kind: DecisionKind
    options: tuple[Action, ...]
    prompt: str = ""
    context: tuple[tuple[str, int], ...] = ()

    def ctx(self, key: str, default: int = NO_ARG) -> int:
        for k, v in self.context:
            if k == key:
                return v
        return default

    def to_json(self) -> dict[str, Any]:
        return {
            "player": self.player,
            "kind": self.kind.value,
            "options": [o.to_json() for o in self.options],
            "prompt": self.prompt,
            "context": [list(kv) for kv in self.context],
        }

    @staticmethod
    def from_json(d: dict[str, Any]) -> Decision:
        return Decision(
            player=int(d["player"]),
            kind=DecisionKind(d["kind"]),
            options=tuple(Action.from_json(o) for o in d["options"]),
            prompt=str(d["prompt"]),
            context=tuple((str(k), int(v)) for k, v in d["context"]),
        )


class CardInstance:
    """One physical card (or token) in the game.

    ``zone_seq`` increments on every zone change so that effects bound to the previous
    incarnation stop applying (rule 4-1-5). ``known`` is a bitmask of players who know this
    card's identity (bit ``1 << player``).
    """

    __slots__ = (
        "damage",
        "def_id",
        "entered_turn",
        "known",
        "owner",
        "pair",
        "rested",
        "uid",
        "zone",
        "zone_seq",
    )

    def __init__(self, uid: int, def_id: int, owner: int, zone: Zone) -> None:
        self.uid = uid
        self.def_id = def_id
        self.owner = owner
        self.zone = zone
        self.rested = False
        self.damage = 0
        self.pair = NO_ARG
        self.zone_seq = 0
        self.entered_turn = 0
        self.known = 0

    def copy(self) -> CardInstance:
        c = CardInstance.__new__(CardInstance)
        c.uid = self.uid
        c.def_id = self.def_id
        c.owner = self.owner
        c.zone = self.zone
        c.rested = self.rested
        c.damage = self.damage
        c.pair = self.pair
        c.zone_seq = self.zone_seq
        c.entered_turn = self.entered_turn
        c.known = self.known
        return c

    def to_json(self) -> list[int]:
        return [
            self.uid,
            self.def_id,
            self.owner,
            int(self.zone),
            int(self.rested),
            self.damage,
            self.pair,
            self.zone_seq,
            self.entered_turn,
            self.known,
        ]

    @staticmethod
    def from_json(d: list[int]) -> CardInstance:
        c = CardInstance(d[0], d[1], d[2], Zone(d[3]))
        c.rested = bool(d[4])
        c.damage = d[5]
        c.pair = d[6]
        c.zone_seq = d[7]
        c.entered_turn = d[8]
        c.known = d[9]
        return c


@dataclass(slots=True)
class Battle:
    """The battle in progress (rule 8)."""

    attacker: int
    attacker_seq: int
    target: int  # uid of a Unit/Base, or PLAYER_TARGET
    target_seq: int
    defender: int  # defending player index
    battle_id: int
    blocked: bool = False
    damage_only: bool = False
    attack_triggers: bool = True
    ended: bool = False

    def copy(self) -> Battle:
        return Battle(
            self.attacker,
            self.attacker_seq,
            self.target,
            self.target_seq,
            self.defender,
            self.battle_id,
            self.blocked,
            self.damage_only,
            self.attack_triggers,
            self.ended,
        )

    def to_json(self) -> list[int]:
        return [
            self.attacker,
            self.attacker_seq,
            self.target,
            self.target_seq,
            self.defender,
            self.battle_id,
            int(self.blocked),
            int(self.damage_only),
            int(self.attack_triggers),
            int(self.ended),
        ]

    @staticmethod
    def from_json(d: list[int]) -> Battle:
        return Battle(
            d[0], d[1], d[2], d[3], d[4], d[5], bool(d[6]), bool(d[7]), bool(d[8]), bool(d[9])
        )


@dataclass(slots=True)
class Lasting:
    """A lasting effect created by a resolved step (``Apply``) or player-level (``ApplyPlayer``).

    ``targets`` are (uid, zone_seq) pairs; the effect ignores targets that changed zones.
    ``player`` >= 0 marks a player-level effect. ``expires`` encodes the duration boundary.
    """

    effect_key: int  # index into the program table's continuous-effect registry
    controller: int
    source_uid: int
    targets: tuple[tuple[int, int], ...]
    duration: str
    created_turn: int
    battle_id: int
    player: int = NO_ARG
    uses: int = 0
    filters_key: int = NO_ARG
    aux: int = NO_ARG
    serial: int = NO_ARG

    def copy(self) -> Lasting:
        return Lasting(
            self.effect_key,
            self.controller,
            self.source_uid,
            self.targets,
            self.duration,
            self.created_turn,
            self.battle_id,
            self.player,
            self.uses,
            self.filters_key,
            self.aux,
            self.serial,
        )

    def to_json(self) -> list[Any]:
        return [
            self.effect_key,
            self.controller,
            self.source_uid,
            [list(t) for t in self.targets],
            self.duration,
            self.created_turn,
            self.battle_id,
            self.player,
            self.uses,
            self.filters_key,
            self.aux,
            self.serial,
        ]

    @staticmethod
    def from_json(d: list[Any]) -> Lasting:
        return Lasting(
            int(d[0]),
            int(d[1]),
            int(d[2]),
            tuple((int(a), int(b)) for a, b in d[3]),
            str(d[4]),
            int(d[5]),
            int(d[6]),
            int(d[7]),
            int(d[8]),
            int(d[9]),
            int(d[10]),
            int(d[11]),
        )


@dataclass(slots=True)
class TriggerInst:
    """A triggered effect waiting to resolve (rule 10-1-6)."""

    program_id: int
    controller: int
    host: int
    host_seq: int
    card_uid: int
    ability_key: tuple[int, int, int]  # (provider def_id, ability index, unit-text flag)
    event: tuple[tuple[str, int], ...]
    burst: bool = False
    once_key: tuple[int, ...] = ()
    optional: bool = False

    def copy(self) -> TriggerInst:
        return TriggerInst(
            self.program_id,
            self.controller,
            self.host,
            self.host_seq,
            self.card_uid,
            self.ability_key,
            self.event,
            self.burst,
            self.once_key,
            self.optional,
        )

    def to_json(self) -> list[Any]:
        return [
            self.program_id,
            self.controller,
            self.host,
            self.host_seq,
            self.card_uid,
            list(self.ability_key),
            [list(kv) for kv in self.event],
            int(self.burst),
            list(self.once_key),
            int(self.optional),
        ]

    @staticmethod
    def from_json(d: list[Any]) -> TriggerInst:
        ak = d[5]
        return TriggerInst(
            int(d[0]),
            int(d[1]),
            int(d[2]),
            int(d[3]),
            int(d[4]),
            (int(ak[0]), int(ak[1]), int(ak[2])),
            tuple((str(k), int(v)) for k, v in d[6]),
            bool(d[7]),
            tuple(int(x) for x in d[8]),
            bool(d[9]),
        )


@dataclass(slots=True)
class Frame:
    """A resolving effect: a program counter into a flat program plus bound variables."""

    program_id: int
    controller: int
    host: int
    host_seq: int
    card_uid: int
    kind: str
    event: tuple[tuple[str, int], ...] = ()
    pc: int = 0
    vars: dict[str, tuple[int, ...]] = field(default_factory=dict)
    did: bool = True
    answer: bool = False
    waiting: bool = False
    buf: list[int] = field(default_factory=list)
    ints: dict[str, int] = field(default_factory=dict)
    once_key: tuple[int, ...] = ()
    acted: bool = False

    def copy(self) -> Frame:
        return Frame(
            self.program_id,
            self.controller,
            self.host,
            self.host_seq,
            self.card_uid,
            self.kind,
            self.event,
            self.pc,
            dict(self.vars),
            self.did,
            self.answer,
            self.waiting,
            list(self.buf),
            dict(self.ints),
            self.once_key,
            self.acted,
        )

    def ev(self, key: str, default: int = NO_ARG) -> int:
        for k, v in self.event:
            if k == key:
                return v
        return default

    def to_json(self) -> dict[str, Any]:
        return {
            "program_id": self.program_id,
            "controller": self.controller,
            "host": self.host,
            "host_seq": self.host_seq,
            "card_uid": self.card_uid,
            "kind": self.kind,
            "event": [list(kv) for kv in self.event],
            "pc": self.pc,
            "vars": {k: list(v) for k, v in sorted(self.vars.items())},
            "did": self.did,
            "answer": self.answer,
            "waiting": self.waiting,
            "buf": list(self.buf),
            "ints": dict(sorted(self.ints.items())),
            "once_key": list(self.once_key),
            "acted": self.acted,
        }

    @staticmethod
    def from_json(d: dict[str, Any]) -> Frame:
        return Frame(
            program_id=int(d["program_id"]),
            controller=int(d["controller"]),
            host=int(d["host"]),
            host_seq=int(d["host_seq"]),
            card_uid=int(d["card_uid"]),
            kind=str(d["kind"]),
            event=tuple((str(k), int(v)) for k, v in d["event"]),
            pc=int(d["pc"]),
            vars={str(k): tuple(int(x) for x in v) for k, v in d["vars"].items()},
            did=bool(d["did"]),
            answer=bool(d["answer"]),
            waiting=bool(d["waiting"]),
            buf=[int(x) for x in d["buf"]],
            ints={str(k): int(v) for k, v in d["ints"].items()},
            once_key=tuple(int(x) for x in d["once_key"]),
            acted=bool(d["acted"]),
        )


@dataclass(slots=True)
class HistoryEvent:
    """Per-turn event log entry used by "during a turn where ..." conditions."""

    kind: str
    player: int
    by: int
    uid: int
    def_id: int
    source: int = NO_ARG  # the card whose effect caused the event, when known

    def to_json(self) -> list[Any]:
        return [self.kind, self.player, self.by, self.uid, self.def_id, self.source]

    @staticmethod
    def from_json(d: list[Any]) -> HistoryEvent:
        return HistoryEvent(str(d[0]), int(d[1]), int(d[2]), int(d[3]), int(d[4]), int(d[5]))


class GameState:
    """Complete state of one game. Mutated in place by the engine; use :meth:`clone` for search."""

    __slots__ = (
        "_derived",
        "action_count",
        "active",
        "batches",
        "battles",
        "cards",
        "decklists",
        "delayed",
        "effect_hits",
        "end_reason",
        "event_group",
        "first_player",
        "frames",
        "history",
        "lasting",
        "max_actions",
        "next_battle_id",
        "next_lasting_serial",
        "once_used",
        "passes",
        "pending",
        "pending_triggers",
        "phase",
        "priority",
        "redraws",
        "resource_decklists",
        "rng",
        "seed",
        "setup_chooser",
        "step",
        "turn",
        "turn_limit",
        "version",
        "winner",
        "zones",
    )

    def __init__(self, seed: int) -> None:
        self.seed = seed
        self.rng = SplitMix64(seed)
        self.cards: list[CardInstance] = []
        self.zones: list[list[list[int]]] = [[[] for _ in range(N_ZONES)] for _ in range(2)]
        self.turn = 0
        self.active = 0
        self.first_player = 0
        self.setup_chooser = 0
        self.phase = Phase.SETUP
        self.step = Step.SETUP_CHOOSE_FIRST
        self.pending: Decision | None = None
        self.battles: list[Battle] = []
        self.next_battle_id = 1
        self.next_lasting_serial = 0
        self.priority = 0
        self.passes = 0
        self.lasting: list[Lasting] = []
        self.delayed: list[Lasting] = []
        self.pending_triggers: list[TriggerInst] = []
        self.batches: list[list[TriggerInst]] = []
        self.frames: list[Frame] = []
        self.once_used: set[tuple[int, ...]] = set()
        self.history: list[HistoryEvent] = []
        self.winner: int | None = None
        self.end_reason: EndReason | None = None
        self.decklists: tuple[tuple[int, ...], tuple[int, ...]] = ((), ())
        self.resource_decklists: tuple[tuple[int, ...], tuple[int, ...]] = ((), ())
        self.redraws = [False, False]
        self.action_count = 0
        self.event_group = 0
        self.effect_hits: dict[int, int] = {}
        self.turn_limit = 200
        self.max_actions = 20000
        self.version = 0
        self._derived: Any = None

    # -- basic accessors -------------------------------------------------------------------
    def zone(self, player: int, zone: Zone) -> list[int]:
        return self.zones[player][zone]

    @property
    def standby(self) -> int:
        return 1 - self.active

    @property
    def battle(self) -> Battle | None:
        return self.battles[-1] if self.battles else None

    @property
    def game_over(self) -> bool:
        return self.winner is not None

    def touch(self) -> None:
        self.version += 1
        self._derived = None

    # -- cloning -----------------------------------------------------------------------------
    def clone(self) -> GameState:
        s = GameState.__new__(GameState)
        s.seed = self.seed
        s.rng = self.rng.copy()
        s.cards = [c.copy() for c in self.cards]
        s.zones = [[list(z) for z in pz] for pz in self.zones]
        s.turn = self.turn
        s.active = self.active
        s.first_player = self.first_player
        s.setup_chooser = self.setup_chooser
        s.phase = self.phase
        s.step = self.step
        s.pending = self.pending
        s.battles = [b.copy() for b in self.battles]
        s.next_battle_id = self.next_battle_id
        s.next_lasting_serial = self.next_lasting_serial
        s.priority = self.priority
        s.passes = self.passes
        s.lasting = [x.copy() for x in self.lasting]
        s.delayed = [x.copy() for x in self.delayed]
        s.pending_triggers = [t.copy() for t in self.pending_triggers]
        s.batches = [[t.copy() for t in b] for b in self.batches]
        s.frames = [f.copy() for f in self.frames]
        s.once_used = set(self.once_used)
        s.history = list(self.history)
        s.winner = self.winner
        s.end_reason = self.end_reason
        s.decklists = self.decklists
        s.resource_decklists = self.resource_decklists
        s.redraws = list(self.redraws)
        s.action_count = self.action_count
        s.event_group = self.event_group
        s.effect_hits = dict(self.effect_hits)
        s.turn_limit = self.turn_limit
        s.max_actions = self.max_actions
        s.version = self.version
        s._derived = self._derived
        return s

    # -- serialization -------------------------------------------------------------------------
    def to_json(self) -> dict[str, Any]:
        return {
            "seed": self.seed,
            "rng": self.rng.state,
            "cards": [c.to_json() for c in self.cards],
            "zones": [[list(z) for z in pz] for pz in self.zones],
            "turn": self.turn,
            "active": self.active,
            "first_player": self.first_player,
            "setup_chooser": self.setup_chooser,
            "phase": int(self.phase),
            "step": int(self.step),
            "pending": self.pending.to_json() if self.pending else None,
            "battles": [b.to_json() for b in self.battles],
            "next_battle_id": self.next_battle_id,
            "next_lasting_serial": self.next_lasting_serial,
            "priority": self.priority,
            "passes": self.passes,
            "lasting": [x.to_json() for x in self.lasting],
            "delayed": [x.to_json() for x in self.delayed],
            "pending_triggers": [t.to_json() for t in self.pending_triggers],
            "batches": [[t.to_json() for t in b] for b in self.batches],
            "frames": [f.to_json() for f in self.frames],
            "once_used": sorted(list(k) for k in self.once_used),
            "history": [h.to_json() for h in self.history],
            "winner": self.winner,
            "end_reason": self.end_reason.value if self.end_reason else None,
            "decklists": [list(d) for d in self.decklists],
            "resource_decklists": [list(d) for d in self.resource_decklists],
            "redraws": list(self.redraws),
            "action_count": self.action_count,
            "event_group": self.event_group,
            "effect_hits": [[k, v] for k, v in sorted(self.effect_hits.items())],
            "turn_limit": self.turn_limit,
            "max_actions": self.max_actions,
            "version": self.version,
        }

    @staticmethod
    def from_json(d: dict[str, Any]) -> GameState:
        s = GameState(int(d["seed"]))
        s.rng = SplitMix64(int(d["rng"]))
        s.cards = [CardInstance.from_json(c) for c in d["cards"]]
        s.zones = [[[int(u) for u in z] for z in pz] for pz in d["zones"]]
        s.turn = int(d["turn"])
        s.active = int(d["active"])
        s.first_player = int(d["first_player"])
        s.setup_chooser = int(d["setup_chooser"])
        s.phase = Phase(int(d["phase"]))
        s.step = Step(int(d["step"]))
        s.pending = Decision.from_json(d["pending"]) if d["pending"] else None
        s.battles = [Battle.from_json(b) for b in d["battles"]]
        s.next_battle_id = int(d["next_battle_id"])
        s.next_lasting_serial = int(d["next_lasting_serial"])
        s.priority = int(d["priority"])
        s.passes = int(d["passes"])
        s.lasting = [Lasting.from_json(x) for x in d["lasting"]]
        s.delayed = [Lasting.from_json(x) for x in d["delayed"]]
        s.pending_triggers = [TriggerInst.from_json(t) for t in d["pending_triggers"]]
        s.batches = [[TriggerInst.from_json(t) for t in b] for b in d["batches"]]
        s.frames = [Frame.from_json(f) for f in d["frames"]]
        s.once_used = {tuple(int(x) for x in k) for k in d["once_used"]}
        s.history = [HistoryEvent.from_json(h) for h in d["history"]]
        s.winner = None if d["winner"] is None else int(d["winner"])
        s.end_reason = EndReason(d["end_reason"]) if d["end_reason"] else None
        dl = d["decklists"]
        s.decklists = (tuple(int(x) for x in dl[0]), tuple(int(x) for x in dl[1]))
        rl = d["resource_decklists"]
        s.resource_decklists = (tuple(int(x) for x in rl[0]), tuple(int(x) for x in rl[1]))
        s.redraws = [bool(x) for x in d["redraws"]]
        s.action_count = int(d["action_count"])
        s.event_group = int(d["event_group"])
        s.effect_hits = {int(k): int(v) for k, v in d["effect_hits"]}
        s.turn_limit = int(d["turn_limit"])
        s.max_actions = int(d["max_actions"])
        s.version = int(d["version"])
        return s
