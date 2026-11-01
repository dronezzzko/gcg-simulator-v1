//! The golden-format writer: `effects/golden.py:40-51` per card (`Registry::golden_json`) and
//! `tools/golden.py:22-36` per file (test-only: the crate's test regenerates
//! `tests/effects/golden/*.json` byte for byte from the IR, docs/port/PLAN.md §3.4 test 3).

use gcg_core::pyjson::Json;
#[cfg(test)]
use gcg_core::pyjson::{DumpOptions, dumps};
use gcg_data::DefId;
#[cfg(test)]
use gcg_data::{SYNTHETIC_PREFIX, UNIT_VARIANT_PREFIX};

use crate::codec::Field;
use crate::dsl::Node;
use crate::ir::Tables;

/// `json.dumps(data, indent=1, sort_keys=True, ensure_ascii=False)` (`tools/golden.py:36`).
#[cfg(test)]
pub(crate) const GOLDEN_DUMP: DumpOptions = DumpOptions {
    indent: Some(1),
    sort_keys: true,
    compact: false,
    ensure_ascii: false,
};

impl Tables {
    /// `card_golden(cdef, entry, binding_module)` (`effects/golden.py:40-51`). Every entry of
    /// a loaded IR has a script (the loader refuses an unimplemented card, W-69), so the
    /// `error` branch of Python (`:42-44`) cannot occur.
    pub(crate) fn golden_json(&self, def: DefId) -> Json {
        let entry = &self.cards[usize::from(def.0)];
        let text = &self.card_texts[usize::from(def.0)];
        let view = self.view();
        let cx = view.repr_cx();
        let mut rec = Json::object();
        rec.set("name", &*text.name);
        rec.set("text_hash", &*text.text_hash);
        rec.set("source", entry.source.as_str());
        if let Some(module) = entry.binding_module {
            rec.set("binding", view.str(module));
        }
        let script = entry.script.map(|s| view.node(s));
        if let Some(Node::CardScript(script)) = script {
            rec.set("abilities", script.abilities.golden(&cx));
            if !script.unit_abilities.is_empty() {
                rec.set("unit_abilities", script.unit_abilities.golden(&cx));
            }
        }
        rec
    }

    /// The golden files of `tools/golden.py:22-36`: one per card-number prefix
    /// (`card_number.split("-", 1)[0]`, `effects/golden.py:54-55`) over the real cards
    /// (`CardDB.real_cards`, `cards/db.py:309-313`), sorted by prefix, as
    /// `(file stem, file text)`.
    #[cfg(test)]
    pub(crate) fn golden_files(&self) -> Vec<(String, String)> {
        let mut files: Vec<(String, Json)> = Vec::new();
        for (i, text) in self.card_texts.iter().enumerate() {
            let number = &*text.card_number;
            if number.starts_with(SYNTHETIC_PREFIX) || number.starts_with(UNIT_VARIANT_PREFIX) {
                continue;
            }
            let prefix = number.split_once('-').map_or(number, |(p, _)| p);
            let def = DefId(u16::try_from(i).expect("def ids fit u16"));
            let record = self.golden_json(def);
            match files.iter_mut().find(|(p, _)| p == prefix) {
                Some((_, obj)) => obj.set(number, record),
                None => {
                    let mut obj = Json::object();
                    obj.set(number, record);
                    files.push((prefix.to_owned(), obj));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
            .into_iter()
            .map(|(p, obj)| {
                let mut text = dumps(&obj, GOLDEN_DUMP);
                text.push('\n');
                (p, text)
            })
            .collect()
    }
}
