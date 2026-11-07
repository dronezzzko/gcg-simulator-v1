//! PLAN §3.4 test 3 (T-042): the golden writer regenerates all 27 files of
//! `tests/effects/golden/` byte for byte from the IR (`tools/golden.py:22-36`,
//! `effects/golden.py:19-51`).

use std::fs;

use super::{repo_root, tables};

#[test]
fn all_golden_files_regenerate_byte_for_byte() {
    let dir = repo_root().join("tests/effects/golden");
    let mut on_disk: Vec<String> = fs::read_dir(&dir)
        .expect("tests/effects/golden")
        .map(|e| e.expect("entry").file_name().into_string().expect("UTF-8"))
        .filter(|n| n.ends_with(".json"))
        .collect();
    on_disk.sort();
    let files = tables().golden_files();
    let names: Vec<String> = files.iter().map(|(p, _)| format!("{p}.json")).collect();
    assert_eq!(names, on_disk);
    assert_eq!(files.len(), 28);
    for (prefix, text) in &files {
        let want = fs::read_to_string(dir.join(format!("{prefix}.json"))).expect("golden file");
        if *text != want {
            let at = text
                .bytes()
                .zip(want.bytes())
                .position(|(a, b)| a != b)
                .unwrap_or(text.len().min(want.len()));
            let from = at.saturating_sub(80);
            panic!(
                "{prefix}.json differs at byte {at}:\n rust:   {:?}\n python: {:?}",
                text.get(from..(at + 80).min(text.len())),
                want.get(from..(at + 80).min(want.len()))
            );
        }
    }
}

#[test]
fn golden_records_cover_the_real_cards_only() {
    let t = tables();
    let records: usize = t
        .golden_files()
        .iter()
        .map(|(_, text)| text.lines().filter(|l| l.starts_with(" \"")).count())
        .sum();
    assert_eq!(
        records, 1290,
        "the synthetic Unit variant has no golden record"
    );
}
