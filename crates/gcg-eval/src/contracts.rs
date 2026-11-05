//! Contract types of `gcg-eval` (docs/port/PLAN.md A14, A24, §10.4 F, §12.5-§12.6), owned by
//! the lead (§10.1 S).
//!
//! The values the plan fixes for the evaluation harness: the seed roles and their disjoint
//! master ranges (A14), the play/draw protocol (A24), the deck and pair selections, and the
//! mirrored game specs of `ai/selfplay.py` and `ai/compare.py`. The specs live here rather than
//! in `gcg-runner` (PLAN §10.3.6 lists them there): `gcg-eval` is their only consumer, F's brief
//! delivers them, and only this crate can name both configuration types.
//!
//! Workstream F implements the functions frozen in docs/port/contracts.md (gcg-eval section);
//! only the lead edits this file (§10.6 change protocol). Python sources are cited relative to
//! `src/gcg_sim/`; Inv 08 is docs/port/inventory/08-ai-policy-eval.md.

use gcg_ai::Weights;
use gcg_ai_reference::ReferenceConfig;
use gcg_data::DeckList;
use gcg_search::StrengthenedConfig;

/// `MIRROR_BLOCK` (`ai/selfplay.py:23`): games per mirrored block.
pub const MIRROR_BLOCK: u32 = 4;
/// `DECISION_LIMIT` (`ai/selfplay.py:22`): `run_game` stops after this many decisions.
pub const DECISION_LIMIT: u32 = 20_000;

/// `gcg-eval --role` (PLAN A14): each role draws master seeds from its own inclusive range, and
/// `gcg-eval` rejects a master outside the range of the run's role. Every master used is
/// recorded in `docs/port/eval/seed-ledger.json`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SeedRole {
    Tuning,
    Dev,
    Confirm,
    Eval,
    Review,
}

impl SeedRole {
    pub const ALL: [Self; 5] = [
        Self::Tuning,
        Self::Dev,
        Self::Confirm,
        Self::Eval,
        Self::Review,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tuning => "tuning",
            Self::Dev => "dev",
            Self::Confirm => "confirm",
            Self::Eval => "eval",
            Self::Review => "review",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// The inclusive master-seed range `(first, last)` (PLAN A14).
    #[must_use]
    pub const fn bounds(self) -> (u64, u64) {
        match self {
            Self::Tuning => (10_000_000, 19_999_999),
            Self::Dev => (20_000_000, 29_999_999),
            Self::Confirm => (30_000_000, 30_999_999),
            Self::Eval => (90_000_000, 90_999_999),
            Self::Review => (91_000_000, 91_999_999),
        }
    }

    #[must_use]
    pub const fn contains(self, master: u64) -> bool {
        let (first, last) = self.bounds();
        first <= master && master <= last
    }
}

/// `gcg-eval head-to-head --choose-first` (PLAN A24, X13): with `seat0` (the default) the
/// harness answers CHOOSE_FIRST with `go_first(0)`, so each agent is on the play in exactly
/// two games of every mirrored block; `agents` lets the agents choose
/// (`engine/game.py:153`), the W-44 diagnostic.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ChooseFirst {
    Seat0,
    Agents,
}

impl ChooseFirst {
    pub const ALL: [Self; 2] = [Self::Seat0, Self::Agents];
    pub const DEFAULT: Self = Self::Seat0;

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seat0 => "seat0",
            Self::Agents => "agents",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

/// `gcg-eval --decks` (PLAN §10.4 F, §12.5): the example decks, the AI test decks
/// (`tests/ai/decks.py`), the tuning decks (`ai/tuning.py:89-93`), the five evaluation decks
/// (`examples/decks/eval/`), or the panel (examples plus evaluation decks, 28 pairs).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DeckSet {
    Examples,
    Tests,
    Tuning,
    Eval,
    Panel,
}

impl DeckSet {
    pub const ALL: [Self; 5] = [
        Self::Examples,
        Self::Tests,
        Self::Tuning,
        Self::Eval,
        Self::Panel,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Examples => "examples",
            Self::Tests => "tests",
            Self::Tuning => "tuning",
            Self::Eval => "eval",
            Self::Panel => "panel",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.as_str() == value)
    }
}

/// `gcg-eval --pairs` (PLAN §10.4 F, §12.5, X16): every pair, the 15 tuning pairs, or the 13
/// held-out pairs (every pair with a held-out deck). Held-out pairs are refused under
/// `--role tuning|dev|confirm`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PairSet {
    All,
    Tuning,
    HeldOut,
}

impl PairSet {
    pub const ALL: [Self; 3] = [Self::All, Self::Tuning, Self::HeldOut];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Tuning => "tuning",
            Self::HeldOut => "held-out",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == value)
    }
}

/// A parsed agent spec (PLAN §10.4 F): `random`, `greedy`, `ref:<preset>`,
/// `ref:baseline-<preset>`, `search:<preset>` or `search:FILE.json`, each with optional
/// `:field=value,...` overrides. The `ref:` forms are `ai/compare.py:79-113`'s grammar
/// (`parse_agent`): a baseline preset plays with `BASELINE_WEIGHTS` (`:89-94`), `weights=<name>`
/// picks `default`, `hand`, `baseline` or `roles` (`:52-57`, `:103-106`), and an override
/// keeps the configuration's `name`, so variants keep the per-decision seeds. Python's
/// `selfplay.AgentSpec(kind, preset, config, weights)` (`ai/selfplay.py:26-34`).
#[derive(Clone, PartialEq, Debug)]
pub enum AgentDesc {
    /// `make_agent("random", ...)` (`ai/agents.py:120-121`).
    Random,
    /// `make_agent("greedy", weights=...)` (`ai/agents.py:122-123`).
    Greedy { weights: Weights },
    /// `make_agent("mcts", config=..., weights=...)` (`ai/agents.py:124-126`).
    Reference {
        config: ReferenceConfig,
        weights: Weights,
    },
    /// The strengthened search; its weights are part of the configuration.
    Search { config: StrengthenedConfig },
}

/// `GameSpec(index, game_seed, agent_seeds, agents, decks, challenger)`
/// (`ai/selfplay.py:37-47`): `agents[i]` plays `decks[i]` in seat `i`; `challenger` is the seat
/// of the agent under test. Built by `mirrored_specs` (`:88-110`) and `comparison_specs`
/// (`ai/compare.py:127-143`).
#[derive(Clone, PartialEq, Debug)]
pub struct MirroredSpec {
    pub index: u32,
    /// `derive_seed(master, "block", block, "decks", swap_decks)`.
    pub game_seed: u64,
    /// `derive_seed(master, "agent", i, seat)` for seats 0 and 1.
    pub agent_seeds: [u64; 2],
    pub agents: [AgentDesc; 2],
    pub decks: [DeckList; 2],
    pub challenger: u8,
}

/// `GameOutcome(index, challenger, winner, turns, first_player, decisions)`
/// (`ai/selfplay.py:50-57`); `winner` is -1 for a draw or an unfinished game (`:84`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GameOutcome {
    pub index: u32,
    pub challenger: u8,
    pub winner: i8,
    pub turns: i32,
    pub first_player: u8,
    pub decisions: u32,
}

/// The `ValueError`s of `comparison_specs` (`ai/compare.py:131-135`); `Display` (F) is
/// `"need at least two decks"` and `f"games must be a positive multiple of {multiple}"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SpecError {
    TooFewDecks,
    /// `multiple = MIRROR_BLOCK * len(pairs)`.
    GamesNotMultiple {
        multiple: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PLAN A14: the role ranges are disjoint, so a master seed names its role.
    #[test]
    fn seed_role_ranges_are_disjoint() {
        for a in SeedRole::ALL {
            let (first, last) = a.bounds();
            assert!(first <= last, "{a:?}");
            assert!(a.contains(first) && a.contains(last), "{a:?}");
            assert!(!a.contains(first - 1) && !a.contains(last + 1), "{a:?}");
            for b in SeedRole::ALL {
                if a != b {
                    let (bf, bl) = b.bounds();
                    assert!(last < bf || bl < first, "{a:?} overlaps {b:?}");
                }
            }
        }
        assert_eq!(
            SeedRole::ALL.map(|r| SeedRole::from_value(r.as_str())),
            SeedRole::ALL.map(Some)
        );
    }

    #[test]
    fn selection_values_round_trip() {
        assert_eq!(ChooseFirst::DEFAULT.as_str(), "seat0");
        assert_eq!(ChooseFirst::from_value("agents"), Some(ChooseFirst::Agents));
        assert_eq!(
            DeckSet::ALL.map(DeckSet::as_str),
            ["examples", "tests", "tuning", "eval", "panel"]
        );
        assert_eq!(PairSet::from_value("held-out"), Some(PairSet::HeldOut));
        assert_eq!(PairSet::from_value("held_out"), None);
        assert_eq!((MIRROR_BLOCK, DECISION_LIMIT), (4, 20_000));
    }
}
