//! W-09 (T-111): move canonicalisation never pays with an EX Resource on purpose. Gundam Lfrith
//! Ur (GD04-020: "【Once per Turn】During your turn, when you play and activate a (Dawn of Fold)
//! Command card using an EX Resource, draw 1.") is on the field and Witches from Earth
//! (GD04-108, a (Dawn of Fold) Command) is in hand, with four Resources and one EX Resource.
//! Paying with the EX Resource draws a card (and doubles the Command's reduction); paying with a
//! Resource only rests it. The reference keeps the fewest-EX payment, which its no-effect check
//! then prunes, so it never pays with EX; the strengthened root keeps both payments.

use gcg_ai::ex_used;
use gcg_engine::{ActionKind, AddOptions, GameState, Zone};

use crate::aiport::{FILLER, Sc};
use crate::p6e_common::{decide, reference_agent, strengthened};

/// The position: player 0's main phase, Lfrith Ur rested, Witches in hand, 4 + 1 EX Resources.
pub fn ex_payment() -> GameState {
    let mut sc = Sc::new();
    sc.resources(0, 4, 0, 1);
    sc.add_with(
        0,
        "GD04-020",
        Zone::Battle,
        AddOptions {
            rested: true,
            ..AddOptions::default()
        },
    );
    sc.in_hand(0, "GD04-108");
    sc.filler_shields(0, 5);
    sc.deck(0, &["GD04-031"; 20]);
    sc.rested(1, FILLER);
    sc.filler_shields(1, 5);
    for _ in 0..3 {
        sc.hidden_in_hand(1, FILLER);
    }
    sc.deck(1, &[FILLER; 20]);
    sc.start()
}

/// The number of agent seeds `1..=seeds` that play Witches paying with EX.
fn pays_with_ex(
    st: &GameState,
    seeds: u64,
    mut agent: impl FnMut(u64) -> Box<dyn gcg_ai::Agent>,
) -> u32 {
    (1..=seeds)
        .map(|seed| {
            let a = decide(&mut *agent(seed), st);
            u32::from(a.kind == ActionKind::PlayCommand && ex_used(a) >= 1)
        })
        .sum()
}

#[test]
fn ex_sensitive_plays_keep_their_ex_payment() {
    let st = ex_payment();
    let dec = st.pending().expect("the main phase");
    assert!(
        dec.options
            .iter()
            .any(|&a| a.kind == ActionKind::PlayCommand && ex_used(a) == 1),
        "the EX payment is legal"
    );
    let reference = pays_with_ex(&st, 16, reference_agent);
    let null = pays_with_ex(&st, 16, |seed| {
        gcg_search::make_agent(&strengthened(&[]), seed, false)
    });
    let ex = pays_with_ex(&st, 16, |seed| {
        gcg_search::make_agent(&strengthened(&[("ex_variants", "true")]), seed, false)
    });
    eprintln!("W-09 EX payment: reference {reference}/16, null {null}/16, ex_variants {ex}/16");
    // Recorded 2026-11-02: reference 0/16, null configuration 0/16, ex_variants 16/16.
    assert_eq!(reference, 0, "the reference never pays with EX (pinned)");
    assert_eq!(null, 0, "nor does the null configuration");
    assert!(
        ex >= 14,
        "ex_variants pays with EX in at least 14 of 16 seeds: {ex}"
    );
}
