//! W-05 (GAP-A1): why the fusion position's root-decision bar ("the strengthened search avoids
//! the attack in at least 13 of 16 seeds") cannot be met while the leaf evaluation is
//! `ROLE_WEIGHTS`.
//!
//! In P6-C's fusion position (`p6c_w05_fusion::fusion`) the defender's one hidden card is the
//! trick GD02-118 in a share of the worlds. The searcher's choice is between attacking the
//! player with Gundam and ending the main phase. The test runs the search at the reference
//! `standard` settings with a 4,000-iteration budget on three versions of the position: the
//! hidden card as P6-C built it (unknown), and the same position with the defender's hand
//! *revealed* as the trick or as a filler card (the searcher knows the world class). Revealed
//! hands give the value of each move in its world class with the opponent's reply fitted to it
//! (no fusion), so the attack's expected value at a trick share `p` is
//! `p * A_trick + (1 - p) * A_none`, and likewise for ending the phase. The break-even share
//! `p* = (A_none - E_none) / ((A_none - E_none) + (E_trick - A_trick))` is the share above which
//! holding back is right by the search's own evaluation. The scenario's premise, holding back is
//! correct with the trick in half of the worlds, needs `p* < 0.5`.

use gcg_ai::{AgentCx, NoTrace};
use gcg_core::pymath::py_sum;
use gcg_core::rng::SplitMix64;
use gcg_engine::{ActionKind, GameState, Scenario, ScenarioOptions, Zone};
use gcg_search::{Observation, StrengthenedConfig};

use crate::aiport::{registry, with_cx};
use crate::p6c_common::{FILLER, add, hidden, start, strengthened};

const TRICK: &str = "GD02-118";

/// The fusion position (P6-C's) with the defender's card `hand`: hidden, or revealed.
fn position(hand: &str, revealed: bool) -> GameState {
    let mut sc = Scenario::new(&registry(), ScenarioOptions::default());
    add(&mut sc, 0, "GD01-013", Zone::Battle);
    sc.resources(0, 3, 0, 0).expect("resources");
    sc.shields(0, &[FILLER; 5]).expect("shields");
    sc.deck(0, &[FILLER; 20]);
    add(&mut sc, 1, "GD02-079", Zone::Battle);
    sc.resources(1, 3, 0, 0).expect("resources");
    let pool: Vec<&str> = (0..26)
        .map(|i| if i % 2 == 0 { TRICK } else { FILLER })
        .collect();
    if revealed {
        add(&mut sc, 1, hand, Zone::Hand);
    } else {
        hidden(&mut sc, 1, hand);
    }
    sc.shields(1, &pool[1..6]).expect("shields");
    sc.deck(1, &pool[6..]);
    start(sc)
}

fn deep() -> StrengthenedConfig {
    let mut cfg = strengthened(&[]);
    cfg.iterations_per_option = 4000;
    cfg.min_iterations = 4000;
    cfg.max_iterations = 4000;
    cfg
}

/// The mean root values of attacking the player and of ending the main phase over seeds 1..=4.
fn values(st: &GameState) -> (f64, f64) {
    let cfg = deep();
    let (mut attack, mut end) = (Vec::new(), Vec::new());
    for seed in 1..=4u64 {
        let obs = Observation::new(st, 0).expect("an observation");
        let r = with_cx(|cx: &mut AgentCx<'_>| {
            gcg_search::search(&obs, &cfg, &mut SplitMix64::new(seed), cx, &mut NoTrace)
        })
        .expect("a search");
        for a in &r.alternatives {
            match a.action.kind {
                ActionKind::Attack if a.action.b < 0 => attack.push(a.value),
                ActionKind::EndMain => end.push(a.value),
                _ => {}
            }
        }
    }
    assert_eq!(
        (attack.len(), end.len()),
        (4, 4),
        "both moves are root moves"
    );
    let mean = |x: &[f64]| py_sum(x.iter().copied()) / 4.0;
    (mean(&attack), mean(&end))
}

#[test]
fn w05_the_evaluation_prefers_the_attack_below_a_high_trick_share() {
    let (a_hidden, e_hidden) = values(&position(TRICK, false));
    let (a_trick, e_trick) = values(&position(TRICK, true));
    let (a_none, e_none) = values(&position(FILLER, true));
    let gain = a_none - e_none;
    let loss = e_trick - a_trick;
    let break_even = gain / (gain + loss);
    eprintln!(
        "W-05 fusion values at 4,000 iterations (seeds 1-4): hidden card attack {a_hidden:.4} \
         end {e_hidden:.4}; trick revealed attack {a_trick:.4} end {e_trick:.4}; filler \
         revealed attack {a_none:.4} end {e_none:.4}; break-even trick share {break_even:.3}"
    );
    // Recorded 2026-11-02: hidden card attack 0.3163, end 0.2939; trick revealed attack 0.3134,
    // end 0.3349; filler revealed attack 0.2878, end 0.2666; break-even share 0.497. Holding
    // back gains 0.0215 in the trick worlds and loses 0.0212 in the others, so with the trick
    // in half of the worlds the two moves are worth the same to the evaluation (a difference
    // of 0.0002), and at the position's own share (13 of 46 unseen cards, 0.28) the attack is
    // the better move. No search that maximises this leaf value can hold back in 13 of 16
    // seeds at a share of one half.
    assert!(
        a_hidden > e_hidden,
        "the search attacks in the fusion position"
    );
    assert!(
        e_trick > a_trick,
        "with the trick known, holding back is better"
    );
    assert!(a_none > e_none, "without the trick, the attack is better");
    assert!(
        (break_even - 0.5).abs() < 0.05,
        "the moves tie at a trick share of one half: p* = {break_even:.3}"
    );
    assert!(
        break_even > 13.0 / 46.0,
        "at the position's own share the attack is the better move"
    );
}
