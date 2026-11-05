//! Native ports of the `tests/runner` and `tests/reports` cases that no other Rust test
//! ports (T-078; the register `tests/python_suites.json` maps every Python test): the digest
//! changing with the state, per-card instrumentation against the action stream, searching
//! agents that leave the records alone, decision-log order, the agent specs a game builds,
//! the game-1 chooser (FAQ Q9), every example pairing replaying (`@pytest.mark.slow` in
//! Python), the Wilson properties, the runner's preset list and the curated-conflict
//! resolutions. Inputs and assertions are Python's; Hypothesis properties run as seeded
//! `gcg_core::prop` cases.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gcg_ai::{Agent, AgentCx, AgentError, LogEntry, Scratch};
use gcg_core::SplitMix64;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_data::{CardCount, Deck};
use gcg_effects::Registry;
use gcg_engine::observe::determinize;
use gcg_engine::{Action, ActionKind, DecisionKind, GameState, NewGameOptions, Zone, apply, canon};
use gcg_runner::config::AI_PRESETS;
use gcg_runner::reports::stats::{Z95, wilson};
use gcg_runner::test_agents::factory;
use gcg_runner::{
    AgentFactory, AgentSpec, BenchmarkConfig, Format, GameRecord, Outcome, Role, RunError,
    SchemaVersion, TestAgentKind, Workers, game_seed, own_turn, play_game, replay, run_benchmark,
    seat_decks,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn deck(name: &str) -> Deck {
    gcg_data::load_deck(&root().join("examples/decks").join(format!("{name}.txt")))
        .expect("an example deck")
}

fn config(dut: &str, bench: &str, matches: u32, seed: u64) -> BenchmarkConfig {
    BenchmarkConfig {
        deck_under_test: deck(dut),
        benchmark_deck: deck(bench),
        matches,
        seed,
        workers: Workers::Count(1),
        format: Format::Bo3,
        ai_preset: "standard",
        out_dir: PathBuf::from("gcg-sim-out"),
        decision_log: false,
        replays: 3,
        turn_limit: 200,
        max_actions: 20_000,
        schema: SchemaVersion::V1_1,
        package: "0.1.0",
        code_digest: "0000000000000000000000000000000000000000000000000000000000000000",
    }
}

/// `tests/runner/conftest.py` `config`: federation vs zeon, 6 matches, seed 1234.
fn conftest() -> BenchmarkConfig {
    config("blue-white-federation", "red-green-zeon", 6, 1234)
}

fn game(
    cfg: &BenchmarkConfig,
    m: u32,
    g: u32,
    f: &dyn AgentFactory,
) -> Result<GameRecord, RunError> {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        play_game(cfg, m, g, None, f, &mut cx)
    })
}

fn random_game(cfg: &BenchmarkConfig, m: u32) -> GameRecord {
    game(cfg, m, 0, &*factory(TestAgentKind::Random)).expect("a random game")
}

fn multiset<'a>(items: impl IntoIterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut out = BTreeMap::new();
    for i in items {
        *out.entry(i).or_insert(0) += 1;
    }
    out
}

fn contained(small: &BTreeMap<&str, usize>, big: &BTreeMap<&str, usize>) -> bool {
    small
        .iter()
        .all(|(k, v)| big.get(k).copied().unwrap_or(0) >= *v)
}

fn expanded(list: &[CardCount]) -> Vec<&str> {
    list.iter()
        .flat_map(|c| {
            std::iter::repeat_n(&*c.card_number, usize::try_from(c.count).expect("small"))
        })
        .collect()
}

// --- test_digest.py -------------------------------------------------------------------------

#[test]
fn digest_changes_with_the_game_state() {
    let cfg = conftest();
    let decks = seat_decks(&cfg, 0).expect("decks");
    let mut st = gcg_engine::new_game(
        &Registry::embedded(),
        [&decks[0], &decks[1]],
        1,
        NewGameOptions::default(),
    )
    .expect("a game");
    let before = canon::state_digest(&st);
    let first = st.pending().expect("a decision").options[0];
    apply(&mut st, first, true).expect("legal");
    assert_ne!(canon::state_digest(&st), before);
}

// --- test_instrument.py ---------------------------------------------------------------------

/// The `games` fixture: 24 random games of federation vs zeon, `matches=1`, seed 99.
fn instrument_games() -> (BenchmarkConfig, Vec<GameRecord>) {
    let cfg = config("blue-white-federation", "red-green-zeon", 1, 99);
    let games = (0..24).map(|m| random_game(&cfg, m)).collect();
    (cfg, games)
}

#[test]
fn needs_engine_hands_and_redraws() {
    let (_, games) = instrument_games();
    for g in &games {
        let c = &g.dut_cards;
        assert_eq!(c.initial_hand.len(), 5);
        assert_eq!(c.opening_hand.len(), 5);
        assert_eq!(c.redrew, g.redraws[usize::from(g.dut_seat)]);
        if !c.redrew {
            assert_eq!(c.initial_hand, c.opening_hand);
        }
    }
    assert!(games.iter().any(|g| g.dut_cards.redrew));
    assert!(!games.iter().all(|g| g.dut_cards.redrew));
}

#[test]
fn needs_engine_drawn_cards_are_consistent() {
    let (cfg, games) = instrument_games();
    let deck = multiset(expanded(&cfg.deck_under_test.main));
    for g in &games {
        let c = &g.dut_cards;
        let drawn = multiset(c.drawn.iter().map(|n| &**n));
        assert!(contained(
            &multiset(c.opening_hand.iter().map(|n| &**n)),
            &drawn
        ));
        assert!(contained(&drawn, &deck));
        assert!(contained(
            &multiset(c.never_played.iter().map(|n| &**n)),
            &drawn
        ));
        assert!(contained(
            &multiset(c.in_hand_at_end.iter().map(|n| &**n)),
            &drawn
        ));
        let turns_started = own_turn(g.turns, g.dut_seat, g.first_player);
        assert!(c.drawn.len() >= usize::try_from(5 + turns_started - 1).expect("non-negative"));
    }
}

const PLAY_KINDS: [ActionKind; 4] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::PlayCommand,
    ActionKind::Pair,
];

/// `played_by_actions(record, config)`: cards played from hand by (DUT, benchmark), recomputed
/// from the action stream alone.
fn played_by_actions(
    g: &GameRecord,
    cfg: &BenchmarkConfig,
) -> (BTreeMap<String, usize>, BTreeMap<String, usize>) {
    let decks = seat_decks(cfg, g.match_index).expect("decks");
    let opts = NewGameOptions {
        chooser: g.chooser,
        ..NewGameOptions::default()
    };
    let mut st = gcg_engine::new_game(&Registry::embedded(), [&decks[0], &decks[1]], g.seed, opts)
        .expect("a game");
    let (mut dut, mut bench) = (BTreeMap::new(), BTreeMap::new());
    for &action in &g.actions {
        let player = st.pending().expect("a decision").hdr.player;
        if PLAY_KINDS.contains(&action.kind) {
            let uid = u16::try_from(action.a).expect("a card index");
            let number = st.def_of(uid).card_number.to_string();
            let side = if player == g.dut_seat {
                &mut dut
            } else {
                &mut bench
            };
            *side.entry(number).or_insert(0) += 1;
        }
        apply(&mut st, action, true).expect("legal");
    }
    (dut, bench)
}

#[test]
fn needs_engine_played_cards_match_the_action_stream() {
    let (cfg, games) = instrument_games();
    let bench_deck: Vec<&str> = expanded(&cfg.benchmark_deck.main);
    for g in &games {
        let (dut_played, bench_played) = played_by_actions(g, &cfg);
        let mut recorded: BTreeMap<String, usize> = BTreeMap::new();
        for p in &g.dut_cards.played {
            *recorded.entry(p.card_number.to_string()).or_insert(0) += 1;
        }
        assert_eq!(recorded, dut_played);
        assert!(
            bench_played
                .keys()
                .all(|n| g.bench_seen.iter().any(|s| **s == **n))
        );
        assert!(g.bench_seen.iter().all(|n| bench_deck.contains(&&**n)));
        for p in &g.dut_cards.played {
            assert!((1..=g.turns).contains(&p.turn));
            assert!((0..=own_turn(g.turns, g.dut_seat, g.first_player)).contains(&p.own_turn));
        }
        assert!(g.dut_cards.never_played.len() <= g.dut_cards.drawn.len());
    }
}

/// `SearchingRandomAgent`: chooses exactly like the test random agent, after playing 40
/// random actions on a determinized copy.
struct SearchingRandomAgent {
    rng: SplitMix64,
    search: SplitMix64,
}

impl Agent for SearchingRandomAgent {
    fn name(&self) -> &str {
        "searching-random"
    }

    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        _: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError> {
        let mut copy = determinize(st, player, self.search.next_u64())
            .map_err(|e| AgentError::Value(e.to_string()))?;
        for _ in 0..40 {
            let Some(options) = copy.pending().map(|p| p.options.to_vec()) else {
                break;
            };
            let a = options[self.search.randrange_usize(options.len())];
            apply(&mut copy, a, true).map_err(|e| AgentError::Value(e.to_string()))?;
        }
        let options = st.pending().expect("a decision").options;
        Ok(options[self.rng.randrange_usize(options.len())])
    }

    fn decision_log(&self) -> &[LogEntry] {
        &[]
    }
}

struct Searching;

impl AgentFactory for Searching {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        Ok(Box::new(SearchingRandomAgent {
            rng: SplitMix64::new(spec.seed),
            search: SplitMix64::new(spec.seed ^ 0x00AB_CDEF),
        }))
    }

    fn describe(&self) -> String {
        "searching".to_owned()
    }
}

#[test]
fn needs_engine_search_copies_do_not_leak_into_the_records() {
    let cfg = config("blue-white-federation", "red-green-zeon", 1, 5);
    for m in 0..4 {
        let plain = random_game(&cfg, m);
        let mut searched = game(&cfg, m, 0, &Searching).expect("a searched game");
        assert_eq!(&*searched.agents[0], "searching-random");
        searched.agents = plain.agents.clone();
        assert_eq!(searched, plain, "match {m}");
    }
}

#[test]
fn needs_engine_illegal_agent_actions_fail_with_game_context() {
    let cfg = conftest();
    let seed = game_seed(cfg.seed, 2, 1);
    let err = game(&cfg, 2, 1, &*factory(TestAgentKind::Illegal)).expect_err("illegal");
    let RunError::GameFailure(message) = err else {
        panic!("not a game failure: {err:?}")
    };
    assert!(
        message.starts_with(&format!("match 2 game 2 (seed {seed}) failed: Illegal")),
        "{message}"
    );
}

#[test]
fn needs_engine_decision_log_is_collected_in_decision_order() {
    let mut logged = conftest();
    logged.decision_log = true;
    let record = random_game(&logged, 0);
    let log = record.decision_log.as_ref().expect("a decision log");
    assert_eq!(log.len(), record.actions.len());
    assert_eq!(
        log.iter().map(|e| e.chosen).collect::<Vec<_>>(),
        record.actions
    );
    assert_eq!(random_game(&conftest(), 0).decision_log, None);
}

/// Records every spec it is asked for and builds the illegal test agent.
struct Recording(Mutex<Vec<AgentSpec>>);

impl AgentFactory for Recording {
    fn make(&self, spec: &AgentSpec) -> Result<Box<dyn Agent>, AgentError> {
        self.0.lock().expect("lock").push(*spec);
        factory(TestAgentKind::Illegal).make(spec)
    }

    fn describe(&self) -> String {
        "recording".to_owned()
    }
}

#[test]
fn needs_engine_agents_receive_their_role_seat_and_seed() {
    let f = Recording(Mutex::new(Vec::new()));
    let cfg = conftest();
    assert!(matches!(
        game(&cfg, 1, 0, &f),
        Err(RunError::GameFailure(_))
    ));
    let specs = f.0.into_inner().expect("lock");
    assert_eq!(
        specs.iter().map(|s| (s.role, s.seat)).collect::<Vec<_>>(),
        [(Role::Bench, 0), (Role::Dut, 1)]
    );
    assert_ne!(specs[0].seed, specs[1].seed);
    assert!(specs.iter().all(|s| s.preset == "standard"));
}

// --- test_match.py --------------------------------------------------------------------------

// gcg-tags: faq=Q9
#[test]
fn needs_engine_game_one_player_one_is_chosen_by_a_seeded_random_winner_before_hands() {
    let cfg = conftest();
    let mut choosers = Vec::new();
    for m in 0..10 {
        let decks = seat_decks(&cfg, m).expect("decks");
        let st = gcg_engine::new_game(
            &Registry::embedded(),
            [&decks[0], &decks[1]],
            game_seed(cfg.seed, m, 0),
            NewGameOptions::default(),
        )
        .expect("a game");
        let pending = st.pending().expect("a decision");
        assert_eq!(pending.hdr.kind, DecisionKind::ChooseFirst);
        assert_eq!(pending.hdr.player, st.setup_chooser());
        assert!(st.zone(0, Zone::Hand).is_empty());
        assert!(st.zone(1, Zone::Hand).is_empty());
        choosers.push(st.setup_chooser());
    }
    choosers.sort();
    choosers.dedup();
    assert_eq!(choosers, [0, 1]);
}

// --- test_replay.py -------------------------------------------------------------------------

/// `test_every_example_pairing_replays` (slow in Python): every ordered pair of example decks,
/// 60 BO3 matches each on seed `i`, and every game replays to its recorded end.
#[test]
fn needs_engine_every_example_pairing_replays() {
    let names = [
        "blue-white-federation",
        "red-green-zeon",
        "purple-white-tekkadan",
    ];
    let mut i = 0_u64;
    for a in names {
        for b in names {
            if a == b {
                continue;
            }
            let cfg = config(a, b, 60, i);
            i += 1;
            let run = run_benchmark(
                &cfg,
                &*factory(TestAgentKind::Random),
                &mut |_, _| {},
                &Cancel::new(),
            )
            .expect("a run");
            for g in run.games() {
                let st = replay(g, &cfg).expect("replays");
                assert_eq!(canon::state_digest(&st), g.final_state_sha256);
                assert_eq!(st.turn(), g.turns);
                let winner = st.winner().expect("finished");
                let label = if winner < 0 {
                    Outcome::Draw
                } else if u8::try_from(winner).ok() == Some(g.dut_seat) {
                    Outcome::Dut
                } else {
                    Outcome::Bench
                };
                assert_eq!(g.winner, label);
                assert_eq!(st.end_reason(), Some(g.end_reason));
            }
        }
    }
}

// --- test_config.py -------------------------------------------------------------------------

/// The Hypothesis property `test_seeds_are_64_bit_and_depend_on_every_index` with 500 seeded
/// cases: master in `[0, 2**64)`, match in `[0, 10_000]`, game in `[0, 2]` (64 bits by type).
#[test]
fn seeds_depend_on_every_index_property() {
    gcg_core::prop::check("game-seed-indices", 500, |rng| {
        let master = rng.next_u64();
        let m = u32::try_from(rng.randrange(10_001)).expect("small");
        let g = u32::try_from(rng.randrange(3)).expect("small");
        let s = game_seed(master, m, g);
        let ok = s == game_seed(master, m, g)
            && s != game_seed(master, m + 1, g)
            && s != game_seed(master, m, (g + 1) % 3)
            && s != game_seed(master.wrapping_add(1), m, g);
        if ok {
            Ok(())
        } else {
            Err(format!("game_seed({master}, {m}, {g})"))
        }
    });
}

/// `test_runner_presets_are_the_ai_presets`: the runner accepts exactly the reference's three
/// preset names, in `PRESETS` order (`gcg-ai-reference` is not a dependency of this crate;
/// `crates/gcg-eval/tests/parity.rs` runs the reference at each of them).
#[test]
fn runner_presets_are_the_reference_presets() {
    assert_eq!(AI_PRESETS, ["fast", "standard", "strong"]);
    let mut cfg = conftest();
    for p in AI_PRESETS {
        cfg.ai_preset = p;
        assert_eq!(cfg.problems(), Vec::<String>::new(), "{p}");
    }
    cfg.ai_preset = "max";
    assert_eq!(
        cfg.problems(),
        ["AI preset must be one of fast, standard, strong (got 'max')"]
    );
}

// --- test_stats.py --------------------------------------------------------------------------

#[test]
fn wilson_bounds_contain_the_estimate() {
    gcg_core::prop::check("wilson-bounds", 200, |rng| {
        let n = 1 + rng.randrange(5000);
        let k = rng.randrange(n + 1);
        let (lo, hi) = wilson(k, n, Z95).ok_or("valid counts")?;
        #[expect(clippy::cast_precision_loss, reason = "Python's k / n")]
        let p = k as f64 / n as f64;
        if !(0.0 <= lo && lo <= p && p <= hi && hi <= 1.0) {
            return Err(format!("wilson({k}, {n}) = ({lo}, {hi})"));
        }
        let (mlo, mhi) = wilson(n - k, n, Z95).ok_or("valid counts")?;
        if (lo - (1.0 - mhi)).abs() > 1e-12 || (hi - (1.0 - mlo)).abs() > 1e-12 {
            return Err(format!(
                "wilson({k}, {n}) is not the mirror of wilson({}, {n})",
                n - k
            ));
        }
        Ok(())
    });
}

#[test]
fn wilson_narrows_with_more_data() {
    gcg_core::prop::check("wilson-narrows", 200, |rng| {
        let k = 1 + rng.randrange(50);
        let scale = 2 + rng.randrange(19);
        let (lo1, hi1) = wilson(k, 2 * k, Z95).ok_or("valid")?;
        let (lo2, hi2) = wilson(k * scale, 2 * k * scale, Z95).ok_or("valid")?;
        if hi2 - lo2 < hi1 - lo1 {
            Ok(())
        } else {
            Err(format!("k={k} scale={scale}"))
        }
    });
}

// --- test_reports.py ------------------------------------------------------------------------

#[test]
fn resolutions_named_by_a_curated_conflict_are_listed_for_every_card_it_names() {
    let mut d = deck("red-green-zeon");
    d.main = Box::new([CardCount {
        card_number: "GD01-030".into(),
        count: 1,
    }]);
    d.resources = Box::new([]);
    let entries = gcg_data::conflict_resolutions(&[("deck_under_test", &d)]);
    let text = gcg_core::pyjson::dumps(&entries, gcg_core::pyjson::DumpOptions::PRETTY_SORTED_UTF8);
    let v: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert!(
        v.as_array().expect("a list").iter().any(|e| {
            e["card_number"] == "GD01-030" && e["conflict_id"] == "ambiguous:breach-reminder"
        }),
        "{text}"
    );
}
