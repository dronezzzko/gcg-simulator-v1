//! T-130 (W-129, PLAN §6.4): the preset tables of the Rust `docs/AI.md` equal the constants of
//! `crates/gcg-search/src/presets.rs` and of the reference presets they start from, so the
//! documented budgets, settings and mechanisms cannot drift from the code.
//!
//! Each table is found by its header row; a renamed header fails with the header it looked for.

use std::path::PathBuf;

use gcg_ai_reference::{ReferenceConfig, reference_preset};
use gcg_search::{Preset, StrengthenedConfig, preset_config, weights_id};

const PRESETS_HEADER: &str = "| Preset | Use | Starts from (mapping) | Budget scale | \
    `iterations_per_option` | `min_iterations` | `max_iterations` |";
const SETTINGS_HEADER: &str =
    "| Setting | `standard` (= reference `strong`) | `fast` (= reference `standard`) |";
const MECHANISMS_HEADER: &str = "| Mechanism | Item | What it does |";
const REFERENCE_HEADER: &str = "| Reference preset | Agent name | `iterations_per_option` | \
    `min_iterations` | `max_iterations` | `exploration` | `rollout_temperature` | \
    `first_player_games` | `mulligan_samples` | Shipped preset that starts from it |";

fn ai_md() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/AI.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The cells of every body row of the table whose header line is `header`.
fn table(doc: &str, header: &str) -> Vec<Vec<String>> {
    let mut lines = doc.lines().skip_while(|l| *l != header);
    assert!(
        lines.next().is_some(),
        "docs/AI.md has no table with the header {header}"
    );
    let separator = lines.next().unwrap_or_default();
    assert!(separator.starts_with("| ---"), "{header}: no separator row");
    lines
        .take_while(|l| l.starts_with('|'))
        .map(|l| {
            l.trim_matches('|')
                .split(" | ")
                .map(|c| c.trim().to_owned())
                .collect()
        })
        .collect()
}

fn unquote(cell: &str) -> &str {
    cell.trim_matches('`')
}

fn reference(name: &str) -> &'static ReferenceConfig {
    reference_preset(name).unwrap_or_else(|| panic!("no reference preset {name}"))
}

/// The mapped reference preset's name: `reference strong` -> `strong`.
fn mapped(p: Preset) -> &'static str {
    p.mapping()
        .strip_prefix("reference ")
        .expect("a mapping names a reference preset")
}

/// `field` of `cfg` as the documentation writes it.
fn setting(cfg: &StrengthenedConfig, field: &str) -> String {
    match field {
        "exploration" => format!("{:?}", cfg.exploration),
        "rollout_temperature" => format!("{:?}", cfg.rollout_temperature),
        "first_player_games" => cfg.first_player_games.to_string(),
        "mulligan_samples" => cfg.mulligan_samples.to_string(),
        "prior_weight" => format!("{:?}", cfg.prior_weight),
        "prior_temperature" => format!("{:?}", cfg.prior_temperature),
        "horizon_turns" => cfg.horizon_turns.to_string(),
        "value_scale" => format!("{:?}", cfg.value_scale),
        "max_rollout_decisions" => cfg.max_rollout_decisions.to_string(),
        "policy_burst_yes_score" => format!("{:?}", cfg.policy_burst_yes_score),
        "noop_branch_choices" => cfg.noop_branch_choices.to_string(),
        "noop_resource_aware" => cfg.noop_resource_aware.to_string(),
        "strict_attack_prune" => cfg.strict_attack_prune.to_string(),
        "prune_noop_rest_targets" => cfg.prune_noop_rest_targets.to_string(),
        "merge_equivalent_moves" => cfg.merge_equivalent_moves.to_string(),
        "observation_keys" => cfg.observation_keys.to_string(),
        "policy_rule_polarity_table" => cfg.policy_rule_polarity_table.to_string(),
        "policy_zero_ap_face" => cfg.policy_zero_ap_face.to_string(),
        "target_extension" => format!("{:?}", cfg.target_extension),
        "target_extension_z" => format!("{:?}", cfg.target_extension_z),
        "weights" => weights_id(&cfg.weights),
        other => panic!("docs/AI.md documents an unknown setting {other}"),
    }
}

#[test]
fn the_preset_table_equals_the_shipped_budgets_and_their_mapping() {
    let doc = ai_md();
    let rows = table(&doc, PRESETS_HEADER);
    let names: Vec<&str> = rows.iter().map(|r| unquote(&r[0])).collect();
    let expected: Vec<&str> = Preset::ALL.iter().map(|p| p.as_str()).collect();
    assert_eq!(
        names, expected,
        "one row per shipped preset, in Preset::ALL order"
    );
    for (row, p) in rows.iter().zip(Preset::ALL) {
        assert_eq!(row.len(), 7, "{row:?}");
        let cfg = preset_config(p);
        assert_eq!(
            row[1],
            p.help(),
            "{}: the use is the --help wording",
            p.as_str()
        );
        let base = reference(mapped(p));
        let mapping = format!(
            "reference `{}` ({} / {} / {})",
            mapped(p),
            base.iterations_per_option,
            base.min_iterations,
            base.max_iterations
        );
        assert_eq!(row[2], mapping, "{}", p.as_str());
        let scale: f64 = row[3].parse().expect("a numeric budget scale");
        let budget = [
            cfg.iterations_per_option,
            cfg.min_iterations,
            cfg.max_iterations,
        ];
        let documented: Vec<u32> = row[4..7]
            .iter()
            .map(|c| c.parse().expect("an integer budget"))
            .collect();
        assert_eq!(documented, budget, "{}: the documented budget", p.as_str());
        let scaled = [
            base.iterations_per_option,
            base.min_iterations,
            base.max_iterations,
        ]
        .map(|b| f64::from(b) * scale);
        assert_eq!(
            scaled.map(f64::to_bits),
            budget.map(|b| f64::from(b).to_bits()),
            "{}: the budget is the mapped reference budget times the scale",
            p.as_str()
        );
    }
}

#[test]
fn the_settings_table_equals_every_other_preset_field() {
    let doc = ai_md();
    let rows = table(&doc, SETTINGS_HEADER);
    let mut documented = Vec::new();
    for row in &rows {
        assert_eq!(row.len(), 3, "{row:?}");
        let fields: Vec<&str> = row[0].split(", ").map(unquote).collect();
        for (column, p) in [(1, Preset::Standard), (2, Preset::Fast)] {
            let values: Vec<&str> = row[column].split(", ").map(unquote).collect();
            assert!(
                values.len() == 1 || values.len() == fields.len(),
                "{row:?}: one value, or one per field"
            );
            for (i, field) in fields.iter().enumerate() {
                let value = values[if values.len() == 1 { 0 } else { i }];
                assert_eq!(
                    value,
                    setting(preset_config(p), field),
                    "{}: {field}",
                    p.as_str()
                );
            }
        }
        documented.extend(fields);
    }
    for field in [
        "exploration",
        "rollout_temperature",
        "first_player_games",
        "mulligan_samples",
        "weights",
    ] {
        assert!(
            documented.contains(&field),
            "the settings table lacks {field}"
        );
    }
}

#[test]
fn the_mechanism_table_lists_exactly_the_shipped_mechanisms() {
    let doc = ai_md();
    let rows = table(&doc, MECHANISMS_HEADER);
    let mut listed: Vec<&str> = rows.iter().map(|r| unquote(&r[0])).collect();
    listed.sort();
    for p in Preset::ALL {
        assert_eq!(
            listed,
            preset_config(p).mechanisms.enabled(),
            "{}: the documented mechanisms",
            p.as_str()
        );
    }
}

#[test]
fn the_reference_table_equals_the_reference_presets() {
    let doc = ai_md();
    let rows = table(&doc, REFERENCE_HEADER);
    let names: Vec<&str> = rows.iter().map(|r| unquote(&r[0])).collect();
    assert_eq!(names, ["fast", "standard", "strong"]);
    for row in &rows {
        assert_eq!(row.len(), 10, "{row:?}");
        let r = reference(unquote(&row[0]));
        let actual = [
            r.agent_name(),
            r.iterations_per_option.to_string(),
            r.min_iterations.to_string(),
            r.max_iterations.to_string(),
            format!("{:?}", r.exploration),
            format!("{:?}", r.rollout_temperature),
            r.first_player_games.to_string(),
            r.mulligan_samples.to_string(),
        ];
        let cells: Vec<&str> = row[1..9].iter().map(|c| unquote(c)).collect();
        assert_eq!(cells, actual, "reference {}", unquote(&row[0]));
        let shipped: Vec<&str> = Preset::ALL
            .iter()
            .filter(|p| mapped(**p) == unquote(&row[0]))
            .map(|p| p.as_str())
            .collect();
        match shipped.as_slice() {
            [] => assert_eq!(row[9], "none", "{row:?}"),
            [one] => assert!(
                row[9].starts_with(&format!("`{one}` (")),
                "{row:?}: names the shipped preset {one}"
            ),
            more => panic!("two shipped presets start from one reference preset: {more:?}"),
        }
    }
}
