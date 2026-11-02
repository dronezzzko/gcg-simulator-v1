//! `gcg-eval ablation` and `gcg-eval budget-curve` (docs/port/PLAN.md §12.4, §12.8; W-42).
//!
//! **Ablation rows** (§12.4). A mechanism is a list of configuration overrides
//! (`field=value,...`), the agent-spec override grammar of [`crate::agent_spec`]:
//!
//! - `individual`: the reference configuration at the same iteration budget plus only this
//!   mechanism, against the reference. The challenger is `search:null-<preset>` (the
//!   strengthened search with the reference preset's settings and every mechanism off, the
//!   null configuration) with the mechanism's overrides that switch it on; the opponent is
//!   `ref:<preset>` (`--reference`, default `strong`).
//! - `leave-one-out`: the full strengthened bundle against the same bundle without the
//!   mechanism. The challenger is the bundle (`--bundle SPEC`, default `search:standard`, or
//!   `--search-config FILE`); the opponent is the bundle with the overrides that switch the
//!   mechanism off.
//!
//! Rows play mirrored blocks of four with the forced play/draw balance of A24 by default,
//! stratified round-robin over the pairs (default the 15 tuning pairs of the panel, 400
//! games). The report adds `row`, `mechanism`, `reference`/`bundle`, and the §12.4 `summary`
//! (`crate::rows::summary`).
//!
//! **Budget curves** (§12.8 step 1). The challenger, a `search:` spec, is replayed at each
//! scale of its budget fields (default `iterations_per_option`, `min_iterations`,
//! `max_iterations`; `--fields` adds others, such as a root-tree count): each field becomes
//! `max(1, round(value * scale))` (half away from zero). Every point plays the same seeds
//! against the same opponent, so the points share their deals.

use std::io::Write;

use gcg_core::pool::Cancel;
use gcg_core::pyjson::Json;

use crate::AgentDesc;
use crate::agent_spec::{
    NULL_PREFIX, REF_PREFIX, SEARCH_PREFIX, parse_agent_spec, search_with,
    set_search_field_generic, with_overrides,
};
use crate::bench_cmd::CommandError;
use crate::rows::{RowAgents, RunOptions, emit, play_row, prepare, progress_printer, row_into};
use crate::search_config;

/// The ablation report format.
pub const ABLATION_FORMAT: &str = "gcg-eval-ablation";
/// The budget-curve report format.
pub const BUDGET_CURVE_FORMAT: &str = "gcg-eval-budget-curve";
pub const FORMAT_VERSION: u32 = 1;
/// The default budget fields of a curve (§12.8: `(ipo, min, max)`).
pub const BUDGET_FIELDS: [&str; 3] = ["iterations_per_option", "min_iterations", "max_iterations"];
/// The default scales (§12.8 step 1).
pub const DEFAULT_SCALES: [f64; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

/// Which row.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RowKind {
    Individual,
    LeaveOneOut,
}

impl RowKind {
    pub const ALL: [Self; 2] = [Self::Individual, Self::LeaveOneOut];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::LeaveOneOut => "leave-one-out",
        }
    }

    #[must_use]
    pub fn from_value(v: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == v)
    }
}

/// Where the bundle of a leave-one-out row comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Bundle {
    Spec(String),
    File(std::path::PathBuf),
}

/// The parsed `ablation` arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AblationArgs {
    pub row: RowKind,
    pub mechanism: String,
    pub reference: String,
    pub bundle: Bundle,
    pub opts: RunOptions,
}

/// The two agents of an ablation row (module docs).
///
/// # Errors
/// An unknown preset, a bad override or an unreadable configuration file.
pub fn row_agents(args: &AblationArgs) -> Result<RowAgents, String> {
    if args.mechanism.trim().is_empty() {
        return Err("--mechanism needs at least one field=value".to_owned());
    }
    match args.row {
        RowKind::Individual => {
            let null = format!("{SEARCH_PREFIX}{NULL_PREFIX}{}", args.reference);
            let challenger_text = with_overrides(&null, &args.mechanism);
            let opponent_text = format!("{REF_PREFIX}{}", args.reference);
            let challenger = parse_agent_spec(&challenger_text)?;
            let opponent = parse_agent_spec(&opponent_text)?;
            Ok(RowAgents {
                challenger_text,
                opponent_text,
                challenger,
                opponent,
            })
        }
        RowKind::LeaveOneOut => {
            let (text, base) = match &args.bundle {
                Bundle::Spec(spec) => {
                    if !spec.starts_with(SEARCH_PREFIX) {
                        return Err(format!("--bundle must be a search: spec, got {spec:?}"));
                    }
                    (spec.clone(), parse_agent_spec(spec)?)
                }
                Bundle::File(path) => {
                    let cfg = search_config::load(path)?;
                    (
                        format!("{SEARCH_PREFIX}{}", gcg_data::py_path_str(path)),
                        AgentDesc::Search { config: cfg },
                    )
                }
            };
            let AgentDesc::Search { config } = &base else {
                return Err("the bundle must be the strengthened search".to_owned());
            };
            let off = search_with(config.clone(), &text, &args.mechanism)?;
            if off == *config {
                return Err(format!(
                    "--mechanism {:?} does not change the bundle",
                    args.mechanism
                ));
            }
            Ok(RowAgents {
                opponent_text: with_overrides(&text, &args.mechanism),
                challenger_text: text,
                challenger: base,
                opponent: AgentDesc::Search { config: off },
            })
        }
    }
}

/// Runs an ablation row.
///
/// # Errors
/// A setup problem, a failed game, the interrupt, or the output.
pub fn run_ablation(
    args: &AblationArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let agents = row_agents(args).map_err(CommandError::Invalid)?;
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let total = u64::from(args.opts.games);
    let games = {
        let mut printer = progress_printer(err, tty);
        let mut progress = |d: usize| printer.update(u64::try_from(d).unwrap_or(0), total);
        play_row(
            &agents,
            &selection,
            &args.opts,
            args.opts.seed,
            cancel,
            &mut progress,
        )?
    };
    let mut doc = Json::object();
    doc.set("format", ABLATION_FORMAT);
    doc.set("version", FORMAT_VERSION);
    doc.set("row", args.row.as_str());
    doc.set("mechanism", args.mechanism.as_str());
    match args.row {
        RowKind::Individual => doc.set("reference", args.reference.as_str()),
        RowKind::LeaveOneOut => doc.set("bundle", agents.challenger_text.as_str()),
    }
    args.opts.describe(&mut doc);
    row_into(&mut doc, &agents, &selection, args.opts.seed, &games);
    emit(&doc, args.opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set("row", args.row.as_str());
    extra.set("mechanism", args.mechanism.as_str());
    extra.set("challenger", agents.challenger_text.as_str());
    extra.set("opponent", agents.opponent_text.as_str());
    args.opts.record("ablation", extra)
}

/// The parsed `budget-curve` arguments.
#[derive(Clone, PartialEq, Debug)]
pub struct BudgetCurveArgs {
    pub challenger: String,
    pub opponent: String,
    pub scales: Vec<f64>,
    pub fields: Vec<String>,
    pub opts: RunOptions,
}

/// `max(1, round(value * scale))` for an integer budget field.
#[must_use]
pub fn scale_value(value: u64, scale: f64) -> u64 {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "budgets are small non-negative integers; the product is rounded and clamped"
    )]
    let scaled = (value as f64 * scale).round() as u64;
    if scaled < 1 { 1 } else { scaled }
}

/// The challenger's configuration at `scale`, with the scaled field values in field order.
///
/// # Errors
/// A field that is not an integer field of the configuration.
pub fn scaled_config(
    base: &gcg_search::StrengthenedConfig,
    fields: &[String],
    scale: f64,
) -> Result<(gcg_search::StrengthenedConfig, Vec<(String, u64)>), String> {
    let doc = search_config::to_json(base);
    let mut cfg = base.clone();
    let mut values = Vec::with_capacity(fields.len());
    for f in fields {
        let current = match doc.get(f) {
            Some(Json::Int(v)) => u64::try_from(*v).ok(),
            _ => None,
        }
        .ok_or_else(|| {
            format!("--fields: {f:?} is not an integer field of the search configuration")
        })?;
        let v = scale_value(current, scale);
        cfg = set_search_field_generic(&cfg, f, &v.to_string())?
            .ok_or_else(|| format!("--fields: unknown field {f:?}"))?;
        values.push((f.clone(), v));
    }
    Ok((cfg, values))
}

/// Runs a budget curve.
///
/// # Errors
/// A setup problem, a failed game, the interrupt, or the output.
pub fn run_budget_curve(
    args: &BudgetCurveArgs,
    cancel: &Cancel,
    tty: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CommandError> {
    let AgentDesc::Search { config: base } =
        parse_agent_spec(&args.challenger).map_err(CommandError::Invalid)?
    else {
        return Err(CommandError::Invalid(
            "the challenger of a budget curve must be a search: spec".to_owned(),
        ));
    };
    let opponent = parse_agent_spec(&args.opponent).map_err(CommandError::Invalid)?;
    if args.scales.is_empty() || args.scales.iter().any(|s| !(s.is_finite() && *s > 0.0)) {
        return Err(CommandError::Invalid(
            "--scales must be positive numbers".to_owned(),
        ));
    }
    let selection = prepare(&args.opts).map_err(CommandError::Invalid)?;
    let mut points = Vec::new();
    let mut configs = Vec::new();
    for &scale in &args.scales {
        configs.push(scaled_config(&base, &args.fields, scale).map_err(CommandError::Invalid)?);
    }
    let total = u64::from(args.opts.games) * u64::try_from(configs.len()).unwrap_or(0);
    let mut printer = progress_printer(err, tty);
    for (k, (scale, (cfg, values))) in args.scales.iter().zip(configs).enumerate() {
        let overrides: Vec<String> = values.iter().map(|(f, v)| format!("{f}={v}")).collect();
        let agents = RowAgents {
            challenger_text: with_overrides(&args.challenger, &overrides.join(",")),
            opponent_text: args.opponent.clone(),
            challenger: AgentDesc::Search { config: cfg },
            opponent: opponent.clone(),
        };
        let done_before = u64::from(args.opts.games) * u64::try_from(k).unwrap_or(0);
        let mut progress =
            |d: usize| printer.update(done_before + u64::try_from(d).unwrap_or(0), total);
        let games = play_row(
            &agents,
            &selection,
            &args.opts,
            args.opts.seed,
            cancel,
            &mut progress,
        )?;
        let mut point = Json::object();
        point.set("scale", *scale);
        let mut budget = Json::object();
        for (f, v) in &values {
            budget.set(f.as_str(), *v);
        }
        point.set("budget", budget);
        row_into(&mut point, &agents, &selection, args.opts.seed, &games);
        points.push(point);
    }
    let mut doc = Json::object();
    doc.set("format", BUDGET_CURVE_FORMAT);
    doc.set("version", FORMAT_VERSION);
    doc.set("challenger", args.challenger.as_str());
    doc.set("opponent", args.opponent.as_str());
    doc.set(
        "fields",
        Json::Array(args.fields.iter().map(|f| f.as_str().into()).collect()),
    );
    doc.set(
        "scales",
        Json::Array(args.scales.iter().map(|s| (*s).into()).collect()),
    );
    args.opts.describe(&mut doc);
    doc.set("points", Json::Array(points));
    emit(&doc, args.opts.out.as_ref(), out)?;
    let mut extra = Json::object();
    extra.set("challenger", args.challenger.as_str());
    extra.set("opponent", args.opponent.as_str());
    extra.set(
        "scales",
        Json::Array(args.scales.iter().map(|s| (*s).into()).collect()),
    );
    args.opts.record("budget-curve", extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::head_to_head::PairSource;
    use crate::{ChooseFirst, DeckSet, PairSet};

    fn opts() -> RunOptions {
        RunOptions {
            source: PairSource::Set {
                decks: DeckSet::Tuning,
                pairs: PairSet::All,
            },
            games: 12,
            seed: 1,
            role: None,
            ledger: std::path::PathBuf::from("ledger.json"),
            workers: gcg_runner::Workers::Count(1),
            choose_first: ChooseFirst::Seat0,
            out: None,
        }
    }

    #[test]
    fn rows_name_their_agents() {
        let individual = AblationArgs {
            row: RowKind::Individual,
            mechanism: "max_iterations=7".to_owned(),
            reference: "strong".to_owned(),
            bundle: Bundle::Spec("search:standard".to_owned()),
            opts: opts(),
        };
        let a = row_agents(&individual).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.challenger_text, "search:null-strong:max_iterations=7");
        assert_eq!(a.opponent_text, "ref:strong");
        let AgentDesc::Search { config } = &a.challenger else {
            panic!("search")
        };
        assert_eq!(config.max_iterations, 7);
        let loo = AblationArgs {
            row: RowKind::LeaveOneOut,
            mechanism: "observation_keys=0".to_owned(),
            ..individual.clone()
        };
        let b = row_agents(&loo).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(b.challenger_text, "search:standard");
        assert_eq!(b.opponent_text, "search:standard:observation_keys=0");
        let (AgentDesc::Search { config: on }, AgentDesc::Search { config: off }) =
            (&b.challenger, &b.opponent)
        else {
            panic!("search agents")
        };
        assert!(on.observation_keys && !off.observation_keys);
        assert_eq!(on.name, off.name, "a variant keeps the per-decision seeds");
        let same = AblationArgs {
            mechanism: "observation_keys=1".to_owned(),
            ..loo.clone()
        };
        assert!(row_agents(&same).is_err());
        let empty = AblationArgs {
            mechanism: String::new(),
            ..loo
        };
        assert!(row_agents(&empty).is_err());
    }

    #[test]
    fn budgets_scale_and_round() {
        assert_eq!(scale_value(16, 0.25), 4);
        assert_eq!(scale_value(3, 0.25), 1);
        assert_eq!(scale_value(10, 0.25), 3);
        assert_eq!(scale_value(640, 8.0), 5120);
        let base = gcg_search::preset_config(gcg_search::Preset::Fast);
        let fields: Vec<String> = BUDGET_FIELDS.iter().map(|s| (*s).to_owned()).collect();
        let (cfg, values) = scaled_config(base, &fields, 2.0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(cfg.max_iterations, base.max_iterations * 2);
        assert_eq!(cfg.min_iterations, base.min_iterations * 2);
        assert_eq!(values.len(), 3);
        let (same, _) = scaled_config(base, &fields, 1.0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&same, base);
        assert!(scaled_config(base, &["exploration".to_owned()], 2.0).is_err());
    }
}
