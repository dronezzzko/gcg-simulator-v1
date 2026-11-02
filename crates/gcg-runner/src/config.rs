//! Benchmark configuration checks, seat assignment and seed derivation (`runner/config.py`;
//! docs/port/inventory/01-rng-runner-cli.md §2.2).
//!
//! Every seed is a pure function of the master seed and the match and game indices, so a match
//! plays the same way on any worker (`runner/run.py:1-7`).

use gcg_core::derive_seed;
use gcg_core::sys::topology::Topology;
use gcg_data::{DeckError, DeckList, to_decklist};

use crate::{BenchmarkConfig, Workers};

/// `AI_PRESETS` (`runner/config.py:14`): the preset names a configuration accepts. `gcg-sim`
/// passes `standard` or `fast`, `gcg-eval reference-benchmark` any of the three.
pub const AI_PRESETS: [&str; 3] = ["fast", "standard", "strong"];

/// `DEFAULT_TURN_LIMIT` (`runner/config.py:15`).
pub const DEFAULT_TURN_LIMIT: i32 = 200;

impl BenchmarkConfig {
    /// `config_problems(c)` (`runner/config.py:47-61`): the failed checks in Python's order and
    /// wording. The Rust types already rule out three of the eight (a format outside
    /// `FORMATS`, negative replays, a seed outside `[0, 2**64)`); the CLI rejects the rest
    /// before a configuration is built, so this matters only to programmatic callers.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.matches < 1 {
            out.push(format!("matches must be at least 1 (got {})", self.matches));
        }
        if let Workers::Count(n) = self.workers
            && n < 1
        {
            out.push(format!("workers must be at least 1 (got {n})"));
        }
        if !AI_PRESETS.contains(&self.ai_preset) {
            out.push(format!(
                "AI preset must be one of {} (got {})",
                AI_PRESETS.join(", "),
                gcg_data::py_repr(self.ai_preset)
            ));
        }
        if self.turn_limit < 1 {
            out.push(format!(
                "turn limit must be at least 1 (got {})",
                self.turn_limit
            ));
        }
        if self.max_actions < 1 {
            out.push(format!(
                "action limit must be at least 1 (got {})",
                self.max_actions
            ));
        }
        out
    }

    /// `games_per_match` (`runner/config.py:38-40`): 3 for BO3, 1 for BO1.
    #[must_use]
    pub const fn games_per_match(&self) -> u32 {
        self.format.games_per_match()
    }

    /// `wins_needed` (`runner/config.py:42-44`): 2 for BO3, 1 for BO1.
    #[must_use]
    pub const fn wins_needed(&self) -> u32 {
        self.format.wins_needed()
    }
}

/// `dut_seat(match_index)` (`runner/config.py:64-66`): the deck under test sits in seat
/// `match_index % 2`, so seats alternate by match.
#[must_use]
pub const fn dut_seat(m: u32) -> u8 {
    if m.is_multiple_of(2) { 0 } else { 1 }
}

/// `seat_decks(config, match_index)` (`runner/config.py:69-72`): `(dut, bench)` when the deck
/// under test is in seat 0, else `(bench, dut)`, each `to_decklist(deck)`.
///
/// # Errors
/// The [`DeckError`] of `to_decklist` (W-144: a deck too large to expand).
pub fn seat_decks(cfg: &BenchmarkConfig, m: u32) -> Result<[DeckList; 2], DeckError> {
    let dut = to_decklist(&cfg.deck_under_test)?;
    let bench = to_decklist(&cfg.benchmark_deck)?;
    Ok(if dut_seat(m) == 0 {
        [dut, bench]
    } else {
        [bench, dut]
    })
}

/// `game_seed(master, match_index, game_index)` (`runner/config.py:75-76`):
/// `derive_seed(master, "match", m, "game", g)`.
#[must_use]
pub fn game_seed(master: u64, m: u32, g: u32) -> u64 {
    derive_seed!(master, "match", m, "game", g)
}

/// `agent_seed(master, match_index, game_index, seat)` (`runner/config.py:79-80`):
/// `derive_seed(master, "match", m, "game", g, "agent", seat)`; keyed by seat, not role
/// (Inv 01 hazard 10).
#[must_use]
pub fn agent_seed(master: u64, m: u32, g: u32, seat: u8) -> u64 {
    derive_seed!(master, "match", m, "game", g, "agent", u32::from(seat))
}

/// `own_turn(turn, seat, first_player)` (`runner/config.py:83-85`): how many turns `seat` has
/// started by game turn `turn` (Python's floor division).
#[must_use]
pub const fn own_turn(turn: i32, seat: u8, first_player: u8) -> i32 {
    if seat == first_player {
        (turn + 1).div_euclid(2)
    } else {
        turn.div_euclid(2)
    }
}

/// `processes_used(config)` (`runner/run.py:83-85`): the workers that actually play, `1 if
/// workers <= 1 or matches == 1 else min(workers, matches)`. `--workers auto` resolves through
/// [`Topology::auto_workers`] (every CPU of the faster tiers, at most one per match; PLAN §7.4).
#[must_use]
pub fn workers_used(cfg: &BenchmarkConfig, topology: &Topology) -> u32 {
    let requested = match cfg.workers {
        Workers::Count(n) => n,
        Workers::Auto => {
            let matches = usize::try_from(cfg.matches).unwrap_or(usize::MAX);
            u32::try_from(topology.auto_workers(matches)).unwrap_or(u32::MAX)
        }
    };
    if requested <= 1 || cfg.matches == 1 {
        1
    } else {
        requested.min(cfg.matches)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use gcg_core::rng::{SeedPart, derive_seed};
    use gcg_core::sys::topology::TopologySource;
    use gcg_data::{CardCount, Deck};

    use super::*;
    use crate::{Format, SchemaVersion};

    fn deck(name: &str, main: &[(&str, u128)]) -> Deck {
        Deck {
            name: name.into(),
            main: main
                .iter()
                .map(|&(n, c)| CardCount {
                    card_number: n.into(),
                    count: c,
                })
                .collect(),
            resources: vec![CardCount {
                card_number: "R-001".into(),
                count: 10,
            }]
            .into(),
            entries: Box::new([]),
            source: None,
            digest: String::new(),
        }
    }

    pub(crate) fn config() -> BenchmarkConfig {
        BenchmarkConfig {
            deck_under_test: deck("fed", &[("GD01-001", 4), ("GD01-004", 4)]),
            benchmark_deck: deck("zeon", &[("GD01-030", 4)]),
            matches: 6,
            seed: 1234,
            workers: Workers::Count(1),
            format: Format::Bo3,
            ai_preset: "standard",
            out_dir: PathBuf::from("gcg-sim-out"),
            decision_log: false,
            replays: 3,
            turn_limit: DEFAULT_TURN_LIMIT,
            max_actions: 20_000,
            schema: SchemaVersion::V1_1,
            package: "0.1.0",
            code_digest: "digest",
        }
    }

    fn topology(logical: usize) -> Topology {
        Topology {
            levels: Vec::new(),
            logical_cpus: logical,
            cache_line_bytes: None,
            arm_features: Vec::new(),
            source: TopologySource::Fallback,
        }
    }

    #[test]
    fn seeds_follow_the_documented_derivation() {
        // tests/runner/test_config.py:20-23
        assert_eq!(
            game_seed(7, 3, 1),
            derive_seed(
                7,
                &[
                    SeedPart::Str("match"),
                    SeedPart::Int(3),
                    SeedPart::Str("game"),
                    SeedPart::Int(1)
                ]
            )
        );
        assert_eq!(
            agent_seed(7, 3, 1, 0),
            derive_seed(
                7,
                &[
                    SeedPart::Str("match"),
                    SeedPart::Int(3),
                    SeedPart::Str("game"),
                    SeedPart::Int(1),
                    SeedPart::Str("agent"),
                    SeedPart::Int(0)
                ]
            )
        );
        assert_ne!(agent_seed(7, 3, 1, 0), agent_seed(7, 3, 1, 1));
    }

    #[test]
    fn seeds_match_the_python_records() {
        // crates/gcg-runner/tests/vectors/records/random-bo3-tekkadan/games.ndjson, line 1:
        // master 2**64 - 1, match 0, game 0.
        let master = u64::MAX;
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/vectors/records/random-bo3-tekkadan/games.ndjson"
        );
        let text = std::fs::read_to_string(path).expect("record vectors");
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
            let m = u32::try_from(v["match_index"].as_u64().expect("m")).expect("u32");
            let g = u32::try_from(v["game_index"].as_u64().expect("g")).expect("u32");
            assert_eq!(Some(game_seed(master, m, g)), v["seed"].as_u64());
            assert_eq!(
                Some(agent_seed(master, m, g, 0)),
                v["agent_seeds"][0].as_u64()
            );
            assert_eq!(
                Some(agent_seed(master, m, g, 1)),
                v["agent_seeds"][1].as_u64()
            );
            assert_eq!(
                u64::from(dut_seat(m)),
                v["dut_seat"].as_u64().expect("seat")
            );
        }
    }

    #[test]
    fn seeds_depend_on_every_index() {
        // tests/runner/test_config.py:26-33, over a fixed grid instead of hypothesis.
        for master in [0, 1, 2_024, u64::MAX] {
            for m in [0, 1, 9_999] {
                for g in 0..3 {
                    let s = game_seed(master, m, g);
                    assert_eq!(s, game_seed(master, m, g));
                    assert_ne!(s, game_seed(master, m + 1, g));
                    assert_ne!(s, game_seed(master, m, (g + 1) % 3));
                    assert_ne!(s, game_seed(master.wrapping_add(1), m, g));
                }
            }
        }
    }

    #[test]
    fn seats_alternate_between_matches() {
        // tests/runner/test_config.py:36-39
        assert_eq!((0..4).map(dut_seat).collect::<Vec<_>>(), [0, 1, 0, 1]);
        let cfg = config();
        let dut = to_decklist(&cfg.deck_under_test).expect("small deck");
        assert_eq!(seat_decks(&cfg, 0).expect("decks")[0], dut);
        assert_eq!(seat_decks(&cfg, 1).expect("decks")[1], dut);
    }

    #[test]
    fn own_turn_counts_turns_started_by_a_seat() {
        // tests/runner/test_config.py:42-44
        let first: Vec<i32> = (1..7).map(|t| own_turn(t, 0, 0)).collect();
        let second: Vec<i32> = (1..7).map(|t| own_turn(t, 1, 0)).collect();
        assert_eq!(first, [1, 1, 2, 2, 3, 3]);
        assert_eq!(second, [0, 1, 1, 2, 2, 3]);
        assert_eq!(own_turn(0, 0, 0), 0);
    }

    #[test]
    fn format_rules() {
        // tests/runner/test_config.py:47-50
        let mut cfg = config();
        assert_eq!((cfg.games_per_match(), cfg.wins_needed()), (3, 2));
        cfg.format = Format::Bo1;
        assert_eq!((cfg.games_per_match(), cfg.wins_needed()), (1, 1));
    }

    #[test]
    fn invalid_configs_are_rejected_with_pythons_messages() {
        // tests/runner/test_config.py:53-69 (the checks the Rust types can still fail)
        type Change = fn(&mut BenchmarkConfig);
        assert!(config().problems().is_empty());
        let cases: [(Change, &str); 5] = [
            (|c| c.matches = 0, "matches must be at least 1 (got 0)"),
            (
                |c| c.workers = Workers::Count(0),
                "workers must be at least 1 (got 0)",
            ),
            (
                |c| c.ai_preset = "max",
                "AI preset must be one of fast, standard, strong (got 'max')",
            ),
            (
                |c| c.turn_limit = 0,
                "turn limit must be at least 1 (got 0)",
            ),
            (
                |c| c.max_actions = 0,
                "action limit must be at least 1 (got 0)",
            ),
        ];
        for (change, message) in cases {
            let mut cfg = config();
            change(&mut cfg);
            assert_eq!(cfg.problems(), [message]);
        }
        let mut cfg = config();
        cfg.matches = 0;
        cfg.turn_limit = -3;
        assert_eq!(
            cfg.problems(),
            [
                "matches must be at least 1 (got 0)",
                "turn limit must be at least 1 (got -3)"
            ]
        );
        for preset in AI_PRESETS {
            let mut cfg = config();
            cfg.ai_preset = preset;
            assert!(cfg.problems().is_empty());
        }
    }

    #[test]
    fn workers_used_is_processes_used() {
        // runner/run.py:83-85; tests/runner/test_pool.py:62-68
        let mut cfg = config();
        let topo = topology(10);
        for (workers, matches, used) in [(1, 6, 1), (3, 6, 3), (4, 1, 1), (4, 2, 2), (10, 6, 6)] {
            cfg.workers = Workers::Count(workers);
            cfg.matches = matches;
            assert_eq!(workers_used(&cfg, &topo), used, "{workers} {matches}");
        }
        cfg.workers = Workers::Auto;
        for (logical, matches, used) in [(10, 100, 10), (10, 4, 4), (10, 1, 1), (1, 50, 1)] {
            cfg.matches = matches;
            assert_eq!(workers_used(&cfg, &topology(logical)), used);
        }
    }
}
