//! P6-K data tripwires (docs/dev/PLAN.md §12.2 P6-K; docs/dev/eval/weakness-status/P6-K.md).
//!
//! Each tripwire is a pure function over the packaged sources (or a modified copy of them held
//! in memory, the "temporary data copy" of the WEAKNESSES entries) that returns its findings.
//! The test asserts the packaged data gives exactly the reviewed findings, and a negative
//! control shows that a synthetic card or an edited copy makes the tripwire fail. Nothing here
//! changes behaviour: the packaged files are read as they are, and the cached sources stay
//! unedited (CLAUDE.md "Cached sources are read-only").
//!
//! Items: W-55 (Base AP), W-58, W-60/W-155 (stripped parentheticals), W-79, W-80, W-82, W-83,
//! W-84, W-85, W-86, W-87, W-88, W-89, W-116, W-145, W-146, W-157, W-164.

use std::collections::{BTreeMap, BTreeSet};

use gcg_data::{CardDb, CardType, conflict_resolutions, parse_deck, resolve_card_number, versions};
use serde_json::Value;
use sha2::{Digest, Sha256};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const CARDS: &str = "src/gcg_sim/data/gcgapi/cards.ndjson";
const OVERRIDES: &str = "src/gcg_sim/data/overrides.json";
const CURATED: &str = "src/gcg_sim/data/curated_conflicts.json";
const CONFLICTS: &str = "data/conflicts.json";
const LOCK: &str = "data/SOURCES.lock.json";
const RULES_MD: &str = "src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md";
const RULES_PDF_TXT: &str = "data/official_raw/en_pdf_comprehensiverules_en.txt";

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn read_bytes(rel: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("{ROOT}/{rel}")).ok()
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `load_raw_printings()` (`cards/db.py:57-59`).
fn printings(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("a printing record"))
        .collect()
}

fn ndjson(records: &[Value]) -> String {
    records.iter().map(|r| format!("{r}\n")).collect()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

/// A `gcg_core::pyjson::Json` value as a `serde_json::Value`.
fn to_value(j: &gcg_core::pyjson::Json) -> Value {
    let text = gcg_core::pyjson::dumps(j, gcg_core::pyjson::DumpOptions::COMPACT_SORTED_ASCII);
    serde_json::from_str(&text).expect("valid JSON")
}

fn packaged_db() -> &'static CardDb {
    CardDb::embedded()
}

/// A Unit printing for synthetic copies of the snapshot.
fn synthetic_printing(number: &str, card_type: &str, ap: Option<i64>, effect: &str) -> Value {
    serde_json::json!({
        "product_id": number, "card_number": number, "name": format!("Synthetic {number}"),
        "set_code": number.split('-').next().unwrap_or("SY"), "rarity": "C",
        "card_type": card_type, "color": "Blue", "level": 2, "cost": 1,
        "ap": ap, "hp": 2, "ap_raw": ap.map(|a| a.to_string()), "hp_raw": "2",
        "zone": "Space", "trait": "(Earth Federation)", "link": "-",
        "traits": ["Earth Federation"], "effect": effect,
    })
}

fn db_with(extra: &[Value]) -> CardDb {
    let mut ps = printings(&read(CARDS));
    ps.extend(extra.iter().cloned());
    CardDb::from_sources(&ndjson(&ps), &read(OVERRIDES)).expect("the synthetic copy loads")
}

// ---------------------------------------------------------------------------------------------
// The Python text normaliser (effects/text.py), reimplemented for the text tripwires. Its output is
// checked against the IR's `text_hash` of every card (`text_hash`, text.py:98-99).

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `_is_reminder` (`effects/text.py:17-28`).
fn is_reminder(inner: &str) -> bool {
    let s = inner.trim();
    if s.is_empty() {
        return false;
    }
    if s.starts_with('(') && s.ends_with(')') && s.matches('(').count() == 1 {
        return false;
    }
    let plain = !s.chars().any(|c| matches!(c, '.' | '(' | ')'));
    let function_word = s.split(|c: char| !is_word(c)).any(|w| {
        matches!(
            w.to_lowercase().as_str(),
            "this" | "the" | "a" | "an" | "when" | "while" | "to" | "of" | "your"
        )
    });
    if plain && !function_word {
        return false;
    }
    s.ends_with('.') || s.ends_with(".)") || s.split_whitespace().count() >= 4
}

/// The length of a token definition `[Name]((...)･AP1･HP1)` at the start of `s`
/// (`text.py:55`: `\[[^\[\]]+\]\(\((?:[^()]|\([^()]*\))*\)`).
fn token_def_len(s: &[char]) -> Option<usize> {
    let n = s.len();
    let mut j = 1;
    while j < n && s[j] != ']' {
        if s[j] == '[' {
            return None;
        }
        j += 1;
    }
    if j == 1 || j >= n {
        return None;
    }
    j += 1;
    if j + 1 >= n || s[j] != '(' || s[j + 1] != '(' {
        return None;
    }
    j += 2;
    loop {
        match s.get(j)? {
            ')' => return Some(j + 1),
            '(' => {
                let mut k = j + 1;
                while k < n && s[k] != '(' && s[k] != ')' {
                    k += 1;
                }
                if k < n && s[k] == ')' {
                    j = k + 1;
                } else {
                    return None;
                }
            }
            _ => j += 1,
        }
    }
}

/// `strip_reminders` (`effects/text.py:31-62`); every stripped parenthetical's inner text is
/// pushed to `stripped`.
fn strip_reminders(text: &str, stripped: &mut Vec<String>) -> String {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let ch = c[i];
        if ch == '(' && !(i > 0 && c[i - 1] == ']') {
            let mut depth = 0i32;
            let mut j = i;
            while j < n {
                if c[j] == '(' {
                    depth += 1;
                } else if c[j] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            let inner: String = c[i + 1..j.min(n)].iter().collect();
            if j < n && is_reminder(&inner) {
                stripped.push(inner);
                i = j + 1;
                continue;
            }
            out.extend(&c[i..(j + 1).min(n)]);
            i = j + 1;
            continue;
        }
        if ch == '['
            && i + 1 < n
            && let Some(len) = token_def_len(&c[i..])
        {
            out.extend(&c[i..i + len]);
            i += len;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// `re.sub(r"(\w)' (s|t|re|ll|ve|d)\b", r"\1'\2", t)` (`text.py:81`).
fn join_contractions(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    'scan: while i < c.len() {
        if is_word(c[i]) && i + 2 < c.len() && c[i + 1] == '\'' && c[i + 2] == ' ' {
            for suffix in ["s", "t", "re", "ll", "ve", "d"] {
                let sc: Vec<char> = suffix.chars().collect();
                let start = i + 3;
                let end = start + sc.len();
                if end <= c.len() && c[start..end] == sc[..] && (end == c.len() || !is_word(c[end]))
                {
                    out.push(c[i]);
                    out.push('\'');
                    out.extend(&sc);
                    i = end;
                    continue 'scan;
                }
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// `_WS.sub(" ", raw).strip()` with `_WS = [ \t　]+` (`text.py:15`, `:86`).
fn collapse_line(raw: &str) -> String {
    let mut out = String::new();
    let mut in_ws = false;
    for ch in raw.chars() {
        if matches!(ch, ' ' | '\t' | '\u{3000}') {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_owned()
}

/// `normalize` (`effects/text.py:65-95`).
fn normalize(text: &str, stripped: &mut Vec<String>) -> String {
    let t = text
        .replace('・', "･")
        .replace("\r\n", "\n")
        .replace('’', "'")
        .replace(['“', '”'], "\"")
        .replace("【Activate ･Main】", "【Activate･Main】")
        .replace("【Activate ･Action】", "【Activate･Action】")
        .replace("【Activate･ Main】", "【Activate･Main】")
        .replace("【Activate･ Action】", "【Activate･Action】")
        .replace('：', ":");
    let t = join_contractions(&t).replace("[Suppression]", "<Suppression>");
    let t = strip_reminders(&t, stripped);
    t.split('\n')
        .map(collapse_line)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn norm(text: &str) -> String {
    normalize(text, &mut Vec::new())
}

fn single_spaced(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_text_port_reproduces_every_cards_text_hash() {
    let ir = json("crates/gcg-effects/ir/registry.json");
    let db = packaged_db();
    let mut checked = 0;
    for card in arr(&ir["cards"]) {
        let def = db
            .get(s(card, "card_number"))
            .or_else(|| {
                db.defs()
                    .iter()
                    .find(|d| *d.card_number == *s(card, "card_number"))
            })
            .expect("an IR card is a CardDef");
        let hash = sha256_hex(norm(&def.effect).as_bytes());
        assert_eq!(&hash[..16], s(card, "text_hash"), "{}", def.card_number);
        checked += 1;
    }
    assert_eq!(checked, db.len());
}

// ---------------------------------------------------------------------------------------------
// W-55: every Base has 0 AP (Bases deal no battle damage, docs/ASSUMPTIONS.md:54).

fn base_ap_violations(db: &CardDb) -> Vec<String> {
    db.defs()
        .iter()
        .filter(|d| d.card_type.is_base() && d.ap != 0)
        .map(|d| format!("{} ({:?}) has AP {}", d.card_number, d.card_type, d.ap))
        .collect()
}

#[test]
fn w55_bases_have_zero_ap_with_negative_control() {
    assert_eq!(base_ap_violations(packaged_db()), Vec::<String>::new());
    let base = synthetic_printing("SY-B01", "BASE", Some(1), "-");
    let db = db_with(&[base]);
    assert_eq!(
        base_ap_violations(&db),
        vec!["SY-B01 (Base) has AP 1".to_owned()],
        "a Base with AP 1 must trip the wire"
    );
}

// ---------------------------------------------------------------------------------------------
// W-58: 402 conflicts, 0 unresolved, the policy counts; a deck with ST13-013 reports its
// conflict id.

#[test]
fn w58_conflict_counts_and_policies() {
    let cc = json(CONFLICTS);
    let summary = &cc["summary"];
    assert_eq!(summary["total"], 402);
    for key in [
        "unresolved",
        "invalid",
        "stale_curated",
        "orphaned_resolutions",
    ] {
        assert_eq!(arr(&summary[key]).len(), 0, "summary.{key}");
    }
    let conflicts = arr(&cc["conflicts"]);
    assert_eq!(conflicts.len(), 402);
    let mut by_policy: BTreeMap<&str, usize> = BTreeMap::new();
    for c in conflicts {
        *by_policy.entry(s(&c["resolution"], "policy")).or_default() += 1;
    }
    let want: BTreeMap<&str, usize> = [
        ("banlist_most_defensible", 6),
        ("canonical_printing", 45),
        ("edition_beta_superseded", 1),
        ("engine_derives_markers", 5),
        ("errata_supersedes_print", 3),
        ("faq_clarifies_rules", 16),
        ("most_defensible_reading", 110),
        ("null_stat_is_zero", 3),
        ("rules_obvious_reading", 3),
        ("rules_xref_intended_target", 11),
        ("ruling_overrides_text", 192),
        ("text_format_correction", 3),
        ("token_stats_from_effect", 4),
    ]
    .into_iter()
    .collect();
    assert_eq!(by_policy, want);
    let overrides = json(OVERRIDES);
    let ids: BTreeSet<&str> = overrides["resolutions"]
        .as_object()
        .expect("resolutions")
        .keys()
        .map(String::as_str)
        .collect();
    let conflict_ids: BTreeSet<&str> = conflicts.iter().map(|c| s(c, "id")).collect();
    assert_eq!(
        ids, conflict_ids,
        "every conflict has exactly one resolution"
    );
}

#[test]
fn w58_a_deck_with_st13_013_lists_its_conflict_ids() {
    let deck = parse_deck("4 ST13-013\n4 GD01-060\n", "probe", None).expect("deck parses");
    let listed = to_value(&conflict_resolutions(&[("deck_under_test", &deck)]));
    let ids: BTreeSet<&str> = arr(&listed)
        .iter()
        .filter(|e| s(e, "card_number") == "ST13-013")
        .map(|e| s(e, "conflict_id"))
        .collect();
    assert!(
        ids.contains("ambiguous:ST13-013:deploy-1-to-2-tokens"),
        "{listed}"
    );
}

// ---------------------------------------------------------------------------------------------
// W-60, W-155: the stripped-parenthetical gate.

/// Reviewed rule and keyword reminders that `normalize` may strip (W-155). Each names the
/// keyword or rule it restates.
const REMINDER_ALLOWLIST: [(&str, &str); 13] = [
    (
        "Rest a Resource when paying a cost.",
        "Resource rule text (3-6, 7-5-2-2-3)",
    ),
    (
        "Rest this Unit to change the attack target to it.",
        "<Blocker> reminder (13-1-4)",
    ),
    (
        "When this Unit's attack destroys an enemy Unit, deal the specified amount of damage to the first card in that opponent's shield area.",
        "<Breach> reminder, old wording (13-1-2; ambiguous:breach-reminder)",
    ),
    (
        "During your turn, when this Unit destroys an enemy Unit with battle damage, deal the specified amount of damage to the first card in that opponent's shield area.",
        "<Breach> reminder (13-1-2)",
    ),
    (
        "At the end of your turn, this Unit recovers the specified number of HP.",
        "<Repair> reminder (13-1-1)",
    ),
    (
        "Rest this Unit. 1 other friendly Unit gets AP+(specified amount) during this turn.",
        "<Support> reminder (13-1-3)",
    ),
    (
        "While this Unit is attacking, it deals damage before the enemy Unit.",
        "<First Strike> reminder (13-1-5)",
    ),
    (
        "This Unit can't be blocked.",
        "<High-Maneuver> reminder (13-1-6)",
    ),
    (
        "Damage to Shields by an attack is dealt to the first 2 cards simultaneously.",
        "<Suppression> reminder (13-1-7)",
    ),
    (
        "At the start of the game, place 1 active EX Base as your shield area's base.",
        "EX Base setup (6-2-3, 5-17-3-1-2)",
    ),
    (
        "Rest an EX Resource then exile it from the game when paying a cost.",
        "EX Resource cost (5-17-3-2-3)",
    ),
    (
        "At the start of the game, the second-turn player places 1 active EX Resource into their resource area.",
        "EX Resource setup (6-2-4, 5-17-3-2-2)",
    ),
    (
        "At the start of the game, the second-turn player places 1 EX Resource into their resource area.",
        "EX Resource setup, EXRP-001 variant (6-2-4)",
    ),
];

/// Every stripped parenthetical that is not reviewed reminder text, verbatim per card (W-60:
/// the bindings implement these; W-155: a rewording must be reviewed).
const PARENTHETICAL_SNAPSHOT: [(&str, &str); 3] = [
    ("GD05-089", "Don't treat it as a Pilot."),
    ("ST12-013", "You choose first, then your opponent."),
    (
        "ST14-014",
        "This card that is being activated is not counted.",
    ),
];

/// The stripped parentheticals of `(card number, effect text)` pairs, whitespace-normalised.
fn stripped_parentheticals<'a>(
    cards: impl Iterator<Item = (&'a str, &'a str)>,
) -> BTreeMap<String, Vec<String>> {
    let mut by_card: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (number, effect) in cards {
        let mut stripped = Vec::new();
        normalize(effect, &mut stripped);
        for inner in stripped {
            let key = single_spaced(&inner);
            let list = by_card.entry(number.to_owned()).or_default();
            if !list.contains(&key) {
                list.push(key);
            }
        }
    }
    by_card
}

fn parenthetical_gate(by_card: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let allowed: BTreeSet<&str> = REMINDER_ALLOWLIST.iter().map(|(t, _)| *t).collect();
    let snapshot: BTreeMap<&str, &str> = PARENTHETICAL_SNAPSHOT.iter().copied().collect();
    let mut findings = Vec::new();
    for (number, texts) in by_card {
        for t in texts {
            if allowed.contains(t.as_str()) || snapshot.get(number.as_str()) == Some(&t.as_str()) {
                continue;
            }
            match snapshot.get(number.as_str()) {
                Some(old) => findings.push(format!(
                    "{number}: snapshot parenthetical changed: old {old:?}, new {t:?}"
                )),
                None => findings.push(format!("{number}: unreviewed parenthetical {t:?}")),
            }
        }
    }
    for (number, old) in PARENTHETICAL_SNAPSHOT {
        if !by_card
            .get(number)
            .is_some_and(|t| t.iter().any(|x| x == old))
        {
            findings.push(format!("{number}: snapshot parenthetical {old:?} is gone"));
        }
    }
    findings
}

fn packaged_cards() -> Vec<(String, String)> {
    packaged_db()
        .defs()
        .iter()
        .map(|d| (d.card_number.to_string(), d.effect.to_string()))
        .collect()
}

fn gate_on(cards: &[(String, String)]) -> Vec<String> {
    parenthetical_gate(&stripped_parentheticals(
        cards.iter().map(|(n, e)| (n.as_str(), e.as_str())),
    ))
}

#[test]
fn w155_stripped_parentheticals_equal_python() {
    let vector = json("oracle/tests/vectors/p6k_parentheticals.json");
    let mut want: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (text, numbers) in vector["stripped"].as_object().expect("stripped") {
        for n in arr(numbers) {
            want.entry(n.as_str().expect("number").to_owned())
                .or_default()
                .insert(text.clone());
        }
    }
    let cards = packaged_cards();
    let got: BTreeMap<String, BTreeSet<String>> =
        stripped_parentheticals(cards.iter().map(|(n, e)| (n.as_str(), e.as_str())))
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
    assert_eq!(got, want);
    let distinct: BTreeSet<&String> = got.values().flatten().collect();
    assert_eq!(
        distinct.len(),
        16,
        "13 reviewed reminders + 3 snapshot texts"
    );
}

#[test]
fn w155_parenthetical_gate_passes_on_the_packaged_data() {
    assert_eq!(gate_on(&packaged_cards()), Vec::<String>::new());
    let by_card = stripped_parentheticals(
        packaged_cards()
            .iter()
            .map(|(n, e)| (n.as_str(), e.as_str()))
            .collect::<Vec<_>>()
            .into_iter(),
    );
    let allowed: BTreeSet<&str> = REMINDER_ALLOWLIST.iter().map(|(t, _)| *t).collect();
    let unreviewed: BTreeSet<&str> = by_card
        .iter()
        .filter(|(_, texts)| texts.iter().any(|t| !allowed.contains(t.as_str())))
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        unreviewed,
        ["GD05-089", "ST12-013", "ST14-014"].into_iter().collect(),
        "W-60: the semantic parentheticals are exactly the known cards"
    );
}

fn edit(cards: &[(String, String)], number: &str, from: &str, to: &str) -> Vec<(String, String)> {
    let mut out = cards.to_vec();
    let card = out
        .iter_mut()
        .find(|(n, _)| n == number)
        .expect("card present");
    assert!(card.1.contains(from), "{number}: {from:?} not in its text");
    card.1 = card.1.replace(from, to);
    out
}

#[test]
fn w155_parenthetical_gate_negative_controls() {
    let cards = packaged_cards();
    let reworded = edit(
        &cards,
        "GD05-089",
        "(Don't treat it as a Pilot.)",
        "(Treat it as a Pilot as well.)",
    );
    let findings = gate_on(&reworded);
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("GD05-089: snapshot parenthetical changed")),
        "{findings:?}"
    );
    let typographic = edit(
        &cards,
        "GD01-009",
        "(This Unit can't be blocked.)",
        "(This  Unit can’t be   blocked.)",
    );
    assert_eq!(
        gate_on(&typographic),
        Vec::<String>::new(),
        "normalize removes it"
    );
    let rewritten = edit(
        &cards,
        "GD01-009",
        "(This Unit can't be blocked.)",
        "(This Unit can't be blocked by Units with <Blocker>.)",
    );
    let findings = gate_on(&rewritten);
    assert_eq!(
        findings,
        vec![
            "GD01-009: unreviewed parenthetical \"This Unit can't be blocked by Units with <Blocker>.\""
                .to_owned()
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// W-79: every official erratum is either in the gcg-api text or covered by an override.

struct Erratum {
    source: String,
    card: String,
    after: String,
}

/// The errata notices of the official-source data (`data/sources.d/official.json` entries
/// `official-en-errata-*`), parsed from their cached text extraction.
fn official_errata() -> Vec<Erratum> {
    let mut out = Vec::new();
    for e in arr(&json("data/sources.d/official.json")) {
        if !s(e, "id").starts_with("official-en-errata-") {
            continue;
        }
        let txt = s(e, "local_path").replace(".html", ".txt");
        let text = read(&txt);
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let after_label = |label: &str| {
            lines
                .iter()
                .position(|l| *l == label)
                .and_then(|i| lines.get(i + 1))
                .map(|l| (*l).to_owned())
                .unwrap_or_else(|| panic!("{txt}: no {label:?} block"))
        };
        let card = after_label("Applicable Card")
            .split_whitespace()
            .next()
            .expect("card number")
            .to_owned();
        out.push(Erratum {
            source: s(e, "id").to_owned(),
            card,
            after: after_label("After").trim_end_matches('.').to_owned(),
        });
    }
    out
}

fn errata_findings(db: &CardDb, overrides: &Value) -> Vec<String> {
    let api = json("src/gcg_sim/data/gcgapi/errata.json");
    let mut findings = Vec::new();
    for e in official_errata() {
        let def = db.get(&e.card).expect("an errata card is in the data");
        let traits = def
            .traits
            .iter()
            .map(|t| format!("({t})"))
            .collect::<String>();
        let effective = def.effect.contains(&e.after) || traits.contains(&e.after);
        let in_api = arr(&api)
            .iter()
            .any(|a| s(a, "card_number") == e.card && s(a, "status") == "applied");
        let by_override = overrides["resolutions"]
            .as_object()
            .expect("resolutions")
            .values()
            .any(|r| {
                s(r, "policy") == "errata_supersedes_print"
                    && r.get("field_overrides").is_some()
                    && arr(&r["card_numbers"]).iter().any(|n| n == e.card.as_str())
            });
        if !effective {
            findings.push(format!(
                "{}: {} lacks the errata text {:?}",
                e.source, e.card, e.after
            ));
        }
        if !in_api && !by_override {
            findings.push(format!(
                "{}: {} is neither in gcg-api errata.json nor covered by an override",
                e.source, e.card
            ));
        }
    }
    findings
}

#[test]
fn w79_errata_are_applied_with_negative_control() {
    let overrides = json(OVERRIDES);
    let errata = official_errata();
    let cards: BTreeSet<&str> = errata.iter().map(|e| e.card.as_str()).collect();
    assert_eq!(
        cards,
        ["GD04-067", "ST12-001", "T-013"].into_iter().collect()
    );
    assert_eq!(
        errata_findings(packaged_db(), &overrides),
        Vec::<String>::new()
    );
    let st12 = packaged_db().get("ST12-001").expect("ST12-001");
    assert!(
        st12.effect
            .contains("【During Pair･Lv.5 or Higher Pilot】【Once per Turn】")
    );

    let mut without = overrides.clone();
    without["resolutions"]
        .as_object_mut()
        .expect("resolutions")
        .remove("errata-unapplied:ST12-001:news-02_193")
        .expect("the ST12-001 resolution");
    let db = CardDb::from_sources(&read(CARDS), &without.to_string()).expect("loads");
    let findings = errata_findings(&db, &without);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings.iter().all(|f| f.contains("ST12-001")),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// W-80: bracketed keyword names and ASCII cost separators.

const KEYWORDS: [&str; 7] = [
    "Repair",
    "Breach",
    "Support",
    "Blocker",
    "First Strike",
    "High-Maneuver",
    "Suppression",
];

/// `_KEYWORD_TAG.sub(" ", effect)` (`sources/conflicts.py:152`): `<Keyword>` and
/// `<Keyword N>` tags become a space.
fn without_keyword_tags(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '<' && c.get(i + 1).is_some_and(char::is_ascii_alphabetic) {
            let mut j = i + 2;
            while j < c.len() && (c[j].is_ascii_alphabetic() || c[j] == ' ' || c[j] == '-') {
                j += 1;
            }
            if j < c.len() && c[j].is_ascii_digit() && c[j - 1].is_whitespace() {
                while j < c.len() && c[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if c.get(j) == Some(&'>') {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// `_BARE_KEYWORD` (`sources/conflicts.py:155-157`) over the tag-free text.
fn bare_keywords(effect: &str) -> Vec<&'static str> {
    let text = without_keyword_tags(effect);
    let mut found = Vec::new();
    for kw in KEYWORDS {
        for (p, _) in text.match_indices(kw) {
            let before = text[..p].chars().next_back();
            let after = text[p + kw.len()..].chars().next();
            let bad_before =
                before.is_some_and(|b| b == '<' || b == '(' || b.is_ascii_alphabetic());
            let bad_after = after.is_some_and(|a| a == '>' || a == ')' || a.is_ascii_alphabetic());
            if !bad_before && !bad_after && !found.contains(&kw) {
                found.push(kw);
            }
        }
    }
    found
}

/// A line of an activated effect whose cost separator is an ASCII colon
/// (`sources/conflicts.py:904-907`: `【Activate` in the line, no `：`, `(?<!\d):\s`).
fn ascii_cost_colon(effect: &str) -> bool {
    effect.lines().any(|line| {
        line.contains("【Activate")
            && !line.contains('：')
            && line.char_indices().any(|(i, ch)| {
                ch == ':'
                    && !line[..i]
                        .chars()
                        .next_back()
                        .is_some_and(|b| b.is_ascii_digit())
                    && line[i + 1..]
                        .chars()
                        .next()
                        .is_some_and(char::is_whitespace)
            })
    })
}

fn format_defects(ps: &[Value]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in ps {
        let effect = s(p, "effect");
        for kw in bare_keywords(effect) {
            out.entry(s(p, "card_number").to_owned())
                .or_default()
                .push(format!("{}: bare keyword {kw}", s(p, "product_id")));
        }
        if ascii_cost_colon(effect) {
            out.entry(s(p, "card_number").to_owned())
                .or_default()
                .push(format!("{}: ASCII cost separator", s(p, "product_id")));
        }
    }
    out
}

#[test]
fn w80_text_format_defects_are_exactly_the_known_cards_and_covered() {
    let ps = printings(&read(CARDS));
    let defects = format_defects(&ps);
    let numbers: BTreeSet<&str> = defects.keys().map(String::as_str).collect();
    assert_eq!(
        numbers,
        ["GD02-053", "GD03-015", "GD03-035"].into_iter().collect(),
        "{defects:#?}"
    );
    let overrides = json(OVERRIDES);
    let db = packaged_db();
    for number in numbers {
        let covered = overrides["resolutions"]
            .as_object()
            .expect("resolutions")
            .values()
            .any(|r| {
                s(r, "policy") == "text_format_correction"
                    && arr(&r["card_numbers"]).iter().any(|n| n == number)
                    && r["field_overrides"].get("effect").is_some()
            });
        assert!(covered, "{number}: no text_format_correction override");
        let effective = &db.get(number).expect("card").effect;
        assert!(bare_keywords(effective).is_empty(), "{number}: {effective}");
        assert!(!ascii_cost_colon(effective), "{number}: {effective}");
    }
    assert!(
        db.get("GD02-053")
            .expect("GD02-053")
            .effect
            .starts_with("<Suppression>")
    );
    let mut synthetic = ps.clone();
    synthetic.push(synthetic_printing(
        "SY-K01",
        "UNIT",
        Some(2),
        "[Blocker]\n【Activate･Main】①: Draw 1.",
    ));
    let found = format_defects(&synthetic);
    assert_eq!(
        found.get("SY-K01").map(Vec::len),
        Some(2),
        "negative control: {found:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// W-82: null printed AP.

fn reviewed_null_ap(overrides: &Value) -> BTreeSet<String> {
    let mut reviewed = BTreeSet::new();
    for id in [
        "data-null:BASE:ap",
        "data-null:EX-BASE:ap",
        "data-null:UNIT:ap",
    ] {
        for n in arr(&overrides["resolutions"][id]["card_numbers"]) {
            reviewed.insert(n.as_str().expect("number").to_owned());
        }
    }
    reviewed
}

/// Reviewed null-AP token printings: T-012 and T-014 have no printed AP (0, `cards/db.py:110`),
/// and T-001_p1 is the defective Edition Beta record of W-83 (`divergent:T-001`).
const REVIEWED_NULL_AP_TOKENS: [&str; 3] = ["T-001_p1", "T-012", "T-014"];

fn null_ap_findings(ps: &[Value], overrides: &Value) -> (BTreeMap<String, usize>, Vec<String>) {
    let reviewed = reviewed_null_ap(overrides);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut findings = Vec::new();
    for p in ps {
        let ty = s(p, "card_type");
        if !matches!(ty, "UNIT" | "UNIT TOKEN" | "BASE" | "EX BASE") || !p["ap"].is_null() {
            continue;
        }
        *counts.entry(ty.to_owned()).or_default() += 1;
        let ok = match ty {
            "UNIT TOKEN" => REVIEWED_NULL_AP_TOKENS.contains(&s(p, "product_id")),
            _ => reviewed.contains(s(p, "card_number")),
        };
        if !ok {
            findings.push(format!(
                "{} ({ty}) has a null AP that no review covers",
                s(p, "product_id")
            ));
        }
    }
    (counts, findings)
}

#[test]
fn w82_null_ap_printings_are_reviewed_with_negative_control() {
    let ps = printings(&read(CARDS));
    let overrides = json(OVERRIDES);
    let (counts, findings) = null_ap_findings(&ps, &overrides);
    assert_eq!(findings, Vec::<String>::new());
    let want: BTreeMap<String, usize> = [
        ("BASE", 109),
        ("EX BASE", 33),
        ("UNIT", 13),
        ("UNIT TOKEN", 3),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    assert_eq!(counts, want);
    let mut synthetic = ps;
    synthetic.push(synthetic_printing("SY-N01", "UNIT", None, "-"));
    let (_, findings) = null_ap_findings(&synthetic, &overrides);
    assert_eq!(
        findings,
        vec!["SY-N01 (UNIT) has a null AP that no review covers".to_owned()]
    );
}

// ---------------------------------------------------------------------------------------------
// W-83: Edition Beta stat records.

fn stat(p: &Value) -> (Option<i64>, Option<i64>) {
    (p["ap"].as_i64(), p["hp"].as_i64())
}

fn stat_deviations(ps: &[Value], db: &CardDb, overrides: &Value) -> (Vec<String>, Vec<String>) {
    let by_product: BTreeMap<&str, &Value> = ps.iter().map(|p| (s(p, "product_id"), p)).collect();
    let mut flagged = Vec::new();
    let mut uncovered = Vec::new();
    for p in ps {
        let def = db.get(s(p, "card_number")).expect("card");
        let canonical = by_product[&*def.canonical_product_id];
        let deviates =
            s(p, "product_id") != s(canonical, "product_id") && stat(p) != stat(canonical);
        let resource_stats = s(p, "card_type") == "RESOURCE" && stat(p) != (None, None);
        if !deviates && !resource_stats {
            continue;
        }
        let pid = s(p, "product_id").to_owned();
        let res = &overrides["resolutions"][format!("divergent:{}", s(p, "card_number"))];
        let canonical_choice = res
            .get("canonical_product_id")
            .and_then(Value::as_str)
            .is_some_and(|c| c != pid);
        if s(p, "where_to_get") != "Edition Beta" || !canonical_choice {
            uncovered.push(pid.clone());
        }
        flagged.push(pid);
    }
    (flagged, uncovered)
}

#[test]
fn w83_edition_beta_stat_records_are_all_superseded() {
    let ps = printings(&read(CARDS));
    let overrides = json(OVERRIDES);
    let (flagged, uncovered) = stat_deviations(&ps, packaged_db(), &overrides);
    let flagged: BTreeSet<&str> = flagged.iter().map(String::as_str).collect();
    assert_eq!(
        flagged,
        ["R-001_p4", "T-001_p1", "T-002_p1", "T-003_p1", "T-006_p1"]
            .into_iter()
            .collect()
    );
    assert_eq!(uncovered, Vec::<String>::new());
    let mut without = overrides.clone();
    without["resolutions"]
        .as_object_mut()
        .expect("resolutions")
        .remove("divergent:T-002")
        .expect("divergent:T-002");
    let (_, uncovered) = stat_deviations(&ps, packaged_db(), &without);
    assert_eq!(uncovered, vec!["T-002_p1".to_owned()], "negative control");
}

// ---------------------------------------------------------------------------------------------
// W-84: link, name and trait anomalies.

fn link_shape_ok(link: &str) -> bool {
    link.split(" / ").all(|alt| {
        let alt = alt.trim();
        (alt.starts_with('[') && alt.ends_with(']') && alt.matches('[').count() == 1)
            || (alt.starts_with('(') && (alt.ends_with(')') || alt.ends_with(") Trait")))
    })
}

/// Parenthesised trait references in a card's normalised text (not token definitions, card
/// names in `[...]`, timing brackets or stat blocks).
fn text_trait_refs(effect: &str) -> Vec<String> {
    let t: Vec<char> = norm(effect).chars().collect();
    let mut refs = Vec::new();
    let mut i = 0;
    while i < t.len() {
        match t[i] {
            '[' => {
                if let Some(len) = token_def_len(&t[i..]) {
                    i += len;
                } else {
                    while i < t.len() && t[i] != ']' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '【' => {
                while i < t.len() && t[i] != '】' {
                    i += 1;
                }
                i += 1;
            }
            '(' => {
                let end = t[i + 1..].iter().position(|c| *c == '(' || *c == ')');
                if let Some(k) = end
                    && t[i + 1 + k] == ')'
                {
                    let inner: String = t[i + 1..i + 1 + k].iter().collect();
                    if !inner.contains('･') && !(inner.contains("AP") && inner.contains("HP")) {
                        refs.push(inner);
                    }
                    i += k + 2;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    refs
}

#[test]
fn w84_link_and_trait_anomalies_equal_the_known_set() {
    let db = packaged_db();
    let pilot_names: BTreeSet<&str> = db
        .real_cards()
        .filter_map(|d| match d.card_type {
            CardType::Pilot => Some(d.names().iter().map(|n| &**n).collect::<Vec<_>>()),
            _ => d.pilot_name.as_deref().map(|n| vec![n]),
        })
        .flatten()
        .collect();
    let traits: BTreeSet<&str> = db
        .defs()
        .iter()
        .flat_map(|d| d.traits.iter().map(|t| &**t))
        .collect();
    let mut anomalies = BTreeSet::new();
    for d in db.real_cards() {
        if let Some(link) = &d.link {
            for part in &link.name_parts {
                if !pilot_names.iter().any(|n| n.contains(&**part)) {
                    anomalies.insert(format!("{} link name {part}", d.card_number));
                }
            }
            for t in &link.traits {
                if !traits.contains(&**t) {
                    anomalies.insert(format!("{} link trait {t}", d.card_number));
                }
            }
        }
        for r in text_trait_refs(&d.effect) {
            if !traits.contains(r.as_str()) {
                anomalies.insert(format!("{} text trait {r}", d.card_number));
            }
        }
    }
    for p in printings(&read(CARDS)) {
        let link = s(&p, "link").trim();
        if !link.is_empty() && link != "-" && !link_shape_ok(link) {
            anomalies.insert(format!("{} link shape {link}", s(&p, "product_id")));
        }
    }
    let want: BTreeSet<String> = [
        "GD05-026 link name Ericht Samaya",
        "GD04-069 text trait Dianna Counter",
        "GD04-120 text trait Dianna Counter",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(anomalies, want);
    assert!(
        !link_shape_ok("Trait [Enhanced Human]"),
        "the old GD01-051_p1 shape"
    );
}

// ---------------------------------------------------------------------------------------------
// W-85: curated conflicts, overrides, the generated conflicts report and the lock agree.

fn join_findings(curated: &Value, overrides: &Value, conflicts: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    let resolutions = overrides["resolutions"].as_object().expect("resolutions");
    let report: BTreeMap<&str, &Value> = arr(&conflicts["conflicts"])
        .iter()
        .map(|c| (s(c, "id"), c))
        .collect();
    for c in arr(&curated["conflicts"]) {
        let id = s(c, "id");
        if !resolutions.contains_key(id) {
            findings.push(format!("curated {id} has no resolution"));
        }
        match report.get(id) {
            None => findings.push(format!("curated {id} is missing from data/conflicts.json")),
            Some(r) => {
                for key in ["kind", "severity", "card_numbers", "title", "description"] {
                    if r[key] != c[key] {
                        findings.push(format!("{id}: curated {key} differs from the report"));
                    }
                }
            }
        }
    }
    for (id, res) in resolutions {
        match report.get(id.as_str()) {
            None => findings.push(format!("resolution {id} resolves no conflict")),
            Some(r) => {
                for (key, value) in res.as_object().expect("a resolution object") {
                    if r["resolution"][key] != *value {
                        findings.push(format!("{id}: resolution {key} differs from the report"));
                    }
                }
            }
        }
        if overrides["policies"].get(s(res, "policy")).is_none() {
            findings.push(format!("{id}: unknown policy {}", s(res, "policy")));
        }
    }
    if conflicts["policies"] != overrides["policies"] {
        findings.push("policies differ between overrides.json and the report".to_owned());
    }
    findings
}

fn lock_findings() -> Vec<String> {
    let lock = json(LOCK);
    let mut findings = Vec::new();
    for d in arr(&lock["derived"]) {
        let path = s(d, "path");
        match read_bytes(path) {
            Some(bytes) if sha256_hex(&bytes) == s(d, "sha256") => {}
            Some(_) => findings.push(format!("{path}: hash drift against the lock")),
            None => findings.push(format!("{path}: missing")),
        }
    }
    let cc = json(CONFLICTS);
    for (name, sha) in cc["inputs"]["sha256"].as_object().expect("inputs.sha256") {
        let path = format!("src/gcg_sim/data/{name}");
        if read_bytes(&path).is_none_or(|b| sha256_hex(&b) != sha.as_str().unwrap_or("")) {
            findings.push(format!(
                "{path}: data/conflicts.json was built from other bytes"
            ));
        }
    }
    findings
}

#[test]
fn w85_curated_conflicts_overrides_and_lock_agree_with_negative_control() {
    let curated = json(CURATED);
    let overrides = json(OVERRIDES);
    let conflicts = json(CONFLICTS);
    assert_eq!(arr(&curated["conflicts"]).len(), 333);
    assert_eq!(
        join_findings(&curated, &overrides, &conflicts),
        Vec::<String>::new()
    );
    assert_eq!(lock_findings(), Vec::<String>::new());
    let mut edited = overrides.clone();
    edited["resolutions"]["ruling:GD01-023:Q125"]["decision"] =
        Value::from("An edited decision text.");
    assert_eq!(
        join_findings(&curated, &edited, &conflicts),
        vec!["ruling:GD01-023:Q125: resolution decision differs from the report".to_owned()]
    );
    let mut orphan = overrides.clone();
    orphan["resolutions"]["ruling:XX-000:Q1"] =
        serde_json::json!({"policy": "ruling_overrides_text", "decision": "d", "rationale": "r"});
    assert_eq!(
        join_findings(&curated, &orphan, &conflicts),
        vec!["resolution ruling:XX-000:Q1 resolves no conflict".to_owned()]
    );
}

// ---------------------------------------------------------------------------------------------
// W-86: printing text divergences.

fn printing_divergences(ps: &[Value], db: &CardDb) -> BTreeMap<String, Vec<String>> {
    let by_product: BTreeMap<&str, &Value> = ps.iter().map(|p| (s(p, "product_id"), p)).collect();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in ps {
        let def = db.get(s(p, "card_number")).expect("card");
        let canonical = by_product[&*def.canonical_product_id];
        let text = |r: &Value| {
            norm(if s(r, "effect").is_empty() {
                "-"
            } else {
                s(r, "effect")
            })
        };
        if text(p) != text(canonical) {
            out.entry(def.card_number.to_string())
                .or_default()
                .push(s(p, "product_id").to_owned());
        }
    }
    out
}

#[test]
fn w86_printing_divergences_equal_the_curated_set() {
    let ps = printings(&read(CARDS));
    let divergent = printing_divergences(&ps, packaged_db());
    assert_eq!(divergent.values().map(Vec::len).sum::<usize>(), 20);
    let overrides = json(OVERRIDES);
    for number in divergent.keys() {
        let res = &overrides["resolutions"][format!("divergent:{number}")];
        assert_eq!(
            s(res, "policy"),
            "canonical_printing",
            "{number}: a divergent printing needs a canonical_printing resolution"
        );
    }
    assert_eq!(
        divergent.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "GD01-005", "GD01-087", "GD01-088", "GD01-089", "GD01-090", "GD01-091", "GD03-118",
            "RP-068", "ST01-015", "ST02-010", "ST03-001", "ST03-011", "ST05-001"
        ]
    );
    let mut synthetic = ps;
    let mut alt = synthetic_printing("GD01-060", "UNIT", Some(2), "<Blocker>");
    alt["product_id"] = Value::from("GD01-060_p9");
    synthetic.push(alt);
    let db = CardDb::from_sources(&ndjson(&synthetic), &read(OVERRIDES)).expect("loads");
    let divergent = printing_divergences(&synthetic, &db);
    assert_eq!(
        divergent.get("GD01-060"),
        Some(&vec!["GD01-060_p9".to_owned()]),
        "negative control: an uncurated divergence is found"
    );
    assert!(
        overrides["resolutions"].get("divergent:GD01-060").is_none(),
        "and no resolution covers it"
    );
}

// ---------------------------------------------------------------------------------------------
// W-87: rules cross-references resolve directly or through the curated mapping.

struct RulesDoc {
    headings: BTreeMap<String, String>,
    paragraphs: Vec<(String, String)>,
}

fn rule_id_prefix(s: &str) -> Option<(&str, &str)> {
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(s.len());
    let id = &s[..end];
    (!id.is_empty() && id.starts_with(|c: char| c.is_ascii_digit()) && !id.ends_with('-'))
        .then(|| (id, &s[end..]))
}

/// `parse_rules` (`sources/conflicts.py:326-358`).
fn parse_rules(md: &str) -> RulesDoc {
    let mut headings = BTreeMap::new();
    let mut paragraphs: Vec<(String, String)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in md.lines() {
        let t = line.trim();
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (2..=6).contains(&hashes) && t[hashes..].starts_with(char::is_whitespace) {
            let rest = t[hashes..].trim_start();
            if let Some((id, tail)) = rule_id_prefix(rest)
                && (tail.starts_with(')') || tail.starts_with('.'))
                && tail[1..].starts_with(char::is_whitespace)
                && (tail.starts_with(')') == !id.contains('-') || tail.starts_with('.'))
            {
                headings.insert(id.to_owned(), tail[1..].trim().replace('\\', ""));
                current = None;
                continue;
            }
        }
        if let Some(rest) = t.strip_prefix("**")
            && let Some((id, tail)) = rule_id_prefix(rest)
            && let Some(body) = tail.strip_prefix(".**")
        {
            paragraphs.push((id.to_owned(), body.trim_start().to_owned()));
            current = Some(paragraphs.len() - 1);
            continue;
        }
        if let Some(i) = current
            && t.starts_with('>')
        {
            let more = t.trim_start_matches(['>', ' ']).trim();
            paragraphs[i].1.push(' ');
            paragraphs[i].1.push_str(more);
        }
    }
    RulesDoc {
        headings,
        paragraphs,
    }
}

/// One `_XREF_PAREN` match (`sources/conflicts.py:166`): the target id and the title.
fn xref_at(c: &[char], start: usize) -> Option<(String, Option<String>, usize)> {
    let mut k = start + 1;
    let rest: String = c[k..c.len().min(k + 4)].iter().collect();
    if (rest.starts_with("See") || rest.starts_with("see"))
        && c.get(k + 3).is_some_and(|x| x.is_whitespace())
    {
        k += 3;
        while c.get(k).is_some_and(|x| x.is_whitespace()) {
            k += 1;
        }
    }
    let digits = |from: usize| c[from..].iter().take_while(|x| x.is_ascii_digit()).count();
    let first = digits(k);
    if !(1..=2).contains(&first) {
        return None;
    }
    let mut id: String = c[k..k + first].iter().collect();
    k += first;
    while c.get(k) == Some(&'-') {
        let d = digits(k + 1);
        if !(1..=3).contains(&d) {
            break;
        }
        id.push('-');
        id.extend(&c[k + 1..k + 1 + d]);
        k += 1 + d;
    }
    let mut title = None;
    if c.get(k) == Some(&'.') {
        let close = c[k + 1..].iter().position(|x| *x == '(' || *x == ')')?;
        if c[k + 1 + close] != ')' {
            return None;
        }
        let t: String = c[k + 1..k + 1 + close].iter().collect();
        let t = t.trim().to_owned();
        title = (!t.is_empty()).then_some(t);
        return Some((id, title, k + 2 + close));
    }
    while c.get(k).is_some_and(|x| x.is_whitespace()) {
        k += 1;
    }
    (c.get(k) == Some(&')')).then_some((id, title.take(), k + 1))
}

/// `_title_key` (`sources/conflicts.py:1101-1104`).
fn title_key(t: &str) -> String {
    let t = t
        .replace('\\', "")
        .replace(['’', '‘'], "'")
        .replace(['“', '”'], "\"")
        .to_lowercase()
        .replace(['【', '】', '<', '>', '(', ')', '[', ']'], " ");
    single_spaced(&t)
}

/// `_titles_match` (`sources/conflicts.py:1107-1109`).
fn titles_match(reference: &str, heading: &str) -> bool {
    let (r, h) = (title_key(reference), title_key(heading));
    r == h || r.trim_end_matches('s') == h.trim_end_matches('s')
}

fn xrefs(rules: &RulesDoc) -> (usize, Vec<(String, String)>) {
    let known: BTreeSet<&str> = rules
        .headings
        .keys()
        .map(String::as_str)
        .chain(rules.paragraphs.iter().map(|(id, _)| id.as_str()))
        .collect();
    let mut total = 0;
    let mut broken = Vec::new();
    for (citing, text) in &rules.paragraphs {
        let c: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < c.len() {
            if c[i] == '('
                && let Some((target, title, end)) = xref_at(&c, i)
            {
                total += 1;
                let heading = rules.headings.get(&target);
                let ok = known.contains(target.as_str())
                    && match (&title, heading) {
                        (None, _) => true,
                        (Some(_), None) => false,
                        (Some(t), Some(h)) => titles_match(t, h),
                    };
                if !ok {
                    broken.push((citing.clone(), target));
                }
                i = end;
                continue;
            }
            i += 1;
        }
    }
    (total, broken)
}

#[test]
fn w87_every_rules_cross_reference_resolves() {
    let rules = parse_rules(&read(RULES_MD));
    assert_eq!(rules.headings.len() + rules.paragraphs.len(), 579);
    let (total, broken) = xrefs(&rules);
    assert_eq!(
        total, 32,
        "parenthesised references found by the detector's pattern"
    );
    assert_eq!(broken.len(), 11, "{broken:?}");
    let overrides = json(OVERRIDES);
    let conflicts = json(CONFLICTS);
    let report: BTreeMap<&str, &Value> = arr(&conflicts["conflicts"])
        .iter()
        .map(|c| (s(c, "id"), c))
        .collect();
    let known: BTreeSet<&str> = rules
        .headings
        .keys()
        .map(String::as_str)
        .chain(rules.paragraphs.iter().map(|(id, _)| id.as_str()))
        .collect();
    let mut unresolved = Vec::new();
    for (citing, target) in &broken {
        let id = format!("rules-xref:{citing}");
        let mapped = s(&overrides["resolutions"][&id], "policy") == "rules_xref_intended_target"
            && report.get(id.as_str()).is_some_and(|c| {
                c["details"]["target"] == target.as_str()
                    && known.contains(s(&c["details"], "suggested_target"))
            });
        if !mapped {
            unresolved.push(id);
        }
    }
    assert_eq!(unresolved, Vec::<String>::new(), "0 unresolved");
}

// ---------------------------------------------------------------------------------------------
// W-146: each rule's text in the markdown equals the official PDF's.

/// PDF-extraction artefacts that the normalisation below cannot remove, each naming its rule:
/// (rule, text in the PDF extraction, text in the markdown).
const PDF_ARTEFACTS: [(&str, &str, &str); 1] = [("10-3-3", "ta rgets", "targets")];

fn pdf_rules(pdf: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut started = false;
    let mut after_page_marker = false;
    let mut section = 0u32;
    for line in pdf.lines() {
        let t = line.trim();
        if t.starts_with("===== page") {
            after_page_marker = true;
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if after_page_marker && t.chars().all(|c| c.is_ascii_digit()) {
            after_page_marker = false;
            continue;
        }
        after_page_marker = false;
        if t == "Comprehensive Rules" {
            started = true;
            continue;
        }
        if !started {
            continue;
        }
        if t.starts_with('●') {
            break;
        }
        if let Some((id, tail)) = rule_id_prefix(t) {
            if !id.contains('-') && tail.starts_with(") ") && id == (section + 1).to_string() {
                section += 1;
                current = None;
                continue;
            }
            if id.contains('-')
                && tail.starts_with('.')
                && tail[1..].chars().next().is_none_or(char::is_whitespace)
                && !out.contains_key(id)
            {
                out.insert(id.to_owned(), tail[1..].trim().to_owned());
                current = Some(id.to_owned());
                continue;
            }
        }
        if let Some(id) = &current {
            let text = out.get_mut(id).expect("current rule");
            text.push(' ');
            text.push_str(t);
        }
    }
    out
}

fn md_rules(md: &str) -> BTreeMap<String, String> {
    let rules = parse_rules(md);
    let mut out: BTreeMap<String, String> = rules
        .paragraphs
        .into_iter()
        .map(|(id, text)| {
            let text = text.replace("> Ex:", "Ex:");
            (id, text)
        })
        .collect();
    for (id, title) in rules.headings {
        if id.contains('-') {
            out.insert(id, title);
        }
    }
    out
}

/// Whitespace, markdown and PDF-extraction normalisation: one space between words, no markdown
/// escapes or bold markers, and no space next to a hyphen, inside `【】`, around `･`/`・`,
/// before `：` or after `“`.
fn squash(text: &str) -> String {
    let t = single_spaced(&text.replace('\\', "").replace("**", ""));
    let c: Vec<char> = t.chars().collect();
    let mut out = String::new();
    for (i, ch) in c.iter().enumerate() {
        if *ch == ' ' {
            let prev = out.chars().next_back();
            let next = c.get(i + 1);
            if prev.is_some_and(|p| matches!(p, '-' | '【' | '･' | '・' | '“'))
                || next.is_some_and(|n| matches!(n, '-' | '】' | '･' | '・' | '：'))
            {
                continue;
            }
        }
        out.push(*ch);
    }
    out
}

fn rule_text_differences(md: &str, pdf: &str) -> Vec<String> {
    let (m, p) = (md_rules(md), pdf_rules(pdf));
    let mut diffs = Vec::new();
    let ids: BTreeSet<&String> = m.keys().chain(p.keys()).collect();
    for id in ids {
        let (Some(a), Some(b)) = (m.get(id), p.get(id)) else {
            diffs.push(format!("{id}: only in one of the two texts"));
            continue;
        };
        let mut b = squash(b);
        for (rule, artefact, intended) in PDF_ARTEFACTS {
            if rule == id {
                b = b.replace(artefact, intended);
            }
        }
        if squash(a) != b {
            diffs.push(format!("{id}: markdown {:?} vs PDF {:?}", squash(a), b));
        }
    }
    diffs
}

#[test]
fn w146_rules_markdown_matches_the_pdf_rule_by_rule_with_negative_control() {
    let md = read(RULES_MD);
    let pdf = read(RULES_PDF_TXT);
    assert_eq!(md_rules(&md).len(), 566);
    assert_eq!(pdf_rules(&pdf).len(), 566);
    assert_eq!(rule_text_differences(&md, &pdf), Vec::<String>::new());
    assert_eq!(PDF_ARTEFACTS.len(), 1, "the reviewed allowlist size");
    let edited = md.replacen(
        "any player may concede at any time",
        "any player may surrender at any time",
        1,
    );
    assert_ne!(edited, md);
    let diffs = rule_text_differences(&edited, &pdf);
    assert_eq!(diffs.len(), 1, "{diffs:?}");
    assert!(diffs[0].starts_with("1-2-4: "), "{diffs:?}");
}

// ---------------------------------------------------------------------------------------------
// W-88: provenance listing (the logic of `gcg-eval data-status --sources`).

fn provenance_listing() -> Vec<(String, &'static str)> {
    let raw_txt: Vec<(String, String)> = std::fs::read_dir(format!("{ROOT}/data/official_raw"))
        .expect("data/official_raw")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .map(|p| {
            let bytes = std::fs::read(&p).expect("readable");
            (p.display().to_string(), sha256_hex(&bytes))
        })
        .collect();
    let mut out = Vec::new();
    for fragment in ["gcgapi", "official", "rules"] {
        for e in arr(&json(&format!("data/sources.d/{fragment}.json"))) {
            let path = e["local_path"].as_str().unwrap_or("not-packaged");
            let class = if read_bytes(path).is_some() {
                "packaged"
            } else {
                let notes = s(e, "notes");
                let text_only = notes.contains("text extraction")
                    && raw_txt.iter().any(|(_, sha)| notes.contains(&sha[..9]));
                if text_only { "text-only" } else { "hash-only" }
            };
            out.push((s(e, "id").to_owned(), class));
        }
    }
    out
}

#[test]
fn w88_provenance_listing_covers_every_source() {
    let listing = provenance_listing();
    let lock = json(LOCK);
    let lock_ids: BTreeSet<&str> = arr(&lock["sources"]).iter().map(|e| s(e, "id")).collect();
    let listed: BTreeSet<&str> = listing.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(listed, lock_ids, "every lock source is listed once");
    assert_eq!(listing.len(), 49);
    let class: BTreeMap<&str, &str> = listing.iter().map(|(id, c)| (id.as_str(), *c)).collect();
    assert_eq!(class["official-asia-floor-pdf"], "text-only");
    assert_eq!(class["gcgapi:commit"], "hash-only");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, c) in &listing {
        *counts.entry(c).or_default() += 1;
    }
    assert_eq!(
        counts.get("text-only"),
        Some(&1),
        "only the Asia floor-rules PDF is text-only: {counts:?}"
    );
    for e in arr(&lock["sources"]) {
        if class[s(e, "id")] == "packaged" && s(e, "kind") != "license" {
            let bytes = read_bytes(s(e, "local_path")).expect("packaged");
            assert_eq!(sha256_hex(&bytes), s(e, "sha256"), "{}", s(e, "id"));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// W-89: dataset_version's sha is the code commit, not the data commit.

#[test]
fn w89_dataset_version_names_the_code_commit_not_the_data_commit() {
    let manifest = json("src/gcg_sim/data/gcgapi/manifest.json");
    let v = to_value(&versions("p", "d"));
    assert_eq!(v["dataset_version"], manifest["dataset_version"]);
    let dataset = s(&v, "dataset_version");
    let sha = dataset.split_once('-').expect("<run>-<sha>").1;
    assert_eq!(sha, s(&manifest, "source_commit"));
    let commit = arr(&json(LOCK)["sources"])
        .iter()
        .find(|e| s(e, "id") == "gcgapi:commit")
        .map(|e| s(e, "url").rsplit('/').next().unwrap_or("").to_owned())
        .expect("the gcgapi:commit source");
    assert_eq!(commit, "39c85b5032a72b035935b6ca29b326b297908f06");
    assert_ne!(
        sha, commit,
        "the data commit differs from dataset_version's sha"
    );
}

// ---------------------------------------------------------------------------------------------
// W-116: the lexicographic canonical-printing fallback never decides a non-vanilla card.

fn fallback_findings(ps: &[Value], overrides: &Value, db: &CardDb) -> Vec<String> {
    let mut by_number: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for p in ps {
        by_number
            .entry(s(p, "card_number"))
            .or_default()
            .push(s(p, "product_id"));
    }
    let mut preferred = BTreeSet::new();
    for (id, r) in overrides["resolutions"].as_object().expect("resolutions") {
        if r.get("canonical_product_id")
            .and_then(Value::as_str)
            .is_some()
        {
            let numbers: Vec<String> = match r.get("card_numbers").and_then(Value::as_array) {
                Some(ns) if !ns.is_empty() => ns
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                _ => id
                    .split(':')
                    .nth(1)
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
            };
            preferred.extend(numbers);
        }
    }
    let mut findings = Vec::new();
    for (number, products) in by_number {
        if preferred.contains(number) || products.contains(&number) {
            continue;
        }
        let mut sorted = products.clone();
        sorted.sort();
        let def = db.get(number).expect("card");
        if !def.is_vanilla() {
            findings.push(format!(
                "{number}: the lexicographic fallback picks {} for a non-vanilla card",
                sorted[0]
            ));
        }
    }
    findings
}

#[test]
fn w116_the_canonical_fallback_never_decides_a_non_vanilla_card() {
    let ps = printings(&read(CARDS));
    let overrides = json(OVERRIDES);
    assert_eq!(
        fallback_findings(&ps, &overrides, packaged_db()),
        Vec::<String>::new()
    );
    let mut synthetic = ps;
    for pid in ["SY-F01_p10", "SY-F01_p2"] {
        let mut p = synthetic_printing("SY-F01", "UNIT", Some(2), "<Blocker>");
        p["product_id"] = Value::from(pid);
        synthetic.push(p);
    }
    let db = CardDb::from_sources(&ndjson(&synthetic), &read(OVERRIDES)).expect("loads");
    assert_eq!(
        db.get("SY-F01").map(|d| &*d.canonical_product_id),
        Some("SY-F01_p10")
    );
    assert_eq!(
        fallback_findings(&synthetic, &overrides, &db),
        vec![
            "SY-F01: the lexicographic fallback picks SY-F01_p10 for a non-vanilla card".to_owned()
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// W-145: a refreshed snapshot with a new set loads without code changes; the freshness listing.

/// Products released after the snapshot's build date (and on or before `as_of`) whose set has
/// no card number in the data: the listing of `gcg-eval data-status --freshness`.
fn freshness(db: &CardDb, as_of: &str) -> Vec<(String, String)> {
    let manifest = json("src/gcg_sim/data/gcgapi/manifest.json");
    let built = &s(&manifest, "built_at")[..10];
    let sets: BTreeSet<&str> = db.real_cards().map(|d| &*d.set_code).collect();
    let mut out: Vec<(String, String)> = arr(&json("src/gcg_sim/data/gcgapi/products.json"))
        .iter()
        .filter(|p| {
            let date = s(p, "release_date");
            !date.is_empty() && date > built && date <= as_of && !sets.contains(s(p, "set_code"))
        })
        .map(|p| (s(p, "set_code").to_owned(), s(p, "release_date").to_owned()))
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn w145_new_sets_are_data_and_the_freshness_listing_names_gd06() {
    assert_eq!(
        freshness(packaged_db(), "2026-11-01"),
        vec![("GD06".to_owned(), "2026-10-30".to_owned())],
        "GD06 is released and missing; GD07 (2027-01-29) is not yet released"
    );
    let new_cards: Vec<Value> = (1..=10)
        .map(|i| synthetic_printing(&format!("GD06-{i:03}"), "UNIT", Some(2), "-"))
        .collect();
    let db = db_with(&new_cards);
    assert_eq!(db.len(), packaged_db().len() + 10);
    for i in 1..=10 {
        let number = format!("GD06-{i:03}");
        let def = db.get(&number).expect("a new card loads");
        assert_eq!(&*def.set_code, "GD06");
        assert!(def.is_vanilla());
        assert_eq!(resolve_card_number(&number, &db), Some(number.as_str()));
    }
    assert_eq!(freshness(&db, "2026-11-01"), Vec::<(String, String)>::new());
}

// ---------------------------------------------------------------------------------------------
// W-157: the packaged overrides load in full; a broken file is never an empty default.

#[test]
fn w157_packaged_overrides_load_every_card_data_resolution() {
    let overrides = json(OVERRIDES);
    let resolutions = overrides["resolutions"].as_object().expect("resolutions");
    assert_eq!(resolutions.len(), 402);
    let applied: BTreeSet<(&str, &str)> = packaged_db()
        .applied_overrides()
        .iter()
        .map(|o| (&*o.conflict_id, &*o.card_number))
        .collect();
    let mut changing = 0;
    for (id, r) in resolutions {
        if r.get("canonical_product_id").is_none() && r.get("field_overrides").is_none() {
            continue;
        }
        changing += 1;
        let numbers: Vec<&str> = match r.get("card_numbers").and_then(Value::as_array) {
            Some(ns) if !ns.is_empty() => ns.iter().filter_map(Value::as_str).collect(),
            _ => id.split(':').nth(1).into_iter().collect(),
        };
        for n in numbers {
            assert!(
                applied.contains(&(id.as_str(), n)),
                "{id} on {n} is not applied"
            );
        }
    }
    assert!(changing > 50, "{changing}");
    for bad in ["", "{\"resolutions\": "] {
        let e = CardDb::from_sources(&read(CARDS), bad)
            .err()
            .expect("a broken overrides file fails");
        assert!(e.to_string().contains("overrides.json"), "{e}");
    }
}

// ---------------------------------------------------------------------------------------------
// W-164: the packaged data triggers none of the loader's dead paths.

fn dead_path_findings(db: &CardDb, overrides: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    for d in db.defs() {
        if d.card_number.starts_with("TOKEN:") {
            findings.push(format!("synthetic token definition {}", d.card_number));
        }
    }
    let mut seen: BTreeMap<(String, i32, i32, Vec<String>), &str> = BTreeMap::new();
    for d in db
        .defs()
        .iter()
        .filter(|d| d.card_type == CardType::UnitToken)
    {
        let mut traits: Vec<String> = d.traits.iter().map(|t| t.to_string()).collect();
        traits.sort();
        traits.dedup();
        let key = (d.name.to_string(), d.ap, d.hp, traits);
        if let Some(first) = seen.insert(key, &d.card_number) {
            findings.push(format!(
                "token cards {first} and {} share name, stats and traits",
                d.card_number
            ));
        }
    }
    for (id, r) in overrides["resolutions"].as_object().into_iter().flatten() {
        if r.get("card_number").is_some() {
            findings.push(format!("{id}: singular card_number key"));
        }
        if let Some(fo) = r.get("field_overrides").and_then(Value::as_object) {
            let numbers: Vec<&str> = arr(&r["card_numbers"])
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if fo
                .iter()
                .any(|(k, v)| v.is_object() && (numbers.contains(&k.as_str()) || k.contains('-')))
            {
                findings.push(format!("{id}: nested per-number field_overrides"));
            }
        }
    }
    findings
}

#[test]
fn w164_dead_loader_paths_stay_dead_with_negative_controls() {
    assert_eq!(
        dead_path_findings(packaged_db(), &json(OVERRIDES)),
        Vec::<String>::new()
    );
    let fx = json("crates/gcg-data/tests/vectors/synthetic.json");
    let want = [
        ("synthetic-token", "synthetic token definition"),
        ("token-tie", "share name, stats and traits"),
        (
            "nested-field-overrides",
            "nested per-number field_overrides",
        ),
        ("singular-card-number", "singular card_number key"),
    ];
    for (case_id, finding) in want {
        let case = arr(&fx["cases"])
            .iter()
            .find(|c| s(c, "id") == case_id)
            .unwrap_or_else(|| panic!("case {case_id}"));
        let db = CardDb::from_sources(s(case, "cards_ndjson"), s(case, "overrides"))
            .unwrap_or_else(|e| panic!("{case_id}: {e}"));
        let overrides: Value = serde_json::from_str(s(case, "overrides")).expect("overrides");
        let findings = dead_path_findings(&db, &overrides);
        assert!(
            findings.iter().any(|f| f.contains(finding)),
            "{case_id}: {findings:?}"
        );
    }
}
