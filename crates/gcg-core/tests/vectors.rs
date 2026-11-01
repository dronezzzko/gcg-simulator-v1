//! Property tests against vectors exported from the Python reference
//! (`uv run python port/oracle/export_vectors.py`).

use gcg_core::pyfloat::{format_fixed, repr_f64, round_f64};
use gcg_core::pyjson::{DumpOptions, Json, Key, dumps};
use gcg_core::pymath::py_sum;
use gcg_core::{SeedPart, SplitMix64, derive_seed};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).expect("valid vectors.json")
}

fn u64_of(v: &Value) -> u64 {
    v.as_str().expect("u64 as string").parse().expect("u64")
}

fn f64_of(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().expect("hex bits"), 16).expect("hex"))
}

#[test]
fn splitmix_streams() {
    let v = vectors();
    for s in v["streams"].as_array().unwrap() {
        let mut r = SplitMix64::new(u64_of(&s["seed"]));
        for want in s["next_u64"].as_array().unwrap() {
            assert_eq!(r.next_u64(), u64_of(want));
        }
    }
}

#[test]
fn randrange_random_shuffle() {
    let v = vectors();
    for s in v["randrange"].as_array().unwrap() {
        let mut r = SplitMix64::new(u64_of(&s["seed"]));
        let n = u64_of(&s["n"]);
        for want in s["out"].as_array().unwrap() {
            assert_eq!(r.randrange(n), u64_of(want), "randrange({n})");
        }
    }
    for s in v["random"].as_array().unwrap() {
        let mut r = SplitMix64::new(u64_of(&s["seed"]));
        for want in s["out"].as_array().unwrap() {
            assert_eq!(r.random().to_bits(), f64_of(want).to_bits());
        }
    }
    for s in v["shuffle"].as_array().unwrap() {
        let mut r = SplitMix64::new(u64_of(&s["seed"]));
        let mut items: Vec<u64> = (0..s["n"].as_u64().unwrap()).collect();
        r.shuffle(&mut items);
        let want: Vec<u64> = s["out"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        assert_eq!(items, want);
    }
}

#[test]
fn derive_seed_matches() {
    let v = vectors();
    for case in v["derive_seed"].as_array().unwrap() {
        let master: i128 = case["master"].as_str().unwrap().parse().unwrap();
        let master = u64::try_from(master.rem_euclid(1i128 << 64)).unwrap();
        let path: Vec<SeedPart<'_>> = case["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| match p {
                Value::String(s) => SeedPart::Str(s.as_str()),
                Value::Object(o) => SeedPart::Int(o["int"].as_i64().unwrap()),
                other => panic!("unexpected path part {other}"),
            })
            .collect();
        assert_eq!(derive_seed(master, &path), u64_of(&case["out"]), "{case}");
    }
}

#[test]
fn float_repr_matches() {
    let v = vectors();
    for case in v["repr"].as_array().unwrap() {
        let x = f64_of(&case["x"]);
        assert_eq!(
            repr_f64(x),
            case["repr"].as_str().unwrap(),
            "bits {}",
            case["x"]
        );
    }
}

#[test]
fn fixed_format_and_round_match() {
    let v = vectors();
    for case in v["fixed"].as_array().unwrap() {
        let x = f64_of(&case["x"]);
        let prec = usize::try_from(case["prec"].as_u64().unwrap()).unwrap();
        assert_eq!(
            format_fixed(x, prec),
            case["fmt"].as_str().unwrap(),
            "format({x:e}, .{prec}f)"
        );
        assert_eq!(
            round_f64(x, prec).to_bits(),
            f64_of(&case["round"]).to_bits(),
            "round({x:e}, {prec})"
        );
    }
}

#[test]
fn float_sum_matches() {
    let v = vectors();
    for case in v["sum"].as_array().unwrap() {
        let vals: Vec<f64> = case["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(f64_of)
            .collect();
        assert_eq!(
            py_sum(vals.iter().copied()).to_bits(),
            f64_of(&case["sum"]).to_bits(),
            "{vals:?}"
        );
    }
}

fn sample() -> Json {
    let mut nested = Json::object();
    nested.set("z", Json::object());
    nested.set("y", Json::Array(vec![]));
    let mut k = Json::object();
    k.set("k", 3i64);
    nested.set("x", Json::Array(vec![Json::Array(vec![Json::Int(1)]), k]));
    let mut s = Json::object();
    s.set(
        "b",
        Json::Array(vec![
            Json::Int(1),
            Json::Float(2.5),
            Json::Float(-0.0),
            Json::Float(1e-7),
            Json::Null,
            Json::Bool(true),
            Json::Bool(false),
        ]),
    );
    s.set(
        "a",
        "Zeon \u{b7} \u{201c}quoted\u{201d} \\ \"x\" \n\t\u{1} \u{1f680}",
    );
    s.set("nested", nested);
    s.set(Key::Str("seed".into()), u64::MAX);
    s
}

#[test]
fn json_dumps_matches() {
    let v = vectors();
    let s = sample();
    for case in v["dumps"].as_array().unwrap() {
        let opts = match case["opts"].as_str().unwrap() {
            "pretty_sorted_utf8" => DumpOptions::PRETTY_SORTED_UTF8,
            "compact_sorted_ascii" => DumpOptions::COMPACT_SORTED_ASCII,
            "indent1_sorted_utf8" => DumpOptions {
                indent: Some(1),
                sort_keys: true,
                compact: false,
                ensure_ascii: false,
            },
            "default" => DumpOptions {
                indent: None,
                sort_keys: false,
                compact: false,
                ensure_ascii: true,
            },
            other => panic!("unknown opts {other}"),
        };
        assert_eq!(
            dumps(&s, opts),
            case["out"].as_str().unwrap(),
            "{}",
            case["opts"]
        );
    }
}

#[test]
fn libm_parity_matches() {
    use gcg_core::libm;
    let v = vectors();
    let mut square_differs_from_mul = 0;
    for case in v["libm"].as_array().unwrap() {
        let x = f64_of(&case["x"]);
        assert_eq!(
            libm::erfc(x).to_bits(),
            f64_of(&case["erfc"]).to_bits(),
            "erfc({x:e})"
        );
        let sq = f64_of(&case["square"]);
        assert_eq!(libm::square(x).to_bits(), sq.to_bits(), "({x:e})**2");
        if (x * x).to_bits() != sq.to_bits() {
            square_differs_from_mul += 1;
        }
        if !case["exp"].is_null() {
            assert_eq!(
                libm::exp(x).to_bits(),
                f64_of(&case["exp"]).to_bits(),
                "exp({x:e})"
            );
        }
        if !case["log"].is_null() {
            assert_eq!(
                libm::ln(x.abs()).to_bits(),
                f64_of(&case["log"]).to_bits(),
                "log({x:e})"
            );
        }
    }
    eprintln!("x**2 != x*x on {square_differs_from_mul} vectors");
}
