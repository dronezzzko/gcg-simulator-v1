//! The self-play behind the calibrated policy's fit (W-23, PLAN §12.3 S9), feature `slow`:
//! `fitted_policy_reproduces` replays the self-play on the tuning pairs, refits both models and
//! checks the report `docs/dev/eval/P6-F/policy-fit.json` (and so the constants of `fitted.rs`,
//! which `tests::fitted_constants_are_the_recorded_fit` compares with it). When the fit
//! differs, the regenerated report is written to the system's temporary directory and the
//! failure names it (no crate reads an environment variable, W-108).

use gcg_ai::{AgentCx, Scratch, option_scores};
use gcg_core::derive_seed;
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_effects::Registry;
use gcg_engine::{DecisionKind, NewGameOptions, apply};
use serde_json::{Value, json};

use super::calibrated::{self, OFFSETS};
use super::fit::{self, Decision};
use super::tests::{CHECK_MASTER, FIT_MASTER, null_with, repo, report_path};
use crate::StrengthenedConfig;

/// The decks of the panel's tuning pairs (`docs/dev/eval/panel.json`), as decklists.
fn tuning_pairs() -> Vec<(String, [gcg_data::DeckList; 2])> {
    let panel: Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("docs/dev/eval/panel.json")).expect("the panel"),
    )
    .expect("JSON");
    let path_of = |name: &str| {
        panel["decks"]
            .as_array()
            .expect("decks")
            .iter()
            .find(|d| d["name"] == name)
            .and_then(|d| d["path"].as_str())
            .expect("a deck path")
            .to_owned()
    };
    let list = |name: &str| {
        let deck = gcg_data::load_deck(&repo().join(path_of(name))).expect("a deck");
        gcg_data::to_decklist(&deck).expect("a decklist")
    };
    panel["pairs"]
        .as_array()
        .expect("pairs")
        .iter()
        .filter(|p| p["split"] == "tuning")
        .map(|p| {
            let a = p["decks"][0].as_str().expect("a name");
            let b = p["decks"][1].as_str().expect("a name");
            (format!("{a}:{b}"), [list(a), list(b)])
        })
        .collect()
}

/// One recorded decision of the fit: the decision kind, per option the reference score and the
/// offset features, and the option the search chose.
#[derive(Clone, Debug)]
pub(super) struct Record {
    kind: DecisionKind,
    score: Vec<f64>,
    x: Vec<[f64; calibrated::N]>,
    chosen: usize,
}

/// Plays one self-play game between two `cfg` search agents and records every searched
/// decision (more than one option, not REDRAW or CHOOSE_FIRST) on the true state.
fn play_recorded(
    cfg: &StrengthenedConfig,
    decks: &[gcg_data::DeckList; 2],
    seed: u64,
    agent_seeds: [u64; 2],
) -> Vec<Record> {
    use gcg_ai::Agent;
    let reg = Registry::embedded();
    let mut st = gcg_engine::new_game(
        &reg,
        [&decks[0], &decks[1]],
        seed,
        NewGameOptions::default(),
    )
    .expect("a game");
    let mut agents = agent_seeds.map(|s| crate::make_search_agent(cfg, s, false));
    let knobs = cfg.knobs();
    let cancel = Cancel::new();
    let mut out = Vec::new();
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut cx = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        while let Some(dec) = st.pending() {
            let player = dec.hdr.player;
            let kind = dec.hdr.kind;
            let record = (dec.options.len() > 1
                && !matches!(kind, DecisionKind::Redraw | DecisionKind::ChooseFirst))
            .then(|| {
                let in_battle = !st.battles().is_empty();
                let score = option_scores(&st, &dec, dec.options, &knobs).expect("scores");
                let x = dec
                    .options
                    .iter()
                    .map(|&o| calibrated::features(kind, in_battle, o))
                    .collect::<Vec<_>>();
                (score, x, dec.options.to_vec())
            });
            let action = agents[usize::from(player)]
                .choose(&st, player, &mut cx)
                .expect("a move");
            if let Some((score, x, options)) = record {
                let chosen = options
                    .iter()
                    .position(|&o| o == action)
                    .expect("an option");
                out.push(Record {
                    kind,
                    score,
                    x,
                    chosen,
                });
            }
            apply(&mut st, action, true).expect("a legal move");
        }
    });
    out
}

/// Self-play over the tuning pairs: `games` per pair (decks alternating seats), every game's
/// records in (pair, game) order whatever the threads' schedule.
pub(super) fn selfplay(master: u64, games: u32) -> (Vec<Record>, u32) {
    let cfg = null_with(&gcg_ai_reference::STANDARD, &[]);
    let pairs = tuning_pairs();
    let jobs: Vec<(usize, u32)> = (0..pairs.len())
        .flat_map(|p| (0..games).map(move |g| (p, g)))
        .collect();
    let results: Vec<std::sync::Mutex<Vec<Record>>> = jobs
        .iter()
        .map(|_| std::sync::Mutex::new(Vec::new()))
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(p, g)) = jobs.get(i) else { break };
                    let pi = u32::try_from(p).expect("pairs fit in u32");
                    let seed = derive_seed!(master, "p6f-fit", pi, "game", g);
                    let agent_seeds = [0u32, 1]
                        .map(|s| derive_seed!(master, "p6f-fit", pi, "game", g, "agent", s));
                    let decks = &pairs[p].1;
                    let seats = if g % 2 == 0 {
                        [decks[0].clone(), decks[1].clone()]
                    } else {
                        [decks[1].clone(), decks[0].clone()]
                    };
                    let r = play_recorded(&cfg, &seats, seed, agent_seeds);
                    *results[i].lock().expect("a lock") = r;
                }
            });
        }
    });
    let records = results
        .into_iter()
        .flat_map(|m| m.into_inner().expect("a lock"))
        .collect();
    (
        records,
        u32::try_from(jobs.len()).expect("games fit in u32"),
    )
}

/// The own model's decisions: offset `score / temperature`, the six offset features.
fn own_decisions(records: &[Record], temperature: f64) -> Vec<Decision> {
    records
        .iter()
        .filter(|r| matches!(r.kind, DecisionKind::Main | DecisionKind::ActionStep))
        .map(|r| Decision {
            offset: r.score.iter().map(|s| s / temperature).collect(),
            x: r.x.iter().map(|x| x.to_vec()).collect(),
            chosen: r.chosen,
        })
        .collect()
}

/// The opponent model's decisions: no offset, the reference score then the six features.
fn opponent_decisions(records: &[Record]) -> Vec<Decision> {
    records
        .iter()
        .map(|r| Decision {
            offset: vec![0.0; r.score.len()],
            x: r.score
                .iter()
                .zip(&r.x)
                .map(|(&s, x)| std::iter::once(s).chain(x.iter().copied()).collect())
                .collect(),
            chosen: r.chosen,
        })
        .collect()
}

/// Self-play games per tuning pair.
pub(super) const FIT_GAMES: u32 = 8;
pub(super) const CHECK_GAMES: u32 = 2;
/// The rollout temperature the own model is fitted at (reference `standard`).
pub(super) const FIT_TEMPERATURE: f64 = 0.5;

fn hex(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}

/// The frequency shares of each offset under the reference policy (offsets 0), the fitted own
/// model and the search, on `decisions`.
fn share_table(decisions: &[Decision], beta: &[f64]) -> Value {
    let mut out = serde_json::Map::new();
    for (j, name) in OFFSETS.iter().enumerate() {
        let (reference, search) = fit::shares(decisions, &[0.0; calibrated::N], j);
        let (calibrated, _) = fit::shares(decisions, beta, j);
        out.insert(
            (*name).to_owned(),
            json!({"search": search, "reference_policy": reference, "calibrated": calibrated}),
        );
    }
    Value::Object(out)
}

/// The fit report (`docs/dev/eval/P6-F/policy-fit.json`) from the two self-play runs.
fn fit_report(fit_records: &[Record], games: u32, check: &[Record], check_games: u32) -> Value {
    let own = own_decisions(fit_records, FIT_TEMPERATURE);
    let own_fit = fit::fit(&own, calibrated::N);
    let opp = opponent_decisions(fit_records);
    let opp_fit = fit::fit(&opp, calibrated::N + 1);
    let b0 = opp_fit.beta[0];
    let own_offsets: Vec<f64> = own_fit.beta.iter().map(|b| b * FIT_TEMPERATURE).collect();
    let opp_offsets: Vec<f64> = opp_fit.beta[1..].iter().map(|b| b / b0).collect();
    let named = |v: &[f64]| -> Value {
        Value::Object(
            OFFSETS
                .iter()
                .zip(v)
                .map(|(n, x)| ((*n).to_owned(), json!(x)))
                .collect(),
        )
    };
    let bits = |v: &[f64]| -> Value { Value::Array(v.iter().map(|&x| json!(hex(x))).collect()) };
    let check_own = own_decisions(check, FIT_TEMPERATURE);
    json!({
        "format": "gcg-search-p6f-policy-fit",
        "version": 1,
        "items": ["W-23", "W-25"],
        "generated_by": "crates/gcg-search/src/mech/p6f/tests.rs fitted_policy_reproduces (cargo test -p gcg-search --release --features slow --lib fitted_policy_reproduces)",
        "searcher": "search:null-standard (the reference standard configuration, every mechanism off) in both seats",
        "decks": "the 15 tuning pairs of docs/dev/eval/panel.json, decks alternating seats",
        "fit": {"role": "tuning", "master": FIT_MASTER, "games_per_pair": FIT_GAMES, "games": games,
                "decisions": fit_records.len(), "main_and_action_step_decisions": own.len()},
        "check": {"role": "dev", "master": CHECK_MASTER, "games_per_pair": CHECK_GAMES,
                  "games": check_games, "decisions": check.len(),
                  "main_and_action_step_decisions": check_own.len()},
        "method": "conditional logit over each decision's options, maximum likelihood by Newton's method (IRLS) in f64 with fixed-order sums, portable exp/log, ridge 1e-3, Cholesky solve",
        "own": {
            "model": "logit = reference_score / 0.5 + beta . offsets; the offsets in score units are 0.5 * beta",
            "offsets": named(&own_offsets),
            "offset_bits": bits(&own_offsets),
            "se_logit": named(&own_fit.se),
            "log_likelihood": own_fit.log_likelihood,
            "iterations": own_fit.iterations,
            "converged": own_fit.converged,
            "shares_fit": share_table(&own, &own_fit.beta),
            "shares_check": share_table(&check_own, &own_fit.beta),
        },
        "opponent_predictive": {
            "model": "logit = beta0 * reference_score + beta . offsets over every recorded decision; temperature 1 / beta0, offsets beta / beta0",
            "use": "not used: the predictive temperature models the opponent as nearly random; opponent_knobs takes the own offsets at 0.25 x the rollout temperature (calibrated.rs)",
            "score_coefficient": b0,
            "score_coefficient_bits": hex(b0),
            "temperature": 1.0 / b0,
            "offsets": named(&opp_offsets),
            "offset_bits": bits(&opp_offsets),
            "se_logit": Value::Array(opp_fit.se.iter().map(|&x| json!(x)).collect()),
            "log_likelihood": opp_fit.log_likelihood,
            "iterations": opp_fit.iterations,
            "converged": opp_fit.converged,
        },
    })
}

/// Regenerates the fit from self-play and checks it against `fitted.rs` and the recorded
/// report; a different fit is written to the temporary directory for review and fails the test.
#[test]
fn fitted_policy_reproduces() {
    let (records, games) = selfplay(FIT_MASTER, FIT_GAMES);
    let (check, check_games) = selfplay(CHECK_MASTER, CHECK_GAMES);
    let report = fit_report(&records, games, &check, check_games);
    let text = serde_json::to_string_pretty(&report).expect("JSON") + "\n";
    let recorded = std::fs::read_to_string(report_path()).expect("the recorded fit");
    if text != recorded {
        let out = std::env::temp_dir().join("gcg-p6f-policy-fit.json");
        std::fs::write(&out, &text).expect("the regenerated report");
        panic!(
            "the fit changed: review the regenerated report {} against {}",
            out.display(),
            report_path().display()
        );
    }
}
