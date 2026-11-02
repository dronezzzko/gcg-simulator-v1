//! `gcg-eval gates` (docs/port/PLAN.md §12.6, A25, A26; W-19): the final-evaluation gates E1-E4
//! computed from `gcg-eval head-to-head --out` reports.
//!
//! The statistics are recomputed from each report's `outcomes` rows (`[index, pair,
//! challenger, winner, ...]`; the challenger won when `winner == challenger`, so draws are
//! non-wins, W-93), grouped by pair with the `by_pair` rows' names and held-out flags, which
//! must agree with `docs/port/eval/panel.json` when it exists. Wilson 95% bounds are
//! `reports/stats.py`'s (z = 1.96), compared unrounded with the bars.
//!
//! | Gate | Comparison | Reports | Bars |
//! | --- | --- | --- | --- |
//! | E1 | `search:standard` vs `ref:strong` | panel | pooled-all lower >= 0.55; pooled held-out lower >= 0.55; no held-out pair upper < 0.5 |
//! | E2 | `search:fast` vs `ref:standard` | panel | pooled point >= 0.5, all and held-out; no pair upper < 0.5 |
//! | E3 | `search:standard` vs `random` | tests and panel | every game won (tests, and each panel pair, held-out pairs listed on their own), or every loss documented with an existing replay |
//! | E4 | `search:standard` vs `greedy` | tests and panel | tests lower > 0.764; pooled lower > 0.764 over all pairs and over the held-out pairs |
//!
//! Each report is checked against its gate before its numbers count: the agent specs, the
//! deck set (`panel` with all 28 pairs and 13 held-out pairs, or `tests`), at least 400 games
//! per pair, `--choose-first seat0`, and an evaluation-role seed (`eval` or `review`, A14).
//! With `--check` every gate needs its reports and a failed or missing gate exits 1.

use std::path::{Path, PathBuf};

use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_data::py_path_str;
use gcg_runner::reports::stats::{Z95, wilson};
use serde_json::Value;

use crate::bench_cmd::CommandError;
use crate::decks::{PANEL_FILE, Panel};
use crate::stats::RATE_DIGITS;

/// The report format.
pub const GATES_FORMAT: &str = "gcg-eval-gates";
pub const GATES_VERSION: u32 = 1;
/// Games per pair of a gate run (§12.6).
pub const MIN_GAMES_PER_PAIR: u64 = 400;
/// Pairs of the panel and its held-out pairs (§12.5).
pub const PANEL_PAIRS: usize = 28;
pub const PANEL_HELD_OUT_PAIRS: usize = 13;
/// E1's lower-bound bar.
pub const E1_BAR: f64 = 0.55;
/// The greedy bar of E4 (the reference's Wilson upper bound, `BRIEF.md:131`).
pub const E4_BAR: f64 = 0.764;

/// The parsed arguments.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GatesArgs {
    pub e1: Option<PathBuf>,
    pub e2: Option<PathBuf>,
    pub e3_tests: Option<PathBuf>,
    pub e3_panel: Option<PathBuf>,
    pub e4_tests: Option<PathBuf>,
    pub e4_panel: Option<PathBuf>,
    pub losses: Option<PathBuf>,
    pub check: bool,
    pub out: Option<PathBuf>,
}

/// One pair of a report: names, held-out flag, the challenger's wins and games, and its losses
/// (game indices).
#[derive(Clone, PartialEq, Debug)]
pub struct PairStats {
    pub decks: [String; 2],
    pub held_out: bool,
    pub wins: u64,
    pub n: u64,
    pub losses: Vec<u64>,
}

/// A report, read and regrouped.
#[derive(Clone, PartialEq, Debug)]
pub struct Report {
    pub path: String,
    pub challenger: String,
    pub opponent: String,
    pub decks: Option<String>,
    pub pairs_value: Option<String>,
    pub role: Option<String>,
    pub choose_first: Option<String>,
    pub pairs: Vec<PairStats>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

impl Report {
    /// Reads a head-to-head report.
    ///
    /// # Errors
    /// The file is unreadable or not a head-to-head report with outcomes.
    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = py_path_str(path);
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the report {shown}: {e}"))?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
        Self::from_value(&v, &shown).map_err(|e| format!("{shown}: {e}"))
    }

    /// A report from its parsed JSON.
    ///
    /// # Errors
    /// Not a head-to-head report with `by_pair` and `outcomes`.
    pub fn from_value(v: &Value, path: &str) -> Result<Self, String> {
        if v.get("format").and_then(Value::as_str) != Some(crate::head_to_head::REPORT_FORMAT) {
            return Err("not a gcg-eval head-to-head report".to_owned());
        }
        let rows = v
            .get("by_pair")
            .and_then(Value::as_array)
            .ok_or("no by_pair rows")?;
        let mut pairs = Vec::new();
        let mut positions = Vec::new();
        for r in rows {
            let decks = r
                .get("decks")
                .and_then(Value::as_array)
                .ok_or("a by_pair row without decks")?;
            let name = |i: usize| {
                decks
                    .get(i)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            positions.push(
                r.get("position")
                    .and_then(Value::as_u64)
                    .ok_or("a by_pair row without position")?,
            );
            pairs.push(PairStats {
                decks: [name(0), name(1)],
                held_out: r.get("held_out").and_then(Value::as_bool).unwrap_or(false),
                wins: 0,
                n: 0,
                losses: Vec::new(),
            });
        }
        let outcomes = v
            .get("outcomes")
            .and_then(Value::as_array)
            .ok_or("no outcomes (write the report with --out)")?;
        for o in outcomes {
            let row = o.as_array().ok_or("an outcome is not a list")?;
            let int = |i: usize| row.get(i).and_then(Value::as_i64);
            let (Some(index), Some(pos), Some(chal), Some(winner)) =
                (int(0), int(1), int(2), int(3))
            else {
                return Err("an outcome row has no [index, pair, challenger, winner]".to_owned());
            };
            let k = positions
                .iter()
                .position(|p| i64::try_from(*p).ok() == Some(pos))
                .ok_or_else(|| {
                    format!("outcome {index} names pair {pos}, which has no by_pair row")
                })?;
            pairs[k].n += 1;
            if winner == chal {
                pairs[k].wins += 1;
            } else {
                pairs[k].losses.push(u64::try_from(index).unwrap_or(0));
            }
        }
        Ok(Self {
            path: path.to_owned(),
            challenger: s(v, "challenger").unwrap_or_default(),
            opponent: s(v, "opponent").unwrap_or_default(),
            decks: s(v, "decks"),
            pairs_value: s(v, "pairs"),
            role: s(v, "role"),
            choose_first: s(v, "choose_first"),
            pairs,
        })
    }

    fn totals(&self, held_only: bool) -> (u64, u64) {
        self.pairs
            .iter()
            .filter(|p| !held_only || p.held_out)
            .fold((0, 0), |(w, n), p| (w + p.wins, n + p.n))
    }
}

fn wilson95(w: u64, n: u64) -> (f64, f64) {
    wilson(w, n, Z95).unwrap_or((0.0, 1.0))
}

#[expect(clippy::cast_precision_loss, reason = "game counts are far below 2^53")]
fn rate(w: u64, n: u64) -> Option<f64> {
    (n > 0).then(|| w as f64 / n as f64)
}

/// One check of a gate.
#[derive(Clone, PartialEq, Debug)]
pub struct Check {
    pub name: String,
    pub value: Json,
    pub bar: String,
    pub pass: bool,
}

impl Check {
    fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("check", self.name.as_str());
        o.set("value", self.value.clone());
        o.set("bar", self.bar.as_str());
        o.set("pass", self.pass);
        o
    }
}

fn r4(x: f64) -> Json {
    round_f64(x, RATE_DIGITS).into()
}

fn opt_r4(x: Option<f64>) -> Json {
    x.map_or(Json::Null, r4)
}

/// The validity checks of a report for gate `expected` (specs, deck set, games, protocol,
/// role, and the held-out split).
#[must_use]
pub fn validity(r: &Report, spec: (&str, &str), panel: bool, split: Option<&Panel>) -> Vec<Check> {
    let mut out = Vec::new();
    let mut add = |name: &str, value: Json, bar: &str, pass: bool| {
        out.push(Check {
            name: name.to_owned(),
            value,
            bar: bar.to_owned(),
            pass,
        });
    };
    let specs_ok = r.challenger == spec.0 && r.opponent == spec.1;
    add(
        "agents",
        format!("{} vs {}", r.challenger, r.opponent).into(),
        &format!("{} vs {}", spec.0, spec.1),
        specs_ok,
    );
    let set = if panel { "panel" } else { "tests" };
    add(
        "deck set",
        r.decks.clone().map_or(Json::Null, Json::from),
        set,
        r.decks.as_deref() == Some(set) && r.pairs_value.as_deref() == Some("all"),
    );
    let n_pairs = r.pairs.len();
    let held = r.pairs.iter().filter(|p| p.held_out).count();
    if panel {
        add(
            "pairs",
            n_pairs.into(),
            &format!("{PANEL_PAIRS} pairs, {PANEL_HELD_OUT_PAIRS} held out"),
            n_pairs == PANEL_PAIRS && held == PANEL_HELD_OUT_PAIRS,
        );
        if let Some(p) = split {
            let agree = r
                .pairs
                .iter()
                .all(|x| p.is_held_out(&x.decks[0], &x.decks[1]) == x.held_out);
            add(
                "held-out split",
                agree.into(),
                &format!("the flags of {PANEL_FILE}"),
                agree,
            );
        }
    }
    let min_games = r.pairs.iter().map(|p| p.n).min().unwrap_or(0);
    add(
        "games per pair",
        min_games.into(),
        &format!(">= {MIN_GAMES_PER_PAIR}"),
        min_games >= MIN_GAMES_PER_PAIR,
    );
    add(
        "choose first",
        r.choose_first.clone().map_or(Json::Null, Json::from),
        "seat0 (A24)",
        r.choose_first.as_deref() == Some("seat0"),
    );
    add(
        "seed role",
        r.role.clone().map_or(Json::Null, Json::from),
        "eval or review (A14)",
        matches!(r.role.as_deref(), Some("eval" | "review")),
    );
    out
}

/// A documented loss: report path (as given to `gates`) and game index, with its replay.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Loss {
    pub report: String,
    pub index: u64,
    pub replay: String,
}

/// Reads `{"losses": [{"report": PATH, "index": N, "replay": PATH, ...}, ...]}`.
///
/// # Errors
/// The file is unreadable or malformed.
pub fn load_losses(path: &Path) -> Result<Vec<Loss>, String> {
    let shown = py_path_str(path);
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
    let items = v
        .get("losses")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{shown}: \"losses\" must be a list"))?;
    items
        .iter()
        .map(|l| {
            let report = s(l, "report");
            let replay = s(l, "replay");
            let index = l.get("index").and_then(Value::as_u64);
            match (report, index, replay) {
                (Some(report), Some(index), Some(replay)) => Ok(Loss {
                    report,
                    index,
                    replay,
                }),
                _ => Err(format!("{shown}: a loss needs report, index and replay")),
            }
        })
        .collect()
}

fn documented(r: &Report, index: u64, losses: &[Loss]) -> bool {
    losses
        .iter()
        .any(|l| l.report == r.path && l.index == index && Path::new(&l.replay).is_file())
}

/// E1's or E2's statistic checks on a panel report.
#[must_use]
pub fn e1_e2(r: &Report, e1: bool) -> Vec<Check> {
    let mut out = Vec::new();
    for (label, held) in [("all pairs", false), ("held-out pairs", true)] {
        let (w, n) = r.totals(held);
        let (lo, _) = wilson95(w, n);
        if e1 {
            out.push(Check {
                name: format!("pooled lower bound, {label}"),
                value: r4(lo),
                bar: format!(">= {E1_BAR}"),
                pass: n > 0 && lo >= E1_BAR,
            });
        } else {
            let p = rate(w, n);
            out.push(Check {
                name: format!("pooled point estimate, {label}"),
                value: opt_r4(p),
                bar: ">= 0.5".to_owned(),
                pass: p.is_some_and(|p| p >= 0.5),
            });
        }
    }
    let bad: Vec<String> = r
        .pairs
        .iter()
        .filter(|p| !e1 || p.held_out)
        .filter(|p| wilson95(p.wins, p.n).1 < 0.5)
        .map(|p| format!("{}:{}", p.decks[0], p.decks[1]))
        .collect();
    out.push(Check {
        name: if e1 {
            "held-out pairs with upper bound < 0.5".to_owned()
        } else {
            "pairs with upper bound < 0.5".to_owned()
        },
        value: Json::Array(bad.iter().map(|b| b.as_str().into()).collect()),
        bar: "none".to_owned(),
        pass: bad.is_empty(),
    });
    out
}

/// E3's checks on one report: every game won, or every loss documented.
#[must_use]
pub fn e3(r: &Report, losses: &[Loss]) -> Vec<Check> {
    let mut out = Vec::new();
    for p in &r.pairs {
        let undocumented: Vec<u64> = p
            .losses
            .iter()
            .copied()
            .filter(|i| !documented(r, *i, losses))
            .collect();
        let mut value = Json::object();
        value.set("wins", p.wins);
        value.set("n", p.n);
        value.set(
            "losses",
            Json::Array(p.losses.iter().map(|i| Json::from(*i)).collect()),
        );
        value.set(
            "undocumented",
            Json::Array(undocumented.iter().map(|i| Json::from(*i)).collect()),
        );
        out.push(Check {
            name: format!(
                "{}:{}{}",
                p.decks[0],
                p.decks[1],
                if p.held_out { " (held out)" } else { "" }
            ),
            value,
            bar: "every game won, or every loss documented with its replay".to_owned(),
            pass: p.n > 0 && undocumented.is_empty(),
        });
    }
    out
}

/// E4's lower-bound checks: the whole report, then (panel) the held-out pairs alone.
#[must_use]
pub fn e4(r: &Report, panel: bool) -> Vec<Check> {
    let mut groups = vec![("pooled lower bound, all pairs", false)];
    if panel {
        groups.push(("pooled lower bound, held-out pairs", true));
    }
    groups
        .into_iter()
        .map(|(label, held)| {
            let (w, n) = r.totals(held);
            let (lo, _) = wilson95(w, n);
            Check {
                name: label.to_owned(),
                value: r4(lo),
                bar: format!("> {E4_BAR}"),
                pass: n > 0 && lo > E4_BAR,
            }
        })
        .collect()
}

/// One report's checks: missing (`None`), unreadable, or checked.
type Part = Option<Result<Vec<Check>, String>>;

/// One gate's block: its inputs, checks and verdict (`null` when a report is missing).
fn gate(id: &str, parts: Vec<(&str, Part)>) -> (Json, Option<bool>) {
    let mut o = Json::object();
    o.set("gate", id);
    let mut verdict = Some(true);
    let mut reports = Vec::new();
    for (label, part) in parts {
        let mut r = Json::object();
        r.set("report", label);
        match part {
            None => {
                r.set("status", "missing");
                verdict = None;
            }
            Some(Err(e)) => {
                r.set("status", "unreadable");
                r.set("error", e.as_str());
                verdict = verdict.map(|_| false);
            }
            Some(Ok(checks)) => {
                let pass = checks.iter().all(|c| c.pass);
                r.set("status", if pass { "pass" } else { "fail" });
                r.set(
                    "checks",
                    Json::Array(checks.iter().map(Check::to_json).collect()),
                );
                if !pass {
                    verdict = verdict.map(|_| false);
                }
            }
        }
        reports.push(r);
    }
    o.set("reports", Json::Array(reports));
    o.set("pass", verdict);
    (o, verdict)
}

/// The gates document and the ids of the gates that failed or are missing.
///
/// # Errors
/// An unreadable loss file or held-out split.
pub fn evaluate(args: &GatesArgs) -> Result<(Json, Vec<String>), String> {
    let split = if Path::new(PANEL_FILE).exists() {
        Some(Panel::load(Path::new(PANEL_FILE))?)
    } else {
        None
    };
    let losses = match &args.losses {
        Some(p) => load_losses(p)?,
        None => Vec::new(),
    };
    let load = |p: &Option<PathBuf>| p.as_ref().map(|p| Report::load(p));
    let with = |r: Option<Result<Report, String>>,
                spec: (&str, &str),
                panel: bool,
                stats: &dyn Fn(&Report) -> Vec<Check>| {
        r.map(|r| {
            r.map(|r| {
                let mut c = validity(&r, spec, panel, split.as_ref());
                c.extend(stats(&r));
                c
            })
        })
    };
    let e1 = with(
        load(&args.e1),
        ("search:standard", "ref:strong"),
        true,
        &|r| e1_e2(r, true),
    );
    let e2 = with(
        load(&args.e2),
        ("search:fast", "ref:standard"),
        true,
        &|r| e1_e2(r, false),
    );
    let e3t = with(
        load(&args.e3_tests),
        ("search:standard", "random"),
        false,
        &|r| e3(r, &losses),
    );
    let e3p = with(
        load(&args.e3_panel),
        ("search:standard", "random"),
        true,
        &|r| e3(r, &losses),
    );
    let e4t = with(
        load(&args.e4_tests),
        ("search:standard", "greedy"),
        false,
        &|r| e4(r, false),
    );
    let e4p = with(
        load(&args.e4_panel),
        ("search:standard", "greedy"),
        true,
        &|r| e4(r, true),
    );
    let mut gates = Vec::new();
    let mut failed = Vec::new();
    for (id, parts) in [
        ("E1", vec![("panel", e1)]),
        ("E2", vec![("panel", e2)]),
        ("E3", vec![("tests", e3t), ("panel", e3p)]),
        ("E4", vec![("tests", e4t), ("panel", e4p)]),
    ] {
        let (block, verdict) = gate(id, parts);
        if verdict != Some(true) {
            failed.push(id.to_owned());
        }
        gates.push(block);
    }
    let mut doc = Json::object();
    doc.set("format", GATES_FORMAT);
    doc.set("version", GATES_VERSION);
    doc.set("gates", Json::Array(gates));
    doc.set("pass", failed.is_empty());
    Ok((doc, failed))
}

/// Runs `gates`: the document to stdout (and `--out`); with `--check`, exit 1 unless every gate
/// passed.
///
/// # Errors
/// An unreadable input, the output, or (with `--check`) a failed or missing gate.
pub fn run(args: &GatesArgs, out: &mut dyn std::io::Write) -> Result<(), CommandError> {
    let (doc, failed) = evaluate(args).map_err(CommandError::Invalid)?;
    crate::rows::emit(&doc, args.out.as_ref(), out)?;
    if args.check && !failed.is_empty() {
        return Err(CommandError::Invalid(format!(
            "gates not passed: {}",
            failed.join(", ")
        )));
    }
    Ok(())
}
