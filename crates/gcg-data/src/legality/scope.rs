//! What the validator checks and what it does not (W-75, W-76): the deck-construction format
//! applied, the formats it does not support, the tournament-only deck rules and tournament
//! procedures it does not model, and the Edition Beta printings whose tournament legality is
//! officially undecided. The lists come from the packaged official data where it records them
//! (`deck_construction.json`, `banlist.json`, `bo3_match_rules.json`, `edition_language.json`);
//! the rest cite the floor rules. `edition_language.json` is not a file the Python runtime
//! reads, so gcg-data does not embed it (docs/dev/PLAN.md §2.4): the caller passes its text to
//! [`Scope::new`] (`gcg-eval validate` embeds it); the other three are gcg-data's embedded
//! files.

use gcg_core::pyjson::Json;
use serde_json::Value;

use super::lists::BanlistEntry;
use crate::contracts::{DataError, Deck};
use crate::{embedded, py};

const EDITION_FILE: &str = "official/edition_language.json";

/// The one deck-construction format the validator applies: TRM 3.3 "Constructed: Players bring
/// a tournament-legal deck to play in a constructed tournament."
pub const FORMAT_APPLIED: &str = "Constructed";

/// The two-player deck-construction formats of TRM 3.3 (`data/official_raw/en_pdf_floor_rule_en.txt`)
/// that the validator does not support. Sealed Battle also has its own rules notice
/// (`news/sealed-battle.html`, linked from the Rules page and not in the data).
pub const UNSUPPORTED_FORMATS: [(&str, &str); 2] = [
    (
        "Sealed",
        "Players will receive specific event product during the players meeting, and will \
         construct a legal deck from that product only. (TRM 3.3; the Sealed Battle rules page \
         news/sealed-battle.html is not in the data)",
    ),
    (
        "Draft",
        "Players will receive booster packs from a booster box and will draft cards in pods of \
         four. Following the draft, players will construct their deck from their drafted card \
         pool. (TRM 3.3)",
    ),
];

/// Tournament deck rules the data lists nowhere else: the Asia floor rules' Newtype Challenge
/// clause (`data/official_raw/asia_pdf_floor-rule-forasia.txt:312-316`).
const ASIA_NEW_CARD_RULE: &str = "the Asia floor rules' 7-day new-card rule for Newtype \
     Challenge and other sanctioned events (BANDAI CARD GAMES Floor Rules Ver.1.1.1)";

/// The scope statement of the validator.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Scope {
    /// `banlist.json` `format`.
    pub banlist_format: Box<str>,
    /// `banlist.json` `applies_to`.
    pub banlist_applies_to: Box<str>,
    /// `deck_construction.json` `tournament_only_rules_not_modelled`, then the Asia clause and
    /// the sideboard scope of the B&R list.
    pub unchecked_rules: Box<[Box<str>]>,
    /// `bo3_match_rules.json` `ignored_by_simulator`, in file order.
    pub unmodelled_procedures: Box<[(Box<str>, Box<str>)]>,
    /// `edition_language.json` `card_numbers_only_printed_in_edition_beta`.
    pub edition_beta_only: Box<[Box<str>]>,
    /// `edition_language.json` `official_legality_statement_found`.
    pub edition_beta_statement_found: bool,
}

fn at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, k| py::get(v, k))
}

fn missing(file: &str, what: &str, path: &[&str]) -> DataError {
    DataError::Shape {
        file: file.into(),
        detail: format!("no {what} {}", path.join(".")),
    }
}

fn str_at(file: &str, v: &Value, path: &[&str]) -> Result<Box<str>, DataError> {
    at(v, path)
        .and_then(Value::as_str)
        .map(Into::into)
        .ok_or_else(|| missing(file, "string", path))
}

fn strs_at(file: &str, v: &Value, path: &[&str]) -> Result<Vec<Box<str>>, DataError> {
    Ok(at(v, path)
        .and_then(Value::as_array)
        .ok_or_else(|| missing(file, "list", path))?
        .iter()
        .filter_map(Value::as_str)
        .map(Into::into)
        .collect())
}

impl Scope {
    /// The scope of the packaged data, with `edition_language_json` the text of
    /// `official/edition_language.json`.
    ///
    /// # Errors
    /// [`DataError::Json`] when that text is not JSON, [`DataError::Shape`] when a file lacks a
    /// field read here.
    pub fn new(edition_language_json: &str) -> Result<Self, DataError> {
        let construction =
            embedded::json("deck_construction.json", embedded::DECK_CONSTRUCTION_JSON);
        let banlist = embedded::json("banlist.json", embedded::BANLIST_JSON);
        let bo3 = embedded::json("bo3_match_rules.json", embedded::BO3_MATCH_RULES_JSON);
        let edition: Value =
            serde_json::from_str(edition_language_json).map_err(|e| DataError::Json {
                file: EDITION_FILE.into(),
                line: 0,
                message: e.to_string(),
            })?;
        let banlist_file = "official/banlist.json";
        let applies_to = str_at(banlist_file, &banlist, &["applies_to"])?;
        let mut unchecked = strs_at(
            "official/deck_construction.json",
            &construction,
            &["tournament_only_rules_not_modelled"],
        )?;
        unchecked.push(ASIA_NEW_CARD_RULE.into());
        unchecked.push(
            format!("sideboards: the Banned & Restricted list applies to the {applies_to}").into(),
        );
        let bo3_file = "official/bo3_match_rules.json";
        let procedures = at(&bo3, &["ignored_by_simulator"])
            .and_then(Value::as_object)
            .ok_or_else(|| missing(bo3_file, "object", &["ignored_by_simulator"]))?
            .iter()
            .map(|(k, v)| (k.as_str().into(), py::str_of(v).into()))
            .collect();
        let mut beta = strs_at(
            EDITION_FILE,
            &edition,
            &[
                "edition_beta",
                "card_data_facts",
                "card_numbers_only_printed_in_edition_beta",
            ],
        )?;
        beta.sort();
        let statement = ["edition_beta", "official_legality_statement_found"];
        Ok(Self {
            banlist_format: str_at(banlist_file, &banlist, &["format"])?,
            banlist_applies_to: applies_to,
            unchecked_rules: unchecked.into(),
            unmodelled_procedures: procedures,
            edition_beta_only: beta.into(),
            edition_beta_statement_found: at(&edition, &statement)
                .map(py::truthy)
                .ok_or_else(|| missing(EDITION_FILE, "value", &statement))?,
        })
    }

    /// The deck's card numbers that exist only as Edition Beta printings, sorted.
    #[must_use]
    pub fn edition_beta_only_in<'d>(&self, deck: &'d Deck) -> Vec<&'d str> {
        let mut out: Vec<&str> = deck
            .main
            .iter()
            .chain(deck.resources.iter())
            .map(|c| &*c.card_number)
            .filter(|n| self.edition_beta_only.iter().any(|b| &**b == *n))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The `scope` block for `analysis.json` (W-75, W-76): the format applied, the list
    /// applied (`None`: none, as with `--no-banlist`), the unsupported formats, the unchecked
    /// tournament rules, the unmodelled tournament procedures and the Edition Beta facts.
    #[must_use]
    pub fn to_json(&self, list: Option<&BanlistEntry>) -> Json {
        let mut banlist = Json::object();
        banlist.set("applied", list.is_some());
        banlist.set(
            "effective_date",
            list.map(|e| e.list.effective_date.to_string()),
        );
        banlist.set(
            "regions",
            Json::Array(
                list.map_or(&[][..], |e| &e.regions[..])
                    .iter()
                    .map(|r| Json::from(&**r))
                    .collect(),
            ),
        );
        banlist.set("format", &*self.banlist_format);
        let mut formats = Vec::new();
        for (name, description) in UNSUPPORTED_FORMATS {
            let mut f = Json::object();
            f.set("name", name);
            f.set("description", description);
            formats.push(f);
        }
        let mut procedures = Json::object();
        for (k, v) in self.unmodelled_procedures.iter() {
            procedures.set(&**k, &**v);
        }
        let mut beta = Json::object();
        beta.set("validated_by", "card number");
        beta.set(
            "official_legality_statement_found",
            self.edition_beta_statement_found,
        );
        beta.set(
            "card_numbers_only_in_edition_beta",
            Json::Array(
                self.edition_beta_only
                    .iter()
                    .map(|n| Json::from(&**n))
                    .collect(),
            ),
        );
        let mut out = Json::object();
        out.set("format", FORMAT_APPLIED);
        out.set("banlist", banlist);
        out.set("unsupported_formats", Json::Array(formats));
        out.set(
            "unchecked_tournament_rules",
            Json::Array(
                self.unchecked_rules
                    .iter()
                    .map(|r| Json::from(&**r))
                    .collect(),
            ),
        );
        out.set("unmodelled_tournament_procedures", procedures);
        out.set("edition_beta", beta);
        out
    }
}
