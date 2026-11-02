//! P6-J data tripwires over the exported effect IR (docs/port/PLAN.md §12.2 P6-J;
//! docs/port/eval/weakness-status/P6-J.md): W-143 (battle-damage redirect sources), W-158
//! (non-literal damage-prevention thresholds, plus the registry-build rejection), W-159
//! (turn-history filters on characteristics that can change), W-161 (ToDeck selectors that can
//! pick a paired Pilot with its Unit) and W-50 (the `always` events derived from the IR).
//!
//! Each scan reads `crates/gcg-effects/ir/registry.json` (Python's registry, exported) as plain
//! JSON: a node is `{"_": Class, field: value}`, a nested node `{"$": NodeId}`, a program's
//! `instrs` plain node ids. Each scan is a pure function of the document, pinned on the packaged
//! pool and given a negative control: a synthetic edit of an in-memory copy (the "temporary
//! data copy") that the scan must report.

use std::collections::BTreeSet;

use gcg_data::CardDb;
use gcg_effects::{IrError, Registry};
use serde_json::{Value, json};

const IR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ir/registry.json");

fn load() -> Value {
    serde_json::from_str(&std::fs::read_to_string(IR).expect("the IR")).expect("JSON")
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn class(n: &Value) -> &str {
    n["_"].as_str().unwrap_or("")
}

fn id_of(v: &Value) -> Option<usize> {
    v.as_object()
        .filter(|m| m.len() == 1)
        .and_then(|m| m.get("$"))
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
}

/// The node `v` names: an arena reference, or an inline node.
fn deref<'a>(ir: &'a Value, v: &'a Value) -> &'a Value {
    id_of(v).map_or(v, |i| &ir["nodes"][i])
}

/// Every node id reachable from the node ids in `roots`, each once.
fn reachable(ir: &Value, roots: &[usize]) -> BTreeSet<usize> {
    fn refs(v: &Value, out: &mut Vec<usize>) {
        if let Some(i) = id_of(v) {
            out.push(i);
            return;
        }
        match v {
            Value::Object(m) => m.values().for_each(|x| refs(x, out)),
            Value::Array(xs) => xs.iter().for_each(|x| refs(x, out)),
            _ => {}
        }
    }
    let nodes = arr(&ir["nodes"]);
    let mut seen = BTreeSet::new();
    let mut todo = roots.to_vec();
    while let Some(i) = todo.pop() {
        if seen.insert(i) {
            refs(&nodes[i], &mut todo);
        }
    }
    seen
}

/// `(card_number, every node of its script)` for each card with a script, in def_id order.
fn card_nodes(ir: &Value) -> Vec<(String, BTreeSet<usize>)> {
    arr(&ir["cards"])
        .iter()
        .filter_map(|c| {
            let root = usize::try_from(c["script"].as_u64()?).ok()?;
            let number = c["card_number"].as_str()?.to_owned();
            Some((number, reachable(ir, &[root])))
        })
        .collect()
}

/// Appends a node to the arena of `ir` and returns `{"$": id}`.
fn push_node(ir: &mut Value, node: Value) -> Value {
    let nodes = ir["nodes"].as_array_mut().expect("nodes");
    nodes.push(node);
    json!({ "$": nodes.len() - 1 })
}

fn card_index(ir: &Value, number: &str) -> usize {
    arr(&ir["cards"])
        .iter()
        .position(|c| c["card_number"] == number)
        .unwrap_or_else(|| panic!("{number} in the IR"))
}

/// The first node of `number`'s script of `class` (with `pred`), by id.
fn node_of(ir: &Value, number: &str, cls: &str, pred: impl Fn(&Value) -> bool) -> usize {
    let root = usize::try_from(
        ir["cards"][card_index(ir, number)]["script"]
            .as_u64()
            .unwrap(),
    )
    .unwrap();
    *reachable(ir, &[root])
        .iter()
        .find(|&&i| class(&ir["nodes"][i]) == cls && pred(&ir["nodes"][i]))
        .unwrap_or_else(|| panic!("{number}: no {cls}"))
}

// ---------------------------------------------------------------------------------------------
// W-143: the cards whose IR grants REDIRECT_BATTLE_DAMAGE

/// Every card whose script holds a `RuleMod` of kind REDIRECT_BATTLE_DAMAGE.
fn redirect_sources(ir: &Value) -> Vec<String> {
    card_nodes(ir)
        .into_iter()
        .filter(|(_, ns)| {
            ns.iter().any(|&i| {
                let n = &ir["nodes"][i];
                class(n) == "RuleMod" && n["kind"] == "redirect_battle_damage"
            })
        })
        .map(|(c, _)| c)
        .collect()
}

/// A new redirect source can close a new chain or cycle (the two-GD04-095 cycle fails a game,
/// `RedirectCycle`), so each one is reviewed before this list changes. Python lists the same
/// two (`"REDIRECT_BATTLE_DAMAGE" in repr(ability)` over `R.cards`); the registry-level twin
/// is `gcg-engine` `core::tests::redirect_sources_are_reviewed`.
#[test]
fn w143_redirect_sources_are_reviewed() {
    let ir = load();
    assert_eq!(
        redirect_sources(&ir),
        ["GD04-087", "GD04-095"],
        "a new battle-damage redirect source: review it for redirect cycles (W-143)"
    );
}

/// Negative control: a synthetic third card that shares GD04-095's script fails the tripwire.
#[test]
fn w143_a_third_redirect_source_fails_the_tripwire() {
    let mut ir = load();
    let script = ir["cards"][card_index(&ir, "GD04-095")]["script"].clone();
    let i = card_index(&ir, "GD01-060");
    ir["cards"][i]["script"] = script;
    let found = redirect_sources(&ir);
    assert_ne!(found, ["GD04-087", "GD04-095"]);
    assert!(found.iter().any(|c| c == "GD01-060"), "{found:?}");
}

// ---------------------------------------------------------------------------------------------
// W-158: damage-prevention thresholds that are not literal ints

/// `(card, kind)` of every card-script `RuleMod` whose `amount` is not a literal int, and the
/// counts of RuleMods, CANT_RECEIVE_DAMAGE RuleMods (each per card script: a node two cards
/// share counts for both, as a walk of each script does) and the cards with one (W-158's scan).
fn non_literal_amounts(ir: &Value) -> (Vec<(String, String)>, [usize; 3]) {
    let mut out = Vec::new();
    let (mut mods, mut prevention, mut cards) = (0, 0, 0);
    for (card, ns) in card_nodes(ir) {
        let mut has_prevention = false;
        for &i in &ns {
            let n = &ir["nodes"][i];
            if class(n) != "RuleMod" {
                continue;
            }
            mods += 1;
            if n["kind"] == "cant_receive_damage" {
                prevention += 1;
                has_prevention = true;
            }
            if !n["amount"].is_i64() {
                out.push((card.clone(), n["kind"].as_str().unwrap_or("").to_owned()));
            }
        }
        cards += usize::from(has_prevention);
    }
    (out, [mods, prevention, cards])
}

/// No packaged RuleMod has a computed amount (W-158 found 125 RuleMods in the card scripts,
/// 25 of them CANT_RECEIVE_DAMAGE on 24 card definitions), so `_prevented`'s non-int fallback
/// (limit 0: "can't receive any damage", `engine/core.py:525`) is dead today.
#[test]
fn w158_no_prevention_threshold_is_computed() {
    let ir = load();
    let (found, counts) = non_literal_amounts(&ir);
    assert_eq!(
        found,
        Vec::<(String, String)>::new(),
        "a RuleMod with a computed amount: review it (W-158)"
    );
    assert_eq!(
        counts,
        [125, 25, 24],
        "RuleMods, CANT_RECEIVE_DAMAGE ones, their cards"
    );
}

/// The IR with GD05-123 Archangel's threshold ("2 or less") replaced by a `Count` node that
/// precedes it in the arena.
fn counted_threshold(ir: &Value) -> Value {
    let mut ir = ir.clone();
    let rule = node_of(&ir, "GD05-123", "RuleMod", |n| {
        n["kind"] == "cant_receive_damage"
    });
    assert_eq!(ir["nodes"][rule]["amount"], 2);
    let count = arr(&ir["nodes"])
        .iter()
        .position(|n| class(n) == "Count")
        .expect("a Count node");
    assert!(count < rule, "references point backwards");
    ir["nodes"][rule]["amount"] = json!({ "$": count });
    ir
}

/// Negative control: a Count-based threshold in a temporary copy fails the tripwire, and the
/// registry build refuses the copy and names the card instead of reading the threshold as
/// unlimited prevention.
#[test]
fn w158_a_counted_threshold_fails_the_tripwire_and_the_build() {
    let ir = counted_threshold(&load());
    let (found, _) = non_literal_amounts(&ir);
    assert_eq!(
        found,
        [("GD05-123".to_owned(), "cant_receive_damage".to_owned())]
    );
    let err = Registry::from_ir(&ir.to_string(), CardDb::embedded())
        .err()
        .expect("the build fails");
    let IrError::Malformed { path, detail } = err else {
        panic!("{err:?}");
    };
    assert!(
        path.starts_with("$.nodes[") && path.ends_with("].amount"),
        "{path}"
    );
    assert!(detail.starts_with("GD05-123: "), "{detail}");
    assert!(detail.contains("W-158"), "{detail}");
}

/// The packaged IR, unchanged, still builds (the check refuses only computed thresholds).
#[test]
fn w158_the_packaged_registry_builds() {
    let ir = std::fs::read_to_string(IR).expect("the IR");
    Registry::from_ir(&ir, CardDb::embedded()).expect("builds");
}

// ---------------------------------------------------------------------------------------------
// W-159: turn-history filters on characteristics that can change after the event

/// Filters that read only a card's identity, which no later change can affect.
const IDENTITY_FILTERS: [&str; 2] = ["IsRef", "NotRef"];

/// The filter classes in `filters`, nested ones included (`AnyOf`, `AllOf`, `Not`,
/// `PairedTo`, ...), that read something other than the card's identity.
fn characteristic_filters(ir: &Value, filters: &Value, out: &mut BTreeSet<String>) {
    for f in arr(filters) {
        let n = deref(ir, f);
        if !IDENTITY_FILTERS.contains(&class(n)) {
            out.insert(class(n).to_owned());
        }
        for key in ["filters", "filter"] {
            match &n[key] {
                Value::Array(_) => characteristic_filters(ir, &n[key], out),
                v if !v.is_null() => characteristic_filters(ir, &json!([v]), out),
                _ => {}
            }
        }
    }
}

/// `(card, history kind, characteristic filter classes)` of every `HappenedThisTurn` whose
/// `filters` or `source_filters` read a characteristic.
fn history_characteristics(ir: &Value) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for (card, ns) in card_nodes(ir) {
        for &i in &ns {
            let n = &ir["nodes"][i];
            if class(n) != "HappenedThisTurn" {
                continue;
            }
            let mut found = BTreeSet::new();
            characteristic_filters(ir, &n["filters"], &mut found);
            characteristic_filters(ir, &n["source_filters"], &mut found);
            if !found.is_empty() {
                let kind = n["kind"].as_str().unwrap_or("").to_owned();
                out.push((card.clone(), kind, found.into_iter().collect()));
            }
        }
    }
    out
}

fn strs(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|&s| s.to_owned()).collect()
}

/// `HappenedThisTurn` matches its filters against the card as it is now, not as it was when the
/// event happened (W-159; Rust keeps this exactly). Each new use is reviewed before this list
/// changes: exactly the five bindings of the entry. GD03-001's identity-only filter is not on
/// it.
#[test]
fn w159_history_filters_on_characteristics_are_reviewed() {
    let got = history_characteristics(&load());
    let want = vec![
        (
            "GD03-069".to_owned(),
            "paired".to_owned(),
            strs(&["PairedTo"]),
        ),
        (
            "GD03-081".to_owned(),
            "deployed".to_owned(),
            strs(&["HasTrait", "IsKind"]),
        ),
        (
            "GD04-069".to_owned(),
            "cost_paid".to_owned(),
            strs(&["HasTrait", "IsKind"]),
        ),
        (
            "GD05-089".to_owned(),
            "command_activated".to_owned(),
            strs(&["HasTrait"]),
        ),
        (
            "GD05-129".to_owned(),
            "destroyed".to_owned(),
            strs(&["HasTrait", "IsKind"]),
        ),
    ];
    assert_eq!(
        got, want,
        "a turn-history condition on a changeable characteristic: review it (W-159)"
    );
}

/// Negative control: a synthetic trait filter on a "discarded" history condition (GD03-001's
/// node in a temporary copy) fails the tripwire.
#[test]
fn w159_a_trait_filter_on_a_discard_history_fails_the_tripwire() {
    let mut ir = load();
    let node = node_of(&ir, "GD03-001", "HappenedThisTurn", |_| true);
    let trait_filter = arr(&ir["nodes"])
        .iter()
        .position(|n| class(n) == "HasTrait")
        .expect("a HasTrait node");
    ir["nodes"][node]["kind"] = json!("discarded");
    ir["nodes"][node]["filters"] = json!([{ "$": trait_filter }]);
    let got = history_characteristics(&ir);
    assert!(
        got.contains(&(
            "GD03-001".to_owned(),
            "discarded".to_owned(),
            strs(&["HasTrait"])
        )),
        "{got:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// W-161: ToDeck steps whose selector can pick a paired Pilot together with its Unit

/// Where the cards of `r` can come from inside program `instrs`: the `loc` of a selector, `self`
/// for `This`/`ThisCard`, `paired` for `PairedPilotOf`, `battle` for `PairedUnitOf` and
/// `BattlingWith`, and `?<class>` for anything the scan cannot follow (an event card, a
/// variable bound by another kind of step or in another program).
fn ref_sources(ir: &Value, instrs: &[usize], r: &Value, out: &mut BTreeSet<String>) {
    let n = deref(ir, r);
    match class(n) {
        "This" | "ThisCard" => {
            out.insert("self".to_owned());
        }
        "All" => {
            out.insert(
                deref(ir, &n["sel"])["loc"]
                    .as_str()
                    .unwrap_or("?")
                    .to_owned(),
            );
        }
        "PairedPilotOf" => {
            out.insert("paired".to_owned());
        }
        "PairedUnitOf" | "BattlingWith" => {
            out.insert("battle".to_owned());
        }
        "Union" => {
            for x in arr(&n["refs"]) {
                ref_sources(ir, instrs, x, out);
            }
        }
        "Var" => {
            let name = &n["name"];
            let binders: Vec<&Value> = instrs
                .iter()
                .map(|&i| &ir["nodes"][i])
                .filter(|s| &s["var"] == name)
                .collect();
            if binders.is_empty() {
                out.insert(format!("?unbound {name}"));
            }
            for b in binders {
                match class(b) {
                    "Choose" => {
                        let loc = deref(ir, &b["sel"])["loc"].as_str().unwrap_or("?");
                        out.insert(loc.to_owned());
                    }
                    "LookTop" => {
                        out.insert("deck".to_owned());
                    }
                    "BindVar" => ref_sources(ir, instrs, &b["ref"], out),
                    other => {
                        out.insert(format!("?{other}"));
                    }
                }
            }
        }
        other => {
            out.insert(format!("?{other}"));
        }
    }
}

/// `(program label, sources)` of every ToDeck whose ref can draw a paired Pilot or that the scan
/// cannot follow: those are the ones whose targets can hold a Unit and its Pilot, which
/// `_h_to_deck` then moves twice (`engine/interp.py:781-810`).
fn todeck_with_pilots(ir: &Value) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for p in arr(&ir["programs"]) {
        let instrs: Vec<usize> = arr(&p["instrs"])
            .iter()
            .filter_map(|i| i.as_u64().and_then(|i| usize::try_from(i).ok()))
            .collect();
        for &i in &instrs {
            let n = &ir["nodes"][i];
            if class(n) != "ToDeck" {
                continue;
            }
            let mut sources = BTreeSet::new();
            ref_sources(ir, &instrs, &n["ref"], &mut sources);
            if sources.iter().any(|s| s == "paired" || s.starts_with('?')) {
                let label = p["label"].as_str().unwrap_or("").to_owned();
                out.push((label, sources.into_iter().collect()));
            }
        }
    }
    out
}

/// No packaged ToDeck can choose a paired Pilot together with its Unit: every one returns
/// cards chosen from the deck, the trash, the hand, a single battle-area Unit or the card
/// itself. So the duplicate ARRANGE options of W-161 (pinned by
/// `crates/gcg-engine/tests/p6j_engine.rs::w161_duplicate_arrange_options_are_pythons` on a
/// test-only script) are unreachable with today's pool; a new such step fails here.
#[test]
fn w161_no_todeck_can_return_a_unit_with_its_pilot() {
    let ir = load();
    assert_eq!(
        todeck_with_pilots(&ir),
        Vec::<(String, Vec<String>)>::new(),
        "a ToDeck that can return a Unit with its Pilot: review it (W-161)"
    );
    let n = arr(&ir["nodes"])
        .iter()
        .filter(|n| class(n) == "ToDeck")
        .count();
    assert_eq!(n, 7, "the ToDeck steps the scan reviewed");
}

/// Negative control: GD05-002's ToDeck in a temporary copy returning its target and the
/// target's Pilot fails the tripwire.
#[test]
fn w161_a_unit_and_its_pilot_fail_the_tripwire() {
    let mut ir = load();
    let node = node_of(&ir, "GD05-002", "ToDeck", |_| true);
    let var = ir["nodes"][node]["ref"].clone();
    let pilot = push_node(&mut ir, json!({ "_": "PairedPilotOf", "ref": var }));
    let union = push_node(&mut ir, json!({ "_": "Union", "refs": [var, pilot] }));
    ir["nodes"][node]["ref"] = union;
    let got = todeck_with_pilots(&ir);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].0.starts_with("GD05-002#"), "{got:?}");
    assert_eq!(got[0].1, strs(&["battle", "paired"]));
}

// ---------------------------------------------------------------------------------------------
// W-50: the always-listened events

/// The events every game must listen for, derived from the IR and the engine: the keyword
/// triggers of `core.keyword_triggers` (<Repair> at TURN_END, <Breach> on DESTROYS_BY_BATTLE,
/// `engine/core.py:441-482`), the trigger of every DelayedTrigger in any card script (armed
/// at run time, so no decklist names it), and the events of every token (created at run time).
fn derived_always(ir: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = ["turn_end", "destroys_by_battle"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for (_, ns) in card_nodes(ir) {
        for &i in &ns {
            let n = &ir["nodes"][i];
            if class(n) == "DelayedTrigger" {
                let t = deref(ir, &n["trigger"]);
                out.insert(t["event"].as_str().expect("an event").to_owned());
            }
        }
    }
    let db = CardDb::embedded();
    for c in arr(&ir["cards"]) {
        let number = c["card_number"].as_str().expect("a number");
        if db.get(number).is_some_and(|d| d.is_token()) {
            out.extend(
                arr(&c["events"])
                    .iter()
                    .filter_map(|e| e.as_str().map(str::to_owned)),
            );
        }
    }
    out
}

/// The engine's listened set is Python's formula on Python's `always_events`
/// (`effects/registry.py:362-378`, hand-maintained `{TURN_END, DESTROYS_BY_BATTLE}` plus
/// delayed triggers and tokens); the IR-derived set must be a subset of it, or a game would
/// skip a trigger. Today the two are equal.
#[test]
fn w50_the_derived_always_set_is_within_pythons() {
    let ir = load();
    let python: BTreeSet<String> = arr(&ir["always_events"])
        .iter()
        .filter_map(|e| e.as_str().map(str::to_owned))
        .collect();
    let derived = derived_always(&ir);
    assert!(
        derived.is_subset(&python),
        "events every game needs that Python's always set lacks: {:?}",
        derived.difference(&python).collect::<Vec<_>>()
    );
    assert_eq!(
        derived, python,
        "the difference is explained in P6-J.md if this changes"
    );
    let reg = Registry::embedded();
    let engine: BTreeSet<String> = gcg_effects::Ev::ALL
        .iter()
        .filter(|&&e| reg.always_events().contains(e))
        .map(|e| e.as_str().to_owned())
        .collect();
    assert_eq!(engine, python, "the registry loads Python's set");
}

/// Negative control: a delayed trigger on a new event in a temporary copy is not covered by
/// Python's set, and the subset check names it.
#[test]
fn w50_a_new_delayed_trigger_event_is_detected() {
    let mut ir = load();
    let node = arr(&ir["nodes"])
        .iter()
        .position(|n| class(n) == "DelayedTrigger")
        .expect("a DelayedTrigger");
    let t = id_of(&ir["nodes"][node]["trigger"]).expect("an arena trigger");
    ir["nodes"][t]["event"] = json!("exiled");
    let python: BTreeSet<String> = arr(&ir["always_events"])
        .iter()
        .filter_map(|e| e.as_str().map(str::to_owned))
        .collect();
    let derived = derived_always(&ir);
    assert!(derived.contains("exiled") && !python.contains("exiled"));
    assert!(!derived.is_subset(&python));
}
