//! GAP-A2's tests of W-137 (docs/dev/eval/weakness-status/GAP-A2.md): `portable_math` reaches
//! every float path of the strengthened search, the shared `gcg-ai` functions it calls
//! included (the no-effect check's evaluation, the play/draw simulations' Gumbel draws and
//! paired variance, the rollout policy's Gumbel draws), while every reference path keeps the
//! platform library.
//!
//! - The search's provider (`p6o::math::Math`) and `gcg_ai::FloatMath` are one library: the
//!   same bits for each implementation, and the search's portable policy move is
//!   `gcg_ai::choose_with` at `FloatMath::Portable`.
//! - The reference extension points take the configuration's math: the play/draw choice of
//!   `reference::pregame` is `gcg_ai::first_player_choice_with` at the configuration's library,
//!   and `gcg_ai::first_player_choice` itself without `portable_math`.
//! - Platform independence by construction: no non-test source of the crate calls the platform
//!   library or a transcendental `f64` method except through the provider, and none calls a
//!   `gcg-ai` entry point that fixes the platform library (`evaluate`, `logistic`,
//!   `without_effect`, `first_player_choice`, `play_to_end`, `play_until_turn`, or `choose` and
//!   `pick` with a generator). The portable paths are then IEEE-754 basic operations and
//!   `gcg_core::portable` only, which give the same bits on every platform; the recorded search
//!   bits of `tests/scenarios/gapa2_portable_math.rs` check that on any machine that runs them.

use std::path::{Path, PathBuf};

use gcg_ai::{AgentCx, Alternative, FloatMath, Scratch};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_effects::Registry;
use gcg_engine::canon::from_canonical;
use gcg_engine::{Action, GameState, StateSnapshot};
use serde_json::Value;

use super::p6o::math::Math;
use super::tests::searched_states;
use super::{PregameCx, reference};
use crate::{MechanismFlags, Observation, Preset, StrengthenedConfig, reference_mapping};

const INPUTS: [f64; 19] = [
    -745.1, -40.0, -30.5, -2.0, -1.0, -0.3, -1e-9, -0.0, 0.0, 1e-300, 1e-9, 0.1, 0.5, 0.75, 1.0,
    2.5, 30.0, 700.0, 709.7,
];

#[test]
fn the_search_math_and_the_gcg_ai_provider_are_one_library() {
    assert_eq!(Math::Platform.float(), FloatMath::Platform);
    assert_eq!(Math::Portable.float(), FloatMath::Portable);
    for math in [Math::Platform, Math::Portable] {
        let f = math.float();
        for x in INPUTS {
            assert_eq!(
                math.exp(x).to_bits(),
                f.exp(x).to_bits(),
                "{math:?} exp({x})"
            );
            assert_eq!(
                math.square(x).to_bits(),
                f.square(x).to_bits(),
                "{math:?} {x}²"
            );
            assert_eq!(math.sqrt(x.abs()).to_bits(), f.sqrt(x.abs()).to_bits());
            assert_eq!(
                math.logistic(x).to_bits(),
                gcg_ai::logistic_with(x, f).to_bits(),
                "{math:?} logistic({x})"
            );
            if x > 0.0 {
                assert_eq!(math.ln(x).to_bits(), f.ln(x).to_bits(), "{math:?} ln({x})");
            }
        }
    }
    for x in INPUTS {
        assert_eq!(
            gcg_ai::logistic_with(x, FloatMath::Platform).to_bits(),
            gcg_ai::logistic(x).to_bits()
        );
    }
}

/// The rollout policy's move of the search (`Math::choose`) is `gcg_ai::choose_with` at the
/// search's library, draw for draw, and at `Math::Platform` the reference's `gcg_ai::choose`.
#[test]
fn the_search_policy_move_is_gcg_ai_choose_with_its_library() {
    let knobs = reference_mapping(Preset::Fast).knobs();
    let mut checked = 0;
    for (st, _, seed) in searched_states(40) {
        for temperature in [0.0, 0.3, 0.5, 1.5] {
            let mut a = SplitMix64::new(seed);
            let mut b = SplitMix64::new(seed);
            let mine = Math::Portable.choose(&st, &mut a, temperature, &knobs);
            let shared =
                gcg_ai::choose_with(&st, Some(&mut b), temperature, &knobs, FloatMath::Portable);
            assert_eq!(mine, shared, "portable, T = {temperature}");
            assert_eq!(a.state, b.state);
            let mut a = SplitMix64::new(seed);
            let mut b = SplitMix64::new(seed);
            let mine = Math::Platform.choose(&st, &mut a, temperature, &knobs);
            let reference = gcg_ai::choose(&st, Some(&mut b), temperature, &knobs);
            assert_eq!(mine, reference, "platform, T = {temperature}");
            assert_eq!(a.state, b.state);
            let mut c = SplitMix64::new(seed);
            assert_eq!(
                gcg_ai::choose_with(&st, Some(&mut c), temperature, &knobs, FloatMath::Platform),
                reference
            );
            checked += 1;
        }
    }
    assert!(checked >= 120, "{checked} policy moves");
}

/// The CHOOSE_FIRST records of the D2 vectors, as `(state, player, per-decision seed)`.
fn choose_first_records() -> Vec<(GameState, u8, u64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gcg-ai/tests/vectors/search/decisions.ndjson");
    let reg = Registry::embedded();
    std::fs::read_to_string(path)
        .expect("the vectors")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a record"))
        .filter(|rec| rec["kind"].as_str() == Some("choose_first"))
        .map(|rec| {
            let snap = &rec["snapshot"];
            let st = from_canonical(
                &reg,
                &StateSnapshot {
                    state: snap["state"].clone(),
                    continuous: snap["continuous"].clone(),
                    filter_sets: snap["filter_sets"].clone(),
                    rules_checked: snap["rules_checked"].as_i64().expect("rules_checked"),
                },
            )
            .expect("a state");
            let player = u8::try_from(rec["player"].as_i64().expect("a seat")).expect("a seat");
            let seed = rec["seed"]
                .as_u64()
                .or_else(|| rec["seed"].as_str().and_then(|s| s.parse().ok()))
                .expect("a seed");
            (st, player, seed)
        })
        .collect()
}

type Choice = (Action, Vec<(Action, u64, u32)>, u64);

fn choice_bits(out: (Action, Vec<Alternative>), rng: &SplitMix64) -> Choice {
    let (action, alts) = out;
    (
        action,
        alts.iter()
            .map(|a| (a.action, a.value.to_bits(), a.visits))
            .collect(),
        rng.state,
    )
}

/// The play/draw choice of `reference::pregame` with `cfg`, and `direct` on the same inputs.
fn both_choices(
    rec: &(GameState, u8, u64),
    cfg: &StrengthenedConfig,
    direct: FloatMath,
) -> (Choice, Choice) {
    let (st, player, seed) = rec;
    let cancel = Cancel::new();
    let obs = Observation::new(st, *player).expect("an observation");
    Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
        let mut agent = AgentCx {
            scratch,
            par,
            cancel: &cancel,
        };
        let info = obs.info();
        let dec = info.pending().expect("a decision");
        let mut rng = SplitMix64::new(*seed);
        let out = reference::pregame(&mut PregameCx {
            st: info,
            dec: &dec,
            player: *player,
            cfg,
            rng: &mut rng,
            agent: &mut agent,
            applies: 0,
        })
        .expect("the reference pre-game choice");
        let via_mechanisms = choice_bits(out, &rng);
        let mut rng = SplitMix64::new(*seed);
        let out = if direct == FloatMath::Platform {
            gcg_ai::first_player_choice(
                info,
                *player,
                cfg.first_player_games,
                &mut rng,
                &cfg.knobs(),
                &mut agent,
            )
        } else {
            gcg_ai::first_player_choice_with(
                info,
                *player,
                cfg.first_player_games,
                &mut rng,
                &cfg.knobs(),
                &mut agent,
                direct,
            )
        }
        .expect("the direct play/draw choice");
        (via_mechanisms, choice_bits(out, &rng))
    })
}

#[test]
fn the_reference_play_draw_choice_takes_the_configuration_math() {
    let records = choose_first_records();
    assert_eq!(records.len(), 6);
    let mut platform = reference_mapping(Preset::Fast).clone();
    platform.mechanisms = MechanismFlags::NULL;
    let mut portable = platform.clone();
    portable.mechanisms.portable_math = true;
    for rec in &records {
        let (a, b) = both_choices(rec, &platform, FloatMath::Platform);
        assert_eq!(
            a, b,
            "without portable_math it is the reference's first_player_choice"
        );
        let (a, b) = both_choices(rec, &portable, FloatMath::Portable);
        assert_eq!(a, b, "with portable_math the portable library");
    }
}

/// A source file's text without its inline test modules.
fn non_test_code(text: &str) -> &str {
    text.find("#[cfg(test)]\nmod tests {")
        .map_or(text, |i| &text[..i])
}

/// Every `.rs` file under `dir`, sorted.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a source directory")
        .map(|e| e.expect("an entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The names a source imports from `gcg_ai` (the last segment of each path in its `use
/// gcg_ai::...;` statements).
fn gcg_ai_imports(code: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("use gcg_ai::") {
        let tail = &rest[i + "use gcg_ai::".len()..];
        let end = tail.find(';').expect("a use statement ends");
        let body = &tail[..end];
        for item in body.split([',', '{', '}']) {
            let item = item.trim();
            let name = item.split(" as ").next().unwrap_or("").trim();
            if let Some(last) = name.rsplit("::").next().filter(|s| !s.is_empty()) {
                names.push(last.to_owned());
            }
        }
        rest = &tail[end..];
    }
    names
}

/// The argument texts of the call whose `(` is at `open`, split at top-level commas.
fn call_args(code: &str, open: usize) -> Vec<String> {
    let mut depth = 0usize;
    let mut args = vec![String::new()];
    for ch in code[open..].chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            ',' if depth == 1 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = args.last_mut() {
            last.push(ch);
        }
    }
    args.iter().map(|a| a.trim().to_owned()).collect()
}

/// The positions of the calls of `name` in `code` that are `gcg_ai`'s: `gcg_ai::name(`, or a
/// bare `name(` when the file imports it from `gcg_ai` (not a method, a definition or another
/// path).
fn gcg_ai_calls(code: &str, name: &str, imported: bool) -> Vec<usize> {
    let pat = format!("{name}(");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = code[from..].find(&pat) {
        let at = from + i;
        from = at + pat.len();
        let before = &code[..at];
        let ident_char = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if ident_char {
            continue;
        }
        let ours = if before.ends_with("gcg_ai::") {
            true
        } else {
            imported
                && !before.ends_with('.')
                && !before.ends_with("::")
                && !before.trim_end().ends_with("fn")
        };
        if ours {
            out.push(at + name.len());
        }
    }
    out
}

/// Platform independence by construction (W-137): outside the provider `p6o/math.rs`, no
/// non-test source calls the platform library or a transcendental `f64` method on anything but
/// the search's `math`, and every `gcg-ai` function it calls that draws on the library is the
/// `_with` variant (or `choose`/`pick` without a generator, which take the argmax and call no
/// library function).
#[test]
fn every_float_path_of_the_search_goes_through_its_math() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let fixed = [
        "evaluate",
        "logistic",
        "without_effect",
        "first_player_choice",
        "play_to_end",
        "play_until_turn",
    ];
    let methods = [
        "exp", "ln", "log", "log2", "log10", "powf", "powi", "exp_m1", "ln_1p", "sin", "cos",
        "tan", "cbrt", "hypot", "mul_add",
    ];
    let mut checked = 0;
    for path in files {
        let rel = path.strip_prefix(&src).expect("under src");
        let name = rel.to_string_lossy().replace('\\', "/");
        let test_file = name.ends_with("tests.rs") || name.starts_with("mech/gapa2/");
        if test_file || name == "mech/p6o/math.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a source file");
        let code: String = non_test_code(&text)
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!code.contains("libm::"), "{name}: a platform libm call");
        for m in methods {
            let pat = format!(".{m}(");
            let mut from = 0;
            while let Some(i) = code[from..].find(&pat) {
                let at = from + i;
                from = at + pat.len();
                let receiver: String = code[..at]
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                assert_eq!(
                    receiver, "math",
                    "{name}: `.{m}(` on `{receiver}` instead of the search's math"
                );
            }
        }
        let imports = gcg_ai_imports(&code);
        for f in fixed {
            assert!(
                !imports.iter().any(|i| i == f),
                "{name}: imports gcg_ai::{f}, which fixes the platform library"
            );
            assert!(
                gcg_ai_calls(&code, f, false).is_empty(),
                "{name}: calls gcg_ai::{f}, which fixes the platform library"
            );
        }
        for (f, rng_arg) in [("choose", 1), ("pick", 2)] {
            let imported = imports.iter().any(|i| i == f);
            for open in gcg_ai_calls(&code, f, imported) {
                let args = call_args(&code, open);
                assert_eq!(
                    args.get(rng_arg).map(String::as_str),
                    Some("None"),
                    "{name}: gcg_ai::{f} with a generator draws with the platform ln"
                );
            }
        }
        checked += 1;
    }
    assert!(checked >= 50, "{checked} source files checked");
}

/// The lint finds what it bans (a negative control of its scanners).
#[test]
fn the_float_path_lint_finds_platform_calls() {
    let code = "use gcg_ai::{evaluate, policy::pick, choose as c};\n\
                let a = gcg_ai::choose(s, Some(&mut rng), 0.5, k);\n\
                let b = choose(s, None, 0.0, k);\n\
                let x = noop::without_effect(a);\n";
    let imports = gcg_ai_imports(code);
    assert_eq!(imports, ["evaluate", "pick", "choose"]);
    let calls = gcg_ai_calls(code, "choose", true);
    assert_eq!(calls.len(), 2);
    assert_eq!(call_args(code, calls[0])[1], "Some(&mut rng)");
    assert_eq!(call_args(code, calls[1])[1], "None");
    assert!(gcg_ai_calls(code, "without_effect", false).is_empty());
    assert_eq!(
        non_test_code("a\n#[cfg(test)]\nmod tests {\n libm::exp(1.0) }"),
        "a\n"
    );
}
