//! Shared harness of the native Rust versions of the Python AI tests (`tests/ai/*.py`; PLAN §9.1,
//! T-066, T-067): the Rust testkit, the reference agents and searches, and the decision
//! records of `oracle/export_ai_tests.py` (`crates/gcg-ai/tests/vectors/ai_tests/`).
//!
//! Each reproduced test opens a [`Recorder`] for its Python node id and makes every agent
//! decision through it. The recorder compares each decision, as it happens, with the one the
//! Python test made at the same point: the digest of the state decided on, the agent, the
//! player, the action, the per-decision counter, seed and generator state afterwards, every
//! alternative unranked and unrounded (values bit for bit), the search's iterations, budget,
//! cap and root keys, and the decision-log entries written (bytes of
//! `json.dumps(entry, sort_keys=True)`). [`Recorder::finish`] requires that no Python decision
//! is left over. The test's own assertions are reproduced line by line on top.
//!
//! These tests live in `gcg-search` because its test targets are the only ones of the AI
//! crates that build the engine with the `testkit` feature (through the `gcg-diff`
//! dev-dependency) and reach the reference search (dev-dependency `gcg-ai-reference`).

#![allow(
    dead_code,
    reason = "each test file uses its own subset of the helpers"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use gcg_ai::{
    Agent, AgentCx, Alternative, LogAlternative, NoTrace, Scratch, SearchResult, Weights,
};
use gcg_ai_reference::ReferenceConfig;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::state_digest;
use gcg_engine::{
    Action, ActionKind, AddOptions, GameState, NewGameOptions, Scenario, ScenarioOptions, Step,
    Uid, Zone, apply, new_game_by_number,
};
use serde_json::Value;

pub mod positions;

/// `FILLER` of the AI tests (`tests/ai/test_puzzles.py:24`): Zaku Mariner, a vanilla 2/2.
pub const FILLER: &str = "GD01-060";
/// `PLAYER_TARGET` (`engine/types.py`): an attack on the player.
pub const PLAYER: i32 = -2;

/// `json.dumps(x, sort_keys=True)` with Python's default separators (`ensure_ascii`).
pub const PY_SORTED: DumpOptions = DumpOptions {
    indent: None,
    sort_keys: true,
    compact: false,
    ensure_ascii: true,
};

pub fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gcg-ai/tests/vectors/ai_tests")
}

pub fn registry() -> Arc<Registry> {
    Registry::embedded()
}

/// The embedded registry with an IR override fragment (`Registry::with_fragment`): the
/// `card_text` fixtures of `crates/gcg-effects/ir/fragments/<number>.json` or the
/// `command_script` fragments of `ai_tests/fragments/<name>.json`.
pub fn registry_with(fragment_file: &str) -> Arc<Registry> {
    let path = if fragment_file.contains('/') {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fragment_file)
    } else {
        vectors().join("fragments").join(fragment_file)
    };
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let v: Value = serde_json::from_str(&text).expect("a fragment fixture");
    let reg = Registry::embedded()
        .with_fragment(&v["fragment"])
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Arc::new(reg)
}

// ---------------------------------------------------------------------------------------------
// JSON readers

pub fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))
}

/// A u64 in the trace encoding: a number below 2**53, else a decimal string.
pub fn u64_of(v: &Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a u64: {v}"))
}

pub fn action(v: &Value) -> Action {
    let kind = ActionKind::from_value(v[0].as_str().expect("a kind")).expect("an action kind");
    let arg = |i: usize| i32::try_from(int(&v[i])).expect("an i32 argument");
    Action::new(kind, arg(1), arg(2), arg(3))
}

/// `float.fromhex` for the forms `float.hex()` writes (`0x1.8p-2`, `-0x0.0p+0`, subnormals).
pub fn fhex(s: &str) -> f64 {
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let body = body.strip_prefix("0x").expect("a hex float");
    let (mantissa, exp) = body.split_once('p').expect("an exponent");
    let exp: i32 = exp.parse().expect("an int exponent");
    let (int_part, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut m = u64::from_str_radix(int_part, 16).expect("hex digits");
    for c in frac.chars() {
        m = m * 16 + u64::from(c.to_digit(16).expect("a hex digit"));
    }
    if m == 0 {
        return if neg { -0.0 } else { 0.0 };
    }
    let shift = exp - 4 * i32::try_from(frac.len()).expect("short");
    let pow2 = if shift >= -1022 {
        f64::from_bits(u64::try_from(shift + 1023).expect("in range") << 52)
    } else {
        f64::from_bits(1u64 << u32::try_from(shift + 1074).expect("subnormal range"))
    };
    #[expect(clippy::cast_precision_loss, reason = "a 53-bit mantissa is exact")]
    let v = m as f64 * pow2;
    if neg { -v } else { v }
}

/// `[[action, value hex, visits], ...]` as comparable bit triples.
pub fn alternatives(v: &Value) -> Vec<(Action, u64, u32)> {
    v.as_array()
        .expect("alternatives")
        .iter()
        .map(|a| {
            (
                action(&a[0]),
                fhex(a[1].as_str().expect("a hex float")).to_bits(),
                u32::try_from(int(&a[2])).expect("visits"),
            )
        })
        .collect()
}

pub fn bits(alts: &[Alternative]) -> Vec<(Action, u64, u32)> {
    alts.iter()
        .map(|a| (a.action, a.value.to_bits(), a.visits))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The Python records

type Module = BTreeMap<String, Value>;

/// The records of one Python test module (`<module>.ndjson`), by node id, read once per test
/// binary.
fn module(name: &str) -> Arc<Module> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, Arc<Module>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = cache.lock().expect("not poisoned");
    Arc::clone(guard.entry(name.to_owned()).or_insert_with(|| {
        let path = vectors().join(format!("{name}.ndjson"));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut out = Module::new();
        for line in text.lines() {
            let v: Value = serde_json::from_str(line).expect("a JSON record");
            let id = v["id"].as_str().expect("a node id").to_owned();
            out.insert(id, v);
        }
        Arc::new(out)
    }))
}

/// The node ids of a Python module's records, in pytest order of the file.
pub fn python_ids(name: &str) -> Vec<String> {
    let path = vectors().join(format!("{name}.ndjson"));
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("a JSON record");
            v["id"].as_str().expect("a node id").to_owned()
        })
        .collect()
}

/// The Python record of one test (`tests/ai/<module>.py::<test>`).
pub fn python_record(module_name: &str, test: &str) -> Value {
    let id = format!("tests/ai/{module_name}.py::{test}");
    module(module_name)
        .get(&id)
        .cloned()
        .unwrap_or_else(|| panic!("no Python record for {id}"))
}

/// Asserts that the ids of a module's records are exactly `ported` (as `<test>[<params>]`):
/// every Python test of the module has a Rust version and nothing else is claimed.
pub fn assert_ported(module_name: &str, ported: &[String]) {
    let prefix = format!("tests/ai/{module_name}.py::");
    let mut python: Vec<String> = python_ids(module_name)
        .into_iter()
        .map(|id| {
            id.strip_prefix(&prefix)
                .expect("the module prefix")
                .to_owned()
        })
        .collect();
    let mut ours = ported.to_vec();
    python.sort();
    ours.sort();
    assert_eq!(ours, python, "the ported tests of {module_name}");
}

/// `name[p]` for every parameter id.
pub fn ids<T: std::fmt::Display>(name: &str, params: impl IntoIterator<Item = T>) -> Vec<String> {
    params.into_iter().map(|p| format!("{name}[{p}]")).collect()
}

// ---------------------------------------------------------------------------------------------
// Agent context

/// `f(cx)` with an agent context outside the pool (`Pool::run_inline`, docs/dev/contracts.md).
pub fn with_cx<T>(f: impl FnOnce(&mut AgentCx<'_>) -> T) -> T {
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        f(&mut cx)
    })
}

/// `make_agent("mcts", seed=seed)`: the reference `standard` preset with `DEFAULT_WEIGHTS`.
pub fn mcts(seed: u64) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent("mcts", "standard", seed, false, None, None).expect("mcts")
}

/// `make_agent(kind, seed=seed, log=log, config=config, weights=weights)`.
pub fn agent(
    kind: &str,
    seed: u64,
    log: bool,
    config: Option<&ReferenceConfig>,
    weights: Option<&Weights>,
) -> Box<dyn Agent> {
    gcg_ai_reference::make_agent(kind, "standard", seed, log, config, weights)
        .unwrap_or_else(|e| panic!("make_agent({kind}): {e}"))
}

/// `(make_agent("mcts", seed=seed), make_agent("mcts", seed=seed + 100))`
/// (`tests/ai/test_puzzles.py:27-28`).
pub fn agents(seed: u64) -> [Box<dyn Agent>; 2] {
    [mcts(seed), mcts(seed + 100)]
}

// ---------------------------------------------------------------------------------------------
// The recorder

/// The Python decisions of one test, compared one by one with the Rust decisions.
pub struct Recorder {
    id: String,
    expected: Vec<Value>,
    next: usize,
}

impl Recorder {
    /// The recorder of `tests/ai/<module>.py::<test>`; the Python test passed.
    pub fn new(module_name: &str, test: &str) -> Self {
        let rec = python_record(module_name, test);
        assert_eq!(rec["outcome"], "passed", "{test}");
        let expected = rec["decisions"]
            .as_array()
            .unwrap_or_else(|| panic!("{test}: decisions are not recorded"))
            .clone();
        Self {
            id: format!("{module_name}::{test}"),
            expected,
            next: 0,
        }
    }

    /// The recorder of decisions `start..start + len` of a test, for a test whose cases run
    /// side by side (one case per index, [`par_map`]).
    pub fn part(module_name: &str, test: &str, start: usize, len: usize) -> Self {
        let mut rec = Self::new(module_name, test);
        assert!(
            start + len <= rec.expected.len(),
            "{test}: decisions {start}..{} of {}",
            start + len,
            rec.expected.len()
        );
        rec.expected = rec.expected[start..start + len].to_vec();
        rec.id = format!("{}[decisions {start}..{}]", rec.id, start + len);
        rec
    }

    /// A recorder over decisions read from another file (`midgame.json`).
    pub fn from_values(id: String, expected: Vec<Value>) -> Self {
        Self {
            id,
            expected,
            next: 0,
        }
    }

    /// The number of Python decisions of this recorder.
    pub fn len(&self) -> usize {
        self.expected.len()
    }

    fn take(&mut self) -> (usize, Value) {
        let i = self.next;
        let rec = self
            .expected
            .get(i)
            .unwrap_or_else(|| {
                panic!(
                    "{}: Rust makes decision #{i}, Python made {}",
                    self.id,
                    self.expected.len()
                )
            })
            .clone();
        self.next += 1;
        (i, rec)
    }

    /// `agent.choose(st, player)`, compared with Python's decision at this point.
    pub fn choose(&mut self, agent: &mut dyn Agent, st: &GameState, player: u8) -> Action {
        let (i, rec) = self.take();
        let at = format!("{} decision #{i}", self.id);
        assert!(
            rec.get("search").is_none(),
            "{at}: Python searched directly"
        );
        assert_eq!(
            state_digest(st),
            rec["digest"].as_str().expect("a digest"),
            "{at}: state"
        );
        assert_eq!(
            agent.name(),
            rec["agent"].as_str().expect("a name"),
            "{at}: agent"
        );
        assert_eq!(i64::from(player), int(&rec["player"]), "{at}: player");
        let before = agent.decision_log().len();
        let (chosen, trace) = with_cx(|cx| agent.choose_traced(st, player, cx, &mut NoTrace))
            .unwrap_or_else(|e| panic!("{at}: {e}"));
        let trace = trace.unwrap_or_else(|| panic!("{at}: no decision trace"));
        assert_eq!(chosen, action(&rec["action"]), "{at}: action");
        assert_eq!(trace.k, u64_of(&rec["k"]), "{at}: decision counter");
        assert_eq!(trace.seed, u64_of(&rec["seed"]), "{at}: decision seed");
        assert_eq!(
            bits(&trace.alternatives),
            alternatives(&rec["alts"]),
            "{at}: alternatives"
        );
        assert_eq!(
            trace.rng_after,
            u64_of(&rec["rng_after"]),
            "{at}: generator after"
        );
        match (&trace.search, rec.get("iters")) {
            (Some(stats), Some(iters)) => {
                assert_eq!(i64::from(stats.iterations), int(iters), "{at}: iterations");
                assert_eq!(i64::from(stats.budget), int(&rec["budget"]), "{at}: budget");
                assert_eq!(i64::from(stats.cap), int(&rec["cap"]), "{at}: cap");
                let roots: Vec<String> = stats
                    .roots
                    .iter()
                    .map(|k| format!("{},{},{},{}", k.kind.as_str(), k.a, k.b, k.variant))
                    .collect();
                let want: Vec<String> = rec["roots"]
                    .as_array()
                    .expect("root keys")
                    .iter()
                    .map(|k| {
                        let kind = k[0].as_str().expect("a kind");
                        format!("{kind},{},{},{}", int(&k[1]), int(&k[2]), int(&k[3]))
                    })
                    .collect();
                assert_eq!(roots, want, "{at}: root moves");
            }
            (None, None) => {}
            (got, want) => panic!("{at}: search {got:?} vs Python iterations {want:?}"),
        }
        let logged: Vec<String> = agent.decision_log()[before..]
            .iter()
            .map(|e| dumps(&e.to_json(), PY_SORTED))
            .collect();
        let want: Vec<String> = rec["log"]
            .as_array()
            .expect("log entries")
            .iter()
            .map(|e| e.as_str().expect("a log entry").to_owned())
            .collect();
        assert_eq!(logged, want, "{at}: decision log");
        chosen
    }

    /// `mcts.search(st, player, config, weights, SplitMix64(seed))`, compared with Python's
    /// direct search at this point.
    pub fn search(
        &mut self,
        st: &GameState,
        player: u8,
        cfg: &ReferenceConfig,
        w: &Weights,
        seed: u64,
    ) -> SearchResult {
        let (i, rec) = self.take();
        let at = format!("{} search #{i}", self.id);
        assert_eq!(rec["search"], true, "{at}: Python asked an agent");
        assert_eq!(
            state_digest(st),
            rec["digest"].as_str().expect("a digest"),
            "{at}: state"
        );
        assert_eq!(i64::from(player), int(&rec["player"]), "{at}: player");
        assert_eq!(seed, u64_of(&rec["rng_before"]), "{at}: seed");
        let mut rng = SplitMix64::new(seed);
        let result =
            with_cx(|cx| gcg_ai_reference::search(st, player, cfg, w, &mut rng, cx, &mut NoTrace))
                .unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!(result.action, action(&rec["action"]), "{at}: action");
        assert_eq!(
            bits(&result.alternatives),
            alternatives(&rec["alts"]),
            "{at}: alternatives"
        );
        assert_eq!(
            i64::from(result.stats.iterations),
            int(&rec["iters"]),
            "{at}: iterations"
        );
        assert_eq!(
            rng.state,
            u64_of(&rec["rng_after"]),
            "{at}: generator after"
        );
        result
    }

    /// Every Python decision was made.
    pub fn finish(self) {
        assert_eq!(
            self.next,
            self.expected.len(),
            "{}: Rust made {} decisions, Python {}",
            self.id,
            self.next,
            self.expected.len()
        );
    }
}

/// `while st.pending is not None and st.turn < st.turn + turns: apply(st, players[p].choose(st, p))`
/// (`play_turns`, `tests/ai/test_puzzles.py:30-35`).
pub fn play_turns(
    rec: &mut Recorder,
    st: &mut GameState,
    players: &mut [Box<dyn Agent>; 2],
    turns: i32,
) {
    let stop = st.turn() + turns;
    while let Some(dec) = st.pending() {
        if st.turn() >= stop {
            break;
        }
        let p = dec.hdr.player;
        let a = rec.choose(&mut *players[usize::from(p)], st, p);
        apply(st, a, true).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
    }
}

/// `(action, agent.decision_log()[-1]["alternatives"])` of one decision of a logging agent:
/// the `_decision` helpers of the information-set tests (`test_information_set.py:64-69`).
pub type Decision = (Action, Vec<LogAlternative>);

/// `_decision(...)`: `agent.choose(st, st.pending.player)` through the recorder, with the
/// alternatives of the log entry it wrote.
pub fn decision(rec: &mut Recorder, agent: &mut dyn Agent, st: &GameState) -> Decision {
    let p = pending_player(st);
    let a = rec.choose(agent, st, p);
    let entry = agent.decision_log().last().expect("a logging agent");
    (a, entry.alternatives.clone())
}

/// `choice(st, agent)` (`tests/ai/test_puzzles.py:38-40`).
pub fn choice(rec: &mut Recorder, st: &GameState, agent: &mut dyn Agent) -> Action {
    let p = st.pending().expect("a pending decision").hdr.player;
    rec.choose(agent, st, p)
}

pub fn pending_player(st: &GameState) -> u8 {
    st.pending().expect("a pending decision").hdr.player
}

pub fn pending_kind(st: &GameState) -> Option<gcg_engine::DecisionKind> {
    st.pending().map(|d| d.hdr.kind)
}

// ---------------------------------------------------------------------------------------------
// Scenarios

/// `testkit.Scenario` with the defaults of the AI tests and short helpers.
pub struct Sc {
    pub sc: Scenario,
}

impl Sc {
    pub fn new() -> Self {
        Self::on(&registry(), ScenarioOptions::default())
    }

    pub fn active(active: u8) -> Self {
        Self::on(
            &registry(),
            ScenarioOptions {
                active,
                ..ScenarioOptions::default()
            },
        )
    }

    pub fn on(reg: &Arc<Registry>, opts: ScenarioOptions) -> Self {
        Self {
            sc: Scenario::new(reg, opts),
        }
    }

    /// `sc.add(player, number)` (Battle Area).
    pub fn add(&mut self, player: u8, number: &str) -> Uid {
        self.add_with(player, number, Zone::Battle, AddOptions::default())
    }

    /// `sc.add(player, number, rested=True)`.
    pub fn rested(&mut self, player: u8, number: &str) -> Uid {
        self.add_with(
            player,
            number,
            Zone::Battle,
            AddOptions {
                rested: true,
                ..AddOptions::default()
            },
        )
    }

    pub fn add_with(&mut self, player: u8, number: &str, zone: Zone, opts: AddOptions<'_>) -> Uid {
        self.sc
            .add(player, number, zone, opts)
            .unwrap_or_else(|e| panic!("add {number}: {e:?}"))
    }

    /// `sc.add(player, number, Zone.HAND)`.
    pub fn in_hand(&mut self, player: u8, number: &str) -> Uid {
        self.add_with(player, number, Zone::Hand, AddOptions::default())
    }

    /// `sc.add(player, number, Zone.HAND, known=False)`.
    pub fn hidden_in_hand(&mut self, player: u8, number: &str) -> Uid {
        self.add_with(
            player,
            number,
            Zone::Hand,
            AddOptions {
                known: false,
                ..AddOptions::default()
            },
        )
    }

    pub fn hand(&mut self, player: u8, numbers: &[&str]) -> Vec<Uid> {
        self.sc
            .hand(player, numbers)
            .unwrap_or_else(|e| panic!("hand: {e:?}"))
    }

    pub fn trash(&mut self, player: u8, numbers: &[&str]) -> Vec<Uid> {
        self.sc
            .trash(player, numbers)
            .unwrap_or_else(|e| panic!("trash: {e:?}"))
    }

    pub fn shields(&mut self, player: u8, numbers: &[&str]) -> Vec<Uid> {
        self.sc
            .shields(player, numbers)
            .unwrap_or_else(|e| panic!("shields: {e:?}"))
    }

    /// `sc.shields(player, *[FILLER] * n)`.
    pub fn filler_shields(&mut self, player: u8, n: usize) -> Vec<Uid> {
        self.shields(player, &vec![FILLER; n])
    }

    pub fn deck(&mut self, player: u8, numbers: &[&str]) {
        self.sc.deck(player, numbers);
    }

    /// `sc.resources(player, n, rested=rested, ex=ex)`.
    pub fn resources(&mut self, player: u8, n: usize, rested: usize, ex: usize) -> Vec<Uid> {
        self.sc
            .resources(player, n, rested, ex)
            .unwrap_or_else(|e| panic!("resources: {e:?}"))
    }

    /// `sc.base(player, number)`.
    pub fn base(&mut self, player: u8, number: Option<&str>) -> Uid {
        self.sc
            .base(player, number, 0)
            .unwrap_or_else(|e| panic!("base: {e:?}"))
    }

    /// `sc.start()`.
    pub fn start(self) -> GameState {
        self.start_at(Step::Main)
    }

    /// `sc.start(step)`.
    pub fn start_at(mut self, step: Step) -> GameState {
        self.sc
            .start(step)
            .unwrap_or_else(|e| panic!("start: {e:?}"));
        self.sc.into_state()
    }
}

/// `apply(st, action)` (`check=True`).
pub fn apply_ok(st: &mut GameState, a: Action) {
    apply(st, a, true).unwrap_or_else(|e| panic!("apply {a:?}: {e}"));
}

// ---------------------------------------------------------------------------------------------
// The AI test decks and the mid-game states (tests/ai/decks.py, test_information_set.py)

/// `midgame.json`.
pub fn midgame() -> &'static Value {
    static MIDGAME: OnceLock<Value> = OnceLock::new();
    MIDGAME.get_or_init(|| {
        let path = vectors().join("midgame.json");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).expect("midgame.json")
    })
}

/// `DECKS = (FEDERATION_DECK, SEED_DECK)` (`tests/ai/decks.py:68-71`) in list order.
pub fn decks() -> [(Vec<String>, Vec<String>); 2] {
    let part = |deck: &Value, key: &str| -> Vec<String> {
        deck[key]
            .as_array()
            .expect("card numbers")
            .iter()
            .map(|n| n.as_str().expect("a number").to_owned())
            .collect()
    };
    let d = midgame()["decks"].as_array().expect("two decks");
    [
        (part(&d[0], "main"), part(&d[0], "res")),
        (part(&d[1], "main"), part(&d[1], "res")),
    ]
}

/// `new_game(DECKS, seed)` on `reg`.
pub fn new_game_on(reg: &Arc<Registry>, seed: u64) -> GameState {
    let decks = decks();
    let refs: Vec<(Vec<&str>, Vec<&str>)> = decks
        .iter()
        .map(|(m, r)| {
            (
                m.iter().map(String::as_str).collect(),
                r.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    new_game_by_number(
        reg,
        [
            (&refs[0].0[..], &refs[0].1[..]),
            (&refs[1].0[..], &refs[1].1[..]),
        ],
        seed,
        NewGameOptions::default(),
    )
    .expect("the AI test decks are legal")
}

/// `new_game(DECKS, seed)`.
pub fn new_game(seed: u64) -> GameState {
    new_game_on(&registry(), seed)
}

/// `GAME_SEEDS`, `STATES_PER_GAME`, `PERMUTATION_SEEDS`, `AGENT_SEEDS`
/// (`tests/ai/test_information_set.py:22-25`).
pub const GAME_SEEDS: [u64; 5] = [101, 202, 303, 404, 505];
pub const STATES_PER_GAME: usize = 5;
pub const PERMUTATION_SEEDS: [u64; 3] = [1, 2, 3];
pub const AGENT_SEEDS: [u64; 2] = [17, 29];
pub const MIDGAME_STATES: usize = GAME_SEEDS.len() * STATES_PER_GAME;

/// `midgame_states()` (`tests/ai/test_information_set.py:27-43`): every 4th contested decision
/// from turn 3 on, five per game, from greedy self-play; computed once per test binary and
/// checked against Python's digests. Each call returns fresh clones.
pub fn midgame_states() -> Vec<GameState> {
    static STATES: OnceLock<Mutex<Vec<GameState>>> = OnceLock::new();
    let states = STATES.get_or_init(|| {
        let mut out = Vec::new();
        for seed in GAME_SEEDS {
            let mut players = [
                gcg_ai::make_greedy_agent(seed, false, gcg_ai::DEFAULT_WEIGHTS.clone()),
                gcg_ai::make_greedy_agent(seed + 1, false, gcg_ai::DEFAULT_WEIGHTS.clone()),
            ];
            let mut st = new_game(seed);
            let (mut taken, mut contested) = (0, 0);
            while let Some(dec) = st.pending() {
                if taken >= STATES_PER_GAME {
                    break;
                }
                let p = dec.hdr.player;
                if st.turn() >= 3 && dec.options.len() > 1 {
                    if contested % 4 == 0 {
                        out.push(st.clone());
                        taken += 1;
                    }
                    contested += 1;
                }
                let a = with_cx(|cx| players[usize::from(p)].choose(&st, p, cx))
                    .unwrap_or_else(|e| panic!("greedy: {e}"));
                apply_ok(&mut st, a);
            }
        }
        let want = midgame()["states"].as_array().expect("states");
        assert_eq!(out.len(), want.len(), "midgame state count");
        for (i, (st, w)) in out.iter().zip(want).enumerate() {
            assert_eq!(
                state_digest(st),
                w["digest"].as_str().expect("a digest"),
                "midgame state {i}"
            );
        }
        Mutex::new(out)
    });
    states.lock().expect("not poisoned").clone()
}

/// `midgame_states()[index]`, a fresh clone.
pub fn midgame_state(index: usize) -> GameState {
    midgame_states().swap_remove(index)
}

/// The Python snapshot record of mid-game state `index` (`midgame.json`).
pub fn midgame_record(index: usize) -> &'static Value {
    &midgame()["states"][index]
}

/// `f(i)` for `i` in `0..n` on all cores, results in index order. Each `i` is one Python test
/// case, so the cases run side by side as pytest-xdist would run them.
pub fn par_map<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let v = f(i);
                    *slots[i].lock().expect("not poisoned") = Some(v);
                }
            });
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().expect("not poisoned").expect("computed"))
        .collect()
}
