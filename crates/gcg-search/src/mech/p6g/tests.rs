//! P6-G's unit tests (W-34 … W-37, W-39, W-142).

use gcg_ai::{AgentCx, IterTrace, Scratch, SearchTracer};
use gcg_core::pool::{Cancel, Pool, TaskCx};
use gcg_core::rng::SplitMix64;
use gcg_data::CardType;
use gcg_effects::Kw;
use gcg_engine::observe::determinize_into;
use gcg_engine::{Action, GameState};

use super::LEAF_POINT;
use super::api::{NAMES, determinized_features, hand_slot, kw_feature, leaf_features};
use super::features::{KW_START, N_FEATURES};
use super::model::{Model, ModelSpec, parse_table, table_text};
use crate::mech::tests::{config, searched_states};
use crate::{MechanismFlags, Observation};

/// W-142: on 100 positions, the tuning tool's determinised recording equals the leaf's
/// features of the world the search builds with the same seed (`determinize_into` of the
/// information-set state, the reference world of `Mechanisms::world`), bit for bit.
#[test]
fn w142_recorded_features_are_the_leaf_features() {
    let states = searched_states(100);
    assert_eq!(states.len(), 100, "100 searched positions");
    let mut checked = 0;
    for (st, player, seed) in &states {
        for observer in [*player, 1 - *player] {
            let obs = Observation::new(st, observer).expect("an observation");
            let recorded = determinized_features(&obs, *seed).expect("recorded features");
            let mut world = st.clone();
            determinize_into(obs.info(), observer, *seed, &mut world).expect("a world");
            let leaf = leaf_features(&world, observer).expect("leaf features");
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
            assert_eq!(bits(&recorded), bits(&leaf));
            assert_eq!(recorded.len(), N_FEATURES);
            checked += 1;
        }
    }
    assert_eq!(checked, 200);
}

/// Records, per iteration, whether its last engine apply left the line at [`LEAF_POINT`]
/// (judged from the turn before that apply), whether it ended the game, and the leaf's turn.
#[derive(Default)]
struct LeafTracer {
    turn_before: Option<i32>,
    last: Option<(bool, bool, i32)>,
    leaves: Vec<(bool, bool, i32, bool)>,
}

impl SearchTracer for LeafTracer {
    fn iteration(&mut self, world: &GameState, it: &IterTrace) {
        let (holds, over, turn) = self.last.take().unwrap_or((false, false, world.turn()));
        self.leaves.push((holds, over, turn, it.terminal));
        self.turn_before = None;
    }

    fn apply(&mut self, _n: u32, j: u32, _action: Action, after: &GameState) {
        let before = if j == 0 { None } else { self.turn_before };
        let holds = before.is_some_and(|t| LEAF_POINT.holds(Some(t), after));
        self.last = Some((holds, after.winner().is_some(), after.turn()));
        self.turn_before = Some(after.turn());
    }
}

/// W-39: the search scores every line that reached its horizon at [`LEAF_POINT`], the same
/// constant the tuning tool records at (`gcg-eval tune-leaf`): each leaf is a finished game, a
/// rollout stopped at its decision cap, or the first pending decision of the horizon turn.
#[test]
fn w39_search_leaves_are_at_the_recording_point() {
    let cfg = config(&[]);
    let mut leaves = 0;
    for (st, player, seed) in searched_states(6) {
        let obs = Observation::new(&st, player).expect("an observation");
        let horizon = st.turn() + cfg.horizon_turns;
        let cancel = Cancel::new();
        let mut tracer = LeafTracer::default();
        let result = Pool::run_inline(|scratch: &mut Scratch, par: &TaskCx<'_, Scratch>| {
            let mut cx = AgentCx {
                scratch,
                par,
                cancel: &cancel,
            };
            crate::search(&obs, &cfg, &mut SplitMix64::new(seed), &mut cx, &mut tracer)
        })
        .expect("a search");
        let mut capped = 0;
        for &(holds, over, turn, terminal) in &tracer.leaves {
            assert_eq!(over, terminal);
            if over {
                continue;
            }
            if turn < horizon {
                capped += 1;
                continue;
            }
            assert_eq!(turn, horizon);
            assert!(
                holds,
                "a leaf of turn {turn} not at {}",
                LEAF_POINT.as_str()
            );
            leaves += 1;
        }
        assert_eq!(capped, result.stats.effort.rollouts_capped);
    }
    assert!(leaves > 1000, "{leaves} leaves checked");
}

/// W-37: every card type a hand can hold has its own slot; the others are an error, never the
/// reference's catch-all Base slot.
#[test]
fn w37_every_hand_card_type_has_a_slot() {
    let mut slots = Vec::new();
    for t in CardType::ALL {
        match hand_slot(t) {
            Ok(s) => slots.push(s),
            Err(e) => {
                assert!(t == CardType::Resource || t.is_token(), "{t:?}");
                assert!(e.to_string().contains("no hand slot"), "{e}");
            }
        }
    }
    assert_eq!(slots, [0, 1, 2, 3]);
}

/// W-37: every `Kw` variant maps to its own keyword feature pair, named after it.
#[test]
fn w37_every_keyword_has_a_feature() {
    let mut seen = vec![false; Kw::ALL.len()];
    for kw in Kw::ALL {
        let i = kw_feature(kw);
        assert!(!seen[i], "{kw:?} shares a feature");
        seen[i] = true;
        let slug = kw.as_str().to_lowercase().replace([' ', '-'], "_");
        assert_eq!(NAMES[KW_START + 2 * i], format!("tp_kw_{slug}"));
        assert_eq!(NAMES[KW_START + 2 * i + 1], format!("ot_kw_{slug}"));
    }
    assert!(seen.into_iter().all(|s| s));
    assert_eq!(NAMES.len(), N_FEATURES);
}

/// W-39: no strengthened feature set has a tempo feature; the intercept is constant.
#[test]
fn w39_no_tempo_feature() {
    assert!(!NAMES.contains(&"tempo"));
    assert_eq!(NAMES[0], "intercept");
    let mut flags = MechanismFlags::NULL;
    flags.refit_weights = true;
    let spec = ModelSpec::of(&flags).expect("a model");
    assert_eq!(spec.names()[0], "intercept");
    flags.no_tempo_feature = true;
    let spec = ModelSpec::of(&flags).expect("a model");
    assert!(!spec.names().contains(&"intercept"));
}

#[test]
fn model_keys_and_tables_round_trip() {
    let mut flags = MechanismFlags::NULL;
    assert_eq!(ModelSpec::of(&flags), None);
    flags.leaf_density = true;
    assert_eq!(ModelSpec::of(&flags), None);
    flags.hand_slots = true;
    flags.phase_weights = true;
    let spec = ModelSpec::of(&flags).expect("a model");
    assert_eq!(spec.key(), "kw=0,ab=0,hs=1,bp=0,det=0,int=1,ph=1");
    assert_eq!(ModelSpec::parse_key(&spec.key()), Some(spec));
    assert_eq!(ModelSpec::parse_key("kw=0,ab=0"), None);
    let cols = spec.columns();
    assert!(!spec.names().contains(&"hand_units"));
    assert!(spec.names().contains(&"tp_hand_unit"));
    let m = Model {
        spec,
        columns: cols.clone(),
        edges: vec![6, 12],
        weights: (0..3)
            .map(|b| {
                (0..cols.len())
                    .map(|i| 0.25 * f64::from(b) - 0.01 * f64::from(u32::try_from(i).unwrap_or(0)))
                    .collect()
            })
            .collect(),
    };
    let text = table_text(&["test".to_owned()], std::slice::from_ref(&m));
    assert_eq!(parse_table(&text), Ok(vec![m.clone()]));
    assert_eq!(
        (m.bin(3), m.bin(6), m.bin(11), m.bin(12), m.bin(30)),
        (0, 1, 1, 2, 2)
    );
    assert!(parse_table("model kw=0,ab=0,hs=0,bp=0,det=0,int=1,ph=0\n").is_err());
    assert!(parse_table("w 0\n").is_err());
}

/// The recorded leaf values of `leaf_vectors.txt`: per embedded model, the leaf value (with
/// `portable_math` and `leaf_density`) of the determinised world of each of the first 20
/// searched positions, as IEEE-754 bits.
fn leaf_vectors() -> String {
    use std::fmt::Write as _;
    let states = searched_states(20);
    let mut out = String::from(
        "# P6-G leaf values (portable math, leaf density) of the first 20 searched positions of\n\
         # crates/gcg-ai/tests/vectors/search/decisions.ndjson, per embedded model; written by\n\
         # the test w35_leaf_values_match_the_recorded_vectors.\n",
    );
    for m in super::model::embedded_models().expect("the embedded table") {
        let mut cfg = config(&["portable_math", "leaf_density"]);
        cfg.mechanisms.refit_weights = true;
        cfg.mechanisms.keyword_features = m.spec.keywords;
        cfg.mechanisms.ability_count_features = m.spec.abilities;
        cfg.mechanisms.hand_slots = m.spec.hand_slots;
        cfg.mechanisms.burst_shield_probability = m.spec.burst;
        cfg.mechanisms.determinized_fit = m.spec.determinized;
        cfg.mechanisms.no_tempo_feature = !m.spec.intercept;
        cfg.mechanisms.phase_weights = m.spec.phase;
        let _ = writeln!(out, "model {}", m.spec.key());
        let mut bits = Vec::new();
        for (st, player, seed) in &states {
            let obs = Observation::new(st, *player).expect("an observation");
            let mut world = st.clone();
            determinize_into(obs.info(), *player, *seed, &mut world).expect("a world");
            let mut cx = super::super::LeafCx {
                s: &world,
                player: *player,
                root_turn: st.turn(),
                horizon: st.turn() + cfg.horizon_turns,
                decisions: 0,
                capped: false,
                cfg: &cfg,
                weights: &cfg.weights,
                density: gcg_ai::burst_density(obs.info(), *player),
                math: crate::mech::math_of(&cfg),
            };
            let v = super::leaf_value(&cfg.mechanisms, &mut cx).expect("a leaf value");
            bits.push(format!("{:016x}", v.to_bits()));
        }
        let _ = writeln!(out, "v {}", bits.join(" "));
    }
    out
}

/// W-35, W-137: the strengthened leaf values equal the recorded vectors bit for bit. They use
/// only IEEE-754 basic operations and the portable `exp` (the `libm` crate), so the same bits
/// hold on every platform (the CI matrix runs this test on macOS arm64 and Linux x86-64). When
/// the fitted weights change, the regenerated file is written to the temporary directory and
/// the failure names it.
#[test]
fn w35_leaf_values_match_the_recorded_vectors() {
    let want = include_str!("leaf_vectors.txt");
    let got = leaf_vectors();
    if got != want {
        let path = std::env::temp_dir().join("p6g-leaf_vectors.txt");
        std::fs::write(&path, &got).expect("the regenerated vectors");
        panic!(
            "the leaf values differ from leaf_vectors.txt; the regenerated file is {}",
            path.display()
        );
    }
}
