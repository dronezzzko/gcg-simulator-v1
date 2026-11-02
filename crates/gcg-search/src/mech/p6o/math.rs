//! The floating-point functions of the strengthened search (W-137): the platform C library, as
//! the reference uses it to match CPython on the golden platform, or the portable functions of
//! `gcg_core::portable` (the pure-Rust `libm` crate) with squares as `x * x`, which give the same
//! bits on every IEEE-754 platform. `portable_math` selects the second.

use gcg_ai::PolicyKnobs;
use gcg_core::rng::SplitMix64;
use gcg_core::{libm, portable};
use gcg_engine::{Action, EngineError, GameState};

/// Which implementation of `exp`, `ln` and squares the search uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Math {
    /// The platform libm (`gcg_core::libm`): the reference's results.
    Platform,
    /// `gcg_core::portable` and `x * x`: the same bits on every platform.
    Portable,
}

impl Math {
    /// The implementation `portable_math` selects.
    pub(crate) const fn of(portable_math: bool) -> Self {
        if portable_math {
            Self::Portable
        } else {
            Self::Platform
        }
    }

    pub(crate) fn exp(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::exp(x),
            Self::Portable => portable::exp(x),
        }
    }

    pub(crate) fn ln(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::ln(x),
            Self::Portable => portable::log(x),
        }
    }

    /// `x ** 2`: libm `pow` (CPython's `**`), or the correctly rounded product.
    pub(crate) fn square(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::square(x),
            Self::Portable => x * x,
        }
    }

    /// `math.sqrt`, correctly rounded on every platform.
    pub(crate) fn sqrt(self, x: f64) -> f64 {
        libm::sqrt(x)
    }

    /// `logistic(z)` (`ai/evaluation.py:303-307`): `gcg_ai::logistic`, or the same stable form
    /// with the portable `exp`.
    pub(crate) fn logistic(self, z: f64) -> f64 {
        match self {
            Self::Platform => gcg_ai::logistic(z),
            Self::Portable => {
                if z >= 0.0 {
                    return 1.0 / (1.0 + portable::exp(-z));
                }
                let e = portable::exp(z);
                e / (1.0 + e)
            }
        }
    }

    /// The rollout policy's move (`policy.choose(s, rng, temperature, knobs)`,
    /// `ai/policy.py:72-83`): `gcg_ai::choose`, or the same scores and Gumbel-max draws with the
    /// portable `ln` (`gcg_ai::pick`'s order: one `random()` per option, `0.0` read as `1e-300`,
    /// the first strictly greater value wins).
    pub(crate) fn choose(
        self,
        s: &GameState,
        rng: &mut SplitMix64,
        temperature: f64,
        knobs: &PolicyKnobs,
    ) -> Result<Action, EngineError> {
        if self == Self::Platform {
            return gcg_ai::choose(s, Some(rng), temperature, knobs);
        }
        let Some(dec) = s.pending() else {
            return Err(EngineError::Assertion(String::new()));
        };
        if dec.options.len() == 1 {
            return Ok(dec.options[0]);
        }
        let scores = gcg_ai::option_scores(s, &dec, dec.options, knobs)?;
        if dec.options.is_empty() {
            return Err(if temperature <= 0.0 {
                EngineError::Value("max() iterable argument is empty".into())
            } else {
                EngineError::Index("tuple index out of range".into())
            });
        }
        if temperature <= 0.0 {
            return Ok(dec.options[gcg_ai::pick(dec.options, &scores, None, temperature)]);
        }
        let mut best_i = 0;
        let mut best_v = f64::NEG_INFINITY;
        for (i, &x) in scores.iter().enumerate() {
            let mut u = rng.random();
            if u == 0.0 {
                u = 1e-300;
            }
            let v = x / temperature - portable::log(-portable::log(u));
            if v > best_v {
                best_i = i;
                best_v = v;
            }
        }
        Ok(dec.options[best_i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_is_the_reference_and_portable_is_the_libm_crate() {
        let xs = [
            -745.1, -30.5, -1.0, -1e-9, -0.0, 0.0, 1e-300, 0.1, 0.5, 1.0, 2.5, 30.0, 709.7,
        ];
        for x in xs {
            assert_eq!(Math::Platform.exp(x).to_bits(), libm::exp(x).to_bits());
            assert_eq!(Math::Portable.exp(x).to_bits(), portable::exp(x).to_bits());
            assert_eq!(
                Math::Platform.logistic(x).to_bits(),
                gcg_ai::logistic(x).to_bits()
            );
            assert_eq!(
                Math::Platform.square(x).to_bits(),
                libm::square(x).to_bits()
            );
            assert_eq!(Math::Portable.square(x).to_bits(), (x * x).to_bits());
            if x > 0.0 {
                assert_eq!(Math::Platform.ln(x).to_bits(), libm::ln(x).to_bits());
                assert_eq!(Math::Portable.ln(x).to_bits(), portable::log(x).to_bits());
            }
            // Both implementations agree to within one unit in the last place on these inputs.
            let (a, b) = (Math::Platform.exp(x), Math::Portable.exp(x));
            assert!(
                a == b || (a.to_bits().abs_diff(b.to_bits()) <= 1),
                "exp({x})"
            );
            let (a, b) = (Math::Platform.logistic(x), Math::Portable.logistic(x));
            assert!(
                a == b || (a.to_bits().abs_diff(b.to_bits()) <= 1),
                "logistic({x})"
            );
        }
        assert_eq!(Math::of(false), Math::Platform);
        assert_eq!(Math::of(true), Math::Portable);
    }

    /// The portable `exp` and `ln` are within one unit in the last place of CPython's results
    /// on the golden platform (Apple libm) on the 4,000 recorded inputs of
    /// `port/oracle/export_vectors.py` (`crates/gcg-core/tests/vectors.json`, `libm`). They are
    /// not bit-identical to it: 7,358 of the 8,000 results agree exactly (measured 2026-11-02),
    /// which is why the reference keeps the platform functions.
    #[test]
    fn portable_exp_and_ln_are_within_one_ulp_of_the_golden_platform() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../gcg-core/tests/vectors.json");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("the vectors"))
                .expect("JSON");
        let bits = |v: &serde_json::Value| {
            f64::from_bits(u64::from_str_radix(v.as_str().expect("hex bits"), 16).expect("hex"))
        };
        let (mut checked, mut exact) = (0, 0);
        for case in v["libm"].as_array().expect("libm cases") {
            let x = bits(&case["x"]);
            for (got, want) in [
                (!case["exp"].is_null()).then(|| (Math::Portable.exp(x), bits(&case["exp"]))),
                (!case["log"].is_null()).then(|| (Math::Portable.ln(x.abs()), bits(&case["log"]))),
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    got.to_bits().abs_diff(want.to_bits()) <= 1,
                    "{x:e}: {got:e} vs {want:e}"
                );
                checked += 1;
                exact += usize::from(got.to_bits() == want.to_bits());
            }
        }
        assert_eq!(checked, 8000);
        eprintln!("portable exp/ln equal to Apple libm on {exact} of {checked} inputs");
    }
}
