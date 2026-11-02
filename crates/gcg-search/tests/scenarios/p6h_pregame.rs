//! P6-H's scenario tests (docs/port/PLAN.md §12.3 S11, T-122; WEAKNESSES W-44, W-45).
//!
//! **W-45, mulligan hands the reference misjudges.** Each position is a REDRAW decision from a
//! seeded game on two tuning decks of the panel (`docs/port/eval/panel.json`; no held-out deck).
//! Its ground truth comes from 2,000 paired full games on independent determinisations of the
//! decider's information set: KEEP and REDRAW on the same world and the same rollout generator,
//! both players played to the end by the reference rollout policy at the play/draw temperature
//! (0.5, at most 3,000 decisions, a game without a winner scoring 0.5). The truth must be clear
//! (more than three paired standard errors). Then 16 agent seeds decide the position through
//! `gcg_search::pregame` on the decider's observation, at the reference `standard` and `strong`
//! budgets (the starting points of the shipped `fast` and `standard`): the reference
//! configuration (every mechanism off: `opening_score` against 90% of the sampled average) and
//! the same configuration with `paired_mulligan`. The reference is wrong on every seed; the
//! mechanism is right on at least 14 of 16 (W-45's verification), and the counts are printed.
//!
//! **W-44, unfinished play/draw games.** A CHOOSE_FIRST decision of a game with a turn limit,
//! so that part of the simulated games end drawn: the reference (`first_player_choice`) scores
//! them 0.5 and counts every pair; `exclude_capped_play_draw` drops the pairs with an unfinished
//! game from the paired estimate and decides on the rest, which the test recomputes from the
//! same simulations played independently.

#[path = "../common/mod.rs"]
mod common;

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::{AgentCx, Alternative, DEFAULT_WEIGHTS, Scratch, play_to_end};
use gcg_ai_reference::{STANDARD, STRONG};
use gcg_core::derive_seed;
use gcg_core::libm::{sqrt, square};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_data::{DeckList, load_deck, to_decklist};
use gcg_effects::Registry;
use gcg_engine::observe::determinize;
use gcg_engine::{
    Action, ActionKind, DecisionKind, GameState, NewGameOptions, Zone, apply, new_game,
};
use gcg_search::{Observation, StrengthenedConfig};

use common::null_config;

/// The master of every seed in this file.
const MASTER: u64 = 20_260_045;
/// Paired full games behind each ground truth (W-45's verification).
const TRUTH_PAIRS: u64 = 2_000;
/// Agent seeds per position and budget, and the successes the mechanism needs (W-45).
const AGENT_SEEDS: u64 = 16;
const NEEDED: usize = 14;
/// `FIRST_PLAYER_TEMPERATURE` and `MAX_GAME_DECISIONS` (`ai/setup.py:25-26`).
const TEMPERATURE: f64 = 0.5;
const MAX_DECISIONS: u32 = 3000;

fn deck(name: &str) -> DeckList {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/decks");
    let top = root.join(format!("{name}.txt"));
    let path = if top.exists() {
        top
    } else {
        root.join("eval").join(format!("{name}.txt"))
    };
    let parsed = load_deck(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    to_decklist(&parsed).expect("a deck list")
}

fn pool() -> Pool {
    Pool::new(std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN))
}

/// The game `new_game([decks[0], decks[1]], seed)` with seat 0 choosing and going first, after
/// `keeps` KEEP decisions: the pending decision and its seat.
fn opening(reg: &Arc<Registry>, decks: [&str; 2], seed: u64, keeps: usize) -> (GameState, u8) {
    let lists = decks.map(deck);
    let mut st = new_game(
        reg,
        [&lists[0], &lists[1]],
        seed,
        NewGameOptions {
            chooser: Some(0),
            ..NewGameOptions::default()
        },
    )
    .expect("a legal game");
    apply(&mut st, Action::new(ActionKind::GoFirst, 0, -1, -1), true).expect("go first");
    for _ in 0..keeps {
        apply(&mut st, Action::of(ActionKind::Keep), true).expect("keep");
    }
    let dec = st.pending().expect("a decision");
    assert_eq!(dec.hdr.kind, DecisionKind::Redraw);
    let player = dec.hdr.player;
    (st, player)
}

fn hand(st: &GameState, player: u8) -> Vec<String> {
    st.zone(player, Zone::Hand)
        .iter()
        .map(|&u| st.def_of(u).card_number.to_string())
        .collect()
}

/// One full policy game from `choice` in the world `determinize(st, player, seed)`, with the
/// same seed for the rollout generator (`ai/setup.py:114-120`): the winner's seat, `-1` (both
/// players lost or the turn limit), or `None` (capped).
fn play_out(
    st: &GameState,
    player: u8,
    choice: Action,
    seed: u64,
    cfg: &StrengthenedConfig,
) -> Option<i8> {
    let mut w = determinize(st, player, seed).expect("a world");
    apply(&mut w, choice, true).expect("a legal choice");
    let mut rng = SplitMix64::new(seed);
    play_to_end(
        &mut w,
        Some(&mut rng),
        TEMPERATURE,
        MAX_DECISIONS,
        &cfg.knobs(),
    )
    .expect("a game");
    w.winner()
}

/// A game's score for `player`: 1 for a win, 0 for a loss, 0.5 without a winner.
fn score(winner: Option<i8>, player: u8) -> f64 {
    match winner {
        Some(x) if i32::from(x) == i32::from(player) => 1.0,
        Some(-1) | None => 0.5,
        Some(_) => 0.0,
    }
}

/// The mean and the standard error of paired differences, as `ai/setup.py:105-109` computes
/// them.
fn paired(diffs: &[f64]) -> (f64, f64) {
    let n = f64::from(u32::try_from(diffs.len()).expect("a count"));
    let mean = py_sum(diffs.iter().copied()) / n;
    let var = py_sum(diffs.iter().map(|&d| square(d - mean))) / (n - 1.0);
    (mean, sqrt(var / n))
}

/// The paired difference `redraw - keep` over `TRUTH_PAIRS` full games: its mean and standard
/// error. The worlds are independent of every agent's.
fn truth(decks: [&str; 2], seed: u64, keeps: usize, cfg: &StrengthenedConfig) -> (f64, f64) {
    let diffs = pool()
        .map_indexed(
            usize::try_from(TRUTH_PAIRS).expect("a count"),
            &Cancel::new(),
            |i, _: &mut Scratch, _: &TaskCx<'_, Scratch>| -> Result<f64, ()> {
                let reg = Registry::embedded();
                let (st, player) = opening(&reg, decks, seed, keeps);
                let world = derive_seed!(MASTER, "truth", i64::try_from(i).expect("an index"));
                let [keep, redraw] = [ActionKind::Keep, ActionKind::Redraw]
                    .map(|k| score(play_out(&st, player, Action::of(k), world, cfg), player));
                Ok(redraw - keep)
            },
            |_| {},
        )
        .expect("the truth games");
    paired(&diffs)
}

/// The choice of `cfg` at the position for every agent seed (`gcg_search::pregame` on the
/// decider's observation), in seed order.
fn choices(decks: [&str; 2], seed: u64, keeps: usize, cfg: &StrengthenedConfig) -> Vec<Action> {
    pool()
        .map_indexed(
            usize::try_from(AGENT_SEEDS).expect("a count"),
            &Cancel::new(),
            |k, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| -> Result<Action, String> {
                let reg = Registry::embedded();
                let (st, player) = opening(&reg, decks, seed, keeps);
                let obs = Observation::new(&st, player).map_err(|e| e.to_string())?;
                let cancel = Cancel::new();
                let mut cx = AgentCx {
                    scratch,
                    par,
                    cancel: &cancel,
                };
                let mut rng =
                    SplitMix64::new(derive_seed!(MASTER, "agent", i64::try_from(k).expect("k")));
                gcg_search::pregame(&obs, cfg, &mut rng, &mut cx)
                    .map(|(a, _)| a)
                    .map_err(|e| e.to_string())
            },
            |_| {},
        )
        .expect("the agent decisions")
}

/// The W-45 check of one position: a clear truth for `right`, the reference wrong on every seed
/// and `paired_mulligan` right on at least 14 of 16, at both reference budgets.
fn check_mulligan(
    decks: [&str; 2],
    seed: u64,
    keeps: usize,
    expected_hand: &[&str],
    right: ActionKind,
) {
    let reg = Registry::embedded();
    let (st, player) = opening(&reg, decks, seed, keeps);
    assert_eq!(hand(&st, player), expected_hand, "the pinned hand");
    let base = null_config(&STANDARD, DEFAULT_WEIGHTS);
    let (mean, se) = truth(decks, seed, keeps, &base);
    let z = mean / se;
    println!(
        "{decks:?} seed {seed} seat {player}: truth redraw - keep = {mean:+.4} (se {se:.4}, z {z:+.1}) over {TRUTH_PAIRS} paired full games"
    );
    let truth_choice = if mean > 0.0 {
        ActionKind::Redraw
    } else {
        ActionKind::Keep
    };
    assert_eq!(truth_choice, right);
    assert!(z.abs() > 3.0, "the truth is clear: z {z}");
    for reference in [&STANDARD, &STRONG] {
        let null = null_config(reference, DEFAULT_WEIGHTS);
        let mut strengthened = null.clone();
        strengthened.mechanisms.paired_mulligan = true;
        let count_right = |cfg: &StrengthenedConfig| {
            choices(decks, seed, keeps, cfg)
                .iter()
                .filter(|a| a.kind == right)
                .count()
        };
        let (r, s) = (count_right(&null), count_right(&strengthened));
        println!(
            "  reference {} budget (mulligan_samples {}): reference right {r}/{AGENT_SEEDS}, paired_mulligan right {s}/{AGENT_SEEDS}",
            reference.name, null.mulligan_samples
        );
        assert_eq!(r, 0, "the reference misjudges the hand on every seed");
        assert!(s >= NEEDED, "paired_mulligan right on {s}/{AGENT_SEEDS}");
    }
}

/// Blue/Green ramp on the draw against Red/Purple control: Strike Rouge (Ootori), Penelope
/// (Middle Form), Re-GZ BWS, Gundam AGE-2 Normal and G-Self. `opening_score` (8.5 against a
/// threshold of about 11.3) sees Units it cannot cast early and ignores that Re-GZ BWS and G-Self
/// each place an EX Resource, so the reference redraws a hand that wins more kept.
#[test]
fn w45_the_reference_redraws_a_ramp_hand_that_wins_more_kept() {
    check_mulligan(
        ["red-purple-control", "blue-green-ramp"],
        32,
        1,
        &["GD05-005", "GD03-006", "GD05-023", "GD03-019", "EB01-038"],
        ActionKind::Keep,
    );
}

/// Purple/White Tekkadan on the play against Blue/Green ramp: two CGS Mobile Workers, Isaribi
/// (a Base), Gundam Barbatos 4th Form and Shiden Custom (Ryusei-Go). `opening_score` counts the
/// two Level 1 Units early and keeps; the hand develops little that matters and loses more than
/// a fresh five.
#[test]
fn w45_the_reference_keeps_a_hand_that_wins_more_redrawn() {
    check_mulligan(
        ["purple-white-tekkadan", "blue-green-ramp"],
        2,
        0,
        &["ST05-003", "ST05-015", "ST05-003", "ST05-001", "GD05-058"],
        ActionKind::Redraw,
    );
}

fn pregame_inline(
    st: &GameState,
    player: u8,
    cfg: &StrengthenedConfig,
    seed: u64,
) -> (Action, Vec<Alternative>) {
    let obs = Observation::new(st, player).expect("an observation");
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        gcg_search::pregame(&obs, cfg, &mut SplitMix64::new(seed), &mut cx)
            .expect("a play/draw choice")
    })
}

/// W-44: with a turn limit of 20, some of the 48 simulated pairs end drawn. The reference counts
/// every pair and scores a drawn game 0.5 (pinned); `exclude_capped_play_draw` counts only the
/// pairs whose two games have a winner, and its choice and reported means are those of the paired
/// test recomputed here from the same games.
#[test]
fn w44_unfinished_play_draw_games_leave_the_paired_estimate() {
    let reg = Registry::embedded();
    let lists = ["red-green-zeon", "blue-white-federation"].map(deck);
    let st = new_game(
        &reg,
        [&lists[0], &lists[1]],
        7,
        NewGameOptions {
            chooser: Some(0),
            turn_limit: 20,
            ..NewGameOptions::default()
        },
    )
    .expect("a legal game");
    let dec = st.pending().expect("a decision");
    assert_eq!(dec.hdr.kind, DecisionKind::ChooseFirst);
    let player = dec.hdr.player;
    let null = null_config(&STRONG, DEFAULT_WEIGHTS);
    let mut strengthened = null.clone();
    strengthened.mechanisms.exclude_capped_play_draw = true;
    let games = null.first_player_games;
    let decision_seed = derive_seed!(MASTER, "play-draw");
    let first = Action::new(ActionKind::GoFirst, i32::from(player), -1, -1);
    let second = Action::new(ActionKind::GoFirst, 1 - i32::from(player), -1, -1);
    // The games both implementations play: `games` seeds drawn up front, every choice on each.
    let mut rng = SplitMix64::new(decision_seed);
    let seeds: Vec<u64> = (0..games).map(|_| rng.next_u64()).collect();
    let results: Vec<[Option<i8>; 2]> = seeds
        .iter()
        .map(|&s| [first, second].map(|c| play_out(&st, player, c, s, &null)))
        .collect();
    let finished = |w: Option<i8>| w.is_some_and(|x| x >= 0);
    let counted: Vec<f64> = results
        .iter()
        .filter(|[a, b]| finished(*a) && finished(*b))
        .map(|[a, b]| score(*a, player) - score(*b, player))
        .collect();
    let drawn = results.iter().flatten().filter(|w| **w == Some(-1)).count();
    println!(
        "turn limit 20: {} of {games} pairs have a drawn game ({drawn} drawn games); {} pairs counted",
        usize::try_from(games).expect("a count") - counted.len(),
        counted.len()
    );
    assert!(
        drawn > 0 && !counted.is_empty(),
        "the position mixes finished and drawn games"
    );

    let games_f = f64::from(games);
    let mean_of = |k: usize| py_sum(results.iter().map(|r| score(r[k], player))) / games_f;
    let (r_action, r_alts) = pregame_inline(&st, player, &null, decision_seed);
    for (alt, k) in r_alts.iter().zip([0, 1]) {
        assert_eq!(alt.visits, games, "the reference counts every pair");
        assert_eq!(
            alt.value.to_bits(),
            mean_of(k).to_bits(),
            "drawn games score 0.5"
        );
    }
    let (s_action, s_alts) = pregame_inline(&st, player, &strengthened, decision_seed);
    let used = u32::try_from(counted.len()).expect("a count");
    for (alt, k) in s_alts.iter().zip([0, 1]) {
        assert_eq!(alt.visits, used, "only the pairs of finished games count");
        assert_eq!(alt.value.to_bits(), mean_of(k).to_bits());
    }
    let (mean, se) = paired(&counted);
    let expected = if mean < 0.0 && -mean > 2.0 * se {
        second
    } else {
        first
    };
    println!(
        "reference {:?}, exclude_capped_play_draw {:?} (counted mean {mean:+.4}, se {se:.4})",
        r_action.a, s_action.a
    );
    assert_eq!(s_action, expected);
}
