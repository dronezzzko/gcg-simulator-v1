//! `gcg-diff coverage --check` (PLAN §9.3 "Behaviour"): every real card has Rust evidence.
//!
//! - A non-vanilla card (its registry entry has abilities; 840 numbers) needs a passing Rust
//!   replay of a per-test trace tagged with its number, a native Rust test tagged with it, or
//!   an executed program in the corpus games. The last is the Python executed-program census
//!   (`port/corpus/v1/census.json`, `port/oracle/census.py`), which counts for Rust only when
//!   every census game trace passes the real-engine replay: a passing replay reproduces every
//!   recorded state digest, so the same programs ran.
//! - A vanilla card (312 numbers) needs a card of it in a state the Rust replay reached, or is
//!   a token or EX card, which the load-time IR check covers (vanilla tokens that nothing
//!   creates, PLAN §9.3).
//!
//! The report lists every card failing both, and the check fails on any.

use gcg_data::DefId;
use gcg_effects::Registry;
use serde_json::Value;

use crate::evidence::{Evidence, collect};
use crate::manifest::Repo;

/// The outcome of the check.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Coverage {
    pub non_vanilla: usize,
    pub vanilla: usize,
    pub by_trace: usize,
    pub by_native: usize,
    pub by_census: usize,
    pub instantiated: usize,
    pub tokens: usize,
    /// Cards without Rust evidence.
    pub missing: Vec<String>,
}

/// The non-vanilla cards the Python census saw run (`non_vanilla` minus
/// `non_vanilla_not_ran`).
///
/// # Errors
/// An unreadable census.
pub fn census_not_ran(repo: &Repo) -> Result<Vec<String>, String> {
    let p = repo.root.join("port/corpus/v1/census.json");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    v["cards"]["non_vanilla_not_ran"]
        .as_array()
        .ok_or_else(|| format!("{}: no cards.non_vanilla_not_ran", p.display()))?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{}: a card number is not a string", p.display()))
        })
        .collect()
}

/// Whether the real card `def` is vanilla (`census.py`: no own and no Unit abilities).
#[must_use]
pub fn is_vanilla(reg: &Registry, def: DefId) -> bool {
    let c = reg.card(def);
    c.own.is_empty() && c.unit.is_empty()
}

/// Judges every real card against the evidence.
#[must_use]
pub fn judge(reg: &Registry, ev: &Evidence, not_ran: &[String]) -> Coverage {
    let census_ok = ev.games > 0 && ev.failed_games == 0;
    let mut cov = Coverage::default();
    for cd in reg.db().real_cards() {
        let n = &*cd.card_number;
        if is_vanilla(reg, cd.def_id) {
            cov.vanilla += 1;
            if ev
                .instantiated
                .get(usize::from(cd.def_id.0))
                .copied()
                .unwrap_or(false)
            {
                cov.instantiated += 1;
            } else if cd.is_token() {
                cov.tokens += 1;
            } else {
                cov.missing
                    .push(format!("{n} (vanilla, never instantiated)"));
            }
            continue;
        }
        cov.non_vanilla += 1;
        let tag = format!("card:{n}");
        if ev
            .test_tags
            .iter()
            .any(|(id, tags)| tags.contains(&tag) && ev.passed(id))
        {
            cov.by_trace += 1;
        } else if ev
            .native
            .iter()
            .any(|t| t.tags.iter().any(|(k, v)| k == "card" && v == n))
        {
            cov.by_native += 1;
        } else if census_ok && !not_ran.iter().any(|x| x == n) {
            cov.by_census += 1;
        } else {
            cov.missing.push(format!("{n} (non-vanilla)"));
        }
    }
    cov
}

/// Runs the check: the report lines and the problems.
///
/// # Errors
/// The corpus or the census could not be read.
pub fn check(repo: &Repo) -> Result<(Vec<String>, Vec<String>), String> {
    let ev = collect(repo, None, true)?;
    let reg = Registry::embedded();
    let cov = judge(&reg, &ev, &census_not_ran(repo)?);
    let passed = ev.tests.iter().filter(|(_, ok)| *ok).count();
    let lines = vec![
        format!(
            "per-test traces: {passed} of {} passed; census games: {} of {} passed",
            ev.tests.len(),
            ev.games - ev.failed_games,
            ev.games
        ),
        format!(
            "non-vanilla: {} cards, {} by a passing tagged trace, {} by a native test, {} by the census",
            cov.non_vanilla, cov.by_trace, cov.by_native, cov.by_census
        ),
        format!(
            "vanilla: {} cards, {} instantiated by the Rust replay, {} tokens (load-time check)",
            cov.vanilla, cov.instantiated, cov.tokens
        ),
    ];
    let mut problems = ev.errors.clone();
    problems.extend(cov.missing.iter().map(|m| format!("no Rust evidence: {m}")));
    Ok((lines, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_credits_cards_in_order_of_strength() {
        let reg = Registry::embedded();
        let real: Vec<_> = reg.db().real_cards().collect();
        assert_eq!(real.len(), 1152);
        let vanilla = real.iter().filter(|c| is_vanilla(&reg, c.def_id)).count();
        assert_eq!(vanilla, 312);
        // Without any evidence every non-vanilla card and every non-token vanilla card is
        // missing.
        let none = judge(&reg, &Evidence::default(), &[]);
        assert_eq!(none.non_vanilla, 840);
        assert!(none.missing.len() > 840);
        // A passing tagged trace, and a passing census.
        let first = real
            .iter()
            .find(|c| !is_vanilla(&reg, c.def_id))
            .map(|c| c.card_number.to_string())
            .expect("a non-vanilla card");
        let mut ev = Evidence {
            tests: vec![("t".to_owned(), true)],
            test_tags: vec![("t".to_owned(), vec![format!("card:{first}")])],
            games: 1,
            failed_games: 0,
            instantiated: vec![true; reg.db().len()],
            ..Evidence::default()
        };
        let all = judge(&reg, &ev, &[]);
        assert!(all.missing.is_empty(), "{:?}", all.missing);
        assert_eq!(all.by_trace, 1);
        assert_eq!(all.by_census, 839);
        ev.failed_games = 1;
        let failed = judge(&reg, &ev, &[]);
        assert_eq!(failed.missing.len(), 839);
    }
}
