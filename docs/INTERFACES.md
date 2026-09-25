# Frozen interfaces (Phase 4 contract)

These signatures are frozen for parallel implementation. Implementers may add private helpers
and extra optional parameters, but must not change the names, argument order, or semantics
below without the lead's approval. Everything is deterministic: no wall-clock, no global RNG.

## Engine (exists; owned by the lead)

```python
from gcg_sim.engine.game import new_game, legal_actions, apply, step, DeckList, IllegalActionError
from gcg_sim.engine.state import GameState, Action, Decision
from gcg_sim.engine.types import ActionKind, DecisionKind, Zone, Phase, Step, EndReason, PLAYER_TARGET, NO_ARG
from gcg_sim.engine.observe import determinize, permute_hidden, information_set_key, unknown_instances
from gcg_sim.engine import view as V  # V.derived(st), V.ap_of/hp_of, V.cdef(st, uid), V.kw_amount ...
from gcg_sim.rng import SplitMix64, derive_seed
```

- `new_game((DeckList, DeckList), seed, *, chooser=None, turn_limit=200, max_actions=20000) -> GameState`
  advances to the first decision. `chooser` = player who decides who goes first
  (`None` → seeded die roll, rule 6-2-1-4; BO3 games 2–3 pass the loser of the previous game).
- `st.pending: Decision | None` (`player`, `kind`, `options`); `legal_actions(st) == st.pending.options`.
- `apply(st, action)` mutates in place and advances to the next decision. `step(st, a)` clones first.
- `st.clone()` is cheap; `st.to_json()`/`GameState.from_json()` round-trip.
- Game over: `st.winner in (0, 1)` or `-1` (draw: `EndReason.BOTH_DEFEATED` or `TURN_LIMIT`);
  `st.end_reason`, `st.turn`, `st.active` (turn player at the end).
- Decision kinds: `CHOOSE_FIRST` (`GO_FIRST(a=player)`), `REDRAW` (`KEEP`/`REDRAW`), `MAIN`,
  `BLOCK`, `ACTION_STEP`, `ORDER_TRIGGER`, `BURST`/`YES_NO` (`YES`/`NO`), `SELECT`, `EXCESS`,
  `DISCARD`, `ARRANGE`.
- Information sets: an agent for `player` must only use `determinize(st, player, seed)` states for
  look-ahead (hidden identities resampled from the open decklists, game RNG replaced). The true
  `st` may be read only for public information and `player`'s own knowledge
  (`observe.is_hidden_from`).

## Decks — `gcg_sim.deck` (Deck agent)

```python
@dataclass(frozen=True, slots=True)
class DeckEntry:
    count: int
    card_number: str        # canonical (alt-art product ids normalized)
    written: str            # id as written in the file
    name: str | None        # optional name as written (checked against the data)
    line: int

@dataclass(frozen=True, slots=True)
class Deck:
    name: str
    main: tuple[str, ...]        # card numbers, sorted, len 50 when legal
    resources: tuple[str, ...]   # RESOURCE card numbers, len 10 when legal
    entries: tuple[DeckEntry, ...]
    source: str | None

class DeckError(ValueError):     # .problems: list[str]; str(e) is a readable multi-line message

@dataclass(frozen=True, slots=True)
class Violation:
    code: str        # MAIN_SIZE RESOURCE_SIZE COPY_LIMIT COLORS CARD_TYPE BANNED RESTRICTED
                     # BANNED_PAIR ATTRIBUTE_PAIR UNKNOWN_ID NAME_MISMATCH UNIMPLEMENTED SYNTAX
    message: str     # specific, human-readable, names the cards involved
    cards: tuple[str, ...]

def parse_deck(text: str, *, name: str = "deck", source: str | None = None) -> Deck   # raises DeckError(SYNTAX/UNKNOWN_ID/NAME_MISMATCH)
def load_deck(path: str | os.PathLike[str]) -> Deck
def validate_deck(deck: Deck, *, banlist: bool = True) -> list[Violation]            # [] means legal
def require_legal(deck: Deck) -> None                                                # raises DeckError listing every violation
def to_decklist(deck: Deck) -> DeckList
def deck_digest(deck: Deck) -> str                                                   # sha256 of canonical content
```

File format: one entry per line, `<count> <card_number> [name]`; `#` starts a comment line;
blank lines ignored; product ids with `_pN` normalize to the card number; RESOURCE cards form
the resource deck. Legality sources: `src/gcg_sim/data/official/{deck_construction,banlist}.json`.

## AI — `gcg_sim.ai` (AI agent)

```python
class Agent(Protocol):
    name: str
    def choose(self, st: GameState, player: int) -> Action: ...     # st.pending.player == player
    def decision_log(self) -> list[dict[str, Any]]: ...              # [] unless logging enabled

def make_agent(kind: str, *, preset: str = "standard", seed: int = 0, log: bool = False) -> Agent
    # kind: "random" | "greedy" | "mcts";  preset: "standard" | "strong"
PRESETS: Mapping[str, SearchConfig]   # node-count budgets etc.; documented in docs/AI.md
```

Agents are deterministic functions of (construction seed, the calls made). The MCTS agent's
decisions depend only on `player`'s information set. Decision-log entries:
`{"turn", "player", "decision", "chosen", "alternatives": [{"action", "value", "visits"}...]}`.

## Runner — `gcg_sim.runner` (Runner/Reports/CLI agent)

```python
@dataclass(frozen=True)
class BenchmarkConfig:
    deck_under_test: Deck
    benchmark_deck: Deck
    matches: int
    seed: int = 0
    workers: int = 1
    fmt: str = "bo3"              # "bo3" | "bo1"
    ai_preset: str = "standard"
    out_dir: Path = Path("gcg-sim-out")
    decision_log: bool = False
    replays: int = 3              # representative wins and losses to save

def game_seed(master: int, match_index: int, game_index: int) -> int     # derive_seed(master, "match", m, "game", g)
def agent_seed(master: int, match_index: int, game_index: int, seat: int) -> int
def play_game(config, match_index, game_index, chooser: int | None) -> GameRecord
def play_match(config, match_index) -> MatchRecord
def run_benchmark(config, progress: Callable[[int, int], None] | None = None) -> BenchmarkRun
def replay(record: GameRecord, config) -> GameState       # re-applies the recorded actions; same outcome
```

Seats: in match `m` the deck under test sits in seat `m % 2`. Game 1's first-player chooser is the
seeded die roll; games 2–3 are chosen by the previous game's loser (BO3 rules). A game draw
(`BOTH_DEFEATED`) is recorded as a game draw; for BO3 match scoring the turn player at the end
loses (TRM 5.2, see ASSUMPTIONS). Results never depend on `workers`.

`GameRecord` (JSON-serializable dataclass) carries: indices, seed, agent seeds, seats, first
player, `dut_on_play`, winner (`"dut"|"bench"|"draw"`), end reason, turns, the action list
(`[kind, a, b, c]`), redraw decisions, and per-card event data for the deck under test
(opening hand, drawn, played with turn, never-played-while-held) plus benchmark cards seen
in play.

## Reports — `gcg_sim.reports` (Runner/Reports/CLI agent)

```python
def build_results(run: BenchmarkRun) -> dict[str, Any]   # results.json content; byte-stable
def validate_results(obj: dict[str, Any]) -> None       # jsonschema against the packaged schema
def write_reports(run: BenchmarkRun, out_dir: Path) -> ReportPaths
def wilson(successes: int, n: int, z: float = 1.96) -> tuple[float, float]
```

Files: `results.json` (schema `src/gcg_sim/reports/schema/results.v1.schema.json`, `$id` with
version), `summary.md`, `games.ndjson`, `replays/*.json`, and `timing.json` (wall-clock only here).

## CLI — `gcg_sim.cli` (Runner/Reports/CLI agent)

```
gcg-sim benchmark DECK_UNDER_TEST BENCHMARK_DECK --matches N [--seed S] [--workers W]
        [--format bo3|bo1] [--ai-preset standard|strong] [--out DIR] [--decision-log] [--replays K]
gcg-sim validate DECK
gcg-sim data status
gcg-sim replay REPLAY_JSON
```

Invalid input: a clear message on stderr and exit code 2 (usage) or 1 (invalid deck/data).
