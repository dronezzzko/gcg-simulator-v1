//! P6-L data tripwires (docs/port/PLAN.md §12.2 P6-L; docs/port/eval/weakness-status/P6-L.md).
//!
//! Each tripwire is a pure function over the packaged sources (or a modified copy held in
//! memory, the "temporary data copy" of the WEAKNESSES entries) that returns its findings. The
//! test asserts the packaged data gives none, and a negative control shows that a synthetic card
//! or an edited copy makes the tripwire fail. The validator's run-time behaviour is unchanged
//! (predicate OR list, with a note on divergence; `has_effect` as effect text; the primary name
//! for `name_contains`; one colour per card).
//!
//! - W-73: the vanilla banned-pair predicate over the pool equals the enumerated list.
//! - W-74: the readings that hold only for today's pool: "without effects" (keywords count, a
//!   link alone does not), `name_contains` on the primary name only, one colour per card and
//!   no colourless main-deck card.

use gcg_data::legality::BanlistHistory;
use gcg_data::{Banlist, CardDb, CardDef, MAIN_DECK_TYPES, official};
use serde_json::{Value, json};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const CARDS: &str = "src/gcg_sim/data/gcgapi/cards.ndjson";
const OVERRIDES: &str = "src/gcg_sim/data/overrides.json";
const BANLIST: &str = "src/gcg_sim/data/official/banlist.json";

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn printings(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("a printing record"))
        .collect()
}

fn ndjson(records: &[Value]) -> String {
    records.iter().map(|r| format!("{r}\n")).collect()
}

/// A Lv.2, cost 1, 2 AP, 2 HP Unit printing for synthetic copies of the snapshot.
fn vanilla_shape(number: &str, effect: &str, link: &str, color: Value) -> Value {
    json!({
        "product_id": number, "card_number": number, "name": format!("Synthetic {number}"),
        "set_code": "SY01", "rarity": "C", "card_type": "UNIT", "color": color,
        "level": 2, "cost": 1, "ap": 2, "hp": 2, "ap_raw": "2", "hp_raw": "2",
        "zone": "Space", "trait": "(Earth Federation)", "link": link,
        "traits": ["Earth Federation"], "effect": effect,
    })
}

fn with_printings(extra: &[Value]) -> (Vec<Value>, CardDb) {
    let mut ps = printings(&read(CARDS));
    ps.extend(extra.iter().cloned());
    let db = CardDb::from_sources(&ndjson(&ps), &read(OVERRIDES)).expect("the copy loads");
    (ps, db)
}

// ---------------------------------------------------------------------------------------------
// W-73

/// W-73's tripwire: for every attribute-pair rule, the cards the predicate selects in the pool
/// equal the enumerated list, and the data's own record of the predicate's matches
/// (`predicate_matches_in_card_data`) equals both.
fn vanilla_divergence(db: &CardDb, banlist: &Banlist, doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (i, rule) in banlist.attribute_rules.iter().enumerate() {
        let mut by_predicate: Vec<&str> = db
            .real_cards()
            .filter(|d| official::matches(&rule.member_predicate, d, None))
            .map(|d| &*d.card_number)
            .collect();
        by_predicate.sort();
        for n in &by_predicate {
            if rule.enumerated.binary_search_by(|e| (**e).cmp(n)).is_err() {
                out.push(format!(
                    "{}: {n} matches the predicate but is not enumerated",
                    rule.id
                ));
            }
        }
        for n in rule.enumerated.iter() {
            if by_predicate.binary_search(&&**n).is_err() {
                out.push(format!(
                    "{}: {n} is enumerated but does not match the predicate",
                    rule.id
                ));
            }
        }
        let recorded: Vec<&str> = doc["attribute_pair_rules"][i]["predicate_matches_in_card_data"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(Value::as_str)
            .collect();
        if recorded != by_predicate {
            out.push(format!(
                "{}: predicate_matches_in_card_data lists {} cards, the pool has {}",
                rule.id,
                recorded.len(),
                by_predicate.len()
            ));
        }
    }
    out
}

#[test]
fn w73_vanilla_predicate_equals_the_enumerated_list_with_negative_control() {
    let doc: Value = serde_json::from_str(&read(BANLIST)).expect("banlist.json");
    let bl = official::banlist();
    let db = CardDb::embedded();
    assert_eq!(vanilla_divergence(db, bl, &doc), Vec::<String>::new());
    let rule = &bl.attribute_rules[0];
    let matched = db
        .real_cards()
        .filter(|d| official::matches(&rule.member_predicate, d, None))
        .count();
    assert_eq!((matched, rule.enumerated.len()), (22, 22));

    let (_, copy) = with_printings(&[vanilla_shape("SY01-001", "-", "-", json!("Blue"))]);
    assert_eq!(
        vanilla_divergence(&copy, bl, &doc),
        [
            "vanilla-lv2-cost1-ap2-hp2: SY01-001 matches the predicate but is not enumerated",
            "vanilla-lv2-cost1-ap2-hp2: predicate_matches_in_card_data lists 22 cards, the pool \
             has 23"
        ]
    );
    let mut edited = doc.clone();
    edited["attribute_pair_rules"][0]["enumerated_members"]
        .as_array_mut()
        .expect("members")
        .push(json!({"card_number": "GD01-001", "name": "Gundam"}));
    let lists = BanlistHistory::from_json(&edited.to_string()).expect("edited copy loads");
    assert_eq!(
        vanilla_divergence(db, &lists.current().list, &edited),
        ["vanilla-lv2-cost1-ap2-hp2: GD01-001 is enumerated but does not match the predicate"]
    );
}

// ---------------------------------------------------------------------------------------------
// W-74

/// Effect text made only of keyword tokens (`<Blocker>`, `<Repair 1>`) and reminder
/// parentheticals: an effect today ("keywords count"), none under the narrower reading of
/// "without effects".
fn keywords_only(effect: &str) -> bool {
    let mut depth_angle = 0_u32;
    let mut depth_paren = 0_u32;
    let mut rest = String::new();
    for c in effect.chars() {
        match c {
            '<' => depth_angle += 1,
            '>' if depth_angle > 0 => depth_angle -= 1,
            '(' => depth_paren += 1,
            ')' if depth_paren > 0 => depth_paren -= 1,
            _ if depth_angle == 0 && depth_paren == 0 => rest.push(c),
            _ => {}
        }
    }
    rest.trim().is_empty() && effect.contains('<')
}

/// W-74 (a): every card an attribute rule's predicate classifies differently when keywords are
/// not effects, or when a link requirement is an effect.
fn effect_reading_findings(db: &CardDb, banlist: &Banlist) -> Vec<String> {
    let mut out = Vec::new();
    for rule in banlist.attribute_rules.iter() {
        let pred = &rule.member_predicate;
        for d in db.real_cards() {
            let now = official::matches(pred, d, None);
            let mut readings: Vec<(&str, CardDef)> = Vec::new();
            if !d.is_vanilla() && keywords_only(&d.effect) {
                let mut alt = d.clone();
                alt.effect = "-".into();
                readings.push(("keywords are not effects", alt));
            }
            if d.is_vanilla() && d.link.is_some() {
                let mut alt = d.clone();
                alt.effect = "[link requirement]".into();
                readings.push(("a link requirement is an effect", alt));
            }
            for (reading, alt) in readings {
                if official::matches(pred, &alt, None) != now {
                    out.push(format!(
                        "{}: {} is classified differently if {reading}",
                        rule.id, d.card_number
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn w74_without_effects_reading_with_negative_controls() {
    let bl = official::banlist();
    assert_eq!(
        effect_reading_findings(CardDb::embedded(), bl),
        Vec::<String>::new()
    );
    assert!(keywords_only(
        "<Blocker>\n(Rest this Unit to change the attack target to it.)"
    ));
    assert!(!keywords_only("【Deploy】Draw 1."));
    let (_, copy) = with_printings(&[
        vanilla_shape(
            "SY01-002",
            "<Blocker>\n(Rest this Unit to change the attack target to it.)",
            "-",
            json!("Blue"),
        ),
        vanilla_shape("SY01-003", "-", "[Amuro Ray]", json!("Blue")),
    ]);
    assert_eq!(
        effect_reading_findings(&copy, bl),
        [
            "vanilla-lv2-cost1-ap2-hp2: SY01-002 is classified differently if keywords are not \
             effects",
            "vanilla-lv2-cost1-ap2-hp2: SY01-003 is classified differently if a link requirement \
             is an effect"
        ]
    );
}

/// Every `name_contains` value of a predicate, depth first in key order.
fn name_values<'a>(pred: &'a Value, out: &mut Vec<&'a str>) {
    match pred {
        Value::Object(map) => {
            for (k, v) in map {
                match (k.as_str(), v) {
                    ("name_contains", Value::String(s)) => out.push(s),
                    _ => name_values(v, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|p| name_values(p, out)),
        _ => {}
    }
}

/// The text form of rule 2-2-4's alias: "This card's name is also treated as [X]."
const ALSO_TREATED_AS: &str = "This card's name is also treated as [";

/// A card's names other than its primary name (rule 2-2-4): the `names` beyond the first (a
/// Command's 【Pilot】 name, any `extra_names`) and every "also treated as [X]" of its text.
fn aliases(d: &CardDef) -> Vec<String> {
    let mut out: Vec<String> = d.names().iter().skip(1).map(|n| n.to_string()).collect();
    let mut rest = &*d.effect;
    while let Some(i) = rest.find(ALSO_TREATED_AS) {
        rest = &rest[i + ALSO_TREATED_AS.len()..];
        if let Some(end) = rest.find(']') {
            out.push(rest[..end].to_owned());
            rest = &rest[end..];
        }
    }
    out
}

/// W-74 (b): a `name_contains` value that matches an alias of a card (rule 2-2-4) but not its
/// primary name, which `matches` tests.
fn alias_findings(db: &CardDb, preds: &[(&str, &Value)]) -> Vec<String> {
    let mut out = Vec::new();
    for (id, pred) in preds {
        let mut values = Vec::new();
        name_values(pred, &mut values);
        for v in values {
            for d in db.real_cards() {
                let all = aliases(d);
                let alias = all.iter().find(|a| a.contains(v));
                if let Some(a) = alias
                    && !d.name.contains(v)
                {
                    out.push(format!(
                        "{id}: name_contains {v:?} misses {} (alias {a:?}, name {:?})",
                        d.card_number, d.name
                    ));
                }
            }
        }
    }
    out
}

/// Every predicate of a `banlist.json` document: each attribute rule's `member_predicate`,
/// `if_deck_contains` and `then_forbidden`.
fn predicates(doc: &Value) -> Vec<(&str, &Value)> {
    let mut out = Vec::new();
    for r in doc["attribute_pair_rules"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
    {
        let id = r["id"].as_str().unwrap_or("?");
        for key in ["member_predicate", "if_deck_contains", "then_forbidden"] {
            if let Some(p) = r.get(key) {
                out.push((id, p));
            }
        }
    }
    out
}

#[test]
fn w74_name_contains_misses_no_alias_with_negative_control() {
    let db = CardDb::embedded();
    let doc: Value = serde_json::from_str(&read(BANLIST)).expect("banlist.json");
    let preds = predicates(&doc);
    assert_eq!(preds.len(), 3);
    let mut values = Vec::new();
    for (_, p) in &preds {
        name_values(p, &mut values);
    }
    assert!(values.is_empty(), "the packaged list uses no name_contains");
    assert_eq!(alias_findings(db, &preds), Vec::<String>::new());

    let text_aliases: Vec<(&str, Vec<String>)> = db
        .real_cards()
        .filter(|d| d.effect.contains(ALSO_TREATED_AS))
        .map(|d| (&*d.card_number, aliases(d)))
        .collect();
    assert_eq!(
        text_aliases,
        [
            ("GD02-098", vec!["Char Aznable".to_owned()]),
            ("ST12-011", vec!["Zechs Merquise".to_owned()]),
            ("ST12-012", vec!["Marida Cruz".to_owned()]),
        ]
    );
    assert!(db.real_cards().all(|d| d.extra_names.is_empty()));
    let pilot_commands: Vec<&CardDef> = db
        .real_cards()
        .filter(|d| d.card_type == gcg_data::CardType::Command && d.pilot_name.is_some())
        .collect();
    assert_eq!(pilot_commands.len(), 68);

    // Negative controls: a name rule on a 【Pilot】 name and on a text alias.
    let probe = pilot_commands
        .iter()
        .find(|d| !d.name.contains(d.pilot_name.as_deref().unwrap_or("")))
        .expect("a 【Pilot】 Command whose name differs from its pilot name");
    let pilot = probe.pilot_name.as_deref().expect("pilot name");
    for (value, card) in [(pilot, &*probe.card_number), ("Zechs Merquise", "ST12-011")] {
        let mut edited = doc.clone();
        let original = doc["attribute_pair_rules"][0]["member_predicate"].clone();
        edited["attribute_pair_rules"][0]["member_predicate"] =
            json!({"any_of": [original, {"name_contains": value}]});
        BanlistHistory::from_json(&edited.to_string()).expect("the edited copy loads");
        let findings = alias_findings(db, &predicates(&edited));
        assert!(
            findings.iter().any(|f| f.contains(card)),
            "{value}: {findings:#?}"
        );
    }
}

/// W-74 (c): a printing whose colour is not exactly one of the five colours (the validator
/// counts one colour per card), and a main-deck card with no colour (`check_colors` skips it).
fn colour_findings(records: &[Value], db: &CardDb) -> Vec<String> {
    const COLOURS: [&str; 5] = ["Blue", "Green", "Red", "White", "Purple"];
    let mut out = Vec::new();
    for r in records {
        let c = &r["color"];
        if !(c.is_null() || c.as_str().is_some_and(|s| COLOURS.contains(&s))) {
            out.push(format!(
                "{}: color {c} is not one colour",
                r["product_id"].as_str().unwrap_or("?")
            ));
        }
    }
    for d in db.real_cards() {
        if MAIN_DECK_TYPES.contains(&d.card_type) && d.color.is_none() {
            out.push(format!(
                "{}: a {} card without a colour",
                d.card_number,
                d.card_type.as_str()
            ));
        }
    }
    out
}

#[test]
fn w74_one_colour_per_card_with_negative_controls() {
    let records = printings(&read(CARDS));
    assert_eq!(
        colour_findings(&records, CardDb::embedded()),
        Vec::<String>::new()
    );
    let multi = vanilla_shape("SY01-004", "-", "-", json!("Blue/Red"));
    let mut with_multi = records.clone();
    with_multi.push(multi);
    let mut findings = colour_findings(&with_multi, CardDb::embedded());
    assert_eq!(findings, ["SY01-004: color \"Blue/Red\" is not one colour"]);
    assert!(
        CardDb::from_sources(&ndjson(&with_multi), &read(OVERRIDES)).is_err(),
        "the loader rejects it too, without naming the colour rule"
    );
    let (ps, copy) = with_printings(&[vanilla_shape("SY01-005", "-", "-", Value::Null)]);
    findings = colour_findings(&ps, &copy);
    assert_eq!(findings, ["SY01-005: a UNIT card without a colour"]);
}
