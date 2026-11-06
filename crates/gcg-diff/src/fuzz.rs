//! `gcg-diff fuzz` (PLAN §9.4): the Rust version of `tools/fuzz.py:28-166` (the decks, seeds, random
//! fuzz agent, per-game records and summary JSON, invariants after every action), plus the
//! strategic agents of W-110 (`--agent greedy|ref:<preset>|search:<preset>|mixed:A,B`).
//!
//! With the default agent the games are Python's: `deck_for` draws from
//! `SplitMix64(derive_seed(seed, "fuzz-deck", index))`, the game seed is
//! `derive_seed(seed, "fuzz-game", index)` and the agent draws
//! `randrange(len(options))` from `SplitMix64(derive_seed(seed, "fuzz-agent", index))`
//! (`tools/fuzz.py:47-119`). A strategic agent of seat `p` is seeded with
//! `derive_seed(seed, "fuzz-agent", index, p)`; `mixed:A,B` seats A at 0 and B at 1 in even
//! games and swaps them in odd ones. Results are stored by game index, so the output is the same
//! for any `--workers`.

use std::num::NonZeroUsize;

use gcg_ai::{Agent, AgentCx, DEFAULT_WEIGHTS, Scratch};
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::pyjson::{DumpOptions, Json};
use gcg_core::rng::SplitMix64;
use gcg_data::{CardDb, CardType, Color, MAIN_DECK_TYPES};
use gcg_effects::Registry;
use gcg_engine::{EndReason, GameState, NewGameOptions, invariants};
use serde_json::{Value, json};

use crate::engine::{apply_error, new_game_error};

/// `resource_pool()` (`tools/fuzz.py:28-30`): the sorted numbers of the real Resource cards.
#[must_use]
pub fn resource_pool(db: &CardDb) -> Vec<&str> {
    let mut v: Vec<&str> = db
        .real_cards()
        .filter(|c| c.card_type == CardType::Resource)
        .map(|c| &*c.card_number)
        .collect();
    v.sort();
    v
}

/// `main_deck_pool()` (`tools/fuzz.py:33-44`): per colour, the sorted numbers of the real
/// main-deck cards with a script, colours in `Color` declaration order.
#[must_use]
pub fn main_deck_pool(reg: &Registry) -> Vec<(Color, Vec<&str>)> {
    let db = reg.db();
    Color::ALL
        .iter()
        .map(|&color| {
            let mut v: Vec<&str> = db
                .real_cards()
                .filter(|c| {
                    MAIN_DECK_TYPES.contains(&c.card_type)
                        && c.color == Some(color)
                        && reg.card(c.def_id).script.is_some()
                })
                .map(|c| &*c.card_number)
                .collect();
            v.sort();
            (color, v)
        })
        .collect()
}

/// A fuzz deck: main numbers in the order `deck_for` builds them, and 10 resource numbers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuzzDeck {
    pub main: Vec<String>,
    pub resources: Vec<String>,
}

/// The pools `deck_for` reads, computed once.
pub struct Pools<'a> {
    /// Colours sorted by value (`sorted(pool, key=lambda c: c.value)`), with their cards.
    colors: Vec<(Color, Vec<&'a str>)>,
    resources: Vec<&'a str>,
}

impl<'a> Pools<'a> {
    #[must_use]
    pub fn new(reg: &'a Registry) -> Self {
        let mut colors = main_deck_pool(reg);
        colors.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        Self {
            colors,
            resources: resource_pool(reg.db()),
        }
    }

    /// Every number of the main-deck and resource pools (`tools/fuzz.py:149`).
    #[must_use]
    pub fn all(&self) -> Vec<&'a str> {
        let mut v: Vec<&str> = self
            .colors
            .iter()
            .flat_map(|(_, ns)| ns.iter().copied())
            .chain(self.resources.iter().copied())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// `deck_for(index, seed)` (`tools/fuzz.py:47-77`).
    #[must_use]
    pub fn deck_for(&self, index: u64, seed: u64) -> FuzzDeck {
        let n = self.colors.len() as u64;
        let mut rng = SplitMix64::new(derive_seed!(seed, "fuzz-deck", index_part(index)));
        let c1 = usize::try_from(index % n).unwrap_or(0);
        let c2 = usize::try_from((index / n + index) % n).unwrap_or(0);
        let mut cards: Vec<&str> = self.colors[c1].1.clone();
        if c2 != c1 {
            cards.extend(self.colors[c2].1.iter().copied());
        }
        let len = cards.len().max(1) as u64;
        let start = usize::try_from((index / n) * 13 % len).unwrap_or(0);
        let rotated: Vec<&str> = cards[start..]
            .iter()
            .chain(&cards[..start])
            .copied()
            .collect();
        let mut main: Vec<&str> = Vec::with_capacity(50);
        for &card in &rotated {
            if main.len() >= 50 {
                break;
            }
            let copies = usize::try_from(1 + rng.randrange(4)).unwrap_or(1);
            let k = copies.min(50 - main.len());
            main.extend(std::iter::repeat_n(card, k));
        }
        let mut i = 0;
        while main.len() < 50 {
            let card = rotated[i % rotated.len()];
            if main.iter().filter(|&&x| x == card).count() < 4 {
                main.push(card);
            }
            i += 1;
        }
        let res = &self.resources;
        let r0 = usize::try_from(index * 10 % res.len() as u64).unwrap_or(0);
        let resources: Vec<String> = res[r0..]
            .iter()
            .chain(&res[..r0])
            .take(10)
            .map(|s| (*s).to_owned())
            .collect();
        FuzzDeck {
            main: main.into_iter().map(str::to_owned).collect(),
            resources,
        }
    }
}

fn index_part(index: u64) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

/// The player of a fuzz game.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AgentKind {
    /// The fuzz agent of `tools/fuzz.py:94-99`.
    Fuzz,
    Random,
    Greedy,
    /// `ref:<preset>`: the reference search at a reference preset.
    Reference(String),
    /// `search:<preset>`: the strengthened search at a shipped preset.
    Search(gcg_search::Preset),
}

/// `--agent` (PLAN §8.7): the agent of both seats, or `mixed:A,B`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AgentSpec {
    Both(AgentKind),
    Mixed(AgentKind, AgentKind),
}

fn parse_kind(s: &str) -> Result<AgentKind, String> {
    if let Some(p) = s.strip_prefix("ref:") {
        return match gcg_ai_reference::reference_preset(p) {
            Some(_) => Ok(AgentKind::Reference(p.to_owned())),
            None => Err(format!(
                "unknown reference preset {p:?}: choose from fast, standard, strong"
            )),
        };
    }
    if let Some(p) = s.strip_prefix("search:") {
        if p.contains(':') {
            return Err(format!(
                "search:{p}: field overrides are parsed by gcg-eval's agent grammar, not by gcg-diff fuzz"
            ));
        }
        return gcg_search::Preset::from_value(p)
            .map(AgentKind::Search)
            .ok_or_else(|| format!("unknown preset {p:?}: choose from standard, fast"));
    }
    match s {
        "random" => Ok(AgentKind::Random),
        "greedy" => Ok(AgentKind::Greedy),
        "fuzz" => Ok(AgentKind::Fuzz),
        other => Err(format!(
            "unknown agent {other:?}: random, greedy, ref:<preset>, search:<preset> or mixed:A,B"
        )),
    }
}

/// Parses `--agent`.
///
/// # Errors
/// An unknown agent or preset.
pub fn parse_agent(s: &str) -> Result<AgentSpec, String> {
    if let Some(rest) = s.strip_prefix("mixed:") {
        let (a, b) = rest
            .split_once(',')
            .ok_or_else(|| "mixed:A,B needs two agents".to_owned())?;
        let (a, b) = (parse_kind(a)?, parse_kind(b)?);
        if a == AgentKind::Fuzz || b == AgentKind::Fuzz {
            return Err("the fuzz agent cannot be mixed".to_owned());
        }
        return Ok(AgentSpec::Mixed(a, b));
    }
    parse_kind(s).map(AgentSpec::Both)
}

fn make_agent(kind: &AgentKind, seed: u64) -> Result<Box<dyn Agent>, String> {
    Ok(match kind {
        AgentKind::Random => gcg_ai::make_random_agent(seed, false),
        AgentKind::Greedy => gcg_ai::make_greedy_agent(seed, false, (*DEFAULT_WEIGHTS).clone()),
        AgentKind::Reference(p) => {
            let cfg = gcg_ai_reference::reference_preset(p)
                .ok_or_else(|| format!("unknown reference preset {p:?}"))?;
            gcg_ai_reference::make_reference_agent(cfg, seed, false, DEFAULT_WEIGHTS)
        }
        AgentKind::Search(p) => gcg_search::make_agent(gcg_search::preset_config(*p), seed, false),
        AgentKind::Fuzz => return Err("the fuzz agent is not an Agent".to_owned()),
    })
}

/// One game's record (`play_one`'s `out`, `tools/fuzz.py:80-120`).
#[derive(Clone, PartialEq, Debug)]
pub struct GameOut {
    pub index: u64,
    pub seed: u64,
    pub cards: Vec<String>,
    pub ok: bool,
    /// `actions` (finished games) or `actions_so_far` (failed ones).
    pub actions: u64,
    pub end: Option<String>,
    pub turns: Option<i32>,
    pub error: Option<String>,
    pub last_actions: Vec<Value>,
    /// Whether the game finished (it has `actions`, `end` and `turns`).
    pub finished: bool,
}

impl GameOut {
    /// The per-game row of `oracle/traces.py` `_summary_row`: `index`, `seed`, `ok`,
    /// `actions`, `end`, `turns`, `error` where present.
    #[must_use]
    pub fn row(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("index".to_owned(), Value::from(self.index));
        m.insert("seed".to_owned(), Value::from(self.seed));
        m.insert("ok".to_owned(), Value::from(self.ok));
        if self.finished {
            m.insert("actions".to_owned(), Value::from(self.actions));
            m.insert(
                "end".to_owned(),
                Value::from(self.end.clone().unwrap_or_default()),
            );
            m.insert("turns".to_owned(), Value::from(self.turns.unwrap_or(0)));
        }
        if let Some(e) = &self.error {
            m.insert("error".to_owned(), Value::from(e.as_str()));
        }
        Value::Object(m)
    }
}

/// The options of one fuzz run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuzzArgs {
    pub games: u64,
    pub seed: u64,
    pub workers: NonZeroUsize,
    pub agent: AgentSpec,
}

fn action_json(a: &gcg_engine::Action) -> Value {
    json!([a.kind.as_str(), a.a, a.b, a.c])
}

fn choose(
    st: &GameState,
    agents: &mut Option<[Box<dyn Agent>; 2]>,
    rng: &mut SplitMix64,
) -> Result<gcg_engine::Action, String> {
    let dec = st.pending();
    let Some(agents) = agents else {
        let opts = dec.map_or(&[][..], |d| d.options);
        if opts.is_empty() {
            return Err("ValueError: randrange() requires n > 0".to_owned());
        }
        return Ok(opts[rng.randrange_usize(opts.len())]);
    };
    let p = dec
        .map(|d| d.hdr.player)
        .ok_or_else(|| "ValueError: no decision is pending".to_owned())?;
    let agent = &mut agents[usize::from(p)];
    let cancel = Cancel::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        agent.choose(st, p, &mut cx)
    })
    .map_err(|e| format!("{}: {e:?}", e.py_class()))
}

/// `play_one(index, seed)` (`tools/fuzz.py:80-120`) with invariants after every action.
#[must_use]
pub fn play_one(
    reg: &std::sync::Arc<Registry>,
    pools: &Pools<'_>,
    index: u64,
    a: &FuzzArgs,
) -> GameOut {
    let d0 = pools.deck_for(2 * index, a.seed);
    let d1 = pools.deck_for(2 * index + 1, a.seed);
    let game_seed = derive_seed!(a.seed, "fuzz-game", index_part(index));
    let mut rng = SplitMix64::new(derive_seed!(a.seed, "fuzz-agent", index_part(index)));
    let mut cards: Vec<String> = d0
        .main
        .iter()
        .chain(&d1.main)
        .chain(&d0.resources)
        .chain(&d1.resources)
        .cloned()
        .collect();
    cards.sort();
    cards.dedup();
    let mut out = GameOut {
        index,
        seed: game_seed,
        cards,
        ok: false,
        actions: 0,
        end: None,
        turns: None,
        error: None,
        last_actions: Vec::new(),
        finished: false,
    };
    let mut actions: Vec<Value> = Vec::new();
    let result = (|| -> Result<GameState, String> {
        let kinds = match &a.agent {
            AgentSpec::Both(AgentKind::Fuzz) => None,
            AgentSpec::Both(k) => Some([k.clone(), k.clone()]),
            AgentSpec::Mixed(x, y) if index.is_multiple_of(2) => Some([x.clone(), y.clone()]),
            AgentSpec::Mixed(x, y) => Some([y.clone(), x.clone()]),
        };
        let mut agents = match kinds {
            None => None,
            Some([k0, k1]) => Some([
                make_agent(
                    &k0,
                    derive_seed!(a.seed, "fuzz-agent", index_part(index), 0_i64),
                )?,
                make_agent(
                    &k1,
                    derive_seed!(a.seed, "fuzz-agent", index_part(index), 1_i64),
                )?,
            ]),
        };
        let (m0, r0) = deck_strs(&d0);
        let (m1, r1) = deck_strs(&d1);
        let mut st = gcg_engine::new_game_by_number(
            reg,
            [(&m0, &r0), (&m1, &r1)],
            game_seed,
            NewGameOptions::default(),
        )
        .map_err(|e| {
            let r = new_game_error(&e);
            format!("{}: {}", r.class, r.message)
        })?;
        let init = invariants::initial_multiset(&st);
        while st.winner().is_none() {
            let action = choose(&st, &mut agents, &mut rng)?;
            actions.push(action_json(&action));
            gcg_engine::apply(&mut st, action, true).map_err(|e| {
                let r = apply_error(&e);
                format!("{}: {}", r.class, r.message)
            })?;
            invariants::check(&st, Some(&init)).map_err(|e| check_text(&e))?;
        }
        invariants::check(&st, Some(&init)).map_err(|e| check_text(&e))?;
        Ok(st)
    })();
    match result {
        Ok(st) => {
            out.finished = true;
            out.ok = true;
            out.actions = actions.len() as u64;
            out.end = Some(st.end_reason().map_or("none", EndReason::as_str).to_owned());
            out.turns = Some(st.turn());
            if st.end_reason() == Some(EndReason::TurnLimit) {
                out.ok = false;
                out.error = Some(
                    "game ended by the engine safety cap (not a rules-defined end)".to_owned(),
                );
            }
        }
        Err(e) => {
            out.error = Some(e);
            out.actions = actions.len() as u64;
            let from = actions.len().saturating_sub(5);
            out.last_actions = actions[from..].to_vec();
        }
    }
    out
}

fn deck_strs(d: &FuzzDeck) -> (Vec<&str>, Vec<&str>) {
    (
        d.main.iter().map(String::as_str).collect(),
        d.resources.iter().map(String::as_str).collect(),
    )
}

fn check_text(e: &gcg_engine::CheckError) -> String {
    match e {
        gcg_engine::CheckError::Violation(v) => format!("InvariantViolation: {v}"),
        gcg_engine::CheckError::Engine(x) => format!("{}: {x}", x.py_class()),
    }
}

/// Every game of a run, in index order, whatever the worker count.
///
/// # Errors
/// A worker panicked or the run was interrupted.
pub fn run(a: &FuzzArgs) -> Result<Vec<GameOut>, String> {
    let reg = Registry::embedded();
    let pools = Pools::new(&reg);
    let n = usize::try_from(a.games).map_err(|_| "too many games".to_owned())?;
    let cancel = Cancel::new();
    Pool::new(a.workers)
        .map_indexed(
            n,
            &cancel,
            |i, _scratch: &mut Scratch, _cx: &TaskCx<'_, Scratch>| {
                Ok::<GameOut, String>(play_one(&reg, &pools, i as u64, a))
            },
            |_| {},
        )
        .map_err(|e| format!("{e:?}"))
}

/// The summary `tools/fuzz.py:149-159` prints: `json.dumps(summary, indent=1)` and a newline.
#[must_use]
pub fn summary_text(games: &[GameOut], pools: &Pools<'_>) -> String {
    let mut actions: u64 = 0;
    let mut ends: Vec<(String, u64)> = Vec::new();
    let mut failures: u64 = 0;
    let mut seen: Vec<&str> = Vec::new();
    for g in games {
        seen.extend(g.cards.iter().map(String::as_str));
        if g.ok {
            actions += g.actions;
            let end = g.end.clone().unwrap_or_default();
            match ends.iter_mut().find(|(e, _)| *e == end) {
                Some(slot) => slot.1 += 1,
                None => ends.push((end, 1)),
            }
        } else {
            failures += 1;
        }
    }
    seen.sort();
    seen.dedup();
    let pool = pools.all();
    let covered = pool
        .iter()
        .filter(|n| seen.binary_search(n).is_ok())
        .count();
    let missing: Vec<Json> = pool
        .iter()
        .filter(|n| seen.binary_search(n).is_err())
        .take(20)
        .map(|n| Json::from(*n))
        .collect();
    let mut ends_json = Json::object();
    for (e, n) in ends {
        ends_json.set(e, n);
    }
    let mut summary = Json::object();
    summary.set("games", games.len());
    summary.set("actions", actions);
    summary.set("ends", ends_json);
    summary.set("failures", failures);
    summary.set("cards_covered", covered);
    summary.set("cards_in_pool", pool.len());
    summary.set("cards_missing", Json::Array(missing));
    let opts = DumpOptions {
        indent: Some(1),
        sort_keys: false,
        compact: false,
        ensure_ascii: true,
    };
    gcg_core::pyjson::dumps(&summary, opts) + "\n"
}

/// `--compare FILE`: the summary text byte for byte, and with `FILE`'s sibling
/// `<stem>.games.jsonl` (`oracle/traces.py`) every per-game row.
///
/// # Errors
/// The first difference.
pub fn compare(text: &str, games: &[GameOut], file: &std::path::Path) -> Result<(), String> {
    let want = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let rows_path = file
        .to_str()
        .and_then(|s| s.strip_suffix(".json"))
        .map(|s| std::path::PathBuf::from(format!("{s}.games.jsonl")));
    if let Some(p) = rows_path.filter(|p| p.exists()) {
        let rows = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let lines: Vec<&str> = rows.lines().collect();
        if lines.len() != games.len() {
            return Err(format!(
                "{}: {} games, this run {}",
                p.display(),
                lines.len(),
                games.len()
            ));
        }
        for (g, l) in games.iter().zip(lines) {
            let got = crate::json::py_dumps(&g.row());
            if got != l {
                return Err(format!(
                    "game {} differs from {}:\n  python {l}\n  rust   {got}",
                    g.index,
                    p.display()
                ));
            }
        }
    }
    if want == text {
        Ok(())
    } else {
        Err(format!(
            "the summary differs from {}:\n--- python\n{want}--- rust\n{text}",
            file.display()
        ))
    }
}

/// The failure lines `tools/fuzz.py:160-165` prints to stderr (no Python traceback).
#[must_use]
pub fn failure_lines(games: &[GameOut], show: usize) -> Vec<String> {
    games
        .iter()
        .filter(|g| !g.ok)
        .take(show)
        .map(|g| {
            let mut m = Json::object();
            m.set("index", g.index);
            m.set("seed", g.seed);
            m.set("error", g.error.clone());
            let last: Vec<Json> = g
                .last_actions
                .iter()
                .map(gcg_runner::reports::json_from_value)
                .collect();
            m.set(
                "last_actions",
                if g.finished {
                    Json::Null
                } else {
                    Json::Array(last)
                },
            );
            gcg_core::pyjson::dumps(
                &m,
                DumpOptions {
                    indent: None,
                    sort_keys: false,
                    compact: false,
                    ensure_ascii: true,
                },
            )
        })
        .collect()
}

/// `--seed-from-commit SHA`: `derive_seed(0, "fuzz-commit", sha)`.
#[must_use]
pub fn commit_seed(sha: &str) -> u64 {
    derive_seed!(0, "fuzz-commit", sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_parse_like_the_plan_grammar() {
        assert_eq!(
            parse_agent("greedy"),
            Ok(AgentSpec::Both(AgentKind::Greedy))
        );
        assert_eq!(
            parse_agent("ref:fast"),
            Ok(AgentSpec::Both(AgentKind::Reference("fast".to_owned())))
        );
        assert_eq!(
            parse_agent("mixed:greedy,search:fast"),
            Ok(AgentSpec::Mixed(
                AgentKind::Greedy,
                AgentKind::Search(gcg_search::Preset::Fast)
            ))
        );
        assert!(parse_agent("ref:huge").is_err());
        assert!(parse_agent("search:strong").is_err());
        assert!(parse_agent("search:fast:exploration=1").is_err());
        assert!(parse_agent("mixed:greedy").is_err());
        assert!(parse_agent("minimax").is_err());
    }

    #[test]
    fn fuzz_decks_are_pythons() {
        let reg = Registry::embedded();
        let pools = Pools::new(&reg);
        assert_eq!(pools.all().len(), 1194);
        // The 2026-seed summary covers every pool card (tier-1 fuzz summary).
        for (index, seed) in [(0, 1), (1, 1), (7, 5), (12, 2026)] {
            let d = pools.deck_for(index, seed);
            assert_eq!(d.main.len(), 50);
            assert_eq!(d.resources.len(), 10);
            for n in &d.main {
                assert!(d.main.iter().filter(|x| *x == n).count() <= 4);
            }
        }
    }

    #[test]
    fn the_summary_of_no_games_is_pythons_format() {
        let reg = Registry::embedded();
        let pools = Pools::new(&reg);
        let text = summary_text(&[], &pools);
        assert!(
            text.starts_with("{\n \"games\": 0,\n \"actions\": 0,\n \"ends\": {},"),
            "{text}"
        );
        assert!(text.ends_with("]\n}\n"), "{text}");
    }
}
