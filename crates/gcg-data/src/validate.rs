//! Deck legality (`deck/validate.py`): rules 6-1-1, 2-1-2 and 3-x-2, the Banned & Restricted
//! list, and the simulator's own requirement that every card has an implemented effect, with
//! Python's messages in Python's order. Counts are the run-length totals of
//! [`Deck`] (W-144), so no list is expanded per copy.

use std::collections::BTreeMap;

use crate::contracts::{
    AttributePairRule, Banlist, CardDb, CardDef, ConstructionRules, Deck, DeckError,
    ImplementationStatus, NOT_APPLIED, RuleException, ValidateOptions, Violation, ViolationCode,
};
use crate::{names, official, unicode};

/// The data a validation reads: `get_card_db()`, `construction_rules()` and `banlist()`, plus
/// how the starter-deck exceptions stand for the deck (W-71; Python never applies them).
pub(crate) struct Rules<'a> {
    pub(crate) db: &'a CardDb,
    pub(crate) construction: &'a ConstructionRules,
    pub(crate) banlist: &'a Banlist,
    pub(crate) exceptions: Exceptions<'a>,
}

/// The product exceptions of one deck ([`crate::legality`]): the ids whose official decklist
/// the deck equals (their restriction is skipped), and the `(id, product)` whose decklist the
/// data has but the deck differs from. Both empty is Python's behaviour: no exception applies
/// and every note says it cannot be verified.
#[derive(Clone, Copy, Default)]
pub(crate) struct Exceptions<'a> {
    pub(crate) exempt: &'a [Box<str>],
    pub(crate) differs: &'a [crate::legality::ProductException],
}

impl Exceptions<'_> {
    /// Whether one of `exceptions` exempts the deck from the restriction that lists them.
    fn exempts(&self, exceptions: &[RuleException]) -> bool {
        exceptions.iter().any(|e| self.exempt.contains(&e.id))
    }

    /// `_exception_note(exceptions)` (`deck/validate.py:29-30`); an exception whose decklist
    /// the data has and the deck differs from says so instead.
    fn note(&self, exceptions: &[RuleException]) -> String {
        exceptions
            .iter()
            .map(|e| {
                let id = unicode::repr_str(&e.id);
                match self.differs.iter().find(|d| d.id == e.id) {
                    Some(d) => format!(
                        " Exception {id} is not applied: the deck is not the unmodified official \
                         {} decklist.",
                        d.product
                    ),
                    None => format!(" Exception {id} {NOT_APPLIED}."),
                }
            })
            .collect()
    }
}

impl Rules<'static> {
    fn embedded() -> Self {
        Self {
            db: CardDb::embedded(),
            construction: official::construction_rules(),
            banlist: official::banlist(),
            exceptions: Exceptions::default(),
        }
    }
}

fn violation(code: ViolationCode, message: String, cards: Vec<Box<str>>) -> Violation {
    Violation {
        code,
        message,
        cards: cards.into(),
    }
}

impl Rules<'_> {
    /// `_label(db, number)` (`deck/validate.py:20-22`).
    fn label(&self, number: &str) -> String {
        match self.db.get(number) {
            Some(d) => format!("{number} {}", d.name),
            None => number.to_owned(),
        }
    }

    /// `_labels(db, numbers)` (`deck/validate.py:25-26`).
    fn labels<'s>(&self, numbers: impl IntoIterator<Item = &'s str>) -> String {
        numbers
            .into_iter()
            .map(|n| self.label(n))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `_known(db, deck)` (`deck/validate.py:33-36`): the known definitions with their copies.
    fn known<'d>(&self, cards: &'d [crate::CardCount]) -> Vec<(&'d str, &CardDef, u128)> {
        cards
            .iter()
            .filter_map(|c| {
                self.db
                    .get(&c.card_number)
                    .map(|d| (&*c.card_number, d, c.count))
            })
            .collect()
    }

    /// `check_unknown` (`deck/validate.py:39-44`).
    fn check_unknown(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let mut unknown: Vec<&str> = deck
            .main
            .iter()
            .chain(deck.resources.iter())
            .map(|c| &*c.card_number)
            .filter(|n| self.db.get(n).is_none())
            .collect();
        unknown.sort();
        unknown.dedup();
        for n in unknown {
            out.push(violation(
                ViolationCode::UnknownId,
                format!(
                    "unknown card id {} (not in the card data)",
                    unicode::repr_str(n)
                ),
                vec![n.into()],
            ));
        }
    }

    /// `check_names` (`deck/validate.py:47-60`).
    fn check_names(&self, deck: &Deck, out: &mut Vec<Violation>) {
        for e in deck.entries.iter() {
            let Some(name) = e.name.as_deref() else {
                continue;
            };
            let Some(cdef) = self.db.get(&e.card_number) else {
                continue;
            };
            if names::names_match(name, cdef.names()) {
                continue;
            }
            out.push(violation(
                ViolationCode::NameMismatch,
                format!(
                    "line {}: {} is {} in the card data, not {}",
                    e.line,
                    e.written,
                    unicode::repr_str(&cdef.name),
                    unicode::repr_str(name)
                ),
                vec![e.card_number.clone()],
            ));
        }
    }

    /// `check_sizes` (`deck/validate.py:63-84`).
    fn check_sizes(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let rules = self.construction;
        let main: u128 = deck.main.iter().map(|c| c.count).sum();
        let res: u128 = deck.resources.iter().map(|c| c.count).sum();
        if main != u128::from(rules.main_size) {
            out.push(violation(
                ViolationCode::MainSize,
                format!(
                    "the deck has {main} cards; it must have exactly {} (rule 6-1-1)",
                    rules.main_size
                ),
                Vec::new(),
            ));
        }
        if res != u128::from(rules.resource_size) {
            out.push(violation(
                ViolationCode::ResourceSize,
                format!(
                    "the resource deck has {res} cards; it must have exactly {} Resource cards \
                     (rule 6-1-1)",
                    rules.resource_size
                ),
                Vec::new(),
            ));
        }
    }

    /// `check_card_types` (`deck/validate.py:105-116`): main numbers sorted, then resource
    /// numbers sorted, keyed by the definition's card number.
    fn check_card_types(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let rules = self.construction;
        let main = (&*deck.main, &*rules.main_types, false);
        let resources = (&*deck.resources, &*rules.resource_types, true);
        for (cards, allowed, in_resource_deck) in [main, resources] {
            let mut bad: BTreeMap<&str, &CardDef> = BTreeMap::new();
            for (_, d, _) in self.known(cards) {
                if !allowed.iter().any(|t| &**t == d.card_type.as_str()) {
                    bad.insert(&d.card_number, d);
                }
            }
            for (n, d) in bad {
                out.push(violation(
                    ViolationCode::CardType,
                    type_message(d, in_resource_deck),
                    vec![n.into()],
                ));
            }
        }
    }

    /// `check_copies` (`deck/validate.py:119-130`): main deck only, sorted by number.
    fn check_copies(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let limit = self.construction.max_copies;
        for c in deck.main.iter() {
            if c.count > u128::from(limit) {
                out.push(violation(
                    ViolationCode::CopyLimit,
                    format!(
                        "{} copies of {}; at most {limit} copies of cards with the same card \
                         number (all printings count together) may be in a deck (rules 6-1-1-3, \
                         2-1-2)",
                        c.count,
                        self.label(&c.card_number)
                    ),
                    vec![c.card_number.clone()],
                ));
            }
        }
    }

    /// `check_colors` (`deck/validate.py:133-151`).
    fn check_colors(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let rules = self.construction;
        let main = self.known(&deck.main);
        let mut copies: BTreeMap<&str, u128> = BTreeMap::new();
        for (_, d, n) in &main {
            if let Some(c) = d.color {
                *copies.entry(c.as_str()).or_insert(0) += n;
            }
        }
        let distinct = copies.len();
        let in_range = (rules.min_colors as usize..=rules.max_colors as usize).contains(&distinct);
        if main.is_empty() || in_range {
            return;
        }
        let mut ranked: Vec<(&str, u128)> = copies.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let extra: Vec<&str> = ranked
            .iter()
            .skip(rules.max_colors as usize)
            .map(|(c, _)| *c)
            .collect();
        let mut cards: Vec<&str> = main
            .iter()
            .filter(|(_, d, _)| d.color.is_some_and(|c| extra.contains(&c.as_str())))
            .map(|(_, d, _)| &*d.card_number)
            .collect();
        cards.sort();
        cards.dedup();
        let shown = ranked
            .iter()
            .map(|(c, n)| format!("{c} x{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(violation(
            ViolationCode::Colors,
            format!(
                "the deck uses {distinct} colors ({shown}); a deck must be constructed entirely \
                 using either one or two card colors (rule 6-1-1-2). Cards of the extra \
                 color(s): {}",
                self.labels(cards.iter().copied())
            ),
            cards.into_iter().map(Into::into).collect(),
        ));
    }

    fn main_count(deck: &Deck, number: &str) -> u128 {
        deck.main
            .binary_search_by(|c| (*c.card_number).cmp(number))
            .map_or(0, |i| deck.main[i].count)
    }

    /// `check_banned` (`deck/validate.py:154-165`).
    fn check_banned(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let bl = self.banlist;
        for c in deck.main.iter() {
            if bl.banned.binary_search(&c.card_number).is_ok() {
                out.push(violation(
                    ViolationCode::Banned,
                    format!(
                        "{} is banned: no copies may be in a deck (Banned & Restricted list \
                         effective {})",
                        self.label(&c.card_number),
                        bl.effective_date
                    ),
                    vec![c.card_number.clone()],
                ));
            }
        }
    }

    /// `check_restricted` (`deck/validate.py:168-184`): banlist file order.
    fn check_restricted(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let bl = self.banlist;
        for r in bl.restricted.iter() {
            let count = Self::main_count(deck, &r.card_number);
            if count <= u128::from(r.max_copies) || self.exceptions.exempts(&r.exceptions) {
                continue;
            }
            out.push(violation(
                ViolationCode::Restricted,
                format!(
                    "{count} copies of {}; it is restricted to {} copies (Banned & Restricted \
                     list effective {}).{}",
                    self.label(&r.card_number),
                    r.max_copies,
                    bl.effective_date,
                    self.exceptions.note(&r.exceptions)
                ),
                vec![r.card_number.clone()],
            ));
        }
    }

    /// `check_banned_pairs` (`deck/validate.py:187-199`): banlist file order.
    fn check_banned_pairs(&self, deck: &Deck, out: &mut Vec<Violation>) {
        let bl = self.banlist;
        for p in bl.banned_pairs.iter() {
            if p.cards.iter().all(|n| Self::main_count(deck, n) > 0) {
                out.push(violation(
                    ViolationCode::BannedPair,
                    format!(
                        "{} are a banned pair and cannot be in the same deck (Banned & \
                         Restricted list effective {})",
                        self.labels(p.cards.iter().map(|c| &**c)),
                        bl.effective_date
                    ),
                    p.cards.to_vec(),
                ));
            }
        }
    }

    /// `attribute_members(db, rule, numbers)` (`deck/validate.py:211-218`) over the distinct
    /// main-deck numbers (already sorted).
    fn attribute_members<'d>(&self, rule: &AttributePairRule, deck: &'d Deck) -> Vec<&'d str> {
        deck.main
            .iter()
            .map(|c| &*c.card_number)
            .filter(|n| {
                rule.enumerated.binary_search_by(|e| (**e).cmp(n)).is_ok()
                    || self
                        .db
                        .get(n)
                        .is_some_and(|d| official::matches(&rule.member_predicate, d, None))
            })
            .collect()
    }

    /// `_attribute_violation(db, deck, rule)` (`deck/validate.py:221-240`).
    fn attribute_violation(&self, deck: &Deck, rule: &AttributePairRule) -> Option<Violation> {
        if self.exceptions.exempts(&rule.exceptions) {
            return None;
        }
        let members = self.attribute_members(rule, deck);
        let total: u128 = members.iter().map(|n| Self::main_count(deck, n)).sum();
        if members.len() < 2 && total <= u128::from(rule.max_total_copies) {
            return None;
        }
        let mut note_text = String::new();
        for n in &members {
            let Some(d) = self.db.get(n) else {
                continue;
            };
            let listed = rule.enumerated.binary_search_by(|e| (**e).cmp(n)).is_ok();
            let by_predicate = official::matches(&rule.member_predicate, d, None);
            let note = if by_predicate && !listed {
                format!(
                    "{n} matches the description in the card data but is not on the official list"
                )
            } else if listed && !by_predicate {
                format!(
                    "{n} is on the official list but does not match the description in the data"
                )
            } else {
                continue;
            };
            note_text.push_str(&format!(" Note: {note}."));
        }
        Some(violation(
            ViolationCode::AttributePair,
            format!(
                "{} are each \"{}\"; every combination of two such card numbers is a banned \
                 pair, so a deck may contain only one of them, at most {} copies (Banned & \
                 Restricted list effective {}).{}{note_text}",
                self.labels(members.iter().copied()),
                rule.description,
                rule.max_total_copies,
                self.banlist.effective_date,
                self.exceptions.note(&rule.exceptions)
            ),
            members.into_iter().map(Into::into).collect(),
        ))
    }

    /// `check_implemented` (`deck/validate.py:248-265`): `sorted(set(main) | set(resources))`.
    fn check_implemented(
        &self,
        deck: &Deck,
        implemented: &dyn ImplementationStatus,
        out: &mut Vec<Violation>,
    ) {
        let mut numbers: Vec<&str> = deck
            .main
            .iter()
            .chain(deck.resources.iter())
            .map(|c| &*c.card_number)
            .collect();
        numbers.sort();
        numbers.dedup();
        for n in numbers {
            let Some(d) = self.db.get(n) else {
                continue;
            };
            if let Some(error) = implemented.unimplemented_error(d.def_id) {
                out.push(violation(
                    ViolationCode::Unimplemented,
                    format!(
                        "{} has no implemented effect in this simulator version, so games cannot \
                         use it ({error})",
                        self.label(n)
                    ),
                    vec![n.into()],
                ));
            }
        }
    }

    /// `validate_deck(deck, banlist=..., implemented=...)` (`deck/validate.py:268-289`).
    pub(crate) fn validate(&self, deck: &Deck, opts: ValidateOptions<'_>) -> Vec<Violation> {
        let mut out = Vec::new();
        self.check_unknown(deck, &mut out);
        self.check_names(deck, &mut out);
        self.check_sizes(deck, &mut out);
        self.check_card_types(deck, &mut out);
        self.check_copies(deck, &mut out);
        self.check_colors(deck, &mut out);
        if opts.banlist {
            self.check_banned(deck, &mut out);
            self.check_restricted(deck, &mut out);
            self.check_banned_pairs(deck, &mut out);
            for rule in self.banlist.attribute_rules.iter() {
                out.extend(self.attribute_violation(deck, rule));
            }
        }
        if let Some(status) = opts.implemented {
            self.check_implemented(deck, status, &mut out);
        }
        out
    }
}

/// `_type_message(cdef, in_resource_deck)` (`deck/validate.py:87-102`).
fn type_message(d: &CardDef, in_resource_deck: bool) -> String {
    let kind = d.card_type.as_str();
    if d.is_token() {
        format!(
            "{} {} is a token ({kind}); tokens are prepared outside the game and are never part \
             of a deck or resource deck (rule 6-1-2, FAQ Q7)",
            d.card_number, d.name
        )
    } else if in_resource_deck {
        format!(
            "{} {} is a {kind} card; a resource deck is constructed with Resource cards only \
             (rule 6-1-1-4)",
            d.card_number, d.name
        )
    } else {
        format!(
            "{} {} is a {kind} card; a deck is constructed with Unit, Pilot, Command, and Base \
             cards only (rule 6-1-1-1)",
            d.card_number, d.name
        )
    }
}

/// `validate_deck(deck, *, banlist=True, implemented=True)` (`deck/validate.py:268-289`): every
/// legality problem of `deck` in Python's order; empty means legal. `opts.implemented: None`
/// skips the simulator-support check (`implemented=False`).
#[must_use]
pub fn validate(deck: &Deck, opts: ValidateOptions<'_>) -> Vec<Violation> {
    Rules::embedded().validate(deck, opts)
}

/// `require_legal(deck)` (`deck/validate.py:292-300`).
///
/// # Errors
/// [`DeckError::Invalid`] listing every violation as `[{code}] {message}`.
pub fn require_legal(deck: &Deck, implemented: &dyn ImplementationStatus) -> Result<(), DeckError> {
    let violations = validate(
        deck,
        ValidateOptions {
            banlist: true,
            implemented: Some(implemented),
        },
    );
    if violations.is_empty() {
        return Ok(());
    }
    Err(DeckError::Invalid {
        deck_name: Some(deck.name.clone()),
        problems: violations
            .iter()
            .map(|v| format!("[{}] {}", v.code.as_str(), v.message))
            .collect(),
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    struct AllImplemented;

    impl ImplementationStatus for AllImplemented {
        fn unimplemented_error(&self, _: crate::DefId) -> Option<&str> {
            None
        }
    }

    /// `tests/deck/test_validate.py:345-386`: the ATTRIBUTE_PAIR membership notes, reachable
    /// only with a banlist whose enumeration differs from its predicate, equal Python's on the
    /// patched banlists of `decks.json`.
    #[test]
    fn attribute_notes_equal_python() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/decks.json");
        let text = std::fs::read_to_string(path).expect("decks.json (export_data.py)");
        let fx: Value = serde_json::from_str(&text).expect("decks.json parses");
        let cases = fx["attribute_notes"].as_array().expect("attribute_notes");
        assert_eq!(cases.len(), 2);
        for case in cases {
            let mut banlist = official::banlist().clone();
            banlist.attribute_rules[0].enumerated = case["enumerated"]
                .as_array()
                .expect("enumerated")
                .iter()
                .map(|n| n.as_str().expect("number").into())
                .collect();
            let rules = Rules {
                db: CardDb::embedded(),
                construction: official::construction_rules(),
                banlist: &banlist,
                exceptions: Exceptions::default(),
            };
            let deck = crate::parse_deck(case["text"].as_str().expect("text"), "notes", None)
                .expect("deck parses");
            let opts = ValidateOptions {
                banlist: true,
                implemented: Some(&AllImplemented),
            };
            let got: Vec<Value> = rules
                .validate(&deck, opts)
                .iter()
                .map(|v| serde_json::json!([v.code.as_str(), v.message, v.cards]))
                .collect();
            assert_eq!(Value::from(got), case["validate"], "{}", case["id"]);
        }
    }
}
