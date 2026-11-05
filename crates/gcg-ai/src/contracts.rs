//! Contract types of `gcg-ai` (docs/dev/PLAN.md §6.1, §6.3, §6.6, §10.3.5), owned by the lead
//! (§10.1 S).
//!
//! Workstreams D1 (policy, values, targeting, evaluation, playouts, greedy, random) and D2
//! (agent core, decision log, move keys, pruning, merging, fingerprints, the no-effect check,
//! pre-game choices) implement the functions frozen in docs/dev/contracts.md (gcg-ai section)
//! with real bodies; only the lead edits this file (§10.6 change protocol). Fields marked
//! `pub(crate)` are the lead's starting layout of an opaque type: its named owner may change
//! them in its landing diff, because no other crate can observe them.
//!
//! The weight tables are data and land here, with their bit patterns checked against the live
//! Python objects (`ai/evaluation.py:82-129`), so that no preset can be paired with a wrong
//! copy of them (PLAN §6.4).
//!
//! Python sources are cited relative to `src/gcg_sim/`.

use std::borrow::Cow;
use std::fmt;

use gcg_core::pool::{Cancel, TaskCx};
use gcg_engine::{Action, ActionKind, DecisionKind, EngineError, GameState, IllegalActionError};

// ---------------------------------------------------------------------------------------------
// The player interface (ai/base.py:22-31, runner/agents.py:39-46; PLAN §6.1)

/// A player (`Agent` protocol, `ai/base.py:22-31`; the runner's `GameAgent`,
/// `runner/agents.py:39-46`). Agents are rebuilt for every game, so the decision counter of
/// [`AgentCore`] restarts at 1 (`runner/game.py:62-76`). The runner, `gcg-eval`, `gcg-diff` and
/// the scenario tests hold one `Box<dyn Agent>` per seat; the reference search and the
/// strengthened search sit side by side behind this one trait.
pub trait Agent: Send {
    /// Seed-path material (`derive_seed(seed, name, "decision", k)`, `ai/base.py:65`) and
    /// `GameRecord.agents` (`runner/game.py:147`): `random`, `greedy`, `mcts-<config name>`
    /// (`ai/agents.py:26`, `:45`, `:81`), `search-<preset>` (PLAN A9), `test-random`,
    /// `test-illegal` (`tests/runner/random_agent.py:19`, `:54`).
    fn name(&self) -> &str;

    /// One of `st`'s pending options for `player` (`st.pending.player == player`). `st` is the
    /// true state and read-only (W-135): the agent plays on its own clones. Calls to
    /// `gcg_engine::view::derived(st)` fill `st`'s view cache, as Python's calls do (PLAN
    /// §4.6, D10).
    fn choose(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
    ) -> Result<Action, AgentError>;

    /// [`Agent::choose`] that also reports what the decision computed (the trace agent block,
    /// PLAN §8.2, §8.7 level 1) and passes `tracer` to the search it runs (levels 2-3).
    /// `gcg-diff` replays `search` and `baselines` traces through it.
    ///
    /// The action, the draws, the decision counter and the log entry must equal `choose`'s.
    /// The default, for agents without an [`AgentCore`] (the runner's test agents), is
    /// `choose` with no trace. `RandomAgent` and `GreedyAgent` (D1), and the reference and
    /// strengthened agents (D2), override it and return `Some`: each implements both methods
    /// over one generic function that takes the tracer as `&mut T`, so `choose` keeps
    /// [`NoTrace`] monomorphised. A `None` on a decision whose trace has an agent block is a
    /// divergence (docs/dev/contracts.md, gcg-diff section).
    ///
    /// # Errors
    /// As `choose`.
    fn choose_traced(
        &mut self,
        st: &GameState,
        player: u8,
        cx: &mut AgentCx<'_>,
        tracer: &mut dyn SearchTracer,
    ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
        let _ = tracer;
        self.choose(st, player, cx).map(|action| (action, None))
    }

    /// Every entry logged so far, oldest first (`decision_log()`, `ai/base.py:57-58`); empty
    /// unless the agent was built with logging on. The runner slices the new entries per seat
    /// after each decision (`runner/game.py:94-97`).
    fn decision_log(&self) -> &[LogEntry];

    /// The search effort this agent spent since it was built (PLAN §6.6, §12.7; open item O9):
    /// `Some` for the strengthened search's agents, which count every decision and sum the
    /// [`SearchStats`] it reported. `None`, the default, for agents that report nothing beyond
    /// their decisions: the reference search, greedy, random and the runner's test agents. The
    /// runner reads it once per game, after the last decision (agents are rebuilt for every
    /// game), for the per-seat `ai.effort` of results 1.2.0. Reading it never changes a
    /// decision.
    fn effort(&self) -> Option<AgentEffort> {
        None
    }
}

/// What an agent needs besides the state (PLAN §6.1, §7.1): the worker's scratch buffers, the
/// fork-join handle of the running pool task, and the SIGINT flag, which searches read
/// between iterations and the runner between decisions (PLAN §7.5). The runner checks the
/// pool's `lowest_failed` itself (W-149).
pub struct AgentCx<'a> {
    pub scratch: &'a mut Scratch,
    pub par: &'a TaskCx<'a, Scratch>,
    pub cancel: &'a Cancel,
}

/// Per-worker buffers that agents reuse across decisions and games (PLAN §7.1 "Scratch"):
/// the pool gives each worker one and each nested fork-join job one from a free list, so a
/// warm search allocates nothing. Aligned to the 128-byte cache line of the M2 Pro
/// (`hw.cachelinesize`), so two workers never share a written line.
///
/// Opaque; the starting layout belongs to D2 (`scratch.rs`).
#[repr(align(128))]
#[derive(Default)]
#[allow(
    dead_code,
    reason = "filled and read by workstream D2's search (docs/dev/contracts.md)"
)]
pub struct Scratch {
    /// Recycled states for `GameState::clone_from_state` and `determinize_into` (PLAN §4.5).
    pub(crate) states: Vec<GameState>,
    /// Recycled float buffers (per-visit differences, prior weights, policy scores).
    pub(crate) floats: Vec<Vec<f64>>,
}

/// Why an agent could not choose. `py_class` names the Python exception class, which the
/// runner writes into its failure message `"... failed: {Class}: {message}"`
/// (`runner/game.py:132-136`); `Display` (D2) is Python's message where Python has one.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum AgentError {
    /// `ValueError(f"{self.name}: no decision pending for player {player}")`
    /// (`ai/base.py:62-63`).
    NoDecision { agent: Box<str>, player: u8 },
    /// Any other `ValueError` an agent or a factory raises, for example
    /// `"unknown AI preset 'x'; choose from ['fast', 'standard', 'strong']"`
    /// (`ai/agents.py:117-118`).
    Value(String),
    /// A failed `assert` inside an agent (`ai/base.py:83`, `ai/mcts.py:87-88`,
    /// `ai/policy.py:80`, `tests/runner/random_agent.py:25-26`).
    Assertion(String),
    /// An engine error raised inside the agent's own computation (`derived`, `determinize`,
    /// `apply(check=False)` on a clone).
    Engine(EngineError),
    /// `IllegalActionError` from an `apply(check=True)` inside the agent
    /// (`ai/setup.py:116`).
    Illegal(IllegalActionError),
    /// The SIGINT flag was seen between iterations or simulations (PLAN §7.5); Python's
    /// workers ignore SIGINT and are terminated instead (`runner/run.py:41-42`).
    Interrupted,
}

impl AgentError {
    /// The Python exception class this error stands for (`type(exc).__name__`).
    #[must_use]
    pub const fn py_class(&self) -> &'static str {
        match self {
            Self::NoDecision { .. } | Self::Value(_) => "ValueError",
            Self::Assertion(_) => "AssertionError",
            Self::Engine(e) => e.py_class(),
            Self::Illegal(_) => "IllegalActionError",
            Self::Interrupted => "KeyboardInterrupt",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Agent bookkeeping and the decision log (ai/base.py:34-122)

/// `LOG_ALTERNATIVES` (`ai/base.py:15`): a log entry keeps the top 5 alternatives.
pub const LOG_ALTERNATIVES: usize = 5;

/// `Alternative(action, value, visits)` (`ai/base.py:34-40`): one candidate move with its
/// statistics, value from the chooser's point of view. Sources: the MCTS mean;
/// the mulligan's `(keep, current, 1)` and `(redraw, 0.9 * average, samples)`; play/draw score
/// means with `games` visits; greedy `evaluate` with 1 visit; random `1.0 / n` with 0 visits.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Alternative {
    pub action: Action,
    pub value: f64,
    pub visits: u32,
}

/// `AgentCore` (`ai/base.py:43-76`): seed, decision counter and decision log shared by the
/// random, greedy, reference and strengthened agents. `begin` and `finish` (D2, `agent.rs`)
/// are frozen in docs/dev/contracts.md.
#[derive(Clone, Debug)]
pub struct AgentCore {
    pub(crate) name: Box<str>,
    /// The construction seed, as given (`ai/base.py:52`); `derive_seed` masks it to 64 bits.
    pub(crate) seed: u64,
    pub(crate) log_enabled: bool,
    pub(crate) entries: Vec<LogEntry>,
    /// Decisions begun so far; `begin` increments it before seeding (`ai/base.py:64-65`).
    pub(crate) decisions: u64,
}

impl AgentCore {
    /// `AgentCore(name, seed, log)` (`ai/base.py:50-55`): no decisions, an empty log.
    #[must_use]
    pub fn new(name: impl Into<Box<str>>, seed: u64, log: bool) -> Self {
        Self {
            name: name.into(),
            seed,
            log_enabled: log,
            entries: Vec::new(),
            decisions: 0,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn log_enabled(&self) -> bool {
        self.log_enabled
    }

    /// `self._decisions`.
    #[must_use]
    pub const fn decisions(&self) -> u64 {
        self.decisions
    }

    /// `decision_log()` (`ai/base.py:57-58`), without Python's copy.
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }
}

/// One decision-log entry: the dict of `log_entry` (`ai/base.py:79-100`), built by
/// `AgentCore::finish` before the action is applied. `alternatives` are already ranked
/// (stable sort by `(-visits, -value)`, top [`LOG_ALTERNATIVES`]) and their values rounded with
/// `round(value, 4)` (`gcg_core::pyfloat::round_f64`).
///
/// JSON (`LogEntry::to_json`, D2), keys in Python's insertion order: `turn`, `player`,
/// `decision` (kind value), `chosen` (`[kind, a, b, c]`), `label`, `alternatives` (each
/// `action`, `label`, `value`, `visits`). Every writer sorts the keys (`reports/writer.py:34`,
/// `:56`). The test agents' entries have no `label` keys and value `0.0`
/// (`tests/runner/random_agent.py:30-40`); `None` labels stand for those.
#[derive(Clone, PartialEq, Debug)]
pub struct LogEntry {
    pub turn: i32,
    pub player: u8,
    pub decision: DecisionKind,
    pub chosen: Action,
    /// `describe(st, dec, chosen, player)` (`ai/base.py:112-122`).
    pub label: Option<Box<str>>,
    pub alternatives: Vec<LogAlternative>,
}

/// One ranked alternative of a [`LogEntry`] (`ai/base.py:91-98`).
#[derive(Clone, PartialEq, Debug)]
pub struct LogAlternative {
    pub action: Action,
    pub label: Option<Box<str>>,
    /// `round(alt.value, 4)`.
    pub value: f64,
    pub visits: u32,
}

// ---------------------------------------------------------------------------------------------
// Policy knobs (ai/policy.py:50-69)

/// `PolicyKnobs(burst_yes_score=1.0, rule_polarity_table=False, zero_ap_face=False)`
/// (`ai/policy.py:50-57`): the policy settings a search configuration can change
/// (`SearchConfig.policy_*`; `knobs_for`, `:63-69`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PolicyKnobs {
    pub burst_yes_score: f64,
    pub rule_polarity_table: bool,
    pub zero_ap_face: bool,
}

/// `BASELINE_KNOBS = PolicyKnobs()` (`ai/policy.py:60`): greedy, `noop.without_effect`'s
/// default and the default arguments of `play_until_turn`/`play_to_end`.
pub const BASELINE_KNOBS: PolicyKnobs = PolicyKnobs {
    burst_yes_score: 1.0,
    rule_polarity_table: false,
    zero_ap_face: false,
};

impl Default for PolicyKnobs {
    /// Python's field defaults, which are [`BASELINE_KNOBS`].
    fn default() -> Self {
        BASELINE_KNOBS
    }
}

// ---------------------------------------------------------------------------------------------
// Evaluation weights (ai/evaluation.py:30-129)

/// `FEATURES` (`ai/evaluation.py:30-45`): the `base` feature set.
pub const FEATURES: [&str; 14] = [
    "tempo",
    "life",
    "danger",
    "can_kill",
    "ap",
    "hp",
    "linked",
    "ready",
    "hand_units",
    "hand_pilots",
    "hand_commands",
    "hand_bases",
    "level",
    "bursts",
];

/// `N_FEATURES = len(FEATURES)` (`ai/evaluation.py:46`).
pub const N_FEATURES: usize = FEATURES.len();

/// `ROLE_FEATURES` (`ai/evaluation.py:47-53`): `FEATURES` without `can_kill`, then the four
/// role terms.
pub const ROLE_FEATURES: [&str; 17] = [
    "tempo",
    "life",
    "danger",
    "ap",
    "hp",
    "linked",
    "ready",
    "hand_units",
    "hand_pilots",
    "hand_commands",
    "hand_bases",
    "level",
    "bursts",
    "can_kill_turn_player",
    "can_kill_other",
    "inv_race_turn_player",
    "inv_race_other",
];

/// A key of `FEATURE_SETS` (`ai/evaluation.py:54-56`), in dict order (which is also
/// `sorted(FEATURE_SETS)`, the order of the `--feature-set` choices, `ai/tuning.py`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum FeatureSet {
    Base,
    Roles,
}

impl FeatureSet {
    pub const ALL: [Self; 2] = [Self::Base, Self::Roles];

    /// The Python key.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Roles => "roles",
        }
    }

    #[must_use]
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == value)
    }

    /// `FEATURE_SETS[self]`.
    #[must_use]
    pub const fn names(self) -> &'static [&'static str] {
        match self {
            Self::Base => &FEATURES,
            Self::Roles => &ROLE_FEATURES,
        }
    }
}

/// `Weights(values, feature_set="base")` (`ai/evaluation.py:62-79`): one weight per feature
/// of the set. The `__post_init__` length check is [`Weights::new`]; an unknown set name cannot
/// be represented ([`FeatureSet::from_value`] returns `None`). Equality compares both fields,
/// as the frozen dataclass does.
#[derive(Clone, PartialEq, Debug)]
pub struct Weights {
    pub(crate) values: Cow<'static, [f64]>,
    pub(crate) feature_set: FeatureSet,
}

/// `ValueError(f"expected {len(names)} weights, got {len(self.values)}")`
/// (`ai/evaluation.py:75-76`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WeightsError {
    pub expected: usize,
    pub got: usize,
}

impl fmt::Display for WeightsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {} weights, got {}", self.expected, self.got)
    }
}

impl std::error::Error for WeightsError {}

impl Weights {
    /// `Weights(values, feature_set)` (`ai/evaluation.py:69-76`): fitted weights from
    /// `gcg-eval tune` and `search:FILE.json` agent specs.
    ///
    /// # Errors
    /// [`WeightsError`] when the length differs from the set's feature count.
    pub fn new(values: Vec<f64>, feature_set: FeatureSet) -> Result<Self, WeightsError> {
        let expected = feature_set.names().len();
        if values.len() != expected {
            return Err(WeightsError {
                expected,
                got: values.len(),
            });
        }
        Ok(Self {
            values: Cow::Owned(values),
            feature_set,
        })
    }

    /// A shipped table; the length is checked at compile time.
    const fn table(values: &'static [f64], feature_set: FeatureSet) -> Self {
        assert!(values.len() == feature_set.names().len());
        Self {
            values: Cow::Borrowed(values),
            feature_set,
        }
    }

    /// `self.values`, in feature order.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    #[must_use]
    pub const fn feature_set(&self) -> FeatureSet {
        self.feature_set
    }

    /// `as_dict()` (`ai/evaluation.py:78-79`): `(feature name, weight)` in feature order.
    pub fn as_pairs(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        self.feature_set
            .names()
            .iter()
            .copied()
            .zip(self.values.iter().copied())
    }
}

/// `HAND_WEIGHTS` (`ai/evaluation.py:82-84`), feature set `base`.
pub static HAND_WEIGHTS: Weights = Weights::table(
    &[
        0.0, 0.35, -0.30, 1.0, 0.10, 0.10, 0.20, 0.20, 0.30, 0.25, 0.25, 0.25, 0.15, 0.30,
    ],
    FeatureSet::Base,
);

/// `TUNED_WEIGHTS` (`ai/evaluation.py:85-103`), feature set `base`; `BASELINE_WEIGHTS`
/// (`ai/baseline.py:74`).
pub static TUNED_WEIGHTS: Weights = Weights::table(
    &[
        -0.2017, 0.4285, -0.4398, 0.5341, 0.1131, 0.1574, 0.2101, 0.532, 0.7157, 0.4885, 0.3727,
        0.5741, 0.1643, 0.5532,
    ],
    FeatureSet::Base,
);

/// `ROLE_WEIGHTS` (`ai/evaluation.py:104-128`), feature set `roles`.
pub static ROLE_WEIGHTS: Weights = Weights::table(
    &[
        -0.7451, 0.206, -0.459, 0.077, 0.231, 0.2152, 0.3452, 0.7646, 0.5453, 0.5425, 0.5805,
        0.3053, 0.8482, 1.2665, -0.418, 3.2487, -0.8534,
    ],
    FeatureSet::Roles,
);

/// `DEFAULT_WEIGHTS = ROLE_WEIGHTS` (`ai/evaluation.py:129`): the default of greedy and of the
/// MCTS agent (`ai/agents.py:43`, `:79`, `:119`).
pub static DEFAULT_WEIGHTS: &Weights = &ROLE_WEIGHTS;

/// `burst_density(st, observer)` (`ai/evaluation.py:132-140`): per owner (index 0, 1), the
/// fraction of 【Burst】 cards among the main-deck cards the observer cannot see.
pub type Density = [f64; 2];

// ---------------------------------------------------------------------------------------------
// Move keys and root-move flags (ai/actions.py, ai/noop.py; D2)

/// `Key = tuple[str, int, int, int]` (`ai/actions.py:32`), built by `action_key`
/// (`:57-69`): `(kind, a, b, variant)`. `a` and `b` are uids or, for cards the decider knows
/// in a by-definition zone, `1 << 24` plus the def_id (`_ident`, `:48-54`). `variant` is
/// `c // ALT_PLAY_BASE` (or -1) for paid kinds, -1 for SELECT, ATTACK and BLOCK, and `c`
/// otherwise. Keys are compared for equality only (dict and set membership); they are never
/// sorted, so there is no `Ord`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MoveKey {
    pub kind: ActionKind,
    pub a: i64,
    pub b: i64,
    pub variant: i32,
}

/// The keyword flags of `prune_dominated(..., strict_attacks=False, rest_targets=False)`
/// (`ai/actions.py:242-249`); `SearchConfig.strict_attack_prune` and
/// `prune_noop_rest_targets` (`ai/mcts.py:93-94`). Greedy uses the defaults
/// (`ai/agents.py:60`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PruneFlags {
    pub strict_attacks: bool,
    pub rest_targets: bool,
}

/// The keyword flags of `without_effect(..., branch_choices=False, resource_aware=False)`
/// (`ai/noop.py:128-139`); `SearchConfig.noop_branch_choices` and `noop_resource_aware`
/// (`ai/mcts.py:104-105`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct NoopFlags {
    pub branch_choices: bool,
    pub resource_aware: bool,
}

// ---------------------------------------------------------------------------------------------
// Search results and tracing (ai/mcts.py:75-79; PLAN §6.6, §8.2, §8.7)

/// `SearchResult(action, alternatives, iterations)` (`ai/mcts.py:75-79`) of the reference
/// search and of the `gcg-search` v0 copy. Python's `iterations` is `stats.iterations`; the
/// rest of [`SearchStats`] is what the trace agent block records (PLAN §8.2) and the effort
/// counters of PLAN §6.6.
#[derive(Clone, PartialEq, Debug)]
pub struct SearchResult {
    pub action: Action,
    /// One per root move, in root-move order (`ai/mcts.py:331-339`); empty when one move
    /// remained (`:111-112`).
    pub alternatives: Vec<Alternative>,
    pub stats: SearchStats,
}

/// The counts of one search besides its action and alternatives: the `iters`, `budget`,
/// `cap` and `roots` of the trace agent block (PLAN §8.2), all present or all absent, and the
/// effort counts (§6.6). [`DecisionTrace::search`] moves them out of a [`SearchResult`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SearchStats {
    /// `SearchResult.iterations` (`ai/mcts.py:79`); 0 when one move remained (`:111-112`).
    pub iterations: u32,
    /// `config.budget(len(root_moves))` (`ai/mcts.py:121`); 0 when the search returned with
    /// one move.
    pub budget: u32,
    /// `_iteration_cap(config, dec, budget)` (`ai/mcts.py:124`, `:165-178`); 0 likewise.
    pub cap: u32,
    /// The root moves after pruning, the no-effect filter and merging (`ai/mcts.py:89-110`),
    /// in order.
    pub roots: Vec<MoveKey>,
    pub effort: SearchEffort,
}

/// What one decision of an [`AgentCore`] agent computed besides its action
/// ([`Agent::choose_traced`]): the Rust side of the trace agent block (PLAN §8.2 `agent`,
/// §8.7 level 1; `gcg_diff::AgentRec`), compared field by field with Python's.
#[derive(Clone, PartialEq, Debug)]
pub struct DecisionTrace {
    /// `self._decisions` after `_begin` (`ai/base.py:64`): 1 for the first decision.
    pub k: u64,
    /// `derive_seed(self.seed, self.name, "decision", k)` (`ai/base.py:65`).
    pub seed: u64,
    /// The per-decision generator's `state` when the decision returns: `seed` when nothing
    /// was drawn (greedy's and the search agent's single option, `ai/agents.py:50-51`,
    /// `:87-88`).
    pub rng_after: u64,
    /// The list given to `_finish` (`ai/base.py:67-76`), in the agent's order, unranked and
    /// unrounded: the search's alternatives (`ai/agents.py:97-98`), greedy's scored moves
    /// (`:57-66`), random's `1.0 / n` shares, one per option even for a single one
    /// (`:28-32`), the mulligan and play/draw statistics (`:52-54`, `:89-96`), or empty
    /// (greedy's and the search agent's single option, `:50-51`, `:87-88`; greedy's play/draw,
    /// `:55-56`).
    pub alternatives: Vec<Alternative>,
    /// `Some` exactly when the decision called `search` (`ai/agents.py:97`), including a
    /// search that returned with one root move; `None` for a single option, the mulligan, the
    /// play/draw choice and every greedy or random decision.
    pub search: Option<SearchStats>,
}

/// Deterministic effort counts of one decision (PLAN §6.6; W-15, W-21): engine `apply` calls by
/// purpose, determinised worlds built and rollouts stopped by `max_rollout_decisions`. They
/// never stop a search; they go to traces and to the results schema 1.2.0 totals (§12.7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct SearchEffort {
    pub applies_tree: u64,
    pub applies_rollout: u64,
    pub applies_aux: u64,
    pub worlds: u64,
    pub rollouts_capped: u64,
}

/// What an agent spent over its decisions ([`Agent::effort`]; PLAN §6.6, §12.7, O9): the
/// decisions it made, how many of them reported [`SearchStats`] (a search, including one that
/// returned with a single root move, or a pre-game decision whose mechanisms simulated, as
/// [`DecisionTrace::search`] reports them), and the sum of those statistics' iterations and
/// [`SearchEffort`] counts. Deterministic counts, never used to stop a search.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct AgentEffort {
    pub decisions: u64,
    pub searches: u64,
    pub iterations: u64,
    pub applies_tree: u64,
    pub applies_rollout: u64,
    pub applies_aux: u64,
    pub worlds: u64,
    pub rollouts_capped: u64,
}

impl AgentEffort {
    /// Counts one decision and, when it reported them, its search statistics.
    pub fn record(&mut self, search: Option<&SearchStats>) {
        self.decisions += 1;
        if let Some(stats) = search {
            self.searches += 1;
            self.iterations += u64::from(stats.iterations);
            self.applies_tree += stats.effort.applies_tree;
            self.applies_rollout += stats.effort.applies_rollout;
            self.applies_aux += stats.effort.applies_aux;
            self.worlds += stats.effort.worlds;
            self.rollouts_capped += stats.effort.rollouts_capped;
        }
    }

    /// The sum of two counts.
    #[must_use]
    pub const fn plus(&self, o: &Self) -> Self {
        Self {
            decisions: self.decisions + o.decisions,
            searches: self.searches + o.searches,
            iterations: self.iterations + o.iterations,
            applies_tree: self.applies_tree + o.applies_tree,
            applies_rollout: self.applies_rollout + o.applies_rollout,
            applies_aux: self.applies_aux + o.applies_aux,
            worlds: self.worlds + o.worlds,
            rollouts_capped: self.rollouts_capped + o.rollouts_capped,
        }
    }

    /// Engine applies of every purpose.
    #[must_use]
    pub const fn applies(&self) -> u64 {
        self.applies_tree + self.applies_rollout + self.applies_aux
    }
}

/// The configuration of the search an agent factory's agents run, as results 1.2.0 records
/// it under `ai.search` (PLAN §12.7, O9; `gcg_runner::AgentFactory::search_block`). Only the
/// strengthened search's factories have one (`gcg_search::search_settings`); the report adds
/// `algorithm` (`gcg-search`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SearchSettings {
    /// The shipped preset the configuration is (`standard` or `fast`).
    pub preset: &'static str,
    /// The reference preset its budgets start from (`gcg_search::Preset::mapping`).
    pub mapping: &'static str,
    /// The iteration budget `clamp(iterations_per_option * n, min_iterations,
    /// max_iterations)` (`ai/config.py:55-57`).
    pub iterations_per_option: u32,
    pub min_iterations: u32,
    pub max_iterations: u32,
    /// The auxiliary engine applies (trial picks of `macro_multi_pick`) a decision may spend
    /// per iteration of its iteration cap.
    pub aux_apply_cap: u64,
    /// The enabled mechanism flags, sorted by name.
    pub mechanisms: Vec<&'static str>,
    /// The evaluation weights: the name of a shipped table (`ROLE_WEIGHTS`, `TUNED_WEIGHTS`,
    /// `HAND_WEIGHTS`) or `<feature set>:<16 hex digits>` for other weights
    /// (`gcg_search::weights_id`).
    pub weights: String,
}

/// One iteration of a search, as drill-down level 2 records it (PLAN §8.2 `iter`, §8.7).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IterTrace {
    /// Iteration index, from 0.
    pub n: u32,
    /// Root child index (root-move order).
    pub child: u32,
    /// The child's visit index `k`, which selects world `k` and rollout seed `k`
    /// (`ai/mcts.py:136-145`).
    pub visit: u32,
    /// Whether this iteration added a new tree node (`ai/mcts.py:205-235`).
    pub fresh: bool,
    /// Length of the backup path, duplicates included (W-06 kept).
    pub path_len: u32,
    pub applies_walk: u32,
    pub applies_rollout: u32,
    /// The leaf value (`ai/mcts.py:254-264`).
    pub leaf: f64,
    pub terminal: bool,
    /// The root child's `total` before and after the backup (`per_visit` stores the
    /// difference, `ai/mcts.py:138-141`).
    pub total_before: f64,
    pub total_after: f64,
}

/// Drill-down hooks of the reference search and the `gcg-search` copy (PLAN §8.7 levels 2-3).
/// Searches take the tracer generically, so [`NoTrace`] costs nothing.
pub trait SearchTracer {
    /// Level 2: after iteration `it.n`'s backup; `world` is the determinised world the
    /// iteration played in (before its clone), whose digest prefix the trace records.
    fn iteration(&mut self, world: &GameState, it: &IterTrace);
    /// Level 3: after the `j`-th engine `apply` inside iteration `n`, with the state after it.
    fn apply(&mut self, n: u32, j: u32, action: Action, after: &GameState);
}

/// The tracer of normal runs: records nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct NoTrace;

impl SearchTracer for NoTrace {
    #[inline(always)]
    fn iteration(&mut self, _world: &GameState, _it: &IterTrace) {}

    #[inline(always)]
    fn apply(&mut self, _n: u32, _j: u32, _action: Action, _after: &GameState) {}
}

// ---------------------------------------------------------------------------------------------
// The baseline players (ai/agents.py:18-66; D1)

/// `GREEDY_MULLIGAN_SAMPLES` (`ai/agents.py:18`).
pub const GREEDY_MULLIGAN_SAMPLES: u32 = 48;
/// `GREEDY_MAX_DECISIONS` (`ai/agents.py:19`).
pub const GREEDY_MAX_DECISIONS: u32 = 200;

/// `RandomAgent(seed=0, *, log=False)` (`ai/agents.py:22-32`), name `random`. Its `Agent`
/// impl (D1, `random.rs`) draws `randrange(len(options))` even for one option.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by workstream D1's random.rs")]
pub struct RandomAgent {
    pub(crate) core: AgentCore,
}

impl RandomAgent {
    /// `RandomAgent(seed, log=log)` (`ai/agents.py:25-26`).
    #[must_use]
    pub fn new(seed: u64, log: bool) -> Self {
        Self {
            core: AgentCore::new("random", seed, log),
        }
    }
}

/// `GreedyAgent(seed=0, *, log=False, weights=DEFAULT_WEIGHTS)` (`ai/agents.py:35-66`), name
/// `greedy`. Its `Agent` impl (D1, `greedy.rs`) is the one-ply look-ahead of `:48-66`.
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "read by workstream D1's greedy.rs")]
pub struct GreedyAgent {
    pub(crate) core: AgentCore,
    pub(crate) weights: Weights,
}

impl GreedyAgent {
    /// `GreedyAgent(seed, log=log, weights=weights)` (`ai/agents.py:42-46`).
    #[must_use]
    pub fn new(seed: u64, log: bool, weights: Weights) -> Self {
        Self {
            core: AgentCore::new("greedy", seed, log),
            weights,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gcg_core::pool::Pool;

    fn bits(w: &Weights) -> Vec<u64> {
        w.values().iter().map(|v| v.to_bits()).collect()
    }

    /// Bit patterns printed from the live Python objects (`struct.pack('<d', v)` of
    /// `HAND_WEIGHTS.values`, `TUNED_WEIGHTS.values`, `ROLE_WEIGHTS.values`, Python 3.15.1).
    #[test]
    fn weight_tables_are_pythons_bit_for_bit() {
        assert_eq!(
            bits(&HAND_WEIGHTS),
            [
                0x0000000000000000,
                0x3FD6666666666666,
                0xBFD3333333333333,
                0x3FF0000000000000,
                0x3FB999999999999A,
                0x3FB999999999999A,
                0x3FC999999999999A,
                0x3FC999999999999A,
                0x3FD3333333333333,
                0x3FD0000000000000,
                0x3FD0000000000000,
                0x3FD0000000000000,
                0x3FC3333333333333,
                0x3FD3333333333333,
            ]
        );
        assert_eq!(
            bits(&TUNED_WEIGHTS),
            [
                0xBFC9D14E3BCD35A8,
                0x3FDB6C8B43958106,
                0xBFDC25AEE631F8A1,
                0x3FE11758E219652C,
                0x3FBCF41F212D7732,
                0x3FC425AEE631F8A1,
                0x3FCAE48E8A71DE6A,
                0x3FE10624DD2F1AA0,
                0x3FE6E703AFB7E910,
                0x3FDF4395810624DD,
                0x3FD7DA5119CE075F,
                0x3FE25F06F6944674,
                0x3FC507C84B5DCC64,
                0x3FE1B3D07C84B5DD,
            ]
        );
        assert_eq!(
            bits(&ROLE_WEIGHTS),
            [
                0xBFE7D7DBF487FCB9,
                0x3FCA5E353F7CED91,
                0xBFDD604189374BC7,
                0x3FB3B645A1CAC083,
                0x3FCD916872B020C5,
                0x3FCB8BAC710CB296,
                0x3FD617C1BDA5119D,
                0x3FE8779A6B50B0F2,
                0x3FE17318FC504817,
                0x3FE15C28F5C28F5C,
                0x3FE29374BC6A7EFA,
                0x3FD38A0902DE00D2,
                0x3FEB2474538EF34D,
                0x3FF44395810624DD,
                0xBFDAC083126E978D,
                0x4009FD566CF41F21,
                0xBFEB4F0D844D013B,
            ]
        );
        assert_eq!(HAND_WEIGHTS.feature_set(), FeatureSet::Base);
        assert_eq!(TUNED_WEIGHTS.feature_set(), FeatureSet::Base);
        assert_eq!(ROLE_WEIGHTS.feature_set(), FeatureSet::Roles);
        assert!(std::ptr::eq(DEFAULT_WEIGHTS, &ROLE_WEIGHTS));
    }

    #[test]
    fn feature_sets_and_weight_validation_follow_python() {
        assert_eq!(N_FEATURES, 14);
        assert_eq!(
            ROLE_FEATURES[..13],
            FEATURES
                .iter()
                .copied()
                .filter(|n| *n != "can_kill")
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(FeatureSet::from_value("roles"), Some(FeatureSet::Roles));
        assert_eq!(FeatureSet::from_value("Roles"), None);
        let err = Weights::new(vec![0.0; 3], FeatureSet::Roles).unwrap_err();
        assert_eq!(err.to_string(), "expected 17 weights, got 3");
        let w = Weights::new(vec![0.5; 14], FeatureSet::Base).unwrap();
        assert_eq!(w.as_pairs().next(), Some(("tempo", 0.5)));
        assert_eq!(w.as_pairs().count(), 14);
        assert_ne!(w, HAND_WEIGHTS);
    }

    #[test]
    fn knobs_and_agents_start_as_python() {
        assert_eq!(PolicyKnobs::default(), BASELINE_KNOBS);
        assert_eq!(BASELINE_KNOBS.burst_yes_score.to_bits(), 1.0f64.to_bits());
        let core = RandomAgent::new(7, true).core;
        assert_eq!(
            (
                core.name(),
                core.seed(),
                core.log_enabled(),
                core.decisions()
            ),
            ("random", 7, true, 0)
        );
        assert!(core.entries().is_empty());
        assert_eq!(
            GreedyAgent::new(1, false, TUNED_WEIGHTS.clone())
                .core
                .name(),
            "greedy"
        );
        assert_eq!(
            AgentError::NoDecision {
                agent: "random".into(),
                player: 1
            }
            .py_class(),
            "ValueError"
        );
        assert_eq!(
            AgentError::Engine(EngineError::Key("x".into())).py_class(),
            "KeyError"
        );
    }

    /// The runner builds an `AgentCx` from the arguments `Pool::map_indexed` passes a task
    /// (`&mut S`, `&TaskCx<'_, S>`) and the run's `Cancel` (PLAN §7.1).
    fn from_pool_task<'a>(
        scratch: &'a mut Scratch,
        par: &'a TaskCx<'_, Scratch>,
        cancel: &'a Cancel,
    ) -> AgentCx<'a> {
        AgentCx {
            scratch,
            par,
            cancel,
        }
    }

    #[test]
    fn scratch_is_a_pool_value_on_its_own_cache_line() {
        let _ = from_pool_task;
        fn pool_value<S: Default + Send + 'static>() {}
        pool_value::<Scratch>();
        assert_eq!(align_of::<Scratch>(), 128);
        fn object_safe(_: &dyn Agent) {}
        fn send<T: Send>() {}
        send::<Box<dyn Agent>>();
        let _ = object_safe;
        let mut t = NoTrace;
        let _ = &mut t as &mut dyn SearchTracer;
    }

    /// An agent without an `AgentCore` (the runner's test agents) implements only `name`,
    /// `choose` and `decision_log`; `choose_traced` is then `choose` with no trace.
    struct Untraced;

    impl Agent for Untraced {
        fn name(&self) -> &str {
            "test-untraced"
        }

        fn choose(
            &mut self,
            _st: &GameState,
            player: u8,
            _cx: &mut AgentCx<'_>,
        ) -> Result<Action, AgentError> {
            Err(AgentError::NoDecision {
                agent: self.name().into(),
                player,
            })
        }

        fn decision_log(&self) -> &[LogEntry] {
            &[]
        }
    }

    #[test]
    fn decision_traces_carry_the_agent_block() {
        let agent: Box<dyn Agent> = Box::new(Untraced);
        assert!(agent.decision_log().is_empty());
        fn traced(
            a: &mut dyn Agent,
            st: &GameState,
            cx: &mut AgentCx<'_>,
        ) -> Result<(Action, Option<DecisionTrace>), AgentError> {
            a.choose_traced(st, 0, cx, &mut NoTrace)
        }
        let _ = traced;
        let roots = vec![MoveKey {
            kind: ActionKind::Pass,
            a: -1,
            b: -1,
            variant: -1,
        }];
        let result = SearchResult {
            action: Action::new(ActionKind::Pass, -1, -1, -1),
            alternatives: Vec::new(),
            stats: SearchStats {
                iterations: 0,
                budget: 0,
                cap: 0,
                roots: roots.clone(),
                effort: SearchEffort::default(),
            },
        };
        let trace = DecisionTrace {
            k: 1,
            seed: 9,
            rng_after: 9,
            alternatives: result.alternatives,
            search: Some(result.stats),
        };
        assert_eq!(trace.search.map(|s| s.roots), Some(roots));
    }

    /// O9: agents report no effort unless they override `effort`; the counts sum every
    /// decision and the statistics of those that reported a search.
    #[test]
    fn effort_counts_decisions_and_search_statistics() {
        assert_eq!(Untraced.effort(), None);
        let stats = |iterations: u32, tree: u64| SearchStats {
            iterations,
            budget: iterations,
            cap: iterations,
            roots: Vec::new(),
            effort: SearchEffort {
                applies_tree: tree,
                applies_rollout: 2 * tree,
                applies_aux: 1,
                worlds: 3,
                rollouts_capped: 1,
            },
        };
        let mut a = AgentEffort::default();
        a.record(None);
        a.record(Some(&stats(10, 5)));
        a.record(Some(&stats(0, 0)));
        assert_eq!(
            a,
            AgentEffort {
                decisions: 3,
                searches: 2,
                iterations: 10,
                applies_tree: 5,
                applies_rollout: 10,
                applies_aux: 2,
                worlds: 6,
                rollouts_capped: 2,
            }
        );
        assert_eq!(a.applies(), 17);
        let twice = a.plus(&a);
        assert_eq!(
            (twice.decisions, twice.iterations, twice.applies()),
            (6, 20, 34)
        );
        assert_eq!(AgentEffort::default().plus(&a), a);
    }

    /// Tests, `gcg-diff` and tools build their `AgentCx` outside the pool with
    /// `Pool::run_inline` (docs/dev/contracts.md, gcg-ai section).
    #[test]
    fn an_agent_context_can_be_built_outside_the_pool() {
        let cancel = Cancel::new();
        let fresh = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let cx = from_pool_task(scratch, par, &cancel);
            cx.scratch.states.is_empty()
                && cx.scratch.floats.is_empty()
                && !cx.cancel.is_cancelled()
        });
        assert!(fresh);
    }
}
