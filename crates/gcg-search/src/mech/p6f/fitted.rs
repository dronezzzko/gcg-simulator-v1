//! The fitted constants of the calibrated policy (`calibrated.rs`), from the fit of `tests.rs`
//! (`fitted_policy_reproduces`, feature `slow`), recorded with their provenance in
//! `docs/dev/eval/P6-F/policy-fit.json` (`own.offset_bits`; checked by
//! `fitted_constants_are_the_recorded_fit`).

/// The calibrated policy's offsets in score units, in `calibrated::OFFSETS` order.
pub(crate) const OWN_OFFSETS: [f64; 6] = [
    f64::from_bits(0x3fec_9dab_a41e_66b2),
    f64::from_bits(0xbffc_d7df_5e73_072c),
    f64::from_bits(0x0000_0000_0000_0000),
    f64::from_bits(0xbfe1_ccae_b4e7_b259),
    f64::from_bits(0x3fd6_0e31_c894_aab3),
    f64::from_bits(0xbffc_de7f_9b9a_99cc),
];
