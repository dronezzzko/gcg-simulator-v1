//! `gcg-eval fidelity AGENT_A AGENT_B --variants V,... --against DECK` (docs/port/PLAN.md
//! §12.3 S12; W-18): do two agents rank deck variants alike on the same deals?
//!
//! A deck benchmark plays one AI in both seats, so each agent plays every variant against
//! the `--against` deck with itself in both seats: `--games` mirrored games per variant
//! (`mirrored_specs` on the run's seed, so every variant and both agents share the game and
//! agent seeds; the variant sits in seat 0 in games `4b` and `4b + 1` and in seat 1 in the
//! other two), with the forced play/draw balance by default. A variant's rate is its wins
//! over its games (draws are non-wins).
//!
//! The report gives each agent's rate and Wilson interval per variant and its ranks (1 for
//! the best, ties averaged), then the agreement of the two rankings: Spearman's rho (the
//! Pearson correlation of the ranks), Kendall's tau-b, the share of variant pairs both agents
//! order alike (ties in either ranking excluded), and whether both pick the same best
//! variant (the first maximal rate). Per variant, the games of the two agents pair by game
//! index (same deal): the 2x2 table of the variant's wins and the paired difference with its
//! Newcombe interval. `--min-spearman X` exits 1 when rho is below X (or undefined).

use std::io::Write;

use gcg_core::pool::Cancel;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;

use crate::agent_spec::parse_agent_spec;
use crate::bench_cmd::CommandError;
use crate::head_to_head::Selection;
use crate::rows::{RunOptions, emit, prepare, progress_printer};
use crate::selfplay::play_all_traced;
use crate::specs::mirrored_specs;
use crate::stats::{RATE_DIGITS, newcombe_paired, rate_json};
use crate::{AgentDesc, MIRROR_BLOCK, MirroredSpec};

/// The report format.
pub const FIDELITY_FORMAT: &str = "gcg-eval-fidelity";
pub const FIDELITY_VERSION: u32 = 1;

/// The parsed arguments; `opts.source` is the `--pair` list `variant:against,...` and
/// `opts.games` the games per variant.
#[derive(Clone, PartialEq, Debug)]
pub struct FidelityArgs {
    pub agents: [String; 2],
    pub variants: Vec<String>,
    pub against: String,
    pub min_spearman: Option<f64>,
    pub opts: RunOptions,
}

/// Ranks of `values`, 1 for the largest, ties given their average rank.
#[must_use]
pub fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| gcg_core::pymath::cmp_f64(values[b], values[a]));
    let mut out = vec![0.0; values.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && values[order[j + 1]] == values[order[i]] {
            j += 1;
        }
        #[expect(clippy::cast_precision_loss, reason = "ranks are small")]
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            out[order[k]] = avg;
        }
        i = j + 1;
    }
    out
}

/// The Pearson correlation; `None` when either side is constant or there are fewer than two
/// values.
#[must_use]
pub fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len();
    if n < 2 || y.len() != n {
        return None;
    }
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let nf = n as f64;
    let mx = gcg_core::pymath::py_sum(x.iter().copied()) / nf;
    let my = gcg_core::pymath::py_sum(y.iter().copied()) / nf;
    let sxy = gcg_core::pymath::py_sum(x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)));
    let sxx = gcg_core::pymath::py_sum(x.iter().map(|a| (a - mx) * (a - mx)));
    let syy = gcg_core::pymath::py_sum(y.iter().map(|b| (b - my) * (b - my)));
    (sxx > 0.0 && syy > 0.0).then(|| sxy / gcg_core::libm::sqrt(sxx * syy))
}

/// Kendall's tau-b and the share of concordant pairs among the untied ones.
#[must_use]
pub fn kendall(x: &[f64], y: &[f64]) -> (Option<f64>, Option<f64>) {
    let (mut c, mut d, mut tx, mut ty) = (0_u64, 0_u64, 0_u64, 0_u64);
    for i in 0..x.len() {
        for j in i + 1..x.len() {
            let a = gcg_core::pymath::cmp_f64(x[i], x[j]);
            let b = gcg_core::pymath::cmp_f64(y[i], y[j]);
            match (a.is_eq(), b.is_eq()) {
                (true, true) => {}
                (true, false) => tx += 1,
                (false, true) => ty += 1,
                (false, false) if a == b => c += 1,
                (false, false) => d += 1,
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let f = |v: u64| v as f64;
    let den = gcg_core::libm::sqrt(f(c + d + tx) * f(c + d + ty));
    let tau = (den > 0.0).then(|| (f(c) - f(d)) / den);
    let agree = (c + d > 0).then(|| f(c) / f(c + d));
    (tau, agree)
}

fn first_max(xs: &[f64]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, x) in xs.iter().enumerate() {
        if best.is_none_or(|b| *x > xs[b]) {
            best = Some(i);
        }
    }
    best
}

/// The specs of one agent and variant pair: `mirrored_specs` on `seed` with the agent in
/// both seats.
#[must_use]
pub fn variant_specs(
    agent: &AgentDesc,
    selection: &Selection,
    k: usize,
    games: u32,
    seed: u64,
) -> Vec<MirroredSpec> {
    let p = &selection.pairs[k];
    mirrored_specs(
        games,
        seed,
        agent,
        agent,
        [&p.decks[0].deck, &p.decks[1].deck],
    )
}

/// Whether the variant (deck 0 of the pair) won game `i` of its mirrored specs.
#[must_use]
pub fn variant_won(i: usize, winner: i8) -> bool {
    let seat = i8::from((i % MIRROR_BLOCK as usize) >= 2);
    winner == seat
}

/// Runs `fidelity`.
///
/// # Errors
/// A setup problem, a failed game, the interrupt, the output, or a failed `--min-spearman`.
#[expect(
    clippy::too_many_lines,
    reason = "one linear pipeline: play, rank, compare"
)]
pub fn run(
    args: &FidelityArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let agents = [
        parse_agent_spec(&args.agents[0]).map_err(CommandError::Invalid)?,
        parse_agent_spec(&args.agents[1]).map_err(CommandError::Invalid)?,
    ];
    if args.variants.len() < 2 {
        return Err(CommandError::Invalid(
            "--variants needs at least two decks".to_owned(),
        ));
    }
    let games = args.opts.games;
    if games == 0 || !games.is_multiple_of(MIRROR_BLOCK) {
        return Err(CommandError::Invalid(format!(
            "games must be a positive multiple of {MIRROR_BLOCK}"
        )));
    }
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let v = selection.pairs.len();
    let mut specs = Vec::new();
    for agent in &agents {
        for k in 0..v {
            specs.extend(variant_specs(agent, &selection, k, games, args.opts.seed));
        }
    }
    let per = usize::try_from(games).unwrap_or(0);
    let total = u64::try_from(specs.len()).unwrap_or(0);
    let results = {
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        play_all_traced(
            &specs,
            args.opts.workers,
            args.opts.choose_first,
            cancel,
            &mut progress,
        )
        .map_err(CommandError::from)?
    };
    let won = |a: usize, k: usize| -> Vec<bool> {
        (0..per)
            .map(|i| variant_won(i, results[(a * v + k) * per + i].outcome.winner))
            .collect()
    };
    let count = |xs: &[bool]| u64::try_from(xs.iter().filter(|x| **x).count()).unwrap_or(0);
    let n = u64::from(games);
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let rates: Vec<Vec<f64>> = (0..2)
        .map(|a| {
            (0..v)
                .map(|k| count(&won(a, k)) as f64 / n as f64)
                .collect()
        })
        .collect();
    let rank: Vec<Vec<f64>> = rates.iter().map(|r| ranks(r)).collect();
    let rho = pearson(&rank[0], &rank[1]);
    let (tau, agree) = kendall(&rates[0], &rates[1]);
    let best = [first_max(&rates[0]), first_max(&rates[1])];

    let names: Vec<String> = selection
        .pairs
        .iter()
        .map(|p| p.decks[0].name.clone())
        .collect();
    let mut doc = Json::object();
    doc.set("format", FIDELITY_FORMAT);
    doc.set("version", FIDELITY_VERSION);
    doc.set(
        "agents",
        Json::Array(args.agents.iter().map(|a| a.as_str().into()).collect()),
    );
    doc.set(
        "against",
        selection
            .pairs
            .first()
            .map_or("", |p| p.decks[1].name.as_str()),
    );
    doc.set(
        "variants",
        Json::Array(names.iter().map(|n| n.as_str().into()).collect()),
    );
    doc.set("games_per_variant", games);
    doc.set("seed", args.opts.seed);
    doc.set("role", args.opts.role.map(crate::SeedRole::as_str));
    doc.set("choose_first", args.opts.choose_first.as_str());
    let mut by_agent = Vec::new();
    for a in 0..2 {
        let mut o = Json::object();
        o.set("agent", args.agents[a].as_str());
        o.set(
            "results",
            Json::Array((0..v).map(|k| rate_json(count(&won(a, k)), n)).collect()),
        );
        o.set("ranks", rank[a].clone());
        o.set("best", best[a].map(|b| names[b].as_str()));
        by_agent.push(o);
    }
    doc.set("by_agent", Json::Array(by_agent));
    let mut by_variant = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let (x, y) = (won(0, k), won(1, k));
        let (mut ab, mut ao, mut bo, mut ne) = (0_u64, 0_u64, 0_u64, 0_u64);
        for (p, q) in x.iter().zip(&y) {
            match (p, q) {
                (true, true) => ab += 1,
                (true, false) => ao += 1,
                (false, true) => bo += 1,
                (false, false) => ne += 1,
            }
        }
        let (lo, hi) = newcombe_paired(ab, ao, bo, ne, gcg_runner::reports::stats::Z95);
        let mut o = Json::object();
        o.set("variant", name.as_str());
        let mut t = Json::object();
        t.set("both", ab);
        t.set("a_only", ao);
        t.set("b_only", bo);
        t.set("neither", ne);
        #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
        let diff = (ao as f64 - bo as f64) / n as f64;
        t.set("difference", round_f64(diff, RATE_DIGITS));
        t.set(
            "ci95",
            vec![round_f64(lo, RATE_DIGITS), round_f64(hi, RATE_DIGITS)],
        );
        o.set("paired", t);
        by_variant.push(o);
    }
    doc.set("by_variant", Json::Array(by_variant));
    let opt4 = |x: Option<f64>| x.map(|v| round_f64(v, RATE_DIGITS));
    doc.set("spearman", opt4(rho));
    doc.set("kendall_tau_b", opt4(tau));
    doc.set("pairwise_agreement", opt4(agree));
    doc.set("same_best", best[0].is_some() && best[0] == best[1]);
    doc.set(
        "outcomes",
        Json::Array(
            (0..2)
                .flat_map(|a| {
                    let won = &won;
                    (0..v).flat_map(move |k| {
                        won(a, k).into_iter().enumerate().map(move |(i, w)| {
                            Json::Array(vec![a.into(), k.into(), i.into(), w.into()])
                        })
                    })
                })
                .collect(),
        ),
    );
    emit(&doc, args.opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set(
        "agents",
        Json::Array(args.agents.iter().map(|a| a.as_str().into()).collect()),
    );
    args.opts.record("fidelity", extra)?;
    if let Some(min) = args.min_spearman
        && rho.is_none_or(|r| r < min)
    {
        return Err(CommandError::Invalid(format!(
            "fidelity below --min-spearman {min}: rho {}",
            rho.map_or_else(|| "undefined".to_owned(), |r| round_f64(r, 4).to_string())
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_statistics() {
        assert_eq!(ranks(&[0.5, 0.7, 0.5, 0.1]), [2.5, 1.0, 2.5, 4.0]);
        let a = [0.1, 0.4, 0.3, 0.9];
        assert_eq!(pearson(&ranks(&a), &ranks(&a)), Some(1.0));
        let rev: Vec<f64> = a.iter().map(|x| -x).collect();
        assert_eq!(pearson(&ranks(&a), &ranks(&rev)), Some(-1.0));
        assert_eq!(kendall(&a, &a), (Some(1.0), Some(1.0)));
        assert_eq!(kendall(&a, &rev).1, Some(0.0));
        assert_eq!(pearson(&[1.0, 1.0], &[0.0, 1.0]), None);
        assert_eq!(first_max(&[0.2, 0.5, 0.5]), Some(1));
    }

    #[test]
    fn the_variant_sits_in_seat_0_then_seat_1() {
        assert!(variant_won(0, 0) && variant_won(1, 0));
        assert!(variant_won(2, 1) && variant_won(3, 1));
        assert!(!variant_won(2, 0) && !variant_won(4, -1));
    }
}
