//! P6-K scans of the exported effect IR (docs/dev/PLAN.md §12.2 P6-K;
//! docs/dev/eval/weakness-status/P6-K.md).
//!
//! The scans read `crates/gcg-effects/ir/registry.json` (the Python registry, exported) as
//! plain JSON: nodes are `{"_": Class, field: value}`, a nested node is `{"$": NodeId}`. Each
//! scan is a pure function with a negative control. Items: W-55 (resource-deck moves, token
//! supply, pool-dependent N/A reasons), W-59 (pilot routing table), W-62 ('instead'), W-64
//! (hook purposes), W-65 (vocabulary), W-66 (miscompile families), W-69 (fail fast), W-80
//! (GD02-053 keyword), W-81 (lossy gcg-api fields).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gcg_data::{CardDb, CardType, DefId};
use gcg_effects::{Gate, KeywordProgram, Kw, Loc, Registry, Timing};
use serde_json::Value;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const IR: &str = "crates/gcg-effects/ir/registry.json";

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

/// The arena node a `{"$": id}` reference names.
fn deref<'a>(nodes: &'a [Value], v: &Value) -> Option<&'a Value> {
    let id = v.get("$")?.as_u64()?;
    nodes.get(usize::try_from(id).ok()?)
}

/// Every node reachable from `v`, with its ancestors (outermost first).
fn walk<'a>(nodes: &'a [Value], v: &Value, f: &mut dyn FnMut(&'a Value, &[&'a Value])) {
    fn go<'a>(
        nodes: &'a [Value],
        v: &Value,
        stack: &mut Vec<&'a Value>,
        f: &mut dyn FnMut(&'a Value, &[&'a Value]),
    ) {
        if let Some(n) = deref(nodes, v) {
            f(n, stack);
            stack.push(n);
            if let Some(obj) = n.as_object() {
                for (k, x) in obj {
                    if k != "_" {
                        go(nodes, x, stack, f);
                    }
                }
            }
            stack.pop();
        } else if let Some(items) = v.as_array() {
            for x in items {
                go(nodes, x, stack, f);
            }
        }
    }
    go(nodes, v, &mut Vec::new(), f);
}

fn class(n: &Value) -> &str {
    s(n, "_")
}

struct Ir {
    v: Value,
}

impl Ir {
    fn load() -> Self {
        Self { v: json(IR) }
    }

    fn nodes(&self) -> &[Value] {
        arr(&self.v["nodes"])
    }

    fn cards(&self) -> &[Value] {
        arr(&self.v["cards"])
    }

    fn card(&self, number: &str) -> &Value {
        self.cards()
            .iter()
            .find(|c| s(c, "card_number") == number)
            .unwrap_or_else(|| panic!("{number} in the IR"))
    }

    /// The script's abilities, own first, then the paired Unit's.
    fn abilities(&self, card: &Value) -> Vec<(&Value, bool)> {
        let Some(script) = card["script"].as_u64() else {
            return Vec::new();
        };
        let script = &self.nodes()[usize::try_from(script).expect("node id")];
        let mut out = Vec::new();
        for (key, unit) in [("abilities", false), ("unit_abilities", true)] {
            for a in arr(&script[key]) {
                out.push((deref(self.nodes(), a).expect("an ability node"), unit));
            }
        }
        out
    }

    /// Every node of a card's script, with its ancestors inside the script.
    fn script_nodes(&self, card: &Value) -> Vec<(&Value, Vec<&Value>)> {
        let mut out = Vec::new();
        if let Some(script) = card["script"].as_u64() {
            walk(
                self.nodes(),
                &serde_json::json!({ "$": script }),
                &mut |n, anc| {
                    out.push((n, anc.to_vec()));
                },
            );
        }
        out
    }
}

fn db() -> &'static CardDb {
    CardDb::embedded()
}

fn effect_of(number: &str) -> &'static str {
    &db().get(number).expect("card").effect
}

// ---------------------------------------------------------------------------------------------
// W-55: rules decisions that hold only for the current pool.

/// Steps that move more than one card out of the resource deck at once (rule 4-3-3 is N/A
/// only while there are none): a selection over the resource deck, a repeated or loop-driven
/// `PlaceResource`, or two `PlaceResource` steps inside one `Simultaneous`.
fn resource_deck_moves(nodes: &[Value], roots: &[(String, Value)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (owner, root) in roots {
        walk(nodes, root, &mut |n, anc| {
            if class(n) == "Sel" && s(n, "loc") == Loc::ResourceDeck.as_str() {
                findings.push(format!("{owner}: a selection over the resource deck"));
            }
            if class(n) == "PlaceResource"
                && anc.iter().any(|a| matches!(class(a), "Repeat" | "ForEach"))
            {
                findings.push(format!("{owner}: PlaceResource inside a {}", "loop"));
            }
            if class(n) == "Simultaneous" {
                let places = arr(&n["steps"])
                    .iter()
                    .filter(|x| deref(nodes, x).is_some_and(|y| class(y) == "PlaceResource"))
                    .count();
                if places > 1 {
                    findings.push(format!(
                        "{owner}: {places} simultaneous PlaceResource steps"
                    ));
                }
            }
        });
    }
    findings
}

fn card_roots(ir: &Ir) -> Vec<(String, Value)> {
    ir.cards()
        .iter()
        .filter_map(|c| {
            c["script"].as_u64().map(|id| {
                (
                    s(c, "card_number").to_owned(),
                    serde_json::json!({ "$": id }),
                )
            })
        })
        .collect()
}

/// The locations the DSL can name. A location outside the game (a token supply, rules
/// 5-17-4-1, 5-17-4-3, 5-17-4-4 and 6-1-2-1 are N/A only while there is none) would be a new
/// variant: this exhaustive match stops compiling, and the list below fails, until reviewed.
fn location_review(loc: Loc) -> &'static str {
    match loc {
        Loc::Battle
        | Loc::Paired
        | Loc::Base
        | Loc::Shields
        | Loc::ShieldArea
        | Loc::ResourceArea
        | Loc::Hand
        | Loc::Trash
        | Loc::Deck
        | Loc::DeckTop
        | Loc::ResourceDeck
        | Loc::Removal
        | Loc::FieldUnitsAndBases => "inside the game",
    }
}

#[test]
fn w55_resource_deck_moves_and_token_supply_with_negative_control() {
    let ir = Ir::load();
    assert_eq!(
        resource_deck_moves(ir.nodes(), &card_roots(&ir)),
        Vec::<String>::new()
    );
    let names: Vec<&str> = Loc::ALL.iter().map(|l| l.as_str()).collect();
    assert_eq!(
        names,
        [
            "battle",
            "paired",
            "base",
            "shields",
            "shield_area",
            "resource_area",
            "hand",
            "trash",
            "deck",
            "deck_top",
            "resource_deck",
            "removal",
            "units_and_bases"
        ]
    );
    assert!(
        Loc::ALL
            .iter()
            .all(|l| location_review(*l) == "inside the game")
    );
    for d in db().defs() {
        let text = d.effect.to_lowercase();
        assert!(
            !text.contains("resource deck") && !text.contains("outside the game"),
            "{}: text names the resource deck or the outside of the game",
            d.card_number
        );
    }
    let synthetic: Vec<Value> = serde_json::from_str(
        r#"[{"_": "PlaceResource", "player": "you", "rested": false},
            {"_": "Repeat", "times": 2, "steps": [{"$": 0}]},
            {"_": "Sel", "side": "friendly", "loc": "resource_deck", "filters": [], "top_n": 0},
            {"_": "Choose", "var": "t1", "sel": {"$": 2}, "count": 2}]"#,
    )
    .expect("synthetic nodes");
    let roots = vec![
        ("SY-R01".to_owned(), serde_json::json!({"$": 1})),
        ("SY-R02".to_owned(), serde_json::json!({"$": 3})),
    ];
    assert_eq!(
        resource_deck_moves(&synthetic, &roots),
        vec![
            "SY-R01: PlaceResource inside a loop".to_owned(),
            "SY-R02: a selection over the resource deck".to_owned()
        ],
        "negative control: 'place 2 Resources' and a resource-deck choice trip the wire"
    );
}

/// The N/A reasons in `rules_na.json` whose truth depends on the card pool, each with the
/// tripwire that checks its precondition.
const POOL_DEPENDENT_NA: [(&str, &str); 5] = [
    (
        "4-3-3",
        "w55_resource_deck_moves_and_token_supply_with_negative_control (resource-deck moves)",
    ),
    (
        "5-17-4-1",
        "w55_resource_deck_moves_and_token_supply_with_negative_control (no token supply location)",
    ),
    (
        "5-17-4-3",
        "w55_resource_deck_moves_and_token_supply_with_negative_control (no token supply location)",
    ),
    (
        "5-17-4-4",
        "w55_resource_deck_moves_and_token_supply_with_negative_control (no token supply location)",
    ),
    (
        "6-1-2-1",
        "w55_resource_deck_moves_and_token_supply_with_negative_control (no token supply location)",
    ),
];

#[test]
fn w55_every_pool_dependent_na_reason_has_a_tripwire() {
    let na = json("src/gcg_sim/data/rules/rules_na.json");
    let rules = na["rules"].as_object().expect("rules");
    let pool_words = [
        "card pool",
        "tokens from card data",
        "token instances",
        "tokens are created",
    ];
    let dependent: BTreeSet<&str> = rules
        .iter()
        .filter(|(_, v)| {
            let r = s(v, "reason").to_lowercase();
            pool_words.iter().any(|w| r.contains(w))
        })
        .map(|(k, _)| k.as_str())
        .collect();
    let listed: BTreeSet<&str> = POOL_DEPENDENT_NA.iter().map(|(r, _)| *r).collect();
    assert_eq!(dependent, listed);
    for (rule, _) in POOL_DEPENDENT_NA {
        assert_eq!(s(&rules[rule], "status"), "na", "{rule}");
    }
}

// ---------------------------------------------------------------------------------------------
// W-59: the pilot-routing audit table regenerates from the IR.

fn summary(nodes: &[Value], ab: &Value) -> String {
    let cls = class(ab);
    let mut parts: Vec<String> = Vec::new();
    match cls {
        "Triggered" => {
            parts.push(s(deref(nodes, &ab["trigger"]).expect("trigger"), "event").to_owned())
        }
        "Activated" | "Command" => parts.push(s(ab, "timing").to_owned()),
        "Keyword" => {
            let amount = ab["amount"].as_i64().unwrap_or(0);
            let kw = s(ab, "keyword");
            parts.push(if amount == 0 {
                kw.to_owned()
            } else {
                format!("{kw} {amount}")
            });
        }
        "NameAlias" => parts.extend(
            arr(&ab["names"])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        ),
        "Replacement" => parts.push(s(ab, "event").to_owned()),
        _ => {}
    }
    if let Some(w) = ab.get("where").and_then(Value::as_str)
        && w != "field"
    {
        parts.push(format!("where={w}"));
    }
    if let Some(g) = ab.get("gate").and_then(Value::as_str)
        && g != "none"
    {
        parts.push(format!("gate={g}"));
    }
    if parts.is_empty() {
        cls.to_owned()
    } else {
        format!("{cls}({})", parts.join(", "))
    }
}

fn pilot_rows(ir: &Ir) -> Vec<String> {
    let mut rows = vec![
        "| card | name | kept by the Pilot card | gained by the paired Unit |".to_owned(),
        "| --- | --- | --- | --- |".to_owned(),
    ];
    let mut n = 0;
    for card in ir.cards() {
        let def = db().by_id(DefId(
            u16::try_from(card["def_id"].as_u64().expect("def_id")).expect("u16"),
        ));
        if def.card_type != CardType::Pilot {
            continue;
        }
        let abilities = ir.abilities(card);
        let join = |unit: bool| {
            let v: Vec<String> = abilities
                .iter()
                .filter(|(_, u)| *u == unit)
                .map(|(a, _)| summary(ir.nodes(), a))
                .collect();
            if v.is_empty() {
                "—".to_owned()
            } else {
                v.join("; ")
            }
        };
        rows.push(format!(
            "| {} | {} | {} | {} |",
            def.card_number,
            def.name,
            join(false),
            join(true)
        ));
        n += 1;
    }
    rows.push(String::new());
    rows.push(format!("Pilot cards: {n}."));
    rows
}

#[test]
fn w59_pilot_routing_table_regenerates_from_the_ir() {
    let ir = Ir::load();
    let file = read("docs/dev/audit/pilot-routing.md");
    let table: Vec<&str> = file
        .lines()
        .skip_while(|l| !l.starts_with("| card |"))
        .collect();
    let rows = pilot_rows(&ir);
    assert_eq!(table, rows.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(rows.len() > 100, "{} rows", rows.len());
    let routed_to_unit = rows.iter().filter(|r| !r.ends_with("| — |")).count();
    assert!(
        routed_to_unit > 100,
        "most Pilot text is gained by the Unit"
    );
    for r in &rows[2..rows.len() - 2] {
        let own = r.split(" | ").nth(2).expect("own column");
        for part in own.split("; ") {
            let kept = part == "—"
                || part.starts_with("Burst")
                || part.starts_with("NameAlias")
                || part.starts_with("PlayModifier")
                || part.contains("where=hand")
                || part.contains("where=trash");
            assert!(
                kept,
                "{r}: a Pilot card keeps only Burst, alias, play-modifier and hand/trash text"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// W-62: 'instead' wording.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Instead {
    /// A conditional override of the previous step's parameters (an `If` branch or a
    /// conditional value), never a rule 10-1-9 replacement effect.
    Override,
    /// A rule 10-1-9 substitution: a substitution rule modification (rest substitute,
    /// battle-damage redirect) or a destination that replaces the default one.
    Substitution,
}

const INSTEAD_CARDS: [(&str, Instead); 22] = [
    ("GD01-122", Instead::Override),
    ("GD03-079", Instead::Substitution),
    ("GD03-104", Instead::Override),
    ("GD03-109", Instead::Override),
    ("GD03-114", Instead::Override),
    ("GD03-115", Instead::Override),
    ("GD04-039", Instead::Override),
    ("GD04-087", Instead::Substitution),
    ("GD04-095", Instead::Substitution),
    ("GD04-106", Instead::Override),
    ("GD04-108", Instead::Override),
    ("GD05-089", Instead::Substitution),
    ("GD05-124", Instead::Substitution),
    ("GD06-018", Instead::Substitution),
    ("GD06-048", Instead::Override),
    ("GD06-050", Instead::Override),
    ("GD06-121", Instead::Override),
    ("ST02-016", Instead::Override),
    ("ST07-009", Instead::Override),
    ("ST08-013", Instead::Override),
    ("ST12-012", Instead::Substitution),
    ("ST14-014", Instead::Override),
];

const SUBSTITUTION_RULES: [&str; 2] = ["rest_substitute", "redirect_battle_damage"];

fn instead_signature(ir: &Ir, number: &str) -> (bool, bool, bool) {
    let mut sub_rule = false;
    let mut replacement = false;
    let mut conditional = false;
    let card = ir.card(number);
    for (n, _) in ir.script_nodes(card) {
        match class(n) {
            "RuleMod" if SUBSTITUTION_RULES.contains(&s(n, "kind")) => sub_rule = true,
            "Replacement" => replacement = true,
            "If" | "MinOf" => conditional = true,
            "CustomFilter" if s(n, "name") == "wp_gd03_b_cond_holds" => conditional = true,
            _ => {}
        }
    }
    (sub_rule, replacement, conditional)
}

#[test]
fn w62_every_instead_text_maps_to_exactly_one_kind() {
    let ir = Ir::load();
    let in_pool: BTreeSet<&str> = db()
        .real_cards()
        .filter(|d| d.effect.contains("instead"))
        .map(|d| &*d.card_number)
        .collect();
    let table: BTreeSet<&str> = INSTEAD_CARDS.iter().map(|(n, _)| *n).collect();
    assert_eq!(in_pool, table, "a new 'instead' card needs a reviewed kind");
    let subs = INSTEAD_CARDS
        .iter()
        .filter(|(_, k)| *k == Instead::Substitution)
        .count();
    assert_eq!((INSTEAD_CARDS.len() - subs, subs), (15, 7));
    for (number, kind) in INSTEAD_CARDS {
        let (sub_rule, replacement, conditional) = instead_signature(&ir, number);
        match kind {
            Instead::Override => {
                assert!(
                    !sub_rule && !replacement,
                    "{number}: an override uses no substitution node"
                );
                assert!(
                    conditional,
                    "{number}: an override is a conditional parameter"
                );
            }
            Instead::Substitution => {
                let destination = matches!(number, "GD05-089" | "ST12-012");
                assert!(
                    sub_rule || destination,
                    "{number}: a substitution is a substitution rule or a replaced destination"
                );
                assert!(
                    !replacement,
                    "{number}: the pool uses no Replacement ability"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// W-64: custom hooks and their purposes.

/// Every custom hook, its purpose family and what distinguishes it inside its family. Members
/// of one family with the same distinction would be duplicates: the test rejects them unless
/// they are declared aliases (identical bodies kept apart only for `repr`, which the engine
/// dispatches to one body: `crates/gcg-engine/src/interp/hooks/mod.rs`, W-64).
const HOOK_PURPOSES: [(&str, &str, &str); 36] = [
    (
        "return_looked_bottom",
        "return looked cards",
        "random order to the deck bottom",
    ),
    ("deploy_ex_base", "deploy an EX Base", "rule 5-17-3-1"),
    ("support_used", "emit the <Support> event", "GD01-046"),
    ("wp_eb01_a_look_own_top", "look at own top card", "alias"),
    ("wp_eb01_b_look_top_own", "look at own top card", "alias"),
    (
        "wp_gd02_b_pay_deploy_cost",
        "pay a card's cost to deploy it",
        "its cost as modified where it is (ambiguous:pay-its-cost-to-deploy)",
    ),
    (
        "wp_gd03_a_pay_cost_of",
        "pay a card's cost to deploy it",
        "GD03-051: the chosen trash card's cost",
    ),
    (
        "wp_gd03_a_ap_minus_count",
        "AP down by a count fixed at resolution",
        "count of a selection",
    ),
    (
        "wp_gd04_a_ap_down_by_named_trash_units",
        "AP down by a count fixed at resolution",
        "named Units in the trash",
    ),
    ("wp_gd04_a_copy_keywords", "copy keywords", "GD04-067"),
    ("wp_gd05_a_use_once", "shared Once per Turn", "mark used"),
    ("wp_gd05_a_once_free", "shared Once per Turn", "test unused"),
    (
        "wp_gd05_b_destroy_first_shield_area_cards",
        "destroy shield-area cards",
        "GD05",
    ),
    (
        "wp_gd05_b_resolving_command_to_trash",
        "resolving Command to the trash",
        "GD05",
    ),
    ("attack_target_damaged", "attack target state", "damaged"),
    ("is_attack_target", "attack target state", "is the target"),
    (
        "wp_gd01_b_destroyed_with_damage",
        "how a card was destroyed",
        "by battle or effect damage",
    ),
    (
        "wp_gd05_a_destroyed_by_effect",
        "how a card was destroyed",
        "by a destroy effect (by: you|any)",
    ),
    (
        "wp_st12_14_destroyed_by_effect_damage",
        "how a card was destroyed",
        "by this Unit's effect damage",
    ),
    (
        "wp_gd04_b_played_with_ex",
        "played with an EX Resource",
        "the most recent play from the hand",
    ),
    (
        "wp_gd05_b_played_with_ex",
        "played with an EX Resource",
        "the most recent play this turn",
    ),
    (
        "wp_gd03_a_has_destroyed_effect",
        "card has an ability",
        "【Destroyed】",
    ),
    (
        "wp_gd04_a_has_printed_keyword",
        "card has an ability",
        "a printed keyword",
    ),
    ("wp_gd03_a_lowest_hp", "extreme stat", "lowest HP Unit"),
    (
        "wp_gd05_a_lowest_enemy_unit_level",
        "extreme enemy Unit level",
        "lowest, any (0 if none)",
    ),
    (
        "wp_st05_08_highest_enemy_unit_lv",
        "extreme enemy Unit level",
        "highest, any (0 if none)",
    ),
    (
        "wp_st12_14_lowest_rested_enemy_lv",
        "extreme enemy Unit level",
        "lowest, rested (-1 if none)",
    ),
    ("wp_gd03_b_cond_holds", "condition as a filter", "GD03"),
    (
        "wp_gd03_b_battling_enemy_unit",
        "battling enemy Unit",
        "GD03",
    ),
    ("wp_gd04_a_pilot_capable", "pilot-capable card", "GD04"),
    (
        "wp_gd04_a_has_partner_in_trash",
        "partner in the trash",
        "GD04",
    ),
    ("wp_gd05_b_can_pair", "can pair", "GD05"),
    ("wp_gd03_a_ap_per", "count per AP", "GD03-033"),
    (
        "wp_gd03_b_unique_names",
        "distinct card names",
        "GD03-089:Q240",
    ),
    (
        "wp_gd06_a_name_unlike",
        "a name unlike the chosen cards'",
        "GD06-039",
    ),
    (
        "wp_gd06_b_link_names",
        "a link condition naming a Pilot",
        "GD06-086",
    ),
];

/// A hook name without its binding-module prefix, as a set of words (`lv` read as `level`).
fn purpose_words(name: &str) -> BTreeSet<String> {
    let mut words: Vec<&str> = name.split('_').collect();
    if words.first() == Some(&"wp") && words.len() > 3 {
        let skip = if words.get(2).is_some_and(|w| w.len() <= 2) {
            3
        } else {
            2
        };
        words.drain(..skip);
    }
    words
        .into_iter()
        .map(|w| {
            if w == "lv" {
                "level".to_owned()
            } else {
                w.to_owned()
            }
        })
        .collect()
}

#[test]
fn w64_no_two_hooks_share_a_purpose() {
    let ir = Ir::load();
    let mut ir_hooks: Vec<&str> = Vec::new();
    for kind in ["step", "cond", "filter", "value"] {
        ir_hooks.extend(arr(&ir.v["hooks"][kind]).iter().filter_map(Value::as_str));
    }
    let table: Vec<&str> = HOOK_PURPOSES.iter().map(|(n, _, _)| *n).collect();
    let (a, b): (BTreeSet<&str>, BTreeSet<&str>) = (
        ir_hooks.iter().copied().collect(),
        table.iter().copied().collect(),
    );
    assert_eq!(a, b, "every hook is classified once");
    assert_eq!(table.len(), b.len());
    let mut by_family: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for (name, family, distinction) in HOOK_PURPOSES {
        by_family
            .entry((family, distinction))
            .or_default()
            .push(name);
    }
    for ((family, distinction), names) in &by_family {
        if names.len() > 1 {
            assert_eq!(*distinction, "alias", "{family}: {names:?} share a purpose");
            assert_eq!(names, &["wp_eb01_a_look_own_top", "wp_eb01_b_look_top_own"]);
        }
    }
    let mut by_words: BTreeMap<BTreeSet<String>, Vec<&str>> = BTreeMap::new();
    for (name, _, _) in HOOK_PURPOSES {
        by_words.entry(purpose_words(name)).or_default().push(name);
    }
    let collisions: Vec<Vec<&str>> = by_words.into_values().filter(|v| v.len() > 1).collect();
    assert_eq!(
        collisions,
        vec![
            vec!["wp_gd04_b_played_with_ex", "wp_gd05_b_played_with_ex"],
            vec!["wp_eb01_a_look_own_top", "wp_eb01_b_look_top_own"],
        ],
        "every name-normalised collision is a declared alias or a family with a distinction"
    );
}

// ---------------------------------------------------------------------------------------------
// W-65: one keyword and timing vocabulary.

/// The marker text of each timing marker gcg-api's vocabulary lists, derived from the DSL
/// enums with exhaustive matches: a new `Timing` or `Gate` variant stops this compiling.
fn timing_markers() -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = ["Activate", "Burst", "Deploy", "Attack", "Destroyed"]
        .into_iter()
        .collect();
    out.extend(["When Paired", "When Linked", "Once per Turn"]);
    for t in Timing::ALL {
        match t {
            Timing::Main => out.insert("Main"),
            Timing::Action => out.insert("Action"),
            Timing::MainOrAction => out.insert("Main") | out.insert("Action"),
        };
    }
    for g in Gate::ALL {
        match g {
            Gate::None => false,
            Gate::Paired => out.insert("During Pair"),
            Gate::Linked => out.insert("During Link"),
        };
    }
    out
}

fn keyword_program(kp: KeywordProgram) -> Kw {
    match kp {
        KeywordProgram::Repair => Kw::Repair,
        KeywordProgram::Breach => Kw::Breach,
        KeywordProgram::Support => Kw::Support,
    }
}

#[test]
fn w65_python_vocabularies_derive_from_the_rust_enums() {
    let v = json("oracle/tests/vectors/p6k_vocabulary.json");
    let kw: Vec<&str> = Kw::ALL.iter().map(|k| k.as_str()).collect();
    for (name, list) in v["keywords"].as_object().expect("keywords") {
        let list: Vec<&str> = arr(list).iter().filter_map(Value::as_str).collect();
        if name == "dsl.STACKING_KEYWORDS" {
            let mut stacking: Vec<&str> = [
                KeywordProgram::Repair,
                KeywordProgram::Breach,
                KeywordProgram::Support,
            ]
            .into_iter()
            .map(|k| keyword_program(k).as_str())
            .collect();
            stacking.sort();
            assert_eq!(list, stacking, "{name}");
        } else {
            assert_eq!(list, kw, "{name}");
        }
    }
    let timings: Vec<&str> = Timing::ALL.iter().map(|t| t.as_str()).collect();
    let python: Vec<&str> = arr(&v["timings"]["dsl.Timing"])
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(python, timings);
    let markers: BTreeSet<&str> = arr(&v["timings"]["sources.conflicts.TIMING_VOCABULARY"])
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(markers, timing_markers());
    let ir = Ir::load();
    let programs: Vec<&str> = ir.v["keyword_programs"]
        .as_object()
        .expect("keyword_programs")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(programs, ["repair", "breach", "support"]);
}

// ---------------------------------------------------------------------------------------------
// W-66: miscompile families over the compiled (non-binding) cards.

/// A scan hit that was reviewed: the card, the family and the tests that pin its behaviour
/// (a tagged Rust testkit test in `crates/gcg-engine/tests/p6k_behaviour.rs` and a Python test
/// in `oracle/tests/test_p6k_miscompile.py`).
const REVIEWED_HITS: [(&str, &str, &str); 5] = [
    (
        "GD01-103",
        "two-target step without an all-targets guard",
        "w66_two_target_commands_need_both_targets",
    ),
    (
        "GD02-055",
        "two-target step without an all-targets guard",
        "w66_gd02_055_deploy_needs_both_targets",
    ),
    (
        "GD03-116",
        "two-target step without an all-targets guard",
        "w66_two_target_commands_need_both_targets",
    ),
    (
        "GD04-114",
        "two-target step without an all-targets guard",
        "w66_two_target_commands_need_both_targets",
    ),
    (
        "GD05-117",
        "two-target step without an all-targets guard",
        "w66_two_target_commands_need_both_targets",
    ),
];

fn var_name(nodes: &[Value], v: &Value) -> Option<String> {
    deref(nodes, v)
        .filter(|n| class(n) == "Var")
        .map(|n| s(n, "name").to_owned())
}

/// The families of the binding notes cited by W-66 (`wp_gd04_a.py:229`, `:489`, `:508`,
/// `:553`, `wp_gd05_b.py:334-336`, `:395`, `:450-452`, `wp_eb01_b.py:169`, `:232`, `:290`,
/// `:316`, `wp_gd04_b.py:210`).
fn miscompile_scan(ir: &Ir, effect: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
    let nodes = ir.nodes();
    let mut hits: Vec<(String, String)> = Vec::new();
    for card in ir.cards() {
        if s(card, "source") != "compiled" {
            continue;
        }
        let number = s(card, "card_number").to_owned();
        let mut hit = |family: &str| {
            let h = (number.clone(), family.to_owned());
            if !hits.contains(&h) {
                hits.push(h);
            }
        };
        for (ability, _) in ir.abilities(card) {
            let mut items: Vec<(&Value, Vec<&Value>)> = Vec::new();
            let mut collect = |n: &Value| {
                walk(nodes, n, &mut |x, anc| items.push((x, anc.to_vec())));
            };
            for (k, x) in ability.as_object().expect("ability") {
                if k != "_" {
                    collect(x);
                }
            }
            let mut chosen: Vec<String> = Vec::new();
            let mut read: BTreeSet<String> = BTreeSet::new();
            for (n, anc) in &items {
                match class(n) {
                    "Choose" => {
                        chosen.push(s(n, "var").to_owned());
                        read.extend(
                            arr(&n["distinct_from"])
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                        let count = n["count"].as_i64().unwrap_or(0);
                        if n["optional"] == true && n["min_count"].is_null() && count > 1 {
                            hit("optional choice without a minimum");
                        }
                    }
                    "Var" => {
                        read.insert(s(n, "name").to_owned());
                    }
                    "DelayedTrigger" => {
                        read.extend(
                            arr(&n["bind_vars"])
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                    }
                    "PairedPilotOf"
                        if class(ability) == "Triggered"
                            && deref(nodes, &ability["trigger"])
                                .is_some_and(|t| s(t, "event") == "destroyed")
                            && deref(nodes, &n["ref"])
                                .is_some_and(|r| matches!(class(r), "This" | "ThisCard")) =>
                    {
                        hit("Pilot read through the destroyed Unit");
                    }
                    "ChangeAttackTarget" if !anc.iter().any(|a| class(a) == "If") => {
                        hit("attack target changed without a condition");
                    }
                    "EventCard" | "EventAmount" | "EventFrom" | "EventFlag"
                        if matches!(
                            class(ability),
                            "Command" | "Activated" | "Burst" | "Constant"
                        ) =>
                    {
                        hit("event payload read outside a triggered frame");
                    }
                    "Union" => {
                        let refs = arr(&n["refs"]);
                        let any_chosen = refs.iter().any(|r| var_name(nodes, r).is_some());
                        if refs.len() >= 2 && any_chosen && !anc.iter().any(|a| class(a) == "If") {
                            hit("two-target step without an all-targets guard");
                        }
                    }
                    _ => {}
                }
            }
            for v in chosen {
                if !read.contains(&v) {
                    hit("chosen target never used");
                }
            }
        }
        let text = effect(&number);
        let modes = text.matches('■').count();
        if modes >= 2 {
            let mut options = 0;
            for (n, _) in ir.script_nodes(card) {
                if class(n) == "ChooseMode" {
                    options += arr(&n["options"]).len();
                }
            }
            if options < modes {
                hit("not every ■ mode compiled");
            }
        }
        if text.contains("each enemy player")
            && ir
                .script_nodes(card)
                .iter()
                .any(|(n, _)| class(n) == "Sel" && s(n, "side") == "any")
        {
            hit("'each enemy player' compiled as either side");
        }
    }
    hits
}

#[test]
fn w66_miscompile_scan_lists_zero_unreviewed_hits() {
    let ir = Ir::load();
    let hits = miscompile_scan(&ir, &|n| effect_of(n).to_owned());
    let reviewed: Vec<(String, String)> = REVIEWED_HITS
        .iter()
        .map(|(n, f, _)| ((*n).to_owned(), (*f).to_owned()))
        .collect();
    assert_eq!(
        hits, reviewed,
        "an unreviewed hit needs a behaviour test in both implementations"
    );
    let compiled = ir
        .cards()
        .iter()
        .filter(|c| s(c, "source") == "compiled")
        .count();
    assert_eq!(compiled, 607, "the scan covers every compiled card");
    for (number, _, test) in REVIEWED_HITS {
        let engine_tests = read("crates/gcg-engine/tests/p6k_behaviour.rs");
        assert!(
            engine_tests.contains(&format!("fn {test}(")),
            "{number}: Rust test {test}"
        );
        let python = read("oracle/tests/test_p6k_miscompile.py");
        assert!(python.contains(number), "{number}: Python test");
    }
}

#[test]
fn w66_scan_finds_the_binding_fixed_miscompiles_in_their_compiled_form() {
    // Negative control: the shapes the binding notes describe, as compiled IR.
    let synthetic: Value = serde_json::from_str(
        r#"{"nodes": [
            {"_": "Sel", "side": "enemy", "loc": "battle", "filters": [], "top_n": 0},
            {"_": "Choose", "var": "t1", "sel": {"$": 0}, "count": 1, "min_count": null, "optional": false, "distinct_from": []},
            {"_": "Choose", "var": "t2", "sel": {"$": 0}, "count": 1, "min_count": null, "optional": false, "distinct_from": []},
            {"_": "Var", "name": "t1"},
            {"_": "Var", "name": "t2"},
            {"_": "Union", "refs": [{"$": 3}, {"$": 4}]},
            {"_": "Rest", "ref": {"$": 5}},
            {"_": "Command", "timing": "main", "steps": [{"$": 1}, {"$": 2}, {"$": 6}]},
            {"_": "Choose", "var": "t3", "sel": {"$": 0}, "count": 7, "min_count": null, "optional": true, "distinct_from": []},
            {"_": "EventCard", "key": "subject"},
            {"_": "ChangeAttackTarget", "target": {"$": 9}},
            {"_": "Command", "timing": "action", "steps": [{"$": 8}, {"$": 10}]},
            {"_": "CardScript", "card_number": "SY-M01", "abilities": [{"$": 7}, {"$": 11}], "unit_abilities": [], "source": "compiled", "notes": ""}
        ],
        "cards": [{"card_number": "SY-M01", "source": "compiled", "script": 12, "def_id": 0}]}"#,
    )
    .expect("synthetic IR");
    let hits = miscompile_scan(&Ir { v: synthetic }, &|_| "■ a\n■ b".to_owned());
    let families: Vec<&str> = hits.iter().map(|(_, f)| f.as_str()).collect();
    assert_eq!(
        families,
        [
            "two-target step without an all-targets guard",
            "optional choice without a minimum",
            "attack target changed without a condition",
            "event payload read outside a triggered frame",
            "chosen target never used",
            "not every ■ mode compiled",
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// W-69: a broken script fails the registry build and names the card.

#[test]
fn w69_a_broken_binding_fails_the_build_naming_the_card() {
    let text = read(IR);
    let mut ir: Value = serde_json::from_str(&text).expect("IR");
    let nodes = ir["nodes"].as_array().expect("nodes");
    let card = ir["cards"]
        .as_array()
        .expect("cards")
        .iter()
        .find(|c| s(c, "card_number") == "GD04-015")
        .expect("GD04-015");
    let script = usize::try_from(card["script"].as_u64().expect("script")).expect("id");
    let first = deref(nodes, &nodes[script]["abilities"][0])
        .expect("ability")
        .clone();
    let id = nodes.iter().position(|n| *n == first).expect("ability id");
    ir["nodes"][id]["_"] = Value::from("NoSuchAbility");
    let err = Registry::from_ir(&ir.to_string(), db())
        .err()
        .expect("a broken script does not load");
    let message = err.to_string();
    assert!(message.contains("NoSuchAbility"), "{message}");
    assert!(
        message.contains("GD04-015"),
        "the error names the card: {message}"
    );
    let registry: Arc<Registry> = Registry::embedded();
    let counts = &json(IR)["counts"];
    assert_eq!(
        counts["binding"].as_u64().unwrap_or(0)
            + counts["compiled"].as_u64().unwrap_or(0)
            + counts["vanilla"].as_u64().unwrap_or(0),
        u64::try_from(db().len()).expect("len"),
        "the packaged registry builds with no unimplemented card"
    );
    for d in db().defs() {
        let entry = registry.card(d.def_id);
        assert!(
            entry.script.is_some() || d.is_vanilla() || d.card_type.is_token(),
            "{}: a non-vanilla card without a script",
            d.card_number
        );
    }
}

// ---------------------------------------------------------------------------------------------
// W-80: GD02-053 has <Suppression>.

#[test]
fn w80_gd02_053_has_suppression_in_rust() {
    let registry = Registry::embedded();
    let def = db().get("GD02-053").expect("GD02-053");
    let printed = &registry.card(def.def_id).printed_keywords;
    assert_eq!(printed.get(Kw::Suppression), Some(0), "{printed:?}");
    let ir = Ir::load();
    assert_eq!(
        ir.card("GD02-053")["printed_keywords"],
        serde_json::json!([["Suppression", 0]])
    );
}

// ---------------------------------------------------------------------------------------------
// W-81: the lossy gcg-api fields shape nothing.

#[test]
fn w81_blanking_keyword_effects_timing_markers_and_link_refs_changes_nothing() {
    let cards = read("src/gcg_sim/data/gcgapi/cards.ndjson");
    let mut blanked = String::new();
    let mut n = 0;
    for line in cards.lines().filter(|l| !l.trim().is_empty()) {
        let mut p: Value = serde_json::from_str(line).expect("printing");
        for key in ["keyword_effects", "timing_markers", "link_refs"] {
            if p.get(key).is_some() {
                p[key] = Value::Array(Vec::new());
                n += 1;
            }
        }
        blanked.push_str(&p.to_string());
        blanked.push('\n');
    }
    assert!(n > 6000, "{n} fields blanked");
    let copy = CardDb::from_sources(&blanked, &read("src/gcg_sim/data/overrides.json"))
        .expect("the blanked copy loads");
    assert_eq!(copy.fingerprints(), db().fingerprints());
    let copy: &'static CardDb = Box::leak(Box::new(copy));
    let rebuilt = Registry::from_ir(&read(IR), copy).expect("the IR loads over the blanked copy");
    let packaged = Registry::embedded();
    for d in db().defs() {
        assert_eq!(
            rebuilt.golden_json(d.def_id),
            packaged.golden_json(d.def_id),
            "{}",
            d.card_number
        );
        if let Some(script) = packaged.card(d.def_id).script {
            assert_eq!(
                rebuilt.node_content_id(script).as_str(),
                packaged.node_content_id(script).as_str()
            );
        }
    }
}
