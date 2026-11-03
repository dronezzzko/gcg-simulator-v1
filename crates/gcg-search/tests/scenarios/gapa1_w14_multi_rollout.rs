//! W-14 plan item (2) (GAP-A1): several rollouts per world at target decisions
//! (`target_multi_rollouts`), alone and with the target critic (`target_rollouts`), on the
//! <Repair> ping puzzle (`tests/ai/test_puzzles_targets.py:68-86`,
//! `aiport::positions::damage_the_unit_that_cannot_repair`) over 24 agent seeds at the reference
//! `standard` budget.
//!
//! The extra rollouts cut the rollout noise of each world, and the rollouts' own preference is
//! the wrong target (they favour pinging the Guncannon, as the critic's multi-rollout variant
//! did, docs/port/eval/weakness-status/P6-E.md, W-14): the search converges on it. So the
//! mechanism misses the W-14 bar (20/24) alone and costs the critic three seeds.

use crate::aiport::positions::damage_the_unit_that_cannot_repair;
use crate::p6e_common::{strengthened, strengthened_solves};

#[test]
fn multi_rollouts_on_the_repair_ping_are_recorded() {
    let (st, want) = damage_the_unit_that_cannot_repair();
    let null = strengthened_solves(&st, want, 24, &strengthened(&[]));
    let critic = strengthened_solves(&st, want, 24, &strengthened(&[("target_rollouts", "true")]));
    let mut counts = Vec::new();
    for r in ["2", "4", "8"] {
        let alone = strengthened_solves(
            &st,
            want,
            24,
            &strengthened(&[("target_multi_rollouts", r)]),
        );
        let both = strengthened_solves(
            &st,
            want,
            24,
            &strengthened(&[("target_multi_rollouts", r), ("target_rollouts", "true")]),
        );
        eprintln!(
            "W-14 repair ping: target_multi_rollouts={r} {alone}/24, with target_rollouts \
             {both}/24 (null {null}/24, target_rollouts alone {critic}/24)"
        );
        counts.push((alone, both));
    }
    // Recorded 2026-11-02: null 10/24, target_rollouts 24/24; R = 2, 4, 8: alone 2, 0, 0 and
    // with the critic 21, 21, 21.
    assert_eq!(
        (null, critic),
        (10, 24),
        "the P6-E record (p6e_w14_target_critic)"
    );
    assert_eq!(counts, [(2, 21), (0, 21), (0, 21)], "recorded counts");
    assert!(
        counts.iter().all(|&(alone, _)| alone < 20),
        "multi-rollouts alone miss the W-14 bar of 20/24"
    );
}
