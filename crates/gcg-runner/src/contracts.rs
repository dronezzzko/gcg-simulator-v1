//! Contract types of `gcg-runner` (docs/dev/PLAN.md §7, §10.3.6, §12.7), owned by the lead
//! (§10.1 S).
//!
//! Workstream E implements the functions frozen in docs/dev/contracts.md (gcg-runner
//! section) with real bodies; only the lead edits this file (§10.6 change protocol). Fields
//! marked `pub(crate)` are E's starting layout of an opaque type.
//!
//! Records keep Python's field order (`runner/records.py`). Their `to_json` writes the keys in
//! that order, except `MatchRecord.to_json`, which writes `games` last (`records.py:176-184`).
//! Every report writer sorts keys (`reports/writer.py:34`, `:56`), so the order matters only
//! for in-memory parity. Python sources are cited relative to `src/gcg_sim/`.

use std::io;
use std::path::PathBuf;

use gcg_ai::{Agent, AgentEffort, AgentError, LogEntry, SearchSettings};
use gcg_core::SplitMix64;
use gcg_core::sys::topology::Topology;
use gcg_data::Deck;
use gcg_engine::{Action, ActionKind, EndReason, EngineError, UnimplementedCardError};

// ---------------------------------------------------------------------------------------------
// Command-line constants shared by gcg-sim and gcg-eval (cli.py:32-33, :397-399)

/// `EXIT_OK` (`cli.py:33`).
pub const EXIT_OK: u8 = 0;
/// `EXIT_INVALID` (`cli.py:33`): invalid deck, data or replay, or a failed simulation.
pub const EXIT_INVALID: u8 = 1;
/// `EXIT_USAGE` (`cli.py:33`): argparse usage errors.
pub const EXIT_USAGE: u8 = 2;
/// `EXIT_INTERRUPTED` (`cli.py:33`): SIGINT (PLAN §7.5).
pub const EXIT_INTERRUPTED: u8 = 130;
/// Printed to stderr on SIGINT, without the `error: ` prefix (`cli.py:397-399`).
pub const INTERRUPTED_MESSAGE: &str = "interrupted: workers stopped, no reports written";
/// `BenchmarkConfig.out_dir` and `--out` default (`runner/config.py:27`, `cli.py:81`).
pub const DEFAULT_OUT_DIR: &str = "gcg-sim-out";
/// `BenchmarkConfig.replays` and `--replays` default (`runner/config.py:29`, `cli.py:83-85`).
pub const DEFAULT_REPLAYS: u32 = 3;

// ---------------------------------------------------------------------------------------------
// Configuration (runner/config.py)

/// `DUT` / `BENCH` (`runner/records.py:11-12`): the role of a seat. The deck under test sits
/// in seat `m % 2` (`runner/config.py:64-66`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Role {
    Dut,
    Bench,
}

impl Role {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dut => "dut",
            Self::Bench => "bench",
        }
    }
}

/// A game or match result label: `DUT`, `BENCH` or `DRAW` (`runner/records.py:11-13`;
/// `result_label`, `runner/game.py:38-39`; `_winner`, `runner/match.py:11-16`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Outcome {
    Dut,
    Bench,
    Draw,
}

impl Outcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dut => "dut",
            Self::Bench => "bench",
            Self::Draw => "draw",
        }
    }
}

/// `--format` (`FORMATS = ("bo3", "bo1")`, `runner/config.py:13`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Format {
    Bo3,
    Bo1,
}

impl Format {
    /// `FORMATS`, in argparse `choices` order.
    pub const ALL: [Self; 2] = [Self::Bo3, Self::Bo1];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bo3 => "bo3",
            Self::Bo1 => "bo1",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == value)
    }

    /// `games_per_match` (`runner/config.py:38-40`).
    #[must_use]
    pub const fn games_per_match(self) -> u32 {
        match self {
            Self::Bo3 => 3,
            Self::Bo1 => 1,
        }
    }

    /// `wins_needed` (`runner/config.py:42-44`).
    #[must_use]
    pub const fn wins_needed(self) -> u32 {
        match self {
            Self::Bo3 => 2,
            Self::Bo1 => 1,
        }
    }
}

/// `--workers N` (`cli.py:78`, default 1) or the additive `--workers auto` (PLAN A5, §7.4),
/// which resolves through `gcg_core::sys::topology::Topology::auto_workers`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Workers {
    Count(u32),
    Auto,
}

/// The results schema a binary writes (PLAN A10, §12.7): `gcg-eval reference-benchmark`
/// writes Python's 1.1.0 byte for byte, `gcg-sim` 1.2.0.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SchemaVersion {
    V1_1,
    V1_2,
}

impl SchemaVersion {
    /// `schema_version` and `versions.results_schema`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1_1 => "1.1.0",
            Self::V1_2 => "1.2.0",
        }
    }

    /// `schema` (`SCHEMA_ID`, `reports/versions.py:17`).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::V1_1 => "urn:gcg-sim:schema:results:1.1.0",
            Self::V1_2 => "urn:gcg-sim:schema:results:1.2.0",
        }
    }
}

/// `BenchmarkConfig` (`runner/config.py:18-44`), Python's 12 fields in order, then three
/// Rust-only fields: the schema the binary writes and the `versions()` inputs
/// (`gcg_data::versions(package, code_digest)`). Python's `__post_init__` problems
/// (`:47-61`) are `BenchmarkConfig::problems` (E); the CLI validates first, so they are not
/// reachable from the command line.
#[derive(Clone, PartialEq, Debug)]
pub struct BenchmarkConfig {
    pub deck_under_test: Deck,
    pub benchmark_deck: Deck,
    pub matches: u32,
    pub seed: u64,
    pub workers: Workers,
    /// Python `fmt`.
    pub format: Format,
    /// `ai_preset`: `AgentSpec.preset` (`runner/game.py:70`), `results.json` `ai.preset`
    /// (`reports/results.py:126`) and the reproduce line (`reports/writer.py:48`). `gcg-sim`:
    /// `standard`/`fast`; `gcg-eval reference-benchmark`: `fast`/`standard`/`strong`.
    pub ai_preset: &'static str,
    pub out_dir: PathBuf,
    pub decision_log: bool,
    pub replays: u32,
    pub turn_limit: i32,
    pub max_actions: i64,
    pub schema: SchemaVersion,
    pub package: &'static str,
    pub code_digest: &'static str,
}

/// `AgentSpec(role, seat, preset, seed, log)` (`runner/agents.py:28-36`), what a factory needs
/// to build one seat's agent for one game; filled by `_make_agents` (`runner/game.py:62-76`)
/// with `seed = agent_seed(master, m, g, seat)` and `log = config.decision_log`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AgentSpec {
    pub role: Role,
    pub seat: u8,
    pub preset: &'static str,
    pub seed: u64,
    pub log: bool,
}

/// `AgentFactory = Callable[[AgentSpec], GameAgent]` (`runner/agents.py:49`), shared by the
/// pool's workers. Seat 0's agent is built before seat 1's, every game
/// (`runner/game.py:76`).
pub trait AgentFactory: Sync {
    /// `factory(spec)`. An error becomes the game's failure, wrapped as
    /// `"match {m} game {g+1} (seed {seed}) failed: {Class}: {message}"`
    /// (`runner/game.py:130-136`).
    ///
    /// # Errors
    /// Whatever building the agent raises (for example an unknown preset).
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError>;

    /// `describe_factory(factory)` (`runner/agents.py:106-114`): `results.json` `ai.factory`
    /// (`reports/results.py:125`), masked by the normaliser (PLAN A6). See
    /// `gcg_ai_reference::FACTORY_DESCRIPTION`, `gcg_search::Preset::factory_description` and
    /// [`TestAgentKind::factory_description`].
    fn describe(&self) -> String;

    /// The configuration of the strengthened search this factory's agents run, which the
    /// runner records in [`BenchmarkRun::search`] for results 1.2.0 `ai.search` (PLAN §12.7,
    /// O9): `Some` for the `gcg-sim` presets (`gcg_search::search_settings`), `None`, the
    /// default, for every other factory (the reference search, the test agents).
    fn search_block(&self) -> Option<SearchSettings> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Records (runner/records.py)

/// `PlayEvent(card_number, turn, own_turn)` (`runner/records.py:18-31`); JSON
/// `[card_number, turn, own_turn]`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PlayEvent {
    pub card_number: Box<str>,
    pub turn: i32,
    /// `own_turn(turn, seat, first_player)` (`runner/config.py:83-85`).
    pub own_turn: i32,
}

/// `DutCardEvents` (`runner/records.py:34-71`): per-card observations of the deck under test in
/// one game, card numbers one per copy, each list sorted except `played` (the `numbers`
/// helper of `runner/instrument.py:94-107`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DutCardEvents {
    pub initial_hand: Vec<Box<str>>,
    pub redrew: bool,
    pub opening_hand: Vec<Box<str>>,
    pub drawn: Vec<Box<str>>,
    pub played: Vec<PlayEvent>,
    pub never_played: Vec<Box<str>>,
    pub in_hand_at_end: Vec<Box<str>>,
}

/// `GameRecord` (`runner/records.py:74-162`): one played game. `chooser` is the argument given
/// to `new_game` (`None` = the seeded die roll), `chooser_seat` the seat that chose.
/// `actions` are `Action.to_json()` tuples (`ActionJson`, `:15`), held as [`Action`]s.
/// `games.ndjson` lines are `to_json` with sorted keys and `(",", ":")` separators
/// (`reports/writer.py:54-57`).
#[derive(Clone, PartialEq, Debug)]
pub struct GameRecord {
    pub match_index: u32,
    pub game_index: u32,
    pub seed: u64,
    pub agent_seeds: [u64; 2],
    pub agents: [Box<str>; 2],
    pub dut_seat: u8,
    pub chooser: Option<u8>,
    pub chooser_seat: u8,
    pub first_player: u8,
    pub dut_on_play: bool,
    pub winner: Outcome,
    pub end_reason: EndReason,
    /// Who scores the game for the match (`match_point`, `runner/game.py:42-51`).
    pub match_point: Option<Role>,
    pub turns: i32,
    pub active_at_end: u8,
    pub actions: Vec<Action>,
    pub redraws: [bool; 2],
    pub dut_cards: DutCardEvents,
    /// Sorted card numbers (`runner/game.py:161`).
    pub bench_seen: Vec<Box<str>>,
    /// `state_digest(st)` of the final state (`runner/game.py:162`).
    pub final_state_sha256: String,
    pub turn_limit: i32,
    pub max_actions: i64,
    /// Both seats' entries in decision order, or `None` without `--decision-log`
    /// (`runner/game.py:94-97`, `:165`).
    pub decision_log: Option<Vec<LogEntry>>,
    /// Rust only (PLAN §12.7, O9): each seat's [`Agent::effort`] after the game's last
    /// decision, `None` for an agent that reports none. Not part of `to_json`, so
    /// `games.ndjson` keeps Python's keys; `--resume` checkpoints carry it.
    pub effort: [Option<AgentEffort>; 2],
}

impl GameRecord {
    /// `bench_seat` (`runner/records.py:103-105`).
    #[must_use]
    pub const fn bench_seat(&self) -> u8 {
        1 - self.dut_seat
    }
}

/// `MatchRecord` (`runner/records.py:165-184`). JSON (`:176-184`), keys in this order:
/// `match_index`, `dut_seat`, `winner`, `dut_points`, `bench_points`, `games` (the game
/// indices only).
#[derive(Clone, PartialEq, Debug)]
pub struct MatchRecord {
    pub match_index: u32,
    pub dut_seat: u8,
    pub games: Vec<GameRecord>,
    pub winner: Outcome,
    pub dut_points: u32,
    pub bench_points: u32,
}

/// `RunTiming` (`runner/records.py:187-195`): wall-clock values for `timing.json` only, which
/// every comparison ignores (PLAN A17). They are measured and formatted only in
/// `crates/gcg-runner/src/clock.rs`, the one wall-clock exception (PLAN §9.9, §11.8).
#[derive(Clone, PartialEq, Debug)]
pub struct RunTiming {
    /// ISO 8601 UTC, seconds precision (`runner/run.py:29-30`).
    pub started_at: String,
    pub finished_at: String,
    pub wall_seconds: f64,
    /// Per match, in match order.
    pub match_seconds: Vec<f64>,
    /// `processes_used(config)` (`runner/run.py:83-85`): the worker threads that played.
    pub workers: u32,
    /// The CPU tiers, recorded in `timing.json` (PLAN §7.4).
    pub topology: Topology,
}

/// `BenchmarkRun` (`runner/records.py:198-207`): the matches in match order, whatever the
/// worker count (`runner/run.py:1-7`).
#[derive(Clone, PartialEq, Debug)]
pub struct BenchmarkRun {
    pub config: BenchmarkConfig,
    pub matches: Vec<MatchRecord>,
    pub timing: RunTiming,
    /// `describe_factory(factory)` (`runner/run.py:139`).
    pub agent_factory: String,
    /// Rust only (PLAN §12.7, O9): the factory's [`AgentFactory::search_block`], which results
    /// 1.2.0 write as `ai.search`.
    pub search: Option<SearchSettings>,
}

impl BenchmarkRun {
    /// `games` (`runner/records.py:205-207`): every game, in match then game order.
    pub fn games(&self) -> impl Iterator<Item = &GameRecord> + '_ {
        self.matches.iter().flat_map(|m| m.games.iter())
    }
}

/// Why a run produced no matches. `Display` (E) is the text after `"error: simulation
/// failed: "` (`cli.py:208-209`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum RunError {
    /// `GameFailure` (`runner/game.py:30-31`): `f"match {m} game {g + 1} (seed {seed})
    /// failed: {Class}: {exc}"` (`:132-136`; the embedded exception text follows PLAN A19),
    /// `f"match {m} game {g + 1} stopped without a result"` (`:137-138`) or `"the game has
    /// not ended"` (`:45-46`). With several workers the reported failure is the one with the
    /// lowest match index, as the serial path reports (W-149, `runner/run.py:45-53`).
    GameFailure(String),
    /// SIGINT (PLAN §7.5): the CLI prints [`INTERRUPTED_MESSAGE`] and exits 130.
    Interrupted,
}

// ---------------------------------------------------------------------------------------------
// Reports (reports/writer.py, reports/results.py)

/// `ReportPaths` (`reports/writer.py:23-30`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ReportPaths {
    pub out_dir: PathBuf,
    pub results: PathBuf,
    pub summary: PathBuf,
    pub games: PathBuf,
    pub replays: Vec<PathBuf>,
    pub timing: PathBuf,
}

/// A schema violation found by the in-house validator (PLAN §9.5); Python raises
/// `jsonschema.ValidationError` (`reports/results.py:48-50`). Never expected in a run.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SchemaError {
    /// JSON pointer of the offending value, e.g. `/ai/preset`.
    pub pointer: String,
    pub message: String,
}

/// Why `write_reports` stopped.
#[derive(Debug)]
pub enum ReportError {
    /// An `OSError` while writing; the CLI prints `f"cannot write reports to {out}:
    /// {exc.filename or out}: {exc.strerror or exc}"` (`cli.py:210-216`). `path` is the file
    /// being written.
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Schema(SchemaError),
    /// Building a replay document failed: re-simulating its game raised
    /// (`replay_document`, `runner/replayfile.py:104-150`). Python does not catch it in
    /// `write_reports` (`reports/writer.py:79-91`) or `cmd_benchmark` (`cli.py:210-216`): a
    /// traceback and exit 1. The replays before it are written; `results.json` is not.
    Replay(ReplayError),
    /// SIGINT seen before or after a report file (PLAN §7.5).
    Interrupted,
}

// ---------------------------------------------------------------------------------------------
// Replays (runner/replayfile.py)

/// `REPLAY_FORMAT` (`runner/replayfile.py:20`).
pub const REPLAY_FORMAT: &str = "gcg-sim-replay";
/// `REPLAY_VERSION` (`runner/replayfile.py:21`).
pub const REPLAY_VERSION: u32 = 1;

/// `ReplayCheck` (`runner/replayfile.py:153-161`), the result of `check_replay`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ReplayCheck {
    pub ok: bool,
    /// `f"{k}: expected {expected.get(k)!r}, replay gave {v!r}"` per differing key, in the
    /// order `winner_seat`, `end_reason`, `turns`, `final_state_sha256` (`:186-196`).
    pub mismatches: Vec<String>,
    pub winner_seat: Option<i8>,
    pub end_reason: Option<EndReason>,
    pub turns: i32,
    pub final_state_sha256: String,
    pub log: Vec<String>,
}

/// `ReplayError(ValueError)` (`runner/replayfile.py:26-27`) and what escapes `check_replay`.
/// `Display` (E) is Python's message; the CLI prints `f"cannot replay {path}: {exc}"`
/// (`cli.py:353-356`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ReplayError {
    /// `f"not a {REPLAY_FORMAT} v{REPLAY_VERSION} document"` (`:166-167`).
    NotReplay,
    /// `f"the replay uses a card this version cannot play: {exc}"` (`:86-87`).
    Unimplemented(UnimplementedCardError),
    /// `"the replay has more actions than the game had decisions"` (`:92-93`).
    ExtraActions,
    /// `f"action {index} {action} is not legal in the replayed game"` (`:97-98`); `index`
    /// counts from 0.
    Illegal { index: usize, action: Action },
    /// `f"malformed replay document: {type(exc).__name__}: {exc}"` for a `KeyError`,
    /// `TypeError`, `IndexError` or `ValueError` (including `DeckConstructionError` from
    /// `new_game`) raised while reading or replaying (`:180-183`); `detail` follows PLAN A19.
    Malformed {
        py_class: &'static str,
        detail: String,
    },
    /// An engine error, which Python does not catch (a traceback and exit 1).
    Engine(EngineError),
}

// ---------------------------------------------------------------------------------------------
// Test agents (tests/runner/random_agent.py), reachable only through `gcg-eval --agent` and
// tests (PLAN §6.1)

/// The two injected test agents (`tests/runner/random_agent.py`), selected by
/// `gcg-eval reference-benchmark --agent test-random|test-illegal`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TestAgentKind {
    Random,
    Illegal,
}

impl TestAgentKind {
    pub const ALL: [Self; 2] = [Self::Random, Self::Illegal];

    /// The agent name and the `--agent` value (`random_agent.py:19`, `:54`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Random => "test-random",
            Self::Illegal => "test-illegal",
        }
    }

    /// What Python's `describe_factory(FactoryRef("…/random_agent.py:<attr>"))` writes
    /// (`runner/agents.py:109-111`).
    #[must_use]
    pub const fn factory_description(self) -> &'static str {
        match self {
            Self::Random => "random_agent.py:make_random_agent",
            Self::Illegal => "random_agent.py:make_illegal_agent",
        }
    }
}

/// `SeededRandomAgent(seed, log=log)` (`tests/runner/random_agent.py:17-44`): one generator
/// seeded once with the spec's seed (no per-decision seeding), `options[randrange(n)]`, and log
/// entries without labels whose alternatives are every option with value `0.0` and 0 visits.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by workstream E's Agent impl")]
pub struct TestRandomAgent {
    pub(crate) rng: SplitMix64,
    pub(crate) log_enabled: bool,
    pub(crate) log: Vec<LogEntry>,
}

impl TestRandomAgent {
    /// `SeededRandomAgent(seed, log=log)` (`tests/runner/random_agent.py:18-22`).
    #[must_use]
    pub const fn new(seed: u64, log: bool) -> Self {
        Self {
            rng: SplitMix64::new(seed),
            log_enabled: log,
            log: Vec::new(),
        }
    }
}

/// `IllegalActionAgent` (`tests/runner/random_agent.py:51-60`): always answers
/// [`ILLEGAL_ACTION`] and logs nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TestIllegalAgent;

/// `Action(ActionKind.ATTACK, 10**6, 10**6)` (`tests/runner/random_agent.py:57`).
pub const ILLEGAL_ACTION: Action = Action::new(ActionKind::Attack, 1_000_000, 1_000_000, -1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_constants_are_pythons() {
        assert_eq!(
            (EXIT_OK, EXIT_INVALID, EXIT_USAGE, EXIT_INTERRUPTED),
            (0, 1, 2, 130)
        );
        assert_eq!(Format::ALL.map(Format::as_str), ["bo3", "bo1"]);
        assert_eq!(Format::from_value("bo1"), Some(Format::Bo1));
        assert_eq!(Format::from_value("bo5"), None);
        assert_eq!(
            Format::ALL.map(|f| (f.games_per_match(), f.wins_needed())),
            [(3, 2), (1, 1)]
        );
        assert_eq!(
            [Outcome::Dut, Outcome::Bench, Outcome::Draw].map(Outcome::as_str),
            ["dut", "bench", "draw"]
        );
        assert_eq!(Role::Bench.as_str(), "bench");
        assert_eq!(
            SchemaVersion::V1_1.as_str(),
            gcg_data::RESULTS_SCHEMA_VERSION
        );
        assert_eq!(SchemaVersion::V1_1.id(), gcg_data::RESULTS_SCHEMA_ID);
        assert_eq!(
            SchemaVersion::V1_2.id(),
            format!(
                "urn:gcg-sim:schema:results:{}",
                SchemaVersion::V1_2.as_str()
            )
        );
        assert_eq!(
            TestAgentKind::ALL.map(TestAgentKind::as_str),
            ["test-random", "test-illegal"]
        );
        assert_eq!(
            (ILLEGAL_ACTION.a, ILLEGAL_ACTION.b, ILLEGAL_ACTION.c),
            (1_000_000, 1_000_000, -1)
        );
        assert_eq!(TestRandomAgent::new(5, false).rng, SplitMix64::new(5));
    }

    #[test]
    fn factories_and_records_cross_threads() {
        fn sync<T: Sync + ?Sized>() {}
        fn send<T: Send>() {}
        sync::<dyn AgentFactory>();
        send::<BenchmarkRun>();
        send::<RunError>();
        send::<ReportError>();
    }
}
