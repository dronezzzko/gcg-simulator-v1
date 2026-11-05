//! P6-H's mechanisms on the pre-game records of the D2 vectors
//! (`crates/gcg-ai/tests/vectors/search/decisions.ndjson`: 6 CHOOSE_FIRST and 12 REDRAW
//! decisions): the play/draw choice is the reference's whenever every game finishes, unfinished
//! games (capped or drawn) are excluded from the pairs, the mulligan falls back to the reference
//! rule without evidence, its lines stop after the opening turns, and both choices are the same
//! for any worker count.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use gcg_ai::{AgentCx, AgentError, Alternative, Scratch, first_player_choice, mulligan_choice};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, ActionKind, DecisionKind, GameState, StateSnapshot};
use serde_json::Value;

use super::super::PregameCx;
use super::mulligan::{OPENING_TURNS, mulligan_rollout};
use super::play_draw::{PLAY_DRAW_ROLLOUT, choose_with};
use super::sim::{Ending, Rollout, simulate};
use crate::{MechanismFlags, Observation, Preset, StrengthenedConfig, reference_mapping};

/// One pre-game record: the state (with `edit` applied to its canonical JSON), the deciding seat
/// and the per-decision seed.
struct Record {
    st: GameState,
    player: u8,
    seed: u64,
}

fn records(kind: &str, edit: impl Fn(&mut Value)) -> Vec<Record> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["kind"].as_str() == Some(kind))
        .map(|rec| {
            let snap = &rec["snapshot"];
            let mut state = snap["state"].clone();
            edit(&mut state);
            let st = from_canonical(
                &reg,
                &StateSnapshot {
                    state,
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            )
            .expect("a state");
            let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
            let seed = rec["seed"]
                .as_u64()
                .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
                .expect("a seed");
            Record { st, player, seed }
        })
        .collect()
}

/// `fast` (reference `standard` budgets) with only the P6-H flags `paired_mulligan` and
/// `exclude_capped_play_draw` set as given.
fn config(mulligan: bool, play_draw: bool) -> StrengthenedConfig {
    let mut cfg = reference_mapping(Preset::Fast).clone();
    cfg.mechanisms = MechanismFlags::NULL;
    cfg.mechanisms.paired_mulligan = mulligan;
    cfg.mechanisms.exclude_capped_play_draw = play_draw;
    cfg
}

type Choice = ((Action, Vec<Alternative>), u64);

/// `f` on the pre-game context of `rec` (the observation's information-set state) with a fresh
/// generator from the record's seed, inline; the result and the generator state afterwards.
fn with_cx(
    rec: &Record,
    cfg: &StrengthenedConfig,
    f: impl FnOnce(&mut PregameCx<'_, '_>) -> Result<(Action, Vec<Alternative>), AgentError>,
) -> Choice {
    let cancel = Cancel::new();
    let obs = Observation::new(&rec.st, rec.player).expect("an observation");
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut agent = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        let st = obs.info();
        let dec = st.pending().expect("a decision");
        let mut rng = SplitMix64::new(rec.seed);
        let out = f(&mut PregameCx {
            st,
            dec: &dec,
            player: rec.player,
            cfg,
            rng: &mut rng,
            agent: &mut agent,
            applies: 0,
        })
        .unwrap_or_else(|e| panic!("pre-game choice failed: {e}"));
        (out, rng.state)
    })
}

/// `gcg_search::pregame` of `cfg` at `rec` on the pool `par`.
fn pregame_on(
    rec: &Record,
    cfg: &StrengthenedConfig,
    par: &TaskCx<'_, Scratch>,
    scratch: &mut Scratch,
) -> Choice {
    let cancel = Cancel::new();
    let obs = Observation::new(&rec.st, rec.player).expect("an observation");
    let mut cx = AgentCx {
        scratch,
        par,
        cancel: &cancel,
    };
    let mut rng = SplitMix64::new(rec.seed);
    let out = crate::pregame(&obs, cfg, &mut rng, &mut cx)
        .unwrap_or_else(|e| panic!("pre-game choice failed: {e}"));
    (out, rng.state)
}

fn pregame(rec: &Record, cfg: &StrengthenedConfig) -> Choice {
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        pregame_on(rec, cfg, par, scratch)
    })
}

fn bits(c: &Choice) -> (Action, Vec<(Action, u64, u32)>, u64) {
    let ((action, alts), rng) = c;
    (
        *action,
        alts.iter()
            .map(|a| (a.action, a.value.to_bits(), a.visits))
            .collect(),
        *rng,
    )
}

#[test]
fn play_draw_is_the_reference_when_every_game_finishes() {
    let recs = records("choose_first", |_| {});
    assert_eq!(recs.len(), 6);
    for games in [8, 24] {
        let mut on = config(false, true);
        on.first_player_games = games;
        let mut off = on.clone();
        off.mechanisms.exclude_capped_play_draw = false;
        for rec in &recs {
            let reference = pregame(rec, &off);
            let strengthened = pregame(rec, &on);
            // Every vector game finishes, so every pair counts and nothing differs.
            assert!(strengthened.0.1.iter().all(|a| a.visits == games));
            assert_eq!(bits(&strengthened), bits(&reference));
        }
    }
}

#[test]
fn capped_games_are_excluded_from_the_pairs_and_scored_by_the_evaluation() {
    let recs = records("choose_first", |_| {});
    let cfg = config(false, true);
    let capped = Rollout {
        max_decisions: 20,
        ..PLAY_DRAW_ROLLOUT
    };
    for rec in &recs {
        let ((action, alts), _) = with_cx(rec, &cfg, |cx| choose_with(cx, capped));
        // No game finishes in 20 decisions: no pair counts, so the agent goes first.
        assert_eq!(
            action,
            Action::new(ActionKind::GoFirst, i32::from(rec.player), -1, -1)
        );
        for a in &alts {
            assert_eq!(a.visits, 0);
            // The mean evaluation of the capped games, never the reference's 0.5.
            assert!(
                a.value > 0.05 && a.value < 0.95 && a.value != 0.5,
                "{}",
                a.value
            );
        }
        // The same games to the end all finish.
        let ((_, full), _) = with_cx(rec, &cfg, |cx| choose_with(cx, PLAY_DRAW_ROLLOUT));
        assert!(full.iter().all(|a| a.visits == cfg.first_player_games));
    }
}

/// With a turn limit of 3 every game is drawn at the end of turn 3: the reference
/// (`first_player_choice`, pinned) scores each 0.5 and counts every pair; the mechanism counts
/// none, scores the drawn games 0.5 in the reported means, and goes first.
#[test]
fn drawn_games_are_excluded_from_the_pairs_and_the_reference_scores_them_even() {
    let recs = records("choose_first", |st| st["turn_limit"] = Value::from(3));
    let cfg = config(false, true);
    let games = cfg.first_player_games;
    for rec in &recs {
        let ((action, alts), _) = pregame(rec, &cfg);
        assert_eq!(
            action,
            Action::new(ActionKind::GoFirst, i32::from(rec.player), -1, -1)
        );
        assert!(alts.iter().all(|a| a.visits == 0 && a.value == 0.5));
        let ((_, ref_alts), _) = with_cx(rec, &cfg, |cx| {
            first_player_choice(cx.st, cx.player, games, cx.rng, &cx.cfg.knobs(), cx.agent)
        });
        assert!(ref_alts.iter().all(|a| a.visits == games && a.value == 0.5));
    }
}

#[test]
fn the_mulligan_falls_back_to_the_reference_rule_without_evidence() {
    let recs = records("redraw", |_| {});
    assert_eq!(recs.len(), 12);
    for samples in [1, 96] {
        let mut cfg = config(true, false);
        cfg.mulligan_samples = samples;
        for rec in &recs {
            let ((action, alts), rng) = pregame(rec, &cfg);
            let obs = Observation::new(&rec.st, rec.player).expect("an observation");
            let mut reference_rng = SplitMix64::new(rec.seed);
            let (fallback, _) =
                mulligan_choice(obs.info(), rec.player, samples, &mut reference_rng);
            // The reference rule draws first, then one seed per pair.
            for _ in 0..samples {
                reference_rng.next_u64();
            }
            assert_eq!(rng, reference_rng.state);
            assert!(alts.iter().all(|a| a.visits == samples));
            if samples == 1 {
                // One pair has no standard error: the reference rule decides.
                assert_eq!(action, fallback);
            }
        }
    }
}

#[test]
fn mulligan_lines_play_the_opening_turns_on_shared_worlds() {
    let recs = records("redraw", |_| {});
    // The reference `standard` and `strong` budgets: the starting points of `fast` and `standard`.
    for preset in [Preset::Fast, Preset::Standard] {
        let mut cfg = reference_mapping(preset).clone();
        cfg.mechanisms = MechanismFlags::NULL;
        cfg.mechanisms.paired_mulligan = true;
        let seeds: Vec<u64> = (0..cfg.mulligan_samples).map(u64::from).collect();
        let mut applies: u64 = 0;
        for rec in &recs {
            let rollout = mulligan_rollout(rec.st.turn(), cfg.rollout_temperature);
            assert_eq!(rollout.stop_turn, Some(1 + 2 * OPENING_TURNS));
            let keep = Action::of(ActionKind::Keep);
            let redraw = Action::of(ActionKind::Redraw);
            let mut lines = Vec::new();
            with_cx(rec, &cfg, |cx| {
                lines = simulate(cx, &[keep, redraw], &seeds, rollout)?;
                Ok((keep, Vec::new()))
            });
            assert_eq!(lines.len(), 2 * seeds.len());
            for l in &lines {
                assert!(
                    matches!(l.ending, Ending::Stopped | Ending::Finished(_)),
                    "{l:?}"
                );
                assert!(l.applies > 1 && l.value > 0.0 && l.value < 1.0, "{l:?}");
                applies += u64::from(l.applies);
            }
        }
        // The cost of one decision (PLAN §12.3: simulations count as applies), for the status
        // file.
        let decisions = u64::try_from(recs.len()).expect("a count");
        println!(
            "paired_mulligan at mulligan_samples {} (rollout temperature {}): {} engine applies per REDRAW decision on average",
            cfg.mulligan_samples,
            cfg.rollout_temperature,
            applies / decisions
        );
    }
}

#[test]
fn pregame_choices_are_identical_for_any_worker_count() {
    // `GameState` is not `Sync`: each task loads its own copy of the records.
    let load = || {
        let mut recs = records("redraw", |_| {});
        recs.extend(records("choose_first", |_| {}));
        recs
    };
    let recs = load();
    let mut cfg = config(true, true);
    cfg.mulligan_samples = 24;
    let inline: Vec<_> = recs.iter().map(|r| bits(&pregame(r, &cfg))).collect();
    let pool = Pool::new(NonZeroUsize::new(4).expect("four"));
    let parallel = pool
        .map_indexed(
            recs.len(),
            &Cancel::new(),
            |i, scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| -> Result<_, ()> {
                Ok(bits(&pregame_on(&load()[i], &cfg, par, scratch)))
            },
            |_| {},
        )
        .expect("the parallel run");
    assert_eq!(parallel, inline);
    let kinds: Vec<DecisionKind> = recs
        .iter()
        .map(|r| r.st.pending().expect("a decision").hdr.kind)
        .collect();
    assert!(kinds.contains(&DecisionKind::Redraw) && kinds.contains(&DecisionKind::ChooseFirst));
}
