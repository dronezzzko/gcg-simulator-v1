//! The mechanism flags of the strengthened search (docs/dev/PLAN.md §12.2, §12.3): one field per
//! planned mechanism, grouped by work package, so that an ablation row is a flag flip (§12.4).
//!
//! Every field is "off" in the null configuration, which replays the reference search bit for
//! bit (`tests/null_config.rs`). [`MechanismFlags`] has no `Default`: a preset or a configuration
//! file names every field (W-136).
//!
//! ```compile_fail,E0277
//! let _flags = gcg_search::MechanismFlags { ..Default::default() };
//! ```
//!
//! The fields are also reachable by name ([`MechanismFlags::FIELDS`], [`MechanismFlags::get`],
//! [`MechanismFlags::set`], [`MechanismFlags::set_text`], [`MechanismFlags::from_values`]), so
//! `gcg-eval` configuration files and `search:<preset>:field=value` overrides cover every flag a
//! later package implements without per-field code.

/// `opponent_list` (S5, W-134; PLAN A30): where the opponent's unseen cards are drawn from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OpponentList {
    /// The opponent's exact decklist, as the reference (`docs/ASSUMPTIONS.md:62`).
    Known,
    /// A declared prior over candidate lists (opt-in, W-134).
    Archetype,
}

impl OpponentList {
    /// The spelling in configuration files and overrides.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Archetype => "archetype",
        }
    }

    /// The value spelled `text`.
    #[must_use]
    pub fn from_value(text: &str) -> Option<Self> {
        [Self::Known, Self::Archetype]
            .into_iter()
            .find(|v| v.as_str() == text)
    }
}

/// The value of one mechanism field, as a configuration file or an override carries it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FlagValue {
    /// An on/off mechanism.
    Bool(bool),
    /// A mechanism with a count parameter; `None` is off.
    Count(Option<u32>),
    /// A fraction in `[0, 1]`; `0.0` is off.
    Share(f64),
    /// The opponent-list mode; `known` is off.
    List(OpponentList),
}

impl FlagValue {
    /// Whether the mechanism is on (the sorted list of enabled flags, PLAN §12.7).
    #[must_use]
    pub fn is_on(self) -> bool {
        match self {
            Self::Bool(b) => b,
            Self::Count(c) => c.is_some(),
            Self::Share(s) => s != 0.0,
            Self::List(l) => l != OpponentList::Known,
        }
    }
}

/// A field type of [`MechanismFlags`].
trait FlagField: Sized + Copy {
    const KIND: &'static str;
    fn to_value(self) -> FlagValue;
    fn from_value(v: FlagValue) -> Option<Self>;
    fn parse(text: &str) -> Result<Self, String>;
}

impl FlagField for bool {
    const KIND: &'static str = "true or false";

    fn to_value(self) -> FlagValue {
        FlagValue::Bool(self)
    }

    fn from_value(v: FlagValue) -> Option<Self> {
        match v {
            FlagValue::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(format!("expected true or false, got {text:?}")),
        }
    }
}

impl FlagField for Option<u32> {
    const KIND: &'static str = "a count or none";

    fn to_value(self) -> FlagValue {
        FlagValue::Count(self)
    }

    fn from_value(v: FlagValue) -> Option<Self> {
        match v {
            FlagValue::Count(c) => Some(c),
            _ => None,
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
        if text == "none" {
            return Ok(None);
        }
        text.parse::<u32>().map(Some).map_err(|_| {
            format!(
                "expected a count in [0, {}] or none, got {text:?}",
                u32::MAX
            )
        })
    }
}

impl FlagField for f64 {
    const KIND: &'static str = "a number in [0, 1]";

    fn to_value(self) -> FlagValue {
        FlagValue::Share(self)
    }

    fn from_value(v: FlagValue) -> Option<Self> {
        let x = match v {
            FlagValue::Share(x) => x,
            FlagValue::Count(Some(n)) => f64::from(n),
            _ => return None,
        };
        (0.0..=1.0).contains(&x).then_some(x)
    }

    fn parse(text: &str) -> Result<Self, String> {
        text.parse::<f64>()
            .ok()
            .and_then(|x| Self::from_value(FlagValue::Share(x)))
            .ok_or_else(|| format!("expected a number in [0, 1], got {text:?}"))
    }
}

impl FlagField for OpponentList {
    const KIND: &'static str = "known or archetype";

    fn to_value(self) -> FlagValue {
        FlagValue::List(self)
    }

    fn from_value(v: FlagValue) -> Option<Self> {
        match v {
            FlagValue::List(l) => Some(l),
            _ => None,
        }
    }

    fn parse(text: &str) -> Result<Self, String> {
        Self::from_value(text).ok_or_else(|| format!("expected known or archetype, got {text:?}"))
    }
}

macro_rules! mechanism_flags {
    ($( $(#[$doc:meta])* [$pkg:literal] $name:ident : $ty:ty, )*) => {
        /// The mechanism flags of [`crate::StrengthenedConfig::mechanisms`] (PLAN §12.2,
        /// §12.3), one field per planned mechanism; each field's documentation names its
        /// package and its WEAKNESSES items. No `Default` (W-136).
        #[derive(Clone, Copy, PartialEq, Debug)]
        pub struct MechanismFlags {
            $( $(#[$doc])* pub $name: $ty, )*
        }

        impl MechanismFlags {
            /// Every field as `(name, package)`, in declaration order.
            pub const FIELDS: &'static [(&'static str, &'static str)] =
                &[$( (stringify!($name), $pkg), )*];

            /// The value of the field `name`, or `None` for no such field.
            #[must_use]
            pub fn get(&self, name: &str) -> Option<FlagValue> {
                match name {
                    $( stringify!($name) => Some(FlagField::to_value(self.$name)), )*
                    _ => None,
                }
            }

            /// Sets the field `name`; `None` for no such field.
            ///
            /// # Errors
            /// A value of the wrong kind (inside the `Some`).
            pub fn set(&mut self, name: &str, v: FlagValue) -> Option<Result<(), String>> {
                match name {
                    $( stringify!($name) => Some(
                        <$ty as FlagField>::from_value(v)
                            .map(|x| self.$name = x)
                            .ok_or_else(|| format!(
                                "{} must be {}", stringify!($name), <$ty as FlagField>::KIND
                            )),
                    ), )*
                    _ => None,
                }
            }

            /// Sets the field `name` from its text (`true`/`false`/`1`/`0`, a count or `none`,
            /// a number, `known`/`archetype`); `None` for no such field.
            ///
            /// # Errors
            /// A text that is not a value of the field's kind (inside the `Some`).
            pub fn set_text(&mut self, name: &str, text: &str) -> Option<Result<(), String>> {
                match name {
                    $( stringify!($name) => Some(
                        <$ty as FlagField>::parse(text)
                            .map(|x| self.$name = x)
                            .map_err(|e| format!("{}: {e}", stringify!($name))),
                    ), )*
                    _ => None,
                }
            }

            /// Flags built from `value(name)` for every field, so that a configuration file
            /// names every mechanism (W-136).
            ///
            /// # Errors
            /// The first missing field or value of the wrong kind.
            pub fn from_values(
                mut value: impl FnMut(&'static str) -> Option<FlagValue>,
            ) -> Result<Self, String> {
                Ok(Self {
                    $( $name: {
                        let v = value(stringify!($name)).ok_or_else(|| {
                            format!("missing mechanism field {:?}", stringify!($name))
                        })?;
                        <$ty as FlagField>::from_value(v).ok_or_else(|| {
                            format!("{} must be {}", stringify!($name), <$ty as FlagField>::KIND)
                        })?
                    }, )*
                })
            }
        }
    };
}

mechanism_flags! {
    // P6-A horizon and leaf (S2; W-01, W-02, W-15). `horizon_turns` is a configuration field.
    /// Extend the horizon by one turn boundary when the leaf is unsettled (W-01).
    ["P6-A"] adaptive_horizon: bool,
    /// Leaf constants `floor` and `decay = floor / (max_horizon + max_extension + 1)` with the
    /// constructor check, in place of `LEAF_FLOOR` and `WIN_DECAY` (W-02).
    ["P6-A"] generalized_leaf: bool,
    /// A capped rollout finishes its step with the deterministic policy and is flagged (W-15).
    ["P6-A"] capped_rollout_continuation: bool,

    // P6-B tactical solver (S3; W-03, W-16, W-17, W-32, W-38).
    /// Bounded exact attack-step solver on the determinised board.
    ["P6-B"] tactical_solver: bool,
    /// Lethal found by the solver in the first K worlds is played or gets a dominant prior.
    ["P6-B"] lethal_root: bool,
    /// The time-shaped loss value at a leaf where the opponent has lethal.
    ["P6-B"] threat_leaf: bool,
    /// Critical positions get up to two extra budgets.
    ["P6-B"] critical_extension: bool,
    /// Engine-correct blockers (`procedures::eligible_blockers`) in the solver and features.
    ["P6-B"] effective_blockers: bool,
    /// `can_kill` and race features from the solver.
    ["P6-B"] solver_can_kill: bool,

    // P6-C hidden information (S4, S5; W-05, W-07, W-08, W-134, W-139).
    /// Opponent nodes split into at most 4 buckets; a bucket drives selection once it has `m`
    /// visits (`Some(m)`).
    ["P6-C"] opponent_buckets: Option<u32>,
    /// `Some(M)` candidate worlds per needed world, weighted by the opponent's recent public
    /// decisions (with `belief_last_n`).
    ["P6-C"] belief_worlds: Option<u32>,
    /// The number N of the opponent's last public decisions `belief_worlds` weighs.
    ["P6-C"] belief_last_n: Option<u32>,
    /// Exact pools keep card conservation in determinisation (W-08).
    ["P6-C"] exact_pools: bool,
    /// The share of worlds that re-determinise the searcher's own hidden cards from the
    /// opponent's view (W-139); `0.0` is off.
    ["P6-C"] opponent_view_share: f64,
    /// The opponent-list mode (W-134).
    ["P6-C"] opponent_list: OpponentList,

    // P6-D tree and budget (S1, S7, S8; W-04, W-06, W-11, W-18, W-138, W-140).
    /// `Some(K)` deterministic root trees per decision (S1).
    ["P6-D"] root_trees: Option<u32>,
    /// A per-iteration visited set backs a node up once per iteration (W-06).
    ["P6-D"] visited_set_backup: bool,
    /// In-tree moves pass the root filters.
    ["P6-D"] in_tree_filters: bool,
    /// Transpositions within a decision, keyed by `stable_hash` (S7).
    ["P6-D"] transpositions: bool,
    /// The subtree of the observed transition is kept for the next own decision (S7, W-11).
    ["P6-D"] subtree_reuse: bool,
    /// Joint selection of small multi-card choices (S8, W-138).
    ["P6-D"] macro_multi_pick: bool,
    /// In-tree priors average `Some(K)` worlds (W-140).
    ["P6-D"] prior_worlds: Option<u32>,

    // P6-E root filters and targets (S6; W-09, W-10, W-12, W-13, W-14, W-141).
    /// EX-payment variants kept when the follow-up line uses the difference (W-09).
    ["P6-E"] ex_variants: bool,
    /// Uid-independent typed fingerprints (W-10).
    ["P6-E"] typed_fingerprint: bool,
    /// Pruning judged by majority over `Some(K)` common-random-number worlds (W-12).
    ["P6-E"] prune_worlds: Option<u32>,
    /// Branch over every SELECT up to a node cap (W-13).
    ["P6-E"] branch_all_selects: bool,
    /// The REST-target prune judged over the K worlds (W-14).
    ["P6-E"] rest_prune_worlds: bool,
    /// Target rollouts for close target choices (W-141).
    ["P6-E"] target_rollouts: bool,
    /// The no-effect test compares against an idle line that cannot replay the move.
    ["P6-E"] real_idle_line: bool,

    // P6-F rollout policy (S9; W-23 … W-31, W-163).
    /// The fitted softmax rollout policy.
    ["P6-F"] calibrated_policy: bool,
    /// Modal choices scored by simulating their first step.
    ["P6-F"] modal_lookahead: bool,
    /// YES_NO by one-ply look-ahead.
    ["P6-F"] yes_no_lookahead: bool,
    /// End-phase action-step plays.
    ["P6-F"] end_phase_actions: bool,
    /// Burst by one-ply look-ahead with a decline list (W-163 tripwire).
    ["P6-F"] burst_decline_list: bool,
    /// An ORDER_TRIGGER scorer.
    ["P6-F"] order_trigger_scorer: bool,
    /// Exhaustive polarity.
    ["P6-F"] exhaustive_polarity: bool,
    /// Exact fights on clones.
    ["P6-F"] exact_fight: bool,
    /// A separately fitted opponent policy.
    ["P6-F"] opponent_knobs: bool,
    /// Kill-aware targeting (re-measured; dropped if null).
    ["P6-F"] kill_aware_targeting: bool,

    // P6-G evaluation (S10; W-34 … W-37, W-39, W-142).
    /// Weights refitted per preset at the final leaf points (W-34, W-36).
    ["P6-G"] refit_weights: bool,
    /// A phase-conditioned model (W-35).
    ["P6-G"] phase_weights: bool,
    /// Keyword features (W-37).
    ["P6-G"] keyword_features: bool,
    /// Ability-count features (W-37).
    ["P6-G"] ability_count_features: bool,
    /// Hand-slot features.
    ["P6-G"] hand_slots: bool,
    /// Burst probability per Shield (W-37).
    ["P6-G"] burst_shield_probability: bool,
    /// The fit without the tempo feature.
    ["P6-G"] no_tempo_feature: bool,
    /// Positions recorded at consistent determinised leaf points (W-39).
    ["P6-G"] determinized_fit: bool,
    /// Burst density recomputed in the leaf world (W-142).
    ["P6-G"] leaf_density: bool,

    // P6-H pre-game (S11; W-44, W-45).
    /// Capped play/draw simulations scored by the evaluation instead of 0.5 (W-44).
    ["P6-H"] exclude_capped_play_draw: bool,
    /// Mulligan by paired simulation of the opening turns (W-45).
    ["P6-H"] paired_mulligan: bool,

    // P6-O design decisions (W-137, W-143/W-110).
    /// The search's own `exp`, `ln`, squares, logistic and rollout Gumbel draws use
    /// `gcg_core::portable` (the pure-Rust `libm` crate) and `x * x`, so its results do not
    /// depend on the platform C library (W-137).
    ["P6-O"] portable_math: bool,
    /// A simulated line (tree descent or rollout in a determinised world) that ends in an
    /// `EngineError` is scored as a loss for the searching player and counted, instead of
    /// failing the real game (W-143, W-110).
    ["P6-O"] engine_error_leaf: bool,

    // GAP-A1 residual mechanisms (docs/dev/eval/weakness-gaps.md; W-14, W-25).
    /// `Some(R)`: at an effect-target root decision every iteration plays `R` rollouts from the
    /// end of its walk and backs up their mean, and the decision's budget is divided by `R`, so
    /// the rollouts are counted in iterations (W-14 plan item 2: several rollouts per world
    /// instead of more worlds).
    ["GAP-A1"] target_multi_rollouts: Option<u32>,
    /// At an in-tree MAIN decision of the turn player (either side, below the root), the first
    /// move of a lethal line that P6-B's tactical solver proves on the iteration's world is
    /// selected, so the tree's players are as competent at lethal as `tactical_solver` makes the
    /// rollouts' (W-25: below the root the opponent's replies otherwise follow the shared
    /// policy's priors). The solver's applies share P6-B's per-decision cap.
    ["GAP-A1"] tree_lethal_select: bool,
}

impl MechanismFlags {
    /// Every Phase 6 mechanism off, every field spelled out: the null configuration's flags,
    /// with which the strengthened search replays the reference (`tests/null_config.rs`).
    pub const NULL: Self = Self {
        adaptive_horizon: false,
        generalized_leaf: false,
        capped_rollout_continuation: false,
        tactical_solver: false,
        lethal_root: false,
        threat_leaf: false,
        critical_extension: false,
        effective_blockers: false,
        solver_can_kill: false,
        opponent_buckets: None,
        belief_worlds: None,
        belief_last_n: None,
        exact_pools: false,
        opponent_view_share: 0.0,
        opponent_list: OpponentList::Known,
        root_trees: None,
        visited_set_backup: false,
        in_tree_filters: false,
        transpositions: false,
        subtree_reuse: false,
        macro_multi_pick: false,
        prior_worlds: None,
        ex_variants: false,
        typed_fingerprint: false,
        prune_worlds: None,
        branch_all_selects: false,
        rest_prune_worlds: false,
        target_rollouts: false,
        real_idle_line: false,
        calibrated_policy: false,
        modal_lookahead: false,
        yes_no_lookahead: false,
        end_phase_actions: false,
        burst_decline_list: false,
        order_trigger_scorer: false,
        exhaustive_polarity: false,
        exact_fight: false,
        opponent_knobs: false,
        kill_aware_targeting: false,
        refit_weights: false,
        phase_weights: false,
        keyword_features: false,
        ability_count_features: false,
        hand_slots: false,
        burst_shield_probability: false,
        no_tempo_feature: false,
        determinized_fit: false,
        leaf_density: false,
        exclude_capped_play_draw: false,
        paired_mulligan: false,
        portable_math: false,
        engine_error_leaf: false,
        target_multi_rollouts: None,
        tree_lethal_select: false,
    };

    /// The enabled mechanisms, sorted by name (results schema 1.2.0 `ai.search.mechanisms`,
    /// PLAN §12.7).
    #[must_use]
    pub fn enabled(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Self::FIELDS
            .iter()
            .filter(|(name, _)| self.get(name).is_some_and(FlagValue::is_on))
            .map(|&(name, _)| name)
            .collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_parse_by_kind() {
        assert_eq!(bool::parse("true"), Ok(true));
        assert_eq!(bool::parse("0"), Ok(false));
        assert!(bool::parse("yes").is_err());
        assert_eq!(<Option<u32>>::parse("none"), Ok(None));
        assert_eq!(<Option<u32>>::parse("8"), Ok(Some(8)));
        assert!(<Option<u32>>::parse("-1").is_err());
        assert_eq!(f64::parse("0.25"), Ok(0.25));
        assert!(f64::parse("1.5").is_err());
        assert!(f64::parse("nan").is_err());
        assert_eq!(f64::from_value(FlagValue::Count(Some(1))), Some(1.0));
        assert_eq!(
            OpponentList::parse("archetype"),
            Ok(OpponentList::Archetype)
        );
        assert!(OpponentList::parse("Known").is_err());
        assert!(!FlagValue::Share(0.0).is_on() && FlagValue::Share(0.5).is_on());
        assert!(!FlagValue::Count(None).is_on() && FlagValue::Count(Some(0)).is_on());
        assert!(!FlagValue::List(OpponentList::Known).is_on());
    }
}
