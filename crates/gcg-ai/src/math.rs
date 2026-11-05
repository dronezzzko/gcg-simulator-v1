//! The floating-point library of the shared AI functions (W-137; docs/port/PLAN.md §12.3 S10).
//!
//! The reference search reproduces CPython on the golden platform, so it calls the platform C
//! math library ([`FloatMath::Platform`], `gcg_core::libm`). The strengthened search's
//! `portable_math` passes [`FloatMath::Portable`]: `gcg_core::portable` (the pure-Rust `libm`
//! crate, IEEE-754 basic operations only, no fused multiply-add) and squares as `x * x`, which
//! give the same bits on every IEEE-754 platform.
//!
//! Every platform-libm call of this crate goes through this module, and every function that
//! makes one has a `_with` variant taking the provider: `logistic_with`, `evaluate_with`,
//! `pick_with`, `choose_with`, `play_to_end_with`, `first_player_choice_with` and
//! `without_effect_with`. The frozen entry points (`logistic`, `evaluate`, `pick`, `choose`,
//! `play_to_end`, `first_player_choice`, `without_effect`) are those variants at
//! [`FloatMath::Platform`], so the reference keeps its results bit for bit.

use gcg_core::{libm, portable};

/// Which implementation of `exp`, `ln`, squares and `sqrt` a shared AI function uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FloatMath {
    /// The platform C math library (`gcg_core::libm`): CPython's results on the golden
    /// platform, used by the reference and by the strengthened search without `portable_math`.
    Platform,
    /// `gcg_core::portable` and `x * x`: the same bits on every IEEE-754 platform.
    Portable,
}

impl FloatMath {
    /// `math.exp(x)`.
    #[must_use]
    pub fn exp(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::exp(x),
            Self::Portable => portable::exp(x),
        }
    }

    /// `math.log(x)` for `x > 0`.
    #[must_use]
    pub fn ln(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::ln(x),
            Self::Portable => portable::log(x),
        }
    }

    /// `x ** 2`: libm `pow` (CPython's `**`), or the correctly rounded product.
    #[must_use]
    pub fn square(self, x: f64) -> f64 {
        match self {
            Self::Platform => libm::square(x),
            Self::Portable => x * x,
        }
    }

    /// `math.sqrt(x)`: IEEE-754 square root, correctly rounded on every platform, so both
    /// providers return the same bits.
    #[must_use]
    pub fn sqrt(self, x: f64) -> f64 {
        match self {
            Self::Platform | Self::Portable => libm::sqrt(x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUTS: [f64; 17] = [
        -745.1, -30.5, -2.0, -1.0, -0.3, -1e-9, -0.0, 0.0, 1e-300, 1e-9, 0.1, 0.5, 1.0, 2.5, 30.0,
        700.0, 709.7,
    ];

    #[test]
    fn platform_is_gcg_core_libm_and_portable_is_the_libm_crate() {
        for x in INPUTS {
            let m = FloatMath::Platform;
            assert_eq!(m.exp(x).to_bits(), libm::exp(x).to_bits(), "exp({x})");
            assert_eq!(m.square(x).to_bits(), libm::square(x).to_bits(), "{x}²");
            assert_eq!(m.sqrt(x.abs()).to_bits(), libm::sqrt(x.abs()).to_bits());
            let p = FloatMath::Portable;
            assert_eq!(p.exp(x).to_bits(), portable::exp(x).to_bits(), "exp({x})");
            assert_eq!(p.square(x).to_bits(), (x * x).to_bits(), "{x}²");
            assert_eq!(p.sqrt(x.abs()).to_bits(), m.sqrt(x.abs()).to_bits());
            if x > 0.0 {
                assert_eq!(m.ln(x).to_bits(), libm::ln(x).to_bits(), "ln({x})");
                assert_eq!(p.ln(x).to_bits(), portable::log(x).to_bits(), "ln({x})");
            }
        }
    }

    /// On this machine the two providers agree to one unit in the last place on these inputs
    /// (the recorded 8,000-input comparison is `gcg-search`'s
    /// `portable_exp_and_ln_are_within_one_ulp_of_the_golden_platform`).
    #[test]
    fn the_providers_agree_within_one_ulp() {
        for x in INPUTS {
            let close = |a: f64, b: f64| a == b || a.to_bits().abs_diff(b.to_bits()) <= 1;
            let (m, p) = (FloatMath::Platform, FloatMath::Portable);
            assert!(close(m.exp(x), p.exp(x)), "exp({x})");
            assert!(close(m.square(x), p.square(x)), "{x}²");
            if x > 0.0 {
                assert!(close(m.ln(x), p.ln(x)), "ln({x})");
            }
        }
    }

    /// The identifier that ends right before byte `at` of `code`.
    fn receiver(code: &str, at: usize) -> String {
        let rev: String = code[..at]
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        rev.chars().rev().collect()
    }

    /// Platform independence by construction: no platform-libm call of this crate lives
    /// outside this module, and every transcendental method call elsewhere is on a
    /// [`FloatMath`] named `math`, so every other float path is IEEE-754 basic operations.
    #[test]
    fn every_platform_libm_call_of_the_crate_is_in_this_module() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut entries: Vec<_> = std::fs::read_dir(&src)
            .expect("src")
            .map(|e| e.expect("an entry").path())
            .collect();
        entries.sort();
        let methods = [
            "exp", "ln", "log", "log2", "log10", "powf", "powi", "exp_m1", "ln_1p", "sin", "cos",
            "tan", "cbrt", "hypot", "mul_add",
        ];
        let mut checked = 0;
        for path in entries {
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("math.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file");
            for (i, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let at = format!("{}:{}", path.display(), i + 1);
                assert!(
                    !code.contains("libm::"),
                    "{at}: a platform libm call: {line}"
                );
                for m in methods {
                    let pat = format!(".{m}(");
                    let mut from = 0;
                    while let Some(k) = code[from..].find(&pat) {
                        let pos = from + k;
                        from = pos + pat.len();
                        assert_eq!(receiver(code, pos), "math", "{at}: `.{m}(`: {line}");
                    }
                }
            }
            checked += 1;
        }
        assert!(checked >= 14, "{checked} source files checked");
    }

    /// The lint's scanner sees through a receiver chain (a negative control).
    #[test]
    fn the_lint_names_the_receiver() {
        let code = "let a = x.exp(); let b = cx.math.exp(y);";
        let first = code.find(".exp(").expect("a call");
        assert_eq!(receiver(code, first), "x");
        let second = code.rfind(".exp(").expect("a call");
        assert_eq!(receiver(code, second), "math");
    }
}
