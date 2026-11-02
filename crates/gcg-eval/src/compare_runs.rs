//! `gcg-eval compare-runs DIR_A DIR_B` (docs/port/PLAN.md §12.2 P6-I; W-42): the paired
//! analysis of two same-seed `gcg-sim benchmark` (or `gcg-eval *-benchmark`) report
//! directories, game by game and match by match. The reports themselves are never changed.
//!
//! Games pair by `(match_index, game_index)` from each directory's `games.ndjson`; every
//! paired game must carry the same game seed in both runs (same master seed, so the same
//! deal and agent seeds), or the runs are refused. A game is a deck-under-test win when its
//! `winner` is `dut` (draws are non-wins, W-93). A match goes to the side with more match
//! points (`match_point` of its games); equal points are a drawn match, a non-win.
//!
//! For games and for matches the report gives each run's wins/N with its Wilson interval,
//! the paired 2x2 table (`both`, `a_only`, `b_only`, `neither`; `a_only` and `b_only` are
//! McNemar's discordant counts), the paired difference `rate_a - rate_b` with Newcombe's
//! paired interval (method 10, `crate::stats::newcombe_paired`), the unpaired interval of the
//! same difference (Newcombe's method 10 for independent samples: the paired formula with
//! `phi = 0` over each run's own games), and McNemar's exact two-sided p-value
//! `min(1, 2 * P(X <= min(b, c)))`, `X ~ Binomial(b + c, 1/2)` (1 without discordant pairs).
//! Unpaired keys (games present in one run only, as BO3 third games can be) are listed.

use std::io::Write;
use std::path::{Path, PathBuf};

use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_data::py_path_str;
use gcg_runner::reports::stats::{Z95, wilson};
use serde_json::Value;

use crate::bench_cmd::CommandError;
use crate::stats::{RATE_DIGITS, newcombe_paired, rate_json};

/// The report format.
pub const COMPARE_RUNS_FORMAT: &str = "gcg-eval-compare-runs";
pub const COMPARE_RUNS_VERSION: u32 = 1;

/// The parsed arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CompareRunsArgs {
    pub a: PathBuf,
    pub b: PathBuf,
    pub out: Option<PathBuf>,
}

/// One game of a run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunGame {
    pub match_index: u64,
    pub game_index: u64,
    pub seed: u64,
    pub dut_won: bool,
    pub point: Option<String>,
}

/// Reads a report directory's `games.ndjson`.
///
/// # Errors
/// The file is unreadable or a line is not a game record.
pub fn read_games(dir: &Path) -> Result<Vec<RunGame>, String> {
    let path = dir.join("games.ndjson");
    let shown = py_path_str(&path);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, line)| {
            let v: Value = serde_json::from_str(line)
                .map_err(|e| format!("{shown}:{}: not JSON: {e}", i + 1))?;
            let num = |k: &str| v.get(k).and_then(Value::as_u64);
            match (num("match_index"), num("game_index"), num("seed")) {
                (Some(m), Some(g), Some(seed)) => Ok(RunGame {
                    match_index: m,
                    game_index: g,
                    seed,
                    dut_won: v.get("winner").and_then(Value::as_str) == Some("dut"),
                    point: v
                        .get("match_point")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }),
                _ => Err(format!("{shown}:{}: not a game record", i + 1)),
            }
        })
        .collect()
}

/// Each match's deck-under-test win (more match points than the benchmark), by match index.
#[must_use]
pub fn match_wins(games: &[RunGame]) -> Vec<(u64, bool)> {
    let mut out: Vec<(u64, i64)> = Vec::new();
    for g in games {
        let delta = match g.point.as_deref() {
            Some("dut") => 1,
            Some("bench") => -1,
            _ => 0,
        };
        match out.iter_mut().find(|(m, _)| *m == g.match_index) {
            Some((_, d)) => *d += delta,
            None => out.push((g.match_index, delta)),
        }
    }
    out.sort_by_key(|(m, _)| *m);
    out.into_iter().map(|(m, d)| (m, d > 0)).collect()
}

/// McNemar's exact two-sided p-value for discordant counts `b` and `c` (module docs).
#[must_use]
pub fn mcnemar_exact(b: u64, c: u64) -> f64 {
    let n = b + c;
    if n == 0 {
        return 1.0;
    }
    let k = b.min(c);
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let f = |x: u64| x as f64;
    let ln2 = gcg_core::libm::ln(2.0);
    let mut log_c = 0.0;
    let mut total = 0.0;
    for i in 0..=k {
        if i > 0 {
            log_c += gcg_core::libm::ln(f(n - i + 1)) - gcg_core::libm::ln(f(i));
        }
        total += gcg_core::libm::exp(log_c - f(n) * ln2);
    }
    let p = 2.0 * total;
    if p < 1.0 { p } else { 1.0 }
}

/// Newcombe's interval for the difference of two independent proportions (method 10).
#[must_use]
pub fn newcombe_unpaired(wa: u64, na: u64, wb: u64, nb: u64) -> (f64, f64) {
    if na == 0 || nb == 0 {
        return (-1.0, 1.0);
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let (pa, pb) = (wa as f64 / na as f64, wb as f64 / nb as f64);
    let (la, ua) = wilson(wa, na, Z95).unwrap_or((0.0, 1.0));
    let (lb, ub) = wilson(wb, nb, Z95).unwrap_or((0.0, 1.0));
    let d = pa - pb;
    let delta = gcg_core::libm::sqrt((pa - la) * (pa - la) + (ub - pb) * (ub - pb));
    let eps = gcg_core::libm::sqrt((ua - pa) * (ua - pa) + (pb - lb) * (pb - lb));
    (d - delta, d + eps)
}

fn pair_json(r: (f64, f64)) -> Json {
    Json::Array(vec![
        round_f64(r.0, RATE_DIGITS).into(),
        round_f64(r.1, RATE_DIGITS).into(),
    ])
}

/// The comparison block of paired binary outcomes `(key, a, b)` plus each run's own totals.
#[must_use]
pub fn block(paired: &[(Json, bool, bool)], a_all: (u64, u64), b_all: (u64, u64)) -> Json {
    let (mut both, mut a_only, mut b_only, mut neither) = (0_u64, 0_u64, 0_u64, 0_u64);
    for (_, a, b) in paired {
        match (a, b) {
            (true, true) => both += 1,
            (true, false) => a_only += 1,
            (false, true) => b_only += 1,
            (false, false) => neither += 1,
        }
    }
    let n = both + a_only + b_only + neither;
    let mut o = Json::object();
    o.set("a", rate_json(a_all.0, a_all.1));
    o.set("b", rate_json(b_all.0, b_all.1));
    let mut t = Json::object();
    t.set("n", n);
    t.set("both", both);
    t.set("a_only", a_only);
    t.set("b_only", b_only);
    t.set("neither", neither);
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let diff = (n > 0).then(|| (a_only as f64 - b_only as f64) / n as f64);
    t.set("difference", diff.map(|d| round_f64(d, RATE_DIGITS)));
    t.set(
        "ci95",
        pair_json(newcombe_paired(both, a_only, b_only, neither, Z95)),
    );
    t.set("mcnemar_p", round_f64(mcnemar_exact(a_only, b_only), 6));
    o.set("paired", t);
    let mut u = Json::object();
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let ud = (a_all.1 > 0 && b_all.1 > 0)
        .then(|| a_all.0 as f64 / a_all.1 as f64 - b_all.0 as f64 / b_all.1 as f64);
    u.set("difference", ud.map(|d| round_f64(d, RATE_DIGITS)));
    u.set(
        "ci95",
        pair_json(newcombe_unpaired(a_all.0, a_all.1, b_all.0, b_all.1)),
    );
    o.set("unpaired", u);
    o
}

fn count_wins(xs: impl Iterator<Item = bool>) -> (u64, u64) {
    xs.fold((0, 0), |(w, n), x| (w + u64::from(x), n + 1))
}

/// The compare-runs document.
///
/// # Errors
/// An unreadable run, or runs whose paired games have different seeds.
pub fn document(args: &CompareRunsArgs) -> Result<Json, String> {
    let ga = read_games(&args.a)?;
    let gb = read_games(&args.b)?;
    let key = |g: &RunGame| (g.match_index, g.game_index);
    let mut paired = Vec::new();
    let mut only_a = Vec::new();
    for a in &ga {
        match gb.iter().find(|b| key(b) == key(a)) {
            Some(b) if b.seed != a.seed => {
                return Err(format!(
                    "match {} game {} has seed {} in {} and {} in {}: the runs do not share \
                     their master seed",
                    a.match_index,
                    a.game_index + 1,
                    a.seed,
                    py_path_str(&args.a),
                    b.seed,
                    py_path_str(&args.b)
                ));
            }
            Some(b) => paired.push((
                Json::Array(vec![a.match_index.into(), a.game_index.into()]),
                a.dut_won,
                b.dut_won,
            )),
            None => only_a.push(Json::Array(vec![a.match_index.into(), a.game_index.into()])),
        }
    }
    let only_b: Vec<Json> = gb
        .iter()
        .filter(|b| !ga.iter().any(|a| key(a) == key(b)))
        .map(|b| Json::Array(vec![b.match_index.into(), b.game_index.into()]))
        .collect();
    let ma = match_wins(&ga);
    let mb = match_wins(&gb);
    let mut mpaired = Vec::new();
    for (m, a) in &ma {
        if let Some((_, b)) = mb.iter().find(|(x, _)| x == m) {
            mpaired.push((Json::from(*m), *a, *b));
        }
    }
    let mut doc = Json::object();
    doc.set("format", COMPARE_RUNS_FORMAT);
    doc.set("version", COMPARE_RUNS_VERSION);
    doc.set("a", py_path_str(&args.a));
    doc.set("b", py_path_str(&args.b));
    let mut games = block(
        &paired,
        count_wins(ga.iter().map(|g| g.dut_won)),
        count_wins(gb.iter().map(|g| g.dut_won)),
    );
    games.set("only_a", Json::Array(only_a));
    games.set("only_b", Json::Array(only_b));
    doc.set("games", games);
    doc.set(
        "matches",
        block(
            &mpaired,
            count_wins(ma.iter().map(|(_, w)| *w)),
            count_wins(mb.iter().map(|(_, w)| *w)),
        ),
    );
    Ok(doc)
}

/// Runs `compare-runs`.
///
/// # Errors
/// As [`document`], or the output.
pub fn run(args: &CompareRunsArgs, out: &mut dyn Write) -> Result<(), CommandError> {
    let doc = document(args).map_err(CommandError::Invalid)?;
    crate::rows::emit(&doc, args.out.as_ref(), out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcnemar_exact_values() {
        assert_eq!(mcnemar_exact(0, 0), 1.0);
        assert!((mcnemar_exact(0, 5) - 0.0625).abs() < 1e-12);
        assert!((mcnemar_exact(2, 8) - 0.109_375).abs() < 1e-12);
        assert_eq!(mcnemar_exact(5, 5), 1.0);
        assert!(mcnemar_exact(0, 2000) < 1e-300);
    }

    #[test]
    fn unpaired_is_the_paired_formula_without_correlation() {
        let (lo, hi) = newcombe_unpaired(30, 50, 20, 50);
        assert!(lo < 0.2 && 0.2 < hi);
        let (mlo, mhi) = newcombe_unpaired(20, 50, 30, 50);
        assert!((mlo + hi).abs() < 1e-12 && (mhi + lo).abs() < 1e-12);
    }

    #[test]
    fn matches_go_to_the_side_with_more_points() {
        let g = |m, gi, p: &str| RunGame {
            match_index: m,
            game_index: gi,
            seed: 0,
            dut_won: p == "dut",
            point: Some(p.to_owned()),
        };
        let games = [
            g(1, 0, "dut"),
            g(0, 0, "bench"),
            g(0, 1, "dut"),
            g(0, 2, "dut"),
            g(1, 1, "bench"),
            g(1, 2, "bench"),
        ];
        assert_eq!(match_wins(&games), [(0, true), (1, false)]);
    }
}
