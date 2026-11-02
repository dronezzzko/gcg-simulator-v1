//! `gcg-eval calibrate FILE --agent SPEC` (docs/port/PLAN.md §12.2 P6-I, §12.10; W-41):
//! simulated matchup rates against observed real results.
//!
//! FILE lists observed matches in the format of `schema/observed-matches.v1.schema.json`
//! (embedded; validated with gcg-runner's in-house JSON Schema validator before anything is
//! played): per matchup the two decks, deck A's wins, losses and draws, the source and the
//! date. Nothing is fetched or invented; real data comes from the user (PLAN A32 item 5), and
//! the shipped fixture is synthetic.
//!
//! Each matchup `i` is simulated with `--agent` in both seats: `--games` mirrored games
//! (`mirrored_specs` on `derive_seed(seed, "calibrate", i)`, the forced play/draw balance by
//! default); deck A's simulated rate `p_i` is its wins over the games. Draws are non-wins on
//! both sides (W-93). Over the `N` observed games, with `w_i` observed wins of `n_i` games:
//!
//! - Brier score `sum_i [w_i (1 - p_i)^2 + (n_i - w_i) p_i^2] / N`;
//! - log-loss with `p_i` clipped to `[1e-12, 1 - 1e-12]`;
//! - reliability bins (`--bins`, equal width over `[0, 1]`, a matchup in bin
//!   `min(B - 1, floor(p_i B))`): observed games, mean predicted rate
//!   `sum n_i p_i / sum n_i`, observed rate `sum w_i / sum n_i`;
//! - the expected calibration error `sum_bins (N_bin / N) |observed - predicted|` and the
//!   calibration-in-the-large bias `(sum w_i - sum n_i p_i) / N`.
//!
//! `n_i p_i` is computed as `(n_i * simulated wins) / games`, so a fixture whose observed
//! counts equal the simulated ones has an error of exactly 0.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gcg_core::derive_seed;
use gcg_core::pool::Cancel;
use gcg_core::pyfloat::round_f64;
use gcg_core::pyjson::Json;
use gcg_data::py_path_str;
use gcg_runner::reports::json_from_value;
use gcg_runner::reports::schema::Schema;
use serde_json::Value;

use crate::MIRROR_BLOCK;
use crate::agent_spec::parse_agent_spec;
use crate::bench_cmd::CommandError;
use crate::fidelity::variant_won;
use crate::head_to_head::{PairSource, Selection};
use crate::rows::{RunOptions, emit, prepare, progress_printer};
use crate::selfplay::play_all_traced;
use crate::specs::mirrored_specs;
use crate::stats::{RATE_DIGITS, rate_json};

/// The observed-matches schema, embedded.
pub const OBSERVED_SCHEMA: &str = include_str!("../schema/observed-matches.v1.schema.json");
/// The report format.
pub const CALIBRATION_FORMAT: &str = "gcg-eval-calibration";
pub const CALIBRATION_VERSION: u32 = 1;
/// The default number of reliability bins.
pub const DEFAULT_BINS: u32 = 10;
const CLIP: f64 = 1e-12;

fn schema() -> &'static Schema {
    static S: OnceLock<Schema> = OnceLock::new();
    S.get_or_init(|| {
        Schema::compile(OBSERVED_SCHEMA)
            .unwrap_or_else(|e| panic!("the embedded observed-matches schema compiles: {e:?}"))
    })
}

/// One observed matchup.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Observed {
    pub deck_a: String,
    pub deck_b: String,
    pub wins: u64,
    pub losses: u64,
    pub draws: u64,
    pub source: String,
    pub date: String,
}

impl Observed {
    #[must_use]
    pub const fn games(&self) -> u64 {
        self.wins + self.losses + self.draws
    }
}

/// The parsed arguments; `opts.games` is the games per matchup.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CalibrateArgs {
    pub file: PathBuf,
    pub agent: String,
    pub bins: u32,
    pub opts: RunOptions,
}

/// A deck reference of the file: a path relative to the file's directory when that file
/// exists, else the reference as given (a path or a deck name, `decks::resolve_deck`).
fn deck_ref(file: &Path, r: &str) -> String {
    let base = file.parent().unwrap_or_else(|| Path::new(""));
    let candidate = base.join(r);
    if (r.ends_with(".txt") || r.contains('/')) && candidate.is_file() {
        return candidate.to_string_lossy().into_owned();
    }
    r.to_owned()
}

/// Reads and validates an observed-matches file.
///
/// # Errors
/// The file is unreadable, not JSON, or breaks the schema (or has a matchup without games).
pub fn load(file: &Path) -> Result<Vec<Observed>, String> {
    let shown = py_path_str(file);
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{shown}: not JSON: {e}"))?;
    schema()
        .validate(&json_from_value(&v))
        .map_err(|e| format!("{shown}: {e}"))?;
    let items = v["matches"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for (i, m) in items.iter().enumerate() {
        let s = |k: &str| m[k].as_str().unwrap_or_default().to_owned();
        let n = |k: &str| m[k].as_u64().unwrap_or(0);
        let o = Observed {
            deck_a: deck_ref(file, &s("deck_a")),
            deck_b: deck_ref(file, &s("deck_b")),
            wins: n("wins"),
            losses: n("losses"),
            draws: n("draws"),
            source: s("source"),
            date: s("date"),
        };
        if o.games() == 0 {
            return Err(format!("{shown}: matches/{i} has no games"));
        }
        out.push(o);
    }
    Ok(out)
}

#[expect(clippy::cast_precision_loss, reason = "game counts are far below 2^53")]
fn f(x: u64) -> f64 {
    x as f64
}

/// The calibration metrics of observed counts against simulated wins out of `games` each:
/// `(brier, log_loss, ece, bias, bins)`.
#[must_use]
pub fn metrics(
    observed: &[Observed],
    simulated: &[u64],
    games: u64,
    bins: u32,
) -> (f64, f64, f64, f64, Json) {
    let total: u64 = observed.iter().map(Observed::games).sum();
    let nb = usize::try_from(bins.max(1)).unwrap_or(1);
    let mut bin_n = vec![0_u64; nb];
    let mut bin_w = vec![0_u64; nb];
    let mut bin_pred = vec![0.0; nb];
    let mut bin_m = vec![0_u64; nb];
    let mut brier = 0.0;
    let mut ll = 0.0;
    let mut pred_wins = 0.0;
    for (o, &sw) in observed.iter().zip(simulated) {
        let p = f(sw) / f(games);
        let n = o.games();
        let pw = f(n * sw) / f(games);
        pred_wins += pw;
        brier += f(o.wins) * (1.0 - p) * (1.0 - p) + f(n - o.wins) * p * p;
        let pc = p.clamp(CLIP, 1.0 - CLIP);
        ll -= f(o.wins) * gcg_core::libm::ln(pc) + f(n - o.wins) * gcg_core::libm::ln(1.0 - pc);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "p is in [0, 1], so the bin index is a small non-negative integer"
        )]
        let b = ((p * f(u64::from(bins))) as usize).min(nb - 1);
        bin_n[b] += n;
        bin_w[b] += o.wins;
        bin_pred[b] += pw;
        bin_m[b] += 1;
    }
    let tf = f(total.max(1));
    let mut ece = 0.0;
    let mut rows = Vec::new();
    for b in 0..nb {
        let mut o = Json::object();
        #[expect(clippy::cast_precision_loss, reason = "small bin counts")]
        let (lo, hi) = (b as f64 / nb as f64, (b + 1) as f64 / nb as f64);
        o.set(
            "range",
            vec![round_f64(lo, RATE_DIGITS), round_f64(hi, RATE_DIGITS)],
        );
        o.set("matchups", bin_m[b]);
        o.set("games", bin_n[b]);
        if bin_n[b] > 0 {
            let pred = bin_pred[b] / f(bin_n[b]);
            let obs = f(bin_w[b]) / f(bin_n[b]);
            ece += f(bin_n[b]) / tf * (obs - pred).abs();
            o.set("predicted", round_f64(pred, 6));
            o.set("observed", round_f64(obs, 6));
        } else {
            o.set("predicted", Json::Null);
            o.set("observed", Json::Null);
        }
        rows.push(o);
    }
    let wins: u64 = observed.iter().map(|o| o.wins).sum();
    let bias = (f(wins) - pred_wins) / tf;
    (brier / tf, ll / tf, ece, bias, Json::Array(rows))
}

/// Runs `calibrate`.
///
/// # Errors
/// A malformed file, a setup problem, a failed game, the interrupt, or the output.
pub fn run(
    args: &CalibrateArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let observed = load(&args.file).map_err(CommandError::Invalid)?;
    let agent = parse_agent_spec(&args.agent).map_err(CommandError::Invalid)?;
    let games = args.opts.games;
    if games == 0 || !games.is_multiple_of(MIRROR_BLOCK) {
        return Err(CommandError::Invalid(format!(
            "games must be a positive multiple of {MIRROR_BLOCK}"
        )));
    }
    if args.bins == 0 {
        return Err(CommandError::Invalid("--bins must be positive".to_owned()));
    }
    let list: Vec<String> = observed
        .iter()
        .map(|o| format!("{}:{}", o.deck_a, o.deck_b))
        .collect();
    let opts = RunOptions {
        source: PairSource::Named(list.join(",")),
        ..args.opts.clone()
    };
    let selection: Selection = prepare(&opts).map_err(CommandError::Invalid)?;
    let mut specs = Vec::new();
    for (i, p) in selection.pairs.iter().enumerate() {
        specs.extend(mirrored_specs(
            games,
            derive_seed!(opts.seed, "calibrate", i),
            &agent,
            &agent,
            [&p.decks[0].deck, &p.decks[1].deck],
        ));
    }
    let results = {
        let total = u64::try_from(specs.len()).unwrap_or(0);
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        play_all_traced(
            &specs,
            opts.workers,
            opts.choose_first,
            cancel,
            &mut progress,
        )
        .map_err(CommandError::from)?
    };
    let per = usize::try_from(games).unwrap_or(0);
    let won: Vec<Vec<bool>> = (0..observed.len())
        .map(|k| {
            (0..per)
                .map(|i| variant_won(i, results[k * per + i].outcome.winner))
                .collect()
        })
        .collect();
    let simulated: Vec<u64> = won
        .iter()
        .map(|w| u64::try_from(w.iter().filter(|x| **x).count()).unwrap_or(0))
        .collect();
    let g = u64::from(games);
    let (brier, log_loss, ece, bias, bins) = metrics(&observed, &simulated, g, args.bins);
    let mut doc = Json::object();
    doc.set("format", CALIBRATION_FORMAT);
    doc.set("version", CALIBRATION_VERSION);
    doc.set("observed", py_path_str(&args.file));
    doc.set("agent", args.agent.as_str());
    doc.set("games_per_matchup", games);
    doc.set("seed", opts.seed);
    doc.set("role", opts.role.map(crate::SeedRole::as_str));
    doc.set("choose_first", opts.choose_first.as_str());
    let rows = observed
        .iter()
        .zip(&simulated)
        .zip(&selection.pairs)
        .map(|((o, &sw), p)| {
            let mut r = Json::object();
            r.set("deck_a", p.decks[0].name.as_str());
            r.set("deck_b", p.decks[1].name.as_str());
            r.set("source", o.source.as_str());
            r.set("date", o.date.as_str());
            let mut ob = rate_json(o.wins, o.games());
            ob.set("draws", o.draws);
            r.set("observed", ob);
            r.set("simulated", rate_json(sw, g));
            r.set(
                "difference",
                round_f64(f(o.wins) / f(o.games()) - f(sw) / f(g), RATE_DIGITS),
            );
            r
        })
        .collect();
    doc.set("matchups", Json::Array(rows));
    doc.set("brier", round_f64(brier, 6));
    doc.set("log_loss", round_f64(log_loss, 6));
    doc.set("ece", round_f64(ece, 6));
    doc.set("bias", round_f64(bias, 6));
    doc.set("bins", bins);
    doc.set(
        "outcomes",
        Json::Array(
            won.iter()
                .enumerate()
                .flat_map(|(k, w)| {
                    w.iter()
                        .enumerate()
                        .map(move |(i, x)| Json::Array(vec![k.into(), i.into(), (*x).into()]))
                })
                .collect(),
        ),
    );
    emit(&doc, opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set("agent", args.agent.as_str());
    extra.set("observed", py_path_str(&args.file));
    opts.record("calibrate", extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(w: u64, l: u64) -> Observed {
        Observed {
            deck_a: "a".into(),
            deck_b: "b".into(),
            wins: w,
            losses: l,
            draws: 0,
            source: "synthetic".into(),
            date: "2026-11-01".into(),
        }
    }

    #[test]
    fn equal_counts_have_zero_error_and_a_shift_shows_as_bias() {
        let o = [obs(13, 27), obs(31, 9), obs(20, 20)];
        let (_, _, ece, bias, _) = metrics(&o, &[13, 31, 20], 40, 10);
        assert_eq!((ece, bias), (0.0, 0.0));
        let shifted = [obs(23, 17), obs(39, 1), obs(30, 10)];
        let (_, _, ece, bias, _) = metrics(&shifted, &[13, 31, 20], 40, 10);
        assert!((bias - 28.0 / 120.0).abs() < 1e-12, "{bias}");
        assert!(ece > 0.2);
        let (brier, ll, _, _, _) = metrics(&[obs(1, 0)], &[4], 4, 10);
        assert!(brier.abs() < 1e-12 && ll < 1e-11);
    }

    #[test]
    fn the_schema_compiles_and_rejects_malformed_entries() {
        let good = serde_json::json!({
            "format": "gcg-eval-observed-matches", "version": 1,
            "matches": [{"deck_a": "a.txt", "deck_b": "b", "wins": 1, "losses": 2, "draws": 0,
                         "source": "synthetic", "date": "2026-10-01"}]
        });
        assert!(schema().validate(&json_from_value(&good)).is_ok());
        for (key, bad) in [
            ("wins", serde_json::json!(-1)),
            ("date", serde_json::json!("2026-13-01")),
            ("source", serde_json::json!("")),
            ("extra", serde_json::json!(1)),
        ] {
            let mut v = good.clone();
            v["matches"][0][key] = bad;
            assert!(schema().validate(&json_from_value(&v)).is_err(), "{key}");
        }
        let mut missing = good;
        if let Some(m) = missing["matches"][0].as_object_mut() {
            m.remove("date");
        }
        assert!(schema().validate(&json_from_value(&missing)).is_err());
    }
}
