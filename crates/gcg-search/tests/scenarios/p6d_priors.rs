//! W-140 (unit, second test): root priors do not depend on the world they are computed in.
//!
//! The root's priors are the softmax (temperature `prior_temperature`) of
//! `policy.option_scores` over the root moves in world 0 (`ai/mcts.py:126-130`). On 1,000
//! recorded positions, the priors are computed in eight determinisations from the decider's
//! information set and compared bit for bit, per decision kind; a difference would show a
//! scorer that reads hidden state. The positions are every snapshot of the tier-1 game sets
//! (`port/corpus/v1/tier1/`) whose decision has at least two canonical moves (745), topped up
//! with every fifth such decision of rollout-policy games on the fuzz decks.

use std::path::PathBuf;
use std::sync::Arc;

use gcg_ai::{canonical, choose, option_scores};
use gcg_core::derive_seed;
use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_diff::fuzz::Pools;
use gcg_diff::{Record, read_trace};
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::observe::determinize;
use gcg_engine::{DecisionKind, GameState, NewGameOptions, apply, new_game_by_number};
use gcg_search::{Preset, preset_config};

/// Positions compared.
const POSITIONS: usize = 1000;
/// Determinisations per position.
const WORLDS: u64 = 8;
/// The corpus sets with game traces.
const SETS: [&str; 7] = [
    "search",
    "full",
    "fuzz",
    "coverage",
    "baselines",
    "vanilla",
    "tests/ai",
];
/// The master seed of the top-up games.
const MASTER: u64 = 14_007;

/// Whether `st` is a decision with at least two canonical moves.
fn searched(st: &GameState) -> bool {
    st.pending().is_some_and(|d| canonical(st, &d).len() >= 2)
}

/// The corpus positions, in set and file order.
fn corpus_positions(reg: &Arc<Registry>) -> Vec<GameState> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../port/corpus/v1/tier1");
    let mut out = Vec::new();
    for set in SETS {
        let mut files: Vec<PathBuf> = std::fs::read_dir(base.join(set))
            .expect("a corpus set")
            .map(|e| e.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
            .collect();
        files.sort();
        for path in files {
            for trace in read_trace(&path).unwrap_or_else(|e| panic!("{e}")) {
                for rec in &trace.records {
                    let Record::Snapshot(s) = rec else { continue };
                    let st = from_canonical(reg, &s.snapshot).expect("a state");
                    if searched(&st) {
                        out.push(st);
                    }
                }
            }
        }
    }
    out
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// The recorded positions: the corpus positions, then every fifth searched decision of
/// rollout-policy games (temperature 0.5) on the fuzz decks until there are `POSITIONS`.
fn positions() -> Vec<GameState> {
    let reg = Registry::embedded();
    let mut out = corpus_positions(&reg);
    out.truncate(POSITIONS);
    let pools = Pools::new(&reg);
    let knobs = preset_config(Preset::Standard).knobs();
    let mut game: u32 = 0;
    while out.len() < POSITIONS {
        let d0 = pools.deck_for(2 * u64::from(game), MASTER);
        let d1 = pools.deck_for(2 * u64::from(game) + 1, MASTER);
        let (m0, r0, m1, r1) = (
            refs(&d0.main),
            refs(&d0.resources),
            refs(&d1.main),
            refs(&d1.resources),
        );
        let mut st = new_game_by_number(
            &reg,
            [(&m0, &r0), (&m1, &r1)],
            derive_seed!(MASTER, "w140-game", game),
            NewGameOptions::default(),
        )
        .expect("a legal fuzz game");
        let mut rng = SplitMix64::new(derive_seed!(MASTER, "w140-policy", game));
        let mut seen = 0;
        while st.winner().is_none() && out.len() < POSITIONS {
            if searched(&st) {
                if seen % 5 == 0 {
                    out.push(st.clone());
                }
                seen += 1;
            }
            let a = choose(&st, Some(&mut rng), 0.5, &knobs).expect("a policy move");
            apply(&mut st, a, true).expect("a legal move");
        }
        game += 1;
    }
    out
}

/// The root priors of world `w`'s decision, as bits, with the move keys.
fn priors(w: &GameState) -> Vec<(gcg_ai::MoveKey, u64)> {
    let cfg = preset_config(Preset::Standard);
    let dec = w.pending().expect("a decision");
    let moves = canonical(w, &dec);
    let options: Vec<_> = moves.iter().map(|&(_, a)| a).collect();
    let scores = option_scores(w, &dec, &options, &cfg.knobs()).expect("scores");
    let mut top = scores[0];
    for &s in &scores[1..] {
        if s > top {
            top = s;
        }
    }
    let weights: Vec<f64> = scores
        .iter()
        .map(|&x| ((x - top) / cfg.prior_temperature).exp())
        .collect();
    let total = py_sum(weights.iter().copied());
    moves
        .iter()
        .zip(&weights)
        .map(|(&(k, _), &wt)| (k, (wt / total).to_bits()))
        .collect()
}

#[test]
fn root_priors_are_equal_across_eight_determinisations() {
    let positions = positions();
    assert_eq!(positions.len(), POSITIONS);
    // Per decision kind: (positions, positions whose priors differ between worlds).
    let mut table: Vec<(DecisionKind, usize, usize)> = Vec::new();
    for (i, st) in positions.iter().enumerate() {
        let dec = st.pending().expect("a decision");
        let player = dec.hdr.player;
        let seed = gcg_core::derive_seed!(0x5076_3634_u64, "w140", i);
        let first = priors(&determinize(st, player, seed).expect("a world"));
        let differs = (1..WORLDS).any(|k| {
            let w = determinize(st, player, seed.wrapping_add(k)).expect("a world");
            priors(&w) != first
        });
        match table.iter_mut().find(|(k, _, _)| *k == dec.hdr.kind) {
            Some(row) => {
                row.1 += 1;
                row.2 += usize::from(differs);
            }
            None => table.push((dec.hdr.kind, 1, usize::from(differs))),
        }
    }
    table.sort_by_key(|&(k, _, _)| k as u8);
    let differing: Vec<_> = table.iter().filter(|r| r.2 > 0).collect();
    assert!(
        differing.is_empty(),
        "priors differ between worlds: {table:?}"
    );
    // Every decision kind the root meets in these games is covered.
    let kinds: Vec<DecisionKind> = table.iter().map(|r| r.0).collect();
    for k in [
        DecisionKind::Main,
        DecisionKind::Block,
        DecisionKind::ActionStep,
    ] {
        assert!(kinds.contains(&k), "{table:?}");
    }
    println!("W-140 root priors over {WORLDS} determinisations: {table:?}");
}
