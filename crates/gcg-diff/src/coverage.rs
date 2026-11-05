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
//!
//! Two more parts read records the Python reference produced over games and tests that the Rust
//! replay reproduces (docs/port/WEAKNESSES.md W-111, W-151, W-117):
//!
//! - **Execution evidence (W-111).** `scripts/test_census.py` records, per card-tagged Python
//!   test, which tagged cards' programs ran ([`TEST_EXECUTION`]). A passing per-test trace counts
//!   as execution evidence for a card only when that record shows the card's program ran in the
//!   test. A non-vanilla card with a program needs such a trace, or a run in the corpus census
//!   games or the fuzz census; a card without any program (keyword-only, such as a <Blocker>
//!   Unit) has nothing to run and is reported apart. The check fails on a card with a program
//!   and no execution evidence; it reports, without failing, every card whose only execution
//!   evidence is a census game (a Python-side test gap) and every tagged test that never runs a
//!   tagged card.
//! - **Fuzz census (W-151, W-117).** `scripts/fuzz_census.py` records, per card number, the
//!   seed-2026 fuzz games whose decks held it, in which one of its programs ran, and the
//!   instances effects or the setup created ([`FUZZ_CENSUS`]); `gcg-diff fuzz --compare` proves
//!   the Rust fuzz plays the same games. The Rust deck generator must give the census's deck
//!   membership (all 1,064 pool numbers). Every pool card with a program that never ran is listed
//!   with the passing tagged test that covers it, and each of the 88 effect-only numbers (Unit
//!   tokens, EX Bases, EX Resources) needs a creation in fuzz or a golden record and a tagged
//!   test. The check fails on any card without that evidence.

use gcg_data::{CardType, DefId};
use gcg_effects::Registry;
use serde_json::Value;

use crate::evidence::{Evidence, collect};
use crate::fuzz::Pools;
use crate::manifest::Repo;

/// The per-test execution record of `scripts/test_census.py` (W-111).
pub const TEST_EXECUTION: &str = "docs/port/eval/P6-N/test-execution.json";
/// The fuzz census of `scripts/fuzz_census.py` for the seed-2026 10,000-game run (W-151).
pub const FUZZ_CENSUS: &str = "docs/port/eval/P6-N/fuzz-census-2026-10000.json";

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

fn read_json(repo: &Repo, rel: &str) -> Result<Value, String> {
    let p = repo.root.join(rel);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn strings(v: &Value, what: &str) -> Result<Vec<String>, String> {
    v.as_array()
        .ok_or_else(|| format!("{what}: not a list"))?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{what}: an entry is not a string"))
        })
        .collect()
}

/// One card-tagged Python test of [`TEST_EXECUTION`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TestRun {
    /// The pytest node id, which is the per-test trace id.
    pub id: String,
    pub tagged: Vec<String>,
    /// The tagged numbers whose program ran during the test.
    pub executed: Vec<String>,
}

/// [`TEST_EXECUTION`]: per card-tagged test, the tagged cards whose programs ran.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TestExecution {
    pub tests: Vec<TestRun>,
    /// Non-vanilla real cards without any program (keyword-only abilities).
    pub programless: Vec<String>,
    /// Each effect-only number some test instantiates, with the first such test.
    pub effect_only_instantiated: Vec<(String, String)>,
}

impl TestExecution {
    /// Parses the record.
    ///
    /// # Errors
    /// A malformed record.
    pub fn from_value(v: &Value) -> Result<Self, String> {
        if v["format"] != 1 {
            return Err(format!("{TEST_EXECUTION}: format {} is not 1", v["format"]));
        }
        let tests = v["tests"]
            .as_object()
            .ok_or_else(|| format!("{TEST_EXECUTION}: no tests"))?
            .iter()
            .map(|(id, row)| {
                Ok(TestRun {
                    id: id.clone(),
                    tagged: strings(&row["tagged"], id)?,
                    executed: strings(&row["executed"], id)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let effect_only_instantiated = v["effect_only_instantiated"]
            .as_object()
            .ok_or_else(|| format!("{TEST_EXECUTION}: no effect_only_instantiated"))?
            .iter()
            .map(|(n, t)| {
                t.as_str()
                    .map(|t| (n.clone(), t.to_owned()))
                    .ok_or_else(|| format!("{TEST_EXECUTION}: {n}: not a test id"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            tests,
            programless: strings(&v["cards_without_programs"], TEST_EXECUTION)?,
            effect_only_instantiated,
        })
    }
}

/// One card number of [`FUZZ_CENSUS`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuzzCard {
    pub number: String,
    /// Games in which a deck held the card.
    pub decks: u64,
    /// Games in which one of its programs ran.
    pub ran: u64,
    /// Instances created by effects or the setup procedure.
    pub created: u64,
}

/// [`FUZZ_CENSUS`]: the executed-program census of the fuzz games.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FuzzCensus {
    pub games: u64,
    pub seed: u64,
    pub failures: u64,
    pub pool: Vec<String>,
    /// Numbers with a non-zero column, in number order.
    pub cards: Vec<FuzzCard>,
}

impl FuzzCensus {
    /// Parses the census.
    ///
    /// # Errors
    /// A malformed census.
    pub fn from_value(v: &Value) -> Result<Self, String> {
        let num = |k: &str| {
            v[k].as_u64()
                .ok_or_else(|| format!("{FUZZ_CENSUS}: no {k}"))
        };
        if v["format"] != 1 || v["columns"] != serde_json::json!(["decks", "ran", "created"]) {
            return Err(format!("{FUZZ_CENSUS}: not a format-1 census"));
        }
        let cards = v["cards"]
            .as_object()
            .ok_or_else(|| format!("{FUZZ_CENSUS}: no cards"))?
            .iter()
            .map(|(n, row)| {
                let col = |i: usize| {
                    row[i]
                        .as_u64()
                        .ok_or_else(|| format!("{FUZZ_CENSUS}: {n}: column {i}"))
                };
                Ok(FuzzCard {
                    number: n.clone(),
                    decks: col(0)?,
                    ran: col(1)?,
                    created: col(2)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            games: num("games")?,
            seed: num("seed")?,
            failures: num("failures")?,
            pool: strings(&v["pool"], FUZZ_CENSUS)?,
            cards,
        })
    }

    fn card(&self, n: &str) -> Option<&FuzzCard> {
        self.cards.iter().find(|c| c.number == n)
    }

    /// Whether one of `n`'s programs ran in a fuzz game.
    #[must_use]
    pub fn ran(&self, n: &str) -> bool {
        self.card(n).is_some_and(|c| c.ran > 0)
    }
}

/// How the non-vanilla cards' behaviour is backed by execution (W-111).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Execution {
    pub non_vanilla: usize,
    /// A passing tagged per-test trace runs one of the card's programs.
    pub by_tagged_test: usize,
    /// Only a census game (corpus or fuzz) runs the card's programs: Python-side test gaps.
    pub by_census_only: Vec<String>,
    /// Cards without any program (keyword-only).
    pub programless: Vec<String>,
    /// Cards with a program and no execution evidence.
    pub missing: Vec<String>,
    /// Card-tagged tests, and those that never run one of their tagged cards.
    pub tagged_tests: usize,
    pub tests_not_running_a_tag: Vec<String>,
}

/// Judges every non-vanilla real card against the execution record, the corpus census
/// (`census_not_ran`, credited only with `census_ok`) and the fuzz census.
#[must_use]
pub fn judge_execution(
    reg: &Registry,
    ev: &Evidence,
    exec: &TestExecution,
    census: (bool, &[String]),
    fuzz: &FuzzCensus,
) -> Execution {
    let (census_ok, not_ran) = census;
    let mut out = Execution {
        tagged_tests: exec.tests.len(),
        tests_not_running_a_tag: exec
            .tests
            .iter()
            .filter(|t| t.executed.len() < t.tagged.len())
            .map(|t| t.id.clone())
            .collect(),
        ..Execution::default()
    };
    for cd in reg.db().real_cards() {
        if is_vanilla(reg, cd.def_id) {
            continue;
        }
        let n = &*cd.card_number;
        out.non_vanilla += 1;
        if exec.programless.iter().any(|x| x == n) {
            out.programless.push(n.to_owned());
        } else if exec
            .tests
            .iter()
            .any(|t| t.executed.iter().any(|x| x == n) && ev.passed(&t.id))
        {
            out.by_tagged_test += 1;
        } else if (census_ok && !not_ran.iter().any(|x| x == n)) || fuzz.ran(n) {
            out.by_census_only.push(n.to_owned());
        } else {
            out.missing.push(n.to_owned());
        }
    }
    out
}

/// The fuzz part of the four-part differential report (W-117) and the census report (W-151).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FuzzCoverage {
    pub pool: usize,
    /// Pool numbers the Rust deck generator puts in a deck of the census's games.
    pub in_decks: usize,
    /// Pool cards with a program, and those whose program ran.
    pub program_cards: usize,
    pub ran: usize,
    /// Pool cards with a program that never ran, with the passing tagged test covering each.
    pub never_ran: Vec<(String, Option<String>)>,
    /// The effect-only numbers (Unit tokens, EX Bases, EX Resources) with their creation count.
    pub effect_only: Vec<(String, u64)>,
    /// Effect-only numbers never created in fuzz, with the evidence covering each: a passing
    /// per-test trace that instantiates it, a created vanilla printing with the same
    /// characteristics, or a tagged test.
    pub effect_only_uncreated: Vec<(String, Option<String>)>,
    pub problems: Vec<String>,
}

/// A passing tagged trace (preferring one that runs the card's program) or a native test
/// tagged with `n`.
fn covering_test(ev: &Evidence, exec: &TestExecution, n: &str) -> Option<String> {
    let tag = format!("card:{n}");
    exec.tests
        .iter()
        .find(|t| t.executed.iter().any(|x| x == n) && ev.passed(&t.id))
        .map(|t| t.id.clone())
        .or_else(|| {
            ev.test_tags
                .iter()
                .find(|(id, tags)| tags.contains(&tag) && ev.passed(id))
                .map(|(id, _)| id.clone())
        })
        .or_else(|| {
            ev.native
                .iter()
                .find(|t| t.tags.iter().any(|(k, v)| k == "card" && v == n))
                .map(|t| t.id.clone())
        })
}

/// Whether `a` and `b` are vanilla printings with the same characteristics: no ability in the
/// registry, and equal type, name, colour, level, cost, AP, HP, traits, zones and link. Such a
/// card behaves exactly like the other one wherever the engine puts it.
#[must_use]
pub fn same_vanilla_behaviour(
    reg: &Registry,
    a: &gcg_data::CardDef,
    b: &gcg_data::CardDef,
) -> bool {
    is_vanilla(reg, a.def_id)
        && is_vanilla(reg, b.def_id)
        && a.card_type == b.card_type
        && a.name == b.name
        && a.color == b.color
        && (a.level, a.cost, a.ap, a.hp) == (b.level, b.cost, b.ap, b.hp)
        && a.traits == b.traits
        && a.zones == b.zones
        && a.link == b.link
}

/// The pool numbers in a deck of the census's games, from the Rust deck generator.
#[must_use]
pub fn rust_deck_membership(pools: &Pools<'_>, games: u64, seed: u64) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for i in 0..2 * games {
        let d = pools.deck_for(i, seed);
        seen.extend(d.main.into_iter().chain(d.resources));
        seen.sort();
        seen.dedup();
    }
    seen
}

/// Judges the fuzz census: deck membership against the Rust generator, execution per pool
/// card, and the effect-only numbers against creations, golden records and tagged tests.
#[must_use]
pub fn judge_fuzz(
    reg: &Registry,
    ev: &Evidence,
    exec: &TestExecution,
    fuzz: &FuzzCensus,
    rust_pool: &[&str],
    rust_decks: &[String],
    golden: &[String],
) -> FuzzCoverage {
    let mut out = FuzzCoverage {
        pool: fuzz.pool.len(),
        ..FuzzCoverage::default()
    };
    if fuzz
        .pool
        .iter()
        .map(String::as_str)
        .ne(rust_pool.iter().copied())
    {
        out.problems.push(format!(
            "{FUZZ_CENSUS}: the census pool ({} numbers) is not the Rust fuzz pool ({})",
            fuzz.pool.len(),
            rust_pool.len()
        ));
    }
    if fuzz.failures > 0 {
        out.problems
            .push(format!("{FUZZ_CENSUS}: {} failed games", fuzz.failures));
    }
    let census_decks: Vec<&str> = fuzz
        .cards
        .iter()
        .filter(|c| c.decks > 0)
        .map(|c| c.number.as_str())
        .collect();
    if census_decks
        .iter()
        .copied()
        .ne(rust_decks.iter().map(String::as_str))
    {
        out.problems.push(format!(
            "{FUZZ_CENSUS}: deck membership differs from the Rust deck generator ({} vs {})",
            census_decks.len(),
            rust_decks.len()
        ));
    }
    out.in_decks = rust_pool
        .iter()
        .filter(|n| rust_decks.binary_search_by(|x| x.as_str().cmp(n)).is_ok())
        .count();
    if out.in_decks != rust_pool.len() {
        out.problems.push(format!(
            "fuzz deck membership: {}/{} pool numbers",
            out.in_decks,
            rust_pool.len()
        ));
    }
    let db = reg.db();
    for n in rust_pool {
        let Some(cd) = db.get(n) else { continue };
        if is_vanilla(reg, cd.def_id) || exec.programless.iter().any(|x| x == n) {
            continue;
        }
        out.program_cards += 1;
        if fuzz.ran(n) {
            out.ran += 1;
        } else {
            let test = covering_test(ev, exec, n);
            if test.is_none() {
                out.problems.push(format!(
                    "{n}: its program never ran in fuzz and no passing tagged test covers it"
                ));
            }
            out.never_ran.push(((*n).to_owned(), test));
        }
    }
    let effect_only: Vec<&gcg_data::CardDef> = db
        .real_cards()
        .filter(|cd| {
            matches!(
                cd.card_type,
                CardType::UnitToken | CardType::ExBase | CardType::ExResource
            )
        })
        .collect();
    let created_in_fuzz = |n: &str| fuzz.card(n).map_or(0, |c| c.created);
    for cd in &effect_only {
        let n = &*cd.card_number;
        let created = created_in_fuzz(n);
        out.effect_only.push((n.to_owned(), created));
        if created > 0 {
            continue;
        }
        let has_golden = golden.iter().any(|g| g == n);
        let in_test = exec
            .effect_only_instantiated
            .iter()
            .find(|(x, t)| x == n && ev.passed(t))
            .map(|(_, t)| format!("instantiated in the passing trace {t}"));
        let alternate = effect_only
            .iter()
            .find(|c| created_in_fuzz(&c.card_number) > 0 && same_vanilla_behaviour(reg, cd, c))
            .map(|c| {
                format!(
                    "a vanilla alternate printing of {} (identical characteristics; {} created \
                     in fuzz)",
                    c.card_number,
                    created_in_fuzz(&c.card_number)
                )
            });
        let evidence = in_test
            .or(alternate)
            .or_else(|| covering_test(ev, exec, n).map(|t| format!("tagged test {t}")));
        if evidence.is_none() || !has_golden {
            out.problems.push(format!(
                "{n}: an effect-only number never created in fuzz without a golden record \
                 and a test that covers it (golden: {has_golden})"
            ));
        }
        out.effect_only_uncreated.push((n.to_owned(), evidence));
    }
    out
}

/// The card numbers of the golden records (`tests/effects/golden/*.json`, keys of each file).
///
/// # Errors
/// An unreadable golden file.
pub fn golden_numbers(repo: &Repo) -> Result<Vec<String>, String> {
    let dir = repo.root.join("tests/effects/golden");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for p in files {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        out.extend(v.as_object().into_iter().flat_map(|m| m.keys().cloned()));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn listed(items: &[(String, Option<String>)]) -> Vec<String> {
    items
        .iter()
        .map(|(n, t)| {
            format!(
                "    {n}: {}",
                t.as_deref().unwrap_or("NO COVERING EVIDENCE")
            )
        })
        .collect()
}

/// The execution and fuzz-census report lines and problems (W-111, W-151, W-117).
///
/// # Errors
/// An unreadable record.
pub fn execution_report(
    repo: &Repo,
    reg: &Registry,
    ev: &Evidence,
    cov: &Coverage,
    not_ran: &[String],
) -> Result<(Vec<String>, Vec<String>), String> {
    let exec = TestExecution::from_value(&read_json(repo, TEST_EXECUTION)?)?;
    let fuzz = FuzzCensus::from_value(&read_json(repo, FUZZ_CENSUS)?)?;
    let census_ok = ev.games > 0 && ev.failed_games == 0;
    let ex = judge_execution(reg, ev, &exec, (census_ok, not_ran), &fuzz);
    let pools = Pools::new(reg);
    let pool = pools.all();
    let decks = rust_deck_membership(&pools, fuzz.games, fuzz.seed);
    let golden = golden_numbers(repo)?;
    let fz = judge_fuzz(reg, ev, &exec, &fuzz, &pool, &decks, &golden);
    let real = reg.db().real_cards().count();
    let with_golden = reg
        .db()
        .real_cards()
        .filter(|c| {
            golden
                .binary_search_by(|g| g.as_str().cmp(&c.card_number))
                .is_ok()
        })
        .count();
    let mut lines = vec![
        format!(
            "execution (W-111, {TEST_EXECUTION}): {} non-vanilla cards; {} run by a passing \
             tagged trace; {} run only in the census games or fuzz (Python-side test gaps: {}); \
             {} without a program (keyword-only); {} with a program and no execution evidence",
            ex.non_vanilla,
            ex.by_tagged_test,
            ex.by_census_only.len(),
            ex.by_census_only.join(", "),
            ex.programless.len(),
            ex.missing.len()
        ),
        format!(
            "execution (W-111): {} of {} card-tagged Python tests never run one of their tagged \
             cards' programs (listed per test in {TEST_EXECUTION})",
            ex.tests_not_running_a_tag.len(),
            ex.tagged_tests
        ),
        format!(
            "differential report (W-117) 1, golden records and tagged tests: {with_golden}/{real} \
             numbers with a golden record, {}/{real} with Rust evidence",
            real - cov.missing.len()
        ),
        format!(
            "differential report (W-117) 2, deck membership in the fuzz games ({} games, seed \
             {}, {} failures): {}/{}",
            fuzz.games, fuzz.seed, fuzz.failures, fz.in_decks, fz.pool
        ),
        format!(
            "differential report (W-117) 3, execution in the fuzz games (W-151): {} of {} pool \
             cards with a program ran; the {} that never ran and the passing tagged test covering \
             each:",
            fz.ran,
            fz.program_cards,
            fz.never_ran.len()
        ),
    ];
    lines.extend(listed(&fz.never_ran));
    let created: Vec<String> = fz
        .effect_only
        .iter()
        .filter(|(_, c)| *c > 0)
        .map(|(n, c)| format!("{n} x{c}"))
        .collect();
    lines.push(format!(
        "differential report (W-117) 4, effect-only numbers: {} in all; {} created in fuzz ({}); \
         {} never created, each with a golden record and the evidence covering it:",
        fz.effect_only.len(),
        created.len(),
        created.join(", "),
        fz.effect_only_uncreated.len()
    ));
    lines.extend(listed(&fz.effect_only_uncreated));
    if with_golden != real {
        lines.push(format!(
            "  ({} numbers have no golden record)",
            real - with_golden
        ));
    }
    let mut problems: Vec<String> = ex
        .missing
        .iter()
        .map(|n| format!("no execution evidence: {n} has a program that no test or game ran"))
        .collect();
    problems.extend(fz.problems);
    Ok((lines, problems))
}

/// Runs the check: the report lines and the problems.
///
/// # Errors
/// The corpus or the census could not be read.
pub fn check(repo: &Repo) -> Result<(Vec<String>, Vec<String>), String> {
    let ev = collect(repo, None, true)?;
    let reg = Registry::embedded();
    let not_ran = census_not_ran(repo)?;
    let cov = judge(&reg, &ev, &not_ran);
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
    let (more, more_problems) = execution_report(repo, &reg, &ev, &cov, &not_ran)?;
    let mut lines = lines;
    lines.extend(more);
    problems.extend(more_problems);
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

    const CARD: &str = "GD01-001";
    const TEST: &str = "tests/cards/test_lead_examples.py::test_gd01_001_gains_repair_itself";

    fn passing_trace() -> Evidence {
        Evidence {
            tests: vec![(TEST.to_owned(), true)],
            test_tags: vec![(TEST.to_owned(), vec![format!("card:{CARD}")])],
            ..Evidence::default()
        }
    }

    fn run(executed: &[&str]) -> TestExecution {
        TestExecution {
            tests: vec![TestRun {
                id: TEST.to_owned(),
                tagged: vec![CARD.to_owned()],
                executed: executed.iter().map(|s| (*s).to_owned()).collect(),
            }],
            programless: Vec::new(),
            effect_only_instantiated: Vec::new(),
        }
    }

    #[test]
    fn w111_a_tagged_test_that_never_runs_its_card_is_not_execution_evidence() {
        let reg = Registry::embedded();
        let ev = passing_trace();
        let none = FuzzCensus::default();
        let tag_only = judge_execution(&reg, &ev, &run(&[]), (false, &[]), &none);
        assert!(tag_only.missing.iter().any(|n| n == CARD));
        assert_eq!(tag_only.tests_not_running_a_tag, vec![TEST.to_owned()]);
        let executed = judge_execution(&reg, &ev, &run(&[CARD]), (false, &[]), &none);
        assert!(!executed.missing.iter().any(|n| n == CARD));
        assert_eq!(executed.by_tagged_test, 1);
        assert!(executed.tests_not_running_a_tag.is_empty());
        // A failing replay of the test is no evidence either.
        let failing = Evidence {
            tests: vec![(TEST.to_owned(), false)],
            ..passing_trace()
        };
        let lost = judge_execution(&reg, &failing, &run(&[CARD]), (false, &[]), &none);
        assert!(lost.missing.iter().any(|n| n == CARD));
        // A census game that ran it makes it a Python-side test gap, not a failure.
        let gap = judge_execution(&reg, &ev, &run(&[]), (true, &[]), &none);
        assert!(gap.by_census_only.iter().any(|n| n == CARD));
        assert!(!gap.missing.iter().any(|n| n == CARD));
    }

    #[test]
    fn w151_a_card_in_a_deck_whose_program_never_ran_is_reported_as_not_executed() {
        let reg = Registry::embedded();
        let pools = Pools::new(&reg);
        let pool = pools.all();
        let decks: Vec<String> = pool.iter().map(|s| (*s).to_owned()).collect();
        let census = |ran: u64| FuzzCensus {
            games: 1,
            seed: 2026,
            failures: 0,
            pool: decks.clone(),
            cards: decks
                .iter()
                .map(|n| FuzzCard {
                    number: n.clone(),
                    decks: 3,
                    ran: if n == CARD { ran } else { 1 },
                    created: 0,
                })
                .collect(),
        };
        let golden: Vec<String> = reg
            .db()
            .real_cards()
            .map(|c| c.card_number.to_string())
            .collect();
        let ev = passing_trace();
        let never = judge_fuzz(&reg, &ev, &run(&[CARD]), &census(0), &pool, &decks, &golden);
        assert_eq!(never.in_decks, 1064);
        assert_eq!(
            never.never_ran,
            vec![(CARD.to_owned(), Some(TEST.to_owned()))]
        );
        assert_eq!(never.ran + 1, never.program_cards);
        let ran = judge_fuzz(&reg, &ev, &run(&[CARD]), &census(2), &pool, &decks, &golden);
        assert!(ran.never_ran.is_empty());
        // Without a covering test the card is a problem, and so is a deck-membership mismatch.
        let bare = judge_fuzz(
            &reg,
            &Evidence::default(),
            &run(&[]),
            &census(0),
            &pool,
            &decks[1..],
            &golden,
        );
        assert!(bare.problems.iter().any(|p| p.starts_with(CARD)));
        assert!(bare.problems.iter().any(|p| p.contains("deck membership")));
        // The 88 effect-only numbers, none created here: each needs covering evidence.
        assert_eq!(bare.effect_only.len(), 88);
        assert_eq!(bare.effect_only_uncreated.len(), 88);
        assert!(bare.problems.iter().any(|p| p.starts_with("EXB-002:")));
    }

    #[test]
    fn w117_an_uncreated_effect_only_number_needs_a_trace_an_identical_printing_or_a_tag() {
        let reg = Registry::embedded();
        let pools = Pools::new(&reg);
        let pool = pools.all();
        let decks: Vec<String> = pool.iter().map(|s| (*s).to_owned()).collect();
        let golden: Vec<String> = reg
            .db()
            .real_cards()
            .map(|c| c.card_number.to_string())
            .collect();
        let created = |numbers: &[&str]| FuzzCensus {
            games: 1,
            seed: 2026,
            failures: 0,
            pool: decks.clone(),
            cards: decks
                .iter()
                .map(|n| (n.as_str(), 1, 1, 0))
                .chain(numbers.iter().map(|n| (*n, 0, 0, 5)))
                .map(|(n, d, r, c)| FuzzCard {
                    number: n.to_owned(),
                    decks: d,
                    ran: r,
                    created: c,
                })
                .collect(),
        };
        let ev = passing_trace();
        let mut exec = run(&[CARD]);
        let line = |fz: &FuzzCoverage, n: &str| {
            fz.effect_only_uncreated
                .iter()
                .find(|(x, _)| x == n)
                .and_then(|(_, e)| e.clone())
        };
        // EXB-002 is a vanilla printing of the EX Base EXB-001.
        let fz = judge_fuzz(
            &reg,
            &ev,
            &exec,
            &created(&["EXB-001"]),
            &pool,
            &decks,
            &golden,
        );
        assert!(
            line(&fz, "EXB-002")
                .is_some_and(|e| e.starts_with("a vanilla alternate printing of EXB-001")),
            "{:?}",
            line(&fz, "EXB-002")
        );
        assert!(!fz.problems.iter().any(|p| p.starts_with("EXB-002:")));
        // The EX Resource is no EX Base printing.
        assert!(fz.problems.iter().any(|p| p.starts_with("EXR-002:")));
        // A passing trace that instantiates the number covers it.
        exec.effect_only_instantiated = vec![("EXR-002".to_owned(), TEST.to_owned())];
        let fz = judge_fuzz(
            &reg,
            &ev,
            &exec,
            &created(&["EXB-001"]),
            &pool,
            &decks,
            &golden,
        );
        assert_eq!(
            line(&fz, "EXR-002"),
            Some(format!("instantiated in the passing trace {TEST}"))
        );
        assert!(!same_vanilla_behaviour(
            &reg,
            reg.db().get("EXB-001").expect("EXB-001"),
            reg.db().get("EXR-001").expect("EXR-001")
        ));
    }
}
