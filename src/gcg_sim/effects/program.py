"""Flatten DSL step trees into linear programs with explicit jumps.

A resolving effect is then just ``(program_id, pc, vars)``, which keeps game states cheap to
clone and trivially serializable.
"""

from __future__ import annotations

from dataclasses import dataclass

from gcg_sim.effects import dsl as d


@dataclass(frozen=True, slots=True)
class Jump:
    target: int


@dataclass(frozen=True, slots=True)
class JumpIfNot:
    cond: d.Cond
    target: int


@dataclass(frozen=True, slots=True)
class AskMay:
    prompt: str
    player: d.P
    target_if_no: int


@dataclass(frozen=True, slots=True)
class JumpIfNotDid:
    target: int


@dataclass(frozen=True, slots=True)
class ModeSelect:
    labels: tuple[str, ...]
    targets: tuple[int, ...]
    chooser: d.P
    end: int


@dataclass(frozen=True, slots=True)
class LoopInit:
    """Bind the loop list for a ForEach (``key``) or a counter for Repeat."""

    key: str
    ref: d.Ref | None
    times: d.Value | None


@dataclass(frozen=True, slots=True)
class LoopNext:
    """Advance loop ``key``: bind ``var`` to the next item (ForEach) or decrement (Repeat);
    jump to ``end`` when exhausted."""

    key: str
    var: str
    end: int


@dataclass(frozen=True, slots=True)
class SetDid:
    value: bool


@dataclass(frozen=True, slots=True)
class HoldRules:
    """Enter (+1) or leave (-1) a d.Simultaneous block: no rules management inside it."""

    delta: int


type Instr = (
    d.Step
    | Jump
    | JumpIfNot
    | AskMay
    | JumpIfNotDid
    | ModeSelect
    | LoopInit
    | LoopNext
    | SetDid
    | HoldRules
)


@dataclass(frozen=True, slots=True)
class Program:
    instrs: tuple[Instr, ...]
    label: str


class _Builder:
    def __init__(self) -> None:
        self.out: list[Instr | None] = []
        self.loop_n = 0

    def emit(self, instr: Instr | None) -> int:
        self.out.append(instr)
        return len(self.out) - 1

    def patch(self, at: int, instr: Instr) -> None:
        self.out[at] = instr

    def here(self) -> int:
        return len(self.out)

    def steps(self, steps: tuple[d.Step, ...]) -> None:
        for s in steps:
            self.step(s)

    def step(self, s: d.Step) -> None:
        if isinstance(s, d.If):
            j = self.emit(None)
            self.steps(s.then)
            if s.otherwise:
                j_end = self.emit(None)
                self.patch(j, JumpIfNot(s.cond, self.here()))
                self.steps(s.otherwise)
                self.patch(j_end, Jump(self.here()))
            else:
                self.patch(j, JumpIfNot(s.cond, self.here()))
        elif isinstance(s, d.May):
            j = self.emit(None)
            self.steps(s.steps)
            self.patch(j, AskMay(s.prompt, s.player, self.here()))
        elif isinstance(s, d.Simultaneous):
            self.emit(HoldRules(1))
            self.steps(s.steps)
            self.emit(HoldRules(-1))
        elif isinstance(s, d.IfYouDo):
            j = self.emit(None)
            self.steps(s.steps)
            self.patch(j, JumpIfNotDid(self.here()))
        elif isinstance(s, d.ChooseMode):
            sel = self.emit(None)
            starts: list[int] = []
            ends: list[int] = []
            for _label, body in s.options:
                starts.append(self.here())
                self.steps(body)
                ends.append(self.emit(None))
            end = self.here()
            for e in ends:
                self.patch(e, Jump(end))
            self.patch(
                sel,
                ModeSelect(tuple(lbl for lbl, _ in s.options), tuple(starts), s.chooser, end),
            )
        elif isinstance(s, d.ForEach):
            key = f"__loop{self.loop_n}"
            self.loop_n += 1
            self.emit(LoopInit(key, s.ref, None))
            head = self.emit(None)
            self.steps(s.steps)
            self.emit(Jump(head))
            self.patch(head, LoopNext(key, s.var, self.here()))
        elif isinstance(s, d.Repeat):
            key = f"__loop{self.loop_n}"
            self.loop_n += 1
            self.emit(LoopInit(key, None, s.times))
            head = self.emit(None)
            self.steps(s.steps)
            self.emit(Jump(head))
            self.patch(head, LoopNext(key, "", self.here()))
        else:
            self.emit(s)


def flatten(steps: tuple[d.Step, ...], label: str) -> Program:
    b = _Builder()
    b.steps(steps)
    instrs: list[Instr] = []
    for i in b.out:
        if i is None:
            raise AssertionError("unpatched placeholder in program " + label)
        instrs.append(i)
    return Program(tuple(instrs), label)
