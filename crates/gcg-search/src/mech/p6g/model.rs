//! P6-G's fitted evaluation models (docs/port/PLAN.md §12.3 S10; W-34, W-35, W-36, W-142): which
//! columns of the superset vector a model reads ([`ModelSpec`]), and the fitted weights, one
//! vector per phase bin, embedded from `weights.txt`.
//!
//! `weights.txt` is written by `gcg-eval tune-leaf` (the fit's report and regeneration command
//! are in `docs/port/eval/P6-G/`); its lines are
//!
//! ```text
//! # comment
//! model <key>
//! edges <turn> ...          (only for a phase model: bin = the number of edges <= turn)
//! w <bits> <bits> ...       (one line per bin: the weights' IEEE-754 bits in hex)
//! ```
//!
//! A model scores `z = w · x` from the turn player's point of view, summed in column order.

use std::fmt::Write as _;
use std::sync::LazyLock;

use super::features::{
    ABILITY_START, BURST_START, HAND_DIFFS, HAND_START, KW_START, N_ABILITY, N_BASE, N_BURST,
    N_FEATURES, N_HAND, N_KW, NAMES,
};
use crate::MechanismFlags;

/// Which fitted model a configuration uses: the feature groups, the recording mode of the fit
/// and its form. The key spells every field, so a table entry names exactly one model.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per independent model choice, each an ablation flag"
)]
pub struct ModelSpec {
    /// `keyword_features` (W-37).
    pub keywords: bool,
    /// `ability_count_features` (W-37).
    pub abilities: bool,
    /// `hand_slots` (W-37): the hand block replaces the role block's hand differences.
    pub hand_slots: bool,
    /// `burst_shield_probability` (W-37).
    pub burst: bool,
    /// `determinized_fit` (W-142): fitted on determinised recordings.
    pub determinized: bool,
    /// The constant term; `no_tempo_feature` fits without it (W-39).
    pub intercept: bool,
    /// `phase_weights` (W-35): one weight vector per phase bin.
    pub phase: bool,
}

impl ModelSpec {
    /// The model of `flags`, or `None` when no P6-G model flag is on (the leaf then scores with
    /// the configuration's weights).
    #[must_use]
    pub const fn of(flags: &MechanismFlags) -> Option<Self> {
        let on = flags.refit_weights
            || flags.phase_weights
            || flags.keyword_features
            || flags.ability_count_features
            || flags.hand_slots
            || flags.burst_shield_probability
            || flags.no_tempo_feature
            || flags.determinized_fit;
        if !on {
            return None;
        }
        Some(Self {
            keywords: flags.keyword_features,
            abilities: flags.ability_count_features,
            hand_slots: flags.hand_slots,
            burst: flags.burst_shield_probability,
            determinized: flags.determinized_fit,
            intercept: !flags.no_tempo_feature,
            phase: flags.phase_weights,
        })
    }

    /// The table key, every field spelled out: `kw=0,ab=0,hs=0,bp=0,det=0,int=1,ph=0`.
    #[must_use]
    pub fn key(&self) -> String {
        let b = |x: bool| u8::from(x);
        format!(
            "kw={},ab={},hs={},bp={},det={},int={},ph={}",
            b(self.keywords),
            b(self.abilities),
            b(self.hand_slots),
            b(self.burst),
            b(self.determinized),
            b(self.intercept),
            b(self.phase)
        )
    }

    /// The spec a key spells, or `None` for a key that is not one.
    #[must_use]
    pub fn parse_key(key: &str) -> Option<Self> {
        let mut vals = [None; 7];
        let names = ["kw", "ab", "hs", "bp", "det", "int", "ph"];
        for item in key.split(',') {
            let (k, v) = item.split_once('=')?;
            let i = names.iter().position(|n| *n == k)?;
            let v = match v {
                "0" => false,
                "1" => true,
                _ => return None,
            };
            if vals[i].replace(v).is_some() {
                return None;
            }
        }
        let [kw, ab, hs, bp, det, int, ph] = vals;
        Some(Self {
            keywords: kw?,
            abilities: ab?,
            hand_slots: hs?,
            burst: bp?,
            determinized: det?,
            intercept: int?,
            phase: ph?,
        })
    }

    /// The superset columns the model reads, in order.
    #[must_use]
    pub fn columns(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..N_BASE)
            .filter(|&i| {
                (i != 0 || self.intercept) && !(self.hand_slots && HAND_DIFFS.contains(&i))
            })
            .collect();
        let mut block = |on: bool, start: usize, n: usize| {
            if on {
                out.extend(start..start + n);
            }
        };
        block(self.hand_slots, HAND_START, N_HAND);
        block(self.keywords, KW_START, N_KW);
        block(self.abilities, ABILITY_START, N_ABILITY);
        block(self.burst, BURST_START, N_BURST);
        debug_assert!(out.iter().all(|&c| c < N_FEATURES));
        out
    }

    /// The names of [`ModelSpec::columns`].
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.columns().into_iter().map(|c| NAMES[c]).collect()
    }
}

/// One fitted model: its columns, the phase-bin edges (ascending turns) and one weight vector
/// per bin.
#[derive(Clone, PartialEq, Debug)]
pub struct Model {
    pub spec: ModelSpec,
    pub columns: Vec<usize>,
    pub edges: Vec<i32>,
    pub weights: Vec<Vec<f64>>,
}

impl Model {
    /// The bin of a position at `turn`: the number of edges at or below it.
    #[must_use]
    pub fn bin(&self, turn: i32) -> usize {
        self.edges.iter().filter(|&&e| e <= turn).count()
    }

    /// `z = w · x` of the superset vector `x` at `turn`, summed in column order.
    #[must_use]
    pub fn score(&self, x: &[f64], turn: i32) -> f64 {
        let w = &self.weights[self.bin(turn).min(self.weights.len() - 1)];
        let mut z = 0.0;
        for (wi, &c) in w.iter().zip(&self.columns) {
            z += wi * x[c];
        }
        z
    }
}

/// The text of a table of `models`, in order, after the comment lines `header`.
#[must_use]
pub fn table_text(header: &[String], models: &[Model]) -> String {
    let mut out = String::new();
    for h in header {
        let _ = writeln!(out, "# {h}");
    }
    for m in models {
        let _ = writeln!(out, "model {}", m.spec.key());
        if !m.edges.is_empty() {
            let edges: Vec<String> = m.edges.iter().map(i32::to_string).collect();
            let _ = writeln!(out, "edges {}", edges.join(" "));
        }
        for w in &m.weights {
            let bits: Vec<String> = w.iter().map(|x| format!("{:016x}", x.to_bits())).collect();
            let _ = writeln!(out, "w {}", bits.join(" "));
        }
    }
    out
}

/// The models of a table text.
///
/// # Errors
/// A malformed line, a duplicate model, or a weight vector whose length is not the model's
/// column count.
pub fn parse_table(text: &str) -> Result<Vec<Model>, String> {
    let mut out: Vec<Model> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |what: &str| format!("P6-G weights line {}: {what}", n + 1);
        let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
        match tag {
            "model" => {
                let spec = ModelSpec::parse_key(rest).ok_or_else(|| bad("not a model key"))?;
                if out.iter().any(|m| m.spec == spec) {
                    return Err(bad("duplicate model"));
                }
                out.push(Model {
                    spec,
                    columns: spec.columns(),
                    edges: Vec::new(),
                    weights: Vec::new(),
                });
            }
            "edges" => {
                let m = out.last_mut().ok_or_else(|| bad("edges before a model"))?;
                m.edges = rest
                    .split_whitespace()
                    .map(|t| t.parse::<i32>().map_err(|_| bad("not a turn")))
                    .collect::<Result<_, _>>()?;
                if !m.edges.is_sorted() || !m.spec.phase {
                    return Err(bad("edges must ascend and belong to a phase model"));
                }
            }
            "w" => {
                let m = out
                    .last_mut()
                    .ok_or_else(|| bad("weights before a model"))?;
                let w: Vec<f64> = rest
                    .split_whitespace()
                    .map(|t| {
                        u64::from_str_radix(t, 16)
                            .map(f64::from_bits)
                            .map_err(|_| bad("not a weight"))
                    })
                    .collect::<Result<_, _>>()?;
                if w.len() != m.columns.len() || w.iter().any(|x| !x.is_finite()) {
                    return Err(bad("weights do not match the model's columns"));
                }
                m.weights.push(w);
            }
            _ => return Err(bad("unknown line")),
        }
    }
    for m in &out {
        if m.weights.len() != m.edges.len() + 1 {
            return Err(format!(
                "P6-G weights: model {} needs one weight line per bin",
                m.spec.key()
            ));
        }
    }
    Ok(out)
}

/// The embedded table (`weights.txt`).
static TABLE: LazyLock<Result<Vec<Model>, String>> =
    LazyLock::new(|| parse_table(include_str!("weights.txt")));

/// The embedded model of `spec`.
///
/// # Errors
/// The embedded table is malformed, or it has no model for `spec`.
pub fn embedded(spec: &ModelSpec) -> Result<&'static Model, String> {
    let table = TABLE.as_ref().map_err(Clone::clone)?;
    table
        .iter()
        .find(|m| m.spec == *spec)
        .ok_or_else(|| format!("P6-G has no fitted model {} in this build", spec.key()))
}

/// Every embedded model, in table order.
///
/// # Errors
/// The embedded table is malformed.
pub fn embedded_models() -> Result<&'static [Model], String> {
    TABLE.as_ref().map(Vec::as_slice).map_err(Clone::clone)
}
