//! W-14 (T-114): close target choices stay noisy; the <Repair> ping puzzle
//! (`tests/ai/test_puzzles_targets.py:68-86`, `aiport::positions`) over 24 agent seeds.

use crate::aiport::positions::damage_the_unit_that_cannot_repair;
use crate::p6e_common::{reference_agent, solves, strengthened, strengthened_solves};

#[test]
fn the_target_critic_solves_the_repair_ping() {
    let (st, want) = damage_the_unit_that_cannot_repair();
    let reference = solves(&st, want, 24, reference_agent);
    let null = strengthened_solves(&st, want, 24, &strengthened(&[]));
    let critic = strengthened_solves(&st, want, 24, &strengthened(&[("target_rollouts", "true")]));
    eprintln!(
        "W-14 repair ping: reference {reference}/24, null {null}/24, target_rollouts {critic}/24"
    );
    // Recorded 2026-11-02: reference 7/24 (Python's shipped `standard`: 7 of 24, docs/AI.md),
    // null configuration 10/24, target_rollouts 24/24.
    assert_eq!(reference, 7, "the shipped reference standard (pinned)");
    assert!(
        reference <= 12 && null <= 12,
        "the reference misses the bar: {null}"
    );
    assert!(
        critic >= 20,
        "target_rollouts solves it in at least 20 of 24 seeds: {critic}"
    );
}
