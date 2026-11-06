//! P6-L validator items (docs/dev/PLAN.md §12.2 P6-L; docs/dev/eval/weakness-status/P6-L.md):
//! the extended legality check of `gcg_data::legality` behind `gcg-eval validate`.
//!
//! - W-72: the dated, region-tagged B&R model; the default check equals `validate_deck` on the
//!   whole Python deck corpus (`decks.json`); a two-entry fixture selects by date; a date before
//!   every list and an unknown region are errors.
//! - W-71: the starter-deck exception data hook, with a synthetic ST02/ST05 decklist fixture.
//! - W-75: no list applied (`--no-banlist`).
//! - W-76: the 7-day new-card check of an event date and the scope statement.
//! - W-77: hints for EX tokens and `#` comments, violations unchanged.
//! - W-78: rules and simulator-support violations kept apart.
//! - W-144: huge counts through a deck file and the extended check.
//!
//! Fixtures are modified copies of the packaged `banlist.json` held in memory; the packaged
//! files are never edited.

mod common;

use common::{Unimplemented, arr, deck_from, fixture, s, violation_rows};
use gcg_data::legality::{
    BanlistHistory, IsoDate, LegalityOptions, LegalityReport, LegalitySources, ListRequest,
    ReleaseDates, Scope, SelectError, check, releases::number_set, text_hints,
};
use gcg_data::{
    CardDb, Deck, DeckError, ImplementationStatus, ValidateOptions, ViolationCode, official,
    parse_deck, validate,
};
use std::sync::OnceLock;

use serde_json::{Value, json};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn banlist_doc() -> Value {
    serde_json::from_str(&read("src/gcg_sim/data/official/banlist.json")).expect("banlist.json")
}

fn date(s: &str) -> IsoDate {
    IsoDate::parse(s).unwrap_or_else(|| panic!("{s} is a date"))
}

/// The packaged `gcgapi/products.json` (read from disk: gcg-data does not embed it).
fn releases() -> &'static ReleaseDates {
    static DATES: OnceLock<ReleaseDates> = OnceLock::new();
    DATES.get_or_init(|| {
        ReleaseDates::from_json(&read("src/gcg_sim/data/gcgapi/products.json"))
            .expect("products.json loads")
    })
}

/// The scope over the packaged `official/edition_language.json`.
fn scope() -> Scope {
    Scope::new(&read("src/gcg_sim/data/official/edition_language.json")).expect("scope loads")
}

fn sources(lists: &BanlistHistory) -> LegalitySources<'_> {
    LegalitySources {
        lists,
        ..LegalitySources::packaged(releases())
    }
}

struct AllImplemented;

impl ImplementationStatus for AllImplemented {
    fn unimplemented_error(&self, _: gcg_data::DefId) -> Option<&str> {
        None
    }
}

fn deck(text: &str) -> Deck {
    parse_deck(text, "p6l", None).unwrap_or_else(|e| panic!("{e}"))
}

fn example(name: &str) -> String {
    read(&format!("examples/decks/{name}.txt"))
}

fn run(d: &Deck, lists: &BanlistHistory, opts: &LegalityOptions<'_>) -> LegalityReport {
    check(d, &sources(lists), opts).unwrap_or_else(|e| panic!("{e}"))
}

fn codes(report: &LegalityReport) -> Vec<&'static str> {
    report.rules.iter().map(|v| v.code.as_str()).collect()
}

fn packaged_opts() -> LegalityOptions<'static> {
    LegalityOptions::new(&AllImplemented)
}

// ---------------------------------------------------------------------------------------------
// W-72

/// W-72 (and W-77, W-78): with the packaged data and the default options, the extended check's
/// rules violations followed by its simulator-support violations are exactly Python's
/// `validate_deck` results, for every case of the Python deck corpus (default, `banlist=False`
/// and `implemented=False`), and hints never change them.
#[test]
fn w72_default_check_equals_validate_deck_on_the_python_corpus() {
    let fx = fixture("decks.json");
    let mut checked = 0;
    for case in arr(&fx["cases"]) {
        let id = s(&case["id"]);
        let d = match s(&case["kind"]) {
            "text" => match parse_deck(s(&case["text"]), s(&case["name"]), None) {
                Ok(d) => d,
                Err(_) => continue,
            },
            "constructed" => deck_from(&case["deck"]),
            _ => continue,
        };
        let status = Unimplemented::from_case(case);
        let want = &case["validate"];
        for (key, banlist, implemented) in [
            ("default", true, true),
            ("no_banlist", false, true),
            ("no_implemented", true, false),
        ] {
            let opts = LegalityOptions {
                banlist: banlist.then(ListRequest::default),
                implemented: implemented.then_some(&status as &dyn ImplementationStatus),
                event_date: None,
            };
            let r = run(&d, BanlistHistory::packaged(), &opts);
            let mut all = r.rules.clone();
            all.extend(r.simulator.clone().unwrap_or_default());
            assert_eq!(violation_rows(&all), want[key], "{id}: {key}");
            assert!(
                r.rules
                    .iter()
                    .all(|v| v.code != ViolationCode::Unimplemented),
                "{id}"
            );
            assert_eq!(r.simulator.is_some(), implemented, "{id}");
            assert_eq!(r.banlist.is_some(), banlist, "{id}");
            assert!(r.exceptions.is_empty(), "{id}");
            checked += 1;
        }
    }
    assert!(checked >= 600, "{checked} validations checked");
}

/// W-72: the packaged data is one current list covering both English regions, equal to
/// `banlist()`; it is selected by default, for either region and for any date from its own.
#[test]
fn w72_packaged_model_is_one_current_list() {
    let h = BanlistHistory::packaged();
    assert_eq!(h.entries().len(), 1);
    let cur = h.current();
    assert!(cur.current);
    assert_eq!(&cur.list, official::banlist());
    assert_eq!(cur.effective_date, date("2026-09-25"));
    assert_eq!(h.regions(), ["asia-en", "en"]);
    assert!(h.decklists().is_empty());
    let ids: Vec<(&str, &str)> = cur
        .product_exceptions
        .iter()
        .map(|p| (&*p.id, &*p.product))
        .collect();
    assert_eq!(
        ids,
        [
            ("st02-unmodified-starter-deck", "ST02"),
            ("st05-unmodified-starter-deck", "ST05")
        ]
    );
    for (as_of, region) in [
        (None, None),
        (Some(date("2026-09-25")), None),
        (Some(date("2026-11-02")), Some("en")),
        (None, Some("asia-en")),
    ] {
        assert!(std::ptr::eq(h.select(as_of, region).expect("selects"), cur));
    }
    let err = h.select(Some(date("2026-09-24")), None).unwrap_err();
    assert!(matches!(err, SelectError::NoList { .. }), "{err:?}");
    assert_eq!(
        err.to_string(),
        "no Banned & Restricted list in the data was in effect on 2026-09-24; the lists in the \
         data take effect on 2026-09-25 (asia-en, en). Older lists are added by a data refresh \
         (/gcg-refresh-data)"
    );
    let err = h.select(None, Some("us")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "no Banned & Restricted list in the data covers region 'us' (regions in the data: \
         asia-en, en)"
    );
}

/// The July 2026 list as a history entry: the current list's contents with its date and
/// without the two vanilla-list members added at the ST11/ST12 release
/// (`enumerated_members_added_after_2026_07_10`).
fn july_list(effective: &str, regions: &[&str]) -> Value {
    let mut l = banlist_doc();
    let obj = l.as_object_mut().expect("object");
    obj.retain(|k, _| {
        matches!(
            k.as_str(),
            "effective_date" | "banned" | "restricted" | "banned_pairs" | "attribute_pair_rules"
        )
    });
    obj.insert("effective_date".into(), json!(effective));
    obj.insert("regions".into(), json!(regions));
    let rule = &mut l["attribute_pair_rules"][0];
    let members = rule["enumerated_members"].as_array_mut().expect("members");
    members.retain(|m| !matches!(m["card_number"].as_str(), Some("ST11-008" | "ST12-010")));
    assert_eq!(members.len(), 20);
    l
}

fn history_with(lists: &[Value]) -> Result<BanlistHistory, gcg_data::DataError> {
    let mut doc = banlist_doc();
    doc["history"] = Value::Array(lists.to_vec());
    BanlistHistory::from_json(&doc.to_string())
}

/// A 50-card blue-white deck plus `extra` main-deck lines (it then has more than 50 cards;
/// only the B&R violations matter here).
fn with_cards(extra: &str) -> Deck {
    deck(&format!("{}\n{extra}\n", example("blue-white-federation")))
}

/// W-72: with a two-entry fixture, a date before the second list uses the first one (its date
/// in every message, its vanilla list in the membership notes); from the second date on, the
/// second list applies; a date before every list is an error.
#[test]
fn w72_two_entry_fixture_selects_by_date() {
    let h = history_with(&[july_list("2026-07-24", &["asia-en", "en"])]).expect("fixture loads");
    assert_eq!(h.entries().len(), 2);
    assert_eq!(
        h.select(None, None).expect("current").effective_date,
        date("2026-09-25")
    );
    let d = with_cards("1 GD01-020\n1 ST11-008\n1 GD01-035");
    let at = |as_of: &str| -> LegalityReport {
        let opts = LegalityOptions {
            banlist: Some(ListRequest {
                as_of: Some(date(as_of)),
                region: None,
            }),
            ..packaged_opts()
        };
        run(&d, &h, &opts)
    };
    let august = at("2026-08-01");
    assert_eq!(
        august.banlist.as_ref().map(|e| e.effective_date),
        Some(date("2026-07-24"))
    );
    let want = ["MAIN_SIZE", "COLORS", "BANNED", "ATTRIBUTE_PAIR"];
    assert_eq!(codes(&august), want);
    assert!(
        august.rules[2]
            .message
            .ends_with("(Banned & Restricted list effective 2026-07-24)")
    );
    assert!(
        august.rules[3].message.contains(
            " Note: ST11-008 matches the description in the card data but is not on the \
             official list."
        ),
        "{}",
        august.rules[3].message
    );
    for as_of in ["2026-09-25", "2026-11-02"] {
        let current = at(as_of);
        assert_eq!(codes(&current), want);
        assert!(current.rules[2].message.ends_with("effective 2026-09-25)"));
        assert!(!current.rules[3].message.contains("Note:"), "{as_of}");
        let default = validate(
            &d,
            ValidateOptions {
                banlist: true,
                implemented: None,
            },
        );
        assert_eq!(
            current.rules, default,
            "{as_of}: the current list is validate_deck's"
        );
    }
    let opts = LegalityOptions {
        banlist: Some(ListRequest {
            as_of: Some(date("2026-07-23")),
            region: None,
        }),
        ..packaged_opts()
    };
    let err = check(&d, &sources(&h), &opts).unwrap_err();
    assert_eq!(
        err.to_string(),
        "no Banned & Restricted list in the data was in effect on 2026-07-23; the lists in the \
         data take effect on 2026-07-24 (asia-en, en), 2026-09-25 (asia-en, en). Older lists \
         are added by a data refresh (/gcg-refresh-data)"
    );
}

/// W-72: regional lists. The July list took effect on 2026-07-24 in the US (`en`) and on
/// 2026-07-25 in Asia (`asia-en`): `--region` picks each; without it, a date on which the
/// regions disagree is an error that asks for `--region`.
#[test]
fn w72_regional_lists_need_a_region_where_they_differ() {
    let h = history_with(&[
        july_list("2026-07-24", &["en"]),
        july_list("2026-07-25", &["asia-en"]),
    ])
    .expect("fixture loads");
    let pick = |as_of: &str, region: Option<&str>| {
        h.select(Some(date(as_of)), region)
            .map(|e| e.effective_date.to_string())
    };
    assert_eq!(pick("2026-07-24", Some("en")), Ok("2026-07-24".into()));
    assert_eq!(pick("2026-07-25", Some("asia-en")), Ok("2026-07-25".into()));
    assert_eq!(pick("2026-10-01", Some("asia-en")), Ok("2026-09-25".into()));
    assert_eq!(pick("2026-10-01", None), Ok("2026-09-25".into()));
    let err = h
        .select(Some(date("2026-07-24")), Some("asia-en"))
        .unwrap_err();
    assert!(matches!(err, SelectError::NoList { .. }));
    assert!(err.to_string().contains("in region asia-en;"), "{err}");
    let err = h.select(Some(date("2026-07-24")), None).unwrap_err();
    assert_eq!(
        err.to_string(),
        "the regions apply different Banned & Restricted lists on 2026-07-24 (asia-en: no list; \
         en: the list effective 2026-07-24); choose one with --region"
    );
    let err = h.select(Some(date("2026-08-01")), None).unwrap_err();
    assert!(matches!(err, SelectError::Ambiguous { .. }), "{err:?}");
    let latest = h.select(None, Some("en")).expect("en");
    assert!(latest.current);
}

/// W-72: malformed histories fail when the data loads, naming the problem.
#[test]
fn w72_malformed_histories_are_load_errors() {
    let shape = |r: Result<BanlistHistory, gcg_data::DataError>| match r {
        Err(gcg_data::DataError::Shape { detail, .. }) => detail,
        other => panic!("expected a Shape error, got {other:?}"),
    };
    let same_day = shape(history_with(&[july_list("2026-09-25", &["en"])]));
    assert_eq!(same_day, "two lists of region en take effect on 2026-09-25");
    let later = shape(history_with(&[july_list("2026-10-30", &["en"])]));
    assert!(
        later.contains("after the current list (2026-09-25)"),
        "{later}"
    );
    let mut no_region = july_list("2026-07-24", &[]);
    assert_eq!(
        shape(history_with(&[no_region.clone()])),
        "a history list covers no region"
    );
    no_region["regions"] = json!(["en"]);
    no_region["effective_date"] = json!("July 24");
    assert!(shape(history_with(&[no_region])).contains("not YYYY-MM-DD"));
    let mut doc = banlist_doc();
    doc.as_object_mut().expect("object").remove("source_urls");
    let e = BanlistHistory::from_json(&doc.to_string()).unwrap_err();
    assert!(e.to_string().contains("names no region"), "{e}");
    doc["regions"] = json!(["en"]);
    let h = BanlistHistory::from_json(&doc.to_string()).expect("explicit regions");
    assert_eq!(h.regions(), ["en"]);
}

// ---------------------------------------------------------------------------------------------
// W-71

/// A synthetic unmodified ST02 deck: the 16 ST02 card numbers (Green/Blue) with 3x ST02-016
/// Corsica Base, 50 cards, and 10 Resources. Not the official list (the data has none); it
/// only has the official list's shape.
const ST02_FIXTURE: &str = "4 ST02-001\n4 ST02-002\n4 ST02-003\n4 ST02-004\n4 ST02-005\n\
     4 ST02-006\n4 ST02-007\n4 ST02-008\n2 ST02-009\n4 ST02-010\n2 ST02-011\n2 ST02-012\n\
     2 ST02-013\n2 ST02-014\n1 ST02-015\n3 ST02-016\n10 R-001\n";
/// A synthetic unmodified ST05 deck (Purple/White) with ST05-004 and ST05-009, the two vanilla
/// pair members of the exception.
const ST05_FIXTURE: &str = "4 ST05-001\n4 ST05-002\n4 ST05-003\n4 ST05-004\n4 ST05-005\n\
     4 ST05-006\n4 ST05-007\n4 ST05-008\n4 ST05-009\n2 ST05-010\n4 ST05-011\n2 ST05-012\n\
     2 ST05-013\n2 ST05-014\n2 ST05-015\n10 R-001\n";

fn decklist_json(text: &str) -> Value {
    let d = deck(text);
    let side = |cards: &[gcg_data::CardCount]| -> Value {
        cards
            .iter()
            .map(|c| json!({"card_number": &*c.card_number, "count": u64::try_from(c.count).expect("u64")}))
            .collect()
    };
    json!({
        "main": side(&d.main),
        "resources": side(&d.resources),
        "source_url": "synthetic fixture (tests/p6l_validator.rs)",
        "retrieved_at": "2026-11-02T00:00:00Z",
    })
}

fn with_decklists(decklists: Value) -> BanlistHistory {
    let mut doc = banlist_doc();
    doc["official_product_decklists"] = decklists;
    BanlistHistory::from_json(&doc.to_string()).expect("fixture loads")
}

/// W-71: with the packaged data (no decklist) the ST02 and ST05 decks are reported exactly as
/// Python reports them, with the explanatory note; with a decklist fixture, the exact list is
/// exempt from exactly its restriction and a one-card variation is reported again, saying it
/// is not the unmodified decklist.
#[test]
fn w71_starter_deck_exception_hook() {
    let st02 = deck(ST02_FIXTURE);
    let st05 = deck(ST05_FIXTURE);
    let packaged = run(&st02, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&packaged), ["RESTRICTED"]);
    assert!(packaged.rules[0].message.ends_with(
        " Exception 'st02-unmodified-starter-deck' is not applied: the simulator cannot verify \
         an unmodified starter deck."
    ));
    let packaged05 = run(&st05, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&packaged05), ["ATTRIBUTE_PAIR"]);
    assert!(packaged05.rules[0].message.contains(
        " Exception 'st05-unmodified-starter-deck' is not applied: the simulator cannot verify \
         an unmodified starter deck."
    ));
    for (d, r) in [(&st02, &packaged), (&st05, &packaged05)] {
        let python = validate(
            d,
            ValidateOptions {
                banlist: true,
                implemented: None,
            },
        );
        assert_eq!(r.rules, python);
    }

    let h = with_decklists(json!({"ST02": decklist_json(ST02_FIXTURE)}));
    assert_eq!(h.decklists().len(), 1);
    let exact = run(&st02, &h, &packaged_opts());
    assert!(exact.is_legal(), "{:?}", exact.rules);
    assert_eq!(exact.exceptions.len(), 1);
    assert_eq!(&*exact.exceptions[0].id, "st02-unmodified-starter-deck");
    assert_eq!(&*exact.exceptions[0].product, "ST02");
    let varied = deck(&ST02_FIXTURE.replace("4 ST02-001\n", "3 ST02-001\n1 ST02-007\n"));
    let r = run(&varied, &h, &packaged_opts());
    assert_eq!(codes(&r), ["COPY_LIMIT", "RESTRICTED"]);
    assert!(r.rules[1].message.ends_with(
        " Exception 'st02-unmodified-starter-deck' is not applied: the deck is not the \
         unmodified official ST02 decklist."
    ));
    let swapped = deck(&ST02_FIXTURE.replace("2 ST02-009\n", "1 ST02-009\n1 ST01-005\n"));
    let r = run(&swapped, &h, &packaged_opts());
    assert_eq!(codes(&r), ["RESTRICTED"]);
    assert!(r.exceptions.is_empty());
    let r05 = run(&st05, &h, &packaged_opts());
    assert_eq!(
        r05.rules, packaged05.rules,
        "the ST05 exception has no decklist"
    );

    let both = with_decklists(json!({
        "ST02": decklist_json(ST02_FIXTURE),
        "ST05": decklist_json(ST05_FIXTURE),
    }));
    let exact05 = run(&st05, &both, &packaged_opts());
    assert!(exact05.is_legal(), "{:?}", exact05.rules);
    assert_eq!(&*exact05.exceptions[0].id, "st05-unmodified-starter-deck");
    let st05_plus = deck(&ST05_FIXTURE.replace("2 ST05-015\n", "2 ST05-015\n1 GD01-020\n"));
    let r = run(&st05_plus, &both, &packaged_opts());
    assert_eq!(
        codes(&r),
        ["MAIN_SIZE", "COLORS", "BANNED", "ATTRIBUTE_PAIR"]
    );
    assert!(r.exceptions.is_empty(), "a modified deck is never exempt");
}

/// W-71: a decklist without provenance is rejected when the data loads.
#[test]
fn w71_decklists_need_provenance() {
    let mut dl = decklist_json(ST02_FIXTURE);
    dl.as_object_mut().expect("object").remove("source_url");
    let mut doc = banlist_doc();
    doc["official_product_decklists"] = json!({"ST02": dl});
    let e = BanlistHistory::from_json(&doc.to_string()).unwrap_err();
    assert!(
        e.to_string()
            .contains("the ST02 decklist has no source_url (provenance is required)"),
        "{e}"
    );
    let mut dl = decklist_json(ST02_FIXTURE);
    dl["main"][0]["count"] = json!(0);
    doc["official_product_decklists"] = json!({"ST02": dl});
    assert!(BanlistHistory::from_json(&doc.to_string()).is_err());
}

// ---------------------------------------------------------------------------------------------
// W-75, W-78

/// W-75: with no list applied, a deck with 4 copies of the banned GD01-020 passes (the
/// Comprehensive Rules allow 4 copies of any card number, 6-1-1-3); by default it is BANNED.
#[test]
fn w75_no_banlist_accepts_a_banned_card() {
    let colour = |n: &str| CardDb::embedded().get(n).and_then(|x| x.color);
    assert_eq!(colour("GD01-020"), colour("GD01-008"), "both Blue");
    let banned =
        deck(&example("blue-white-federation").replace("4 GD01-008 Guntank", "4 GD01-020"));
    let default = run(&banned, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&default), ["BANNED"]);
    let off = LegalityOptions {
        banlist: None,
        ..packaged_opts()
    };
    let r = run(&banned, BanlistHistory::packaged(), &off);
    assert!(r.is_legal(), "{:?}", r.rules);
    assert!(r.banlist.is_none());
}

/// W-78: a synthetic unimplemented card is reported under simulator support, not under the
/// rules, and the deck passes the rules with the switch off.
#[test]
fn w78_simulator_support_is_reported_apart() {
    let d = deck(&example("blue-white-federation"));
    let gd01_008 = CardDb::embedded().get("GD01-008").expect("card").def_id.0;
    let status = Unimplemented(vec![(gd01_008, "synthetic: not compiled".into())]);
    let opts = LegalityOptions::new(&status);
    let r = run(&d, BanlistHistory::packaged(), &opts);
    assert!(r.rules.is_empty());
    let sim = r.simulator.as_ref().expect("checked");
    assert_eq!(sim.len(), 1);
    assert_eq!(sim[0].code, ViolationCode::Unimplemented);
    assert_eq!(
        sim[0].message,
        "GD01-008 Guntank has no implemented effect in this simulator version, so games cannot \
         use it (synthetic: not compiled)"
    );
    assert!(!r.is_legal());
    let rules_only = LegalityOptions {
        implemented: None,
        ..opts
    };
    let r = run(&d, BanlistHistory::packaged(), &rules_only);
    assert!(r.is_legal());
    assert!(r.simulator.is_none());
}

// ---------------------------------------------------------------------------------------------
// W-76

/// The blue-white example with its two GD04-122 Jaburo replaced by ST11-016 Mad Angler (ST11,
/// released 2026-09-25).
fn new_card_deck() -> Deck {
    deck(&example("blue-white-federation").replace("2 GD04-122 Jaburo", "2 ST11-016 Mad Angler"))
}

fn at_event(d: &Deck, event: &str) -> LegalityReport {
    let opts = LegalityOptions {
        event_date: Some(date(event)),
        ..packaged_opts()
    };
    run(d, BanlistHistory::packaged(), &opts)
}

/// W-76: a card released 3 days before the event date fails the optional check and passes
/// without it; from 7 days after its release it passes; before its release it is "not
/// released". Release dates come from the packaged products; Resources have none and are
/// listed as not checked.
#[test]
fn w76_event_date_seven_day_rule() {
    let d = new_card_deck();
    let without = run(&d, BanlistHistory::packaged(), &packaged_opts());
    assert!(without.is_legal(), "{:?}", without.rules);
    assert!(without.event.is_none());
    let r = at_event(&d, "2026-09-28");
    assert!(r.rules.is_empty());
    let ev = r.event.as_ref().expect("checked");
    assert_eq!(ev.violations.len(), 1);
    let v = &ev.violations[0];
    assert_eq!(
        (&*v.card_number, v.released, v.usable_from, &*v.product),
        ("ST11-016", date("2026-09-25"), date("2026-10-02"), "st11")
    );
    assert_eq!(
        v.message,
        "ST11-016 Mad Angler was first released on 2026-09-25 (Aquatic Assault [ST11]), 3 day(s) \
         before the event on 2026-09-28; a card with a new card number may be used at official \
         tournaments only once it has been sold for at least 7 days, from 2026-10-02 (TRM \
         2.4.1)"
    );
    assert_eq!(ev.unknown, [Box::<str>::from("R-001")]);
    assert!(!r.is_legal());
    assert!(
        at_event(&d, "2026-10-01")
            .event
            .expect("checked")
            .violations
            .len()
            == 1
    );
    assert!(at_event(&d, "2026-10-02").is_legal());
    let early = at_event(&d, "2026-09-20");
    assert!(
        early.event.expect("checked").violations[0]
            .message
            .starts_with("ST11-016 Mad Angler is not released until 2026-09-25 (Aquatic Assault [ST11]), after the event on 2026-09-20;")
    );
    let old = at_event(&deck(&example("blue-white-federation")), "2026-09-28");
    assert!(old.is_legal(), "{:?}", old.event);
}

/// W-76: every main-deck card number of the pool has a release date, through the product of
/// the set its number starts with; the only card numbers without one are Resources, tokens and
/// promotion sets. A reprint does not make a number new: R-001's canonical printing is the
/// SC01 Deck Build Box (2026-07-24), but its number's set `R` has no dated product.
#[test]
fn w76_release_dates_cover_every_main_deck_set() {
    let releases = releases();
    let mut missing: Vec<String> = Vec::new();
    for d in CardDb::embedded().real_cards() {
        let set = number_set(&d.card_number);
        if releases.first_release(set).is_none() {
            missing.push(format!("{set}:{}", d.card_type.as_str()));
        }
    }
    assert_eq!(
        &*CardDb::embedded().get("R-001").expect("R-001").set_code,
        "SC01"
    );
    missing.sort();
    missing.dedup();
    assert_eq!(
        missing,
        [
            "EXB:EX BASE",
            "EXBP:EX BASE",
            "EXR:EX RESOURCE",
            "EXRP:EX RESOURCE",
            "R:RESOURCE",
            "RP:RESOURCE",
            "T:UNIT TOKEN"
        ]
    );
    let (gd06, released) = releases.first_release("GD06").expect("GD06 is listed");
    assert_eq!((&*gd06.product_id, released), ("gd06", date("2026-10-30")));
}

/// W-76 (and W-75): the scope statement lists the unchecked tournament rules, flags the
/// Edition Beta-only printings, names Constructed as the applied format with Sealed and Draft
/// unsupported, and its `analysis.json` block names the format, the list date and the
/// unmodelled tournament procedures.
#[test]
fn w76_scope_statement() {
    let scope = scope();
    assert_eq!(
        &*scope.banlist_format,
        "standard constructed (official and sanctioned tournaments)"
    );
    let unchecked: Vec<&str> = scope.unchecked_rules.iter().map(|r| &**r).collect();
    assert_eq!(
        unchecked[..2],
        [
            "7-day new-card rule for Premier events (TRM 2.4.1; Sanctioned Floor Rules Section 5)",
            "physical card language, condition, sleeves, deck lists and deck checks"
        ]
    );
    assert!(unchecked[3].starts_with(
        "sideboards: the Banned & Restricted list applies to the deck and sideboard combined"
    ));
    assert_eq!(
        &*scope.edition_beta_only,
        [Box::from("EXBP-001"), Box::from("RP-001")]
    );
    assert!(!scope.edition_beta_statement_found);
    let beta =
        deck(&example("blue-white-federation").replace("10 R-001 Resource", "9 R-001\n1 RP-001"));
    assert_eq!(scope.edition_beta_only_in(&beta), ["RP-001"]);
    assert!(run(&beta, BanlistHistory::packaged(), &packaged_opts()).is_legal());
    let j = scope.to_json(Some(BanlistHistory::packaged().current()));
    let text = gcg_core::pyjson::dumps(&j, gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII);
    let v: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(v["format"], "Constructed");
    assert_eq!(v["banlist"]["applied"], true);
    assert_eq!(v["banlist"]["effective_date"], "2026-09-25");
    assert_eq!(v["banlist"]["regions"], json!(["asia-en", "en"]));
    assert_eq!(v["unsupported_formats"][0]["name"], "Sealed");
    assert_eq!(v["unsupported_formats"][1]["name"], "Draft");
    assert_eq!(
        v["unmodelled_tournament_procedures"]["intentional_draws_concessions_penalties"],
        "Not modelled."
    );
    assert_eq!(
        v["edition_beta"]["card_numbers_only_in_edition_beta"],
        json!(["EXBP-001", "RP-001"])
    );
    let none = scope.to_json(None);
    assert_eq!(
        none.get("banlist").and_then(|b| b.get("applied")),
        Some(&gcg_core::pyjson::Json::Bool(false))
    );
}

// ---------------------------------------------------------------------------------------------
// W-77, W-144

/// W-77: an EX Resource or EX Base listed in a deck keeps Python's CARD_TYPE violation (and
/// size violation) and gains a hint naming the token rule and the size it counts toward.
#[test]
fn w77_ex_token_hints() {
    let base = example("blue-white-federation");
    let exr = deck(&base.replace("10 R-001 Resource", "9 R-001\n1 EXR-001"));
    let r = run(&exr, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&r), ["CARD_TYPE"]);
    assert_eq!(
        r.rules,
        validate(
            &exr,
            ValidateOptions {
                banlist: true,
                implemented: None
            }
        )
    );
    assert_eq!(r.hints.len(), 1);
    assert_eq!(r.hints[0].code, ViolationCode::CardType);
    assert!(
        r.hints[0].text.contains("EXR-001 is the EX Resource token"),
        "{}",
        r.hints[0].text
    );
    assert!(
        r.hints[0]
            .text
            .ends_with("count toward the resource deck's 10 cards (RESOURCE_SIZE)")
    );
    let exb = deck(&format!("{base}\n1 EXB-001\n"));
    let r = run(&exb, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&r), ["MAIN_SIZE", "CARD_TYPE"]);
    assert!(
        r.hints[0]
            .text
            .ends_with("count toward the main deck's 50 cards (MAIN_SIZE)")
    );
    assert!(r.hints[0].line.is_some());
    let clean = run(&deck(&base), BanlistHistory::packaged(), &packaged_opts());
    assert!(clean.hints.is_empty());
}

/// W-77: a `#` after a card is not a comment. The violations stay Python's (NAME_MISMATCH,
/// UNKNOWN_ID) and a hint names the line when the text before the `#` is a valid entry.
#[test]
fn w77_comment_hints() {
    let db = CardDb::embedded();
    let text = "4 GD01-008 Guntank  # early game\n4 GD01-004#rest\n2 ST01-005 # cheap\n\
                1 GD01-001 Gundam\n2 ST01-004 Gundam # wrong name anyway\n# a real comment\n";
    let err = parse_deck(text, "hash", None).unwrap_err();
    let DeckError::Invalid { violations, .. } = &err else {
        panic!("{err}")
    };
    let got: Vec<(&str, &str)> = violations
        .iter()
        .map(|v| (v.code.as_str(), v.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "NAME_MISMATCH",
                "line 1: GD01-008 is 'Guntank' in the card data, not 'Guntank  # early game'"
            ),
            (
                "UNKNOWN_ID",
                "line 2: unknown card id 'GD01-004#rest' (not in the card data)"
            ),
            (
                "NAME_MISMATCH",
                "line 3: ST01-005 is 'GM' in the card data, not '# cheap'"
            ),
            (
                "NAME_MISMATCH",
                "line 5: ST01-004 is 'Guntank' in the card data, not 'Gundam # wrong name anyway'"
            ),
        ]
    );
    let hints = text_hints(db, text);
    let lines: Vec<(Option<usize>, ViolationCode)> =
        hints.iter().map(|h| (h.line, h.code)).collect();
    assert_eq!(
        lines,
        [
            (Some(1), ViolationCode::NameMismatch),
            (Some(2), ViolationCode::UnknownId),
            (Some(3), ViolationCode::NameMismatch)
        ]
    );
    assert_eq!(
        hints[0].text,
        "line 1: 'Guntank' matches GD01-008 and the text from '#' is read as part of the name: \
         there are no end-of-line comments, because card names may contain '#'; put the comment \
         on a line of its own"
    );
    assert!(
        hints[1]
            .text
            .starts_with("line 2: 'GD01-004' is a card id but 'GD01-004#rest' is not:")
    );
    assert!(
        hints[2]
            .text
            .starts_with("line 3: the text from '#' is read as the card's name:")
    );
    assert!(text_hints(db, &example("blue-white-federation")).is_empty());
}

/// W-144: a 5,000-digit count in a deck file is a SYNTAX violation naming the line (Python
/// raises ValueError there); a count of 10^12 reaches the size and copy checks of the extended
/// check without expanding any list (10^12 entries could never be allocated).
#[test]
fn w144_huge_counts_in_files_and_the_extended_check() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("p6l-w144");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("big.txt");
    std::fs::write(&path, format!("{} GD01-001\n10 R-001\n", "9".repeat(5_000))).expect("write");
    let err = gcg_data::load_deck(&path).unwrap_err();
    let DeckError::Invalid { violations, .. } = &err else {
        panic!("{err}")
    };
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, ViolationCode::Syntax);
    assert_eq!(
        violations[0].message,
        "line 1: count must be at most 18446744073709551615 for GD01-001"
    );
    let huge = deck("1000000000000 GD01-001\n10 R-001\n");
    let r = run(&huge, BanlistHistory::packaged(), &packaged_opts());
    assert_eq!(codes(&r), ["MAIN_SIZE", "COPY_LIMIT"]);
    assert_eq!(
        r.rules[0].message,
        "the deck has 1000000000000 cards; it must have exactly 50 (rule 6-1-1)"
    );
    let ev = at_event(&huge, "2026-11-02");
    assert!(ev.event.expect("checked").violations.is_empty());
}
