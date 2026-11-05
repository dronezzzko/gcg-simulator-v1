//! Card-text helpers of `cards/model.py` and `cards/tokens.py`, with each regular expression
//! ported to an explicit matcher that follows Python `re`'s backtracking order (Inv 03 §10
//! H19; no regex crate). Python's `\s` and `\d` are the Unicode classes of [`crate::unicode`].

use crate::contracts::{CardDef, CardType, LinkCondition};
use crate::unicode;

impl CardDef {
    /// `CardDef.is_vanilla` (`cards/model.py:139-141`): `effect.strip() in ("", "-")`.
    #[must_use]
    pub fn is_vanilla(&self) -> bool {
        matches!(unicode::strip(&self.effect), "" | "-")
    }
}

/// `CardDef.names` (`cards/model.py:117-123`): `(name, *extra_names)`, then `pilot_name` if it
/// is truthy and not already present.
pub(crate) fn names_of(
    name: &str,
    extra_names: &[Box<str>],
    pilot_name: Option<&str>,
) -> Box<[Box<str>]> {
    let mut out: Vec<Box<str>> = Vec::with_capacity(2 + extra_names.len());
    out.push(name.into());
    out.extend(extra_names.iter().cloned());
    if let Some(p) = pilot_name.filter(|p| !p.is_empty())
        && !out.iter().any(|n| &**n == p)
    {
        out.push(p.into());
    }
    out.into_boxed_slice()
}

/// `CardType.is_unit` as `_make_def` uses it for links (`cards/db.py:176`).
pub(crate) const fn has_link(t: CardType) -> bool {
    t.is_unit()
}

/// Byte spans `(start, end)` of the groups of `open ([^close]+|[^close]*) close` matches in
/// `s`, left to right and non-overlapping (`re.findall`); `allow_empty` selects `*` over `+`.
fn delimited(s: &str, open: char, close: char, allow_empty: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = s[from..].find(open) {
        let start = from + rel + open.len_utf8();
        let Some(len) = s[start..].find(close) else {
            break;
        };
        if len == 0 && !allow_empty {
            from = start;
            continue;
        }
        out.push((start, start + len));
        from = start + len + close.len_utf8();
    }
    out
}

/// `parse_link(raw)` (`cards/model.py:59-74`). `Err(())` is
/// `ValueError("unparseable link condition: ...")`.
pub(crate) fn parse_link(raw: Option<&str>) -> Result<Option<LinkCondition>, ()> {
    let Some(raw) = raw else { return Ok(None) };
    if matches!(unicode::strip(raw), "" | "-") {
        return Ok(None);
    }
    let name_spans = delimited(raw, '[', ']', false);
    let names: Vec<Box<str>> = name_spans
        .iter()
        .map(|&(a, b)| unicode::strip(&raw[a..b]).into())
        .collect();
    // _LINK_NAME.sub("", raw): drop every "[...]" match, brackets included.
    let mut rest = String::with_capacity(raw.len());
    let mut last = 0;
    for &(a, b) in &name_spans {
        rest.push_str(&raw[last..a - 1]);
        last = b + 1;
    }
    rest.push_str(&raw[last..]);
    let traits: Vec<Box<str>> = delimited(&rest, '(', ')', false)
        .iter()
        .map(|&(a, b)| unicode::strip(&rest[a..b]).into())
        .collect();
    if names.is_empty() && traits.is_empty() {
        return Err(());
    }
    Ok(Some(LinkCondition {
        name_parts: names.into(),
        traits: traits.into(),
    }))
}

/// `parse_modifier(raw)` (`cards/model.py:77-81`): `+1` → 1; absent, empty or `-` → 0.
/// `Err(())` where `int(...)` raises (or the value does not fit an `i32`).
pub(crate) fn parse_modifier(raw: Option<&str>) -> Result<i32, ()> {
    let Some(raw) = raw else { return Ok(0) };
    let t = unicode::strip(raw);
    if matches!(t, "" | "-") {
        return Ok(0);
    }
    let value = crate::py::int_of_str(&t.replace('+', "")).ok_or(())?;
    i32::try_from(value).map_err(|_| ())
}

const PILOT_MARK: &str = "【Pilot】";

/// `parse_command_pilot_name(effect)` (`cards/model.py:84-89`): the first match of
/// `【Pilot】\s*\[([^\]]+)\]`, group 1 stripped.
pub(crate) fn parse_command_pilot_name(effect: &str) -> Option<String> {
    let mut from = 0;
    while let Some(rel) = effect[from..].find(PILOT_MARK) {
        let at = from + rel;
        from = at + PILOT_MARK.len();
        let after = effect[from..].trim_start_matches(unicode::is_space);
        let Some(inner) = after.strip_prefix('[') else {
            continue;
        };
        match inner.find(']') {
            Some(len) if len > 0 => return Some(unicode::strip(&inner[..len]).to_owned()),
            _ => {}
        }
    }
    None
}

/// `_QUOTED.search(text)` (`deck/official.py:93-98`): the first `"([^"]+)"` group.
pub(crate) fn first_quoted(text: &str) -> Option<&str> {
    let quotes: Vec<usize> = text.match_indices('"').map(|(i, _)| i).collect();
    quotes
        .windows(2)
        .find(|w| w[1] > w[0] + 1)
        .map(|w| &text[w[0] + 1..w[1]])
}

/// `_AS_UNIT.search(effect)` (`cards/db.py:47`): `deploy it as an? \(AP(\d+)[･・]HP(\d+)\) Unit`;
/// the two digit groups as written.
pub(crate) fn as_unit(effect: &str) -> Option<(&str, &str)> {
    const HEAD: &str = "deploy it as a";
    let mut from = 0;
    while let Some(rel) = effect[from..].find(HEAD) {
        let at = from + rel;
        from = at + 1;
        let after = &effect[at + HEAD.len()..];
        let rest = if after.starts_with("n (AP") {
            &after[1..]
        } else {
            after
        };
        let Some(rest) = rest.strip_prefix(" (AP") else {
            continue;
        };
        let (ap, rest) = digits(rest);
        if ap.is_empty() {
            continue;
        }
        let Some(rest) = strip_dot(rest) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix("HP") else {
            continue;
        };
        let (hp, rest) = digits(rest);
        if !hp.is_empty() && rest.starts_with(") Unit") {
            return Some((ap, hp));
        }
    }
    None
}

/// The longest prefix of Python `\d` characters, and the rest.
fn digits(s: &str) -> (&str, &str) {
    let end = s
        .char_indices()
        .find(|&(_, c)| unicode::decimal_value(c).is_none())
        .map_or(s.len(), |(i, _)| i);
    s.split_at(end)
}

fn is_dot(c: char) -> bool {
    matches!(c, '･' | '・')
}

/// `[･・]` at the start of `s`.
fn strip_dot(s: &str) -> Option<&str> {
    let c = s.chars().next()?;
    is_dot(c).then(|| &s[c.len_utf8()..])
}

/// `int()` of a run of Python `\d` digits, as an `i32`.
pub(crate) fn digits_value(s: &str) -> Option<i32> {
    i32::try_from(crate::py::int_of_str(s)?).ok()
}

// --- inline token definitions (cards/tokens.py) ------------------------------------------------

/// `TokenSpec(name, traits, ap, hp, text)` (`cards/tokens.py:18-29`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenSpec {
    pub(crate) name: String,
    pub(crate) traits: Vec<Box<str>>,
    pub(crate) ap: i32,
    pub(crate) hp: i32,
    pub(crate) text: String,
}

impl TokenSpec {
    /// `TokenSpec.key` (`cards/tokens.py:26-29`): `name|traits joined by /|ap|hp|text`.
    pub(crate) fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.name,
            self.traits.join("/"),
            self.ap,
            self.hp,
            self.text
        )
    }
}

/// `normalize_token_text(text)` (`cards/tokens.py:32-35`).
pub(crate) fn normalize_token_text(text: &str) -> String {
    let replaced = text.replace('・', "･");
    let joined = unicode::split_whitespace(&replaced).join(" ");
    let mut out = unicode::strip(&joined).trim_end_matches('.').to_owned();
    if !unicode::strip(text).is_empty() {
        out.push('.');
    }
    out
}

/// One `TOKEN_DEF` match: byte spans of the groups and the end of the match.
struct TokenMatch {
    name: (usize, usize),
    traits: (usize, usize),
    ap: (usize, usize),
    hp: (usize, usize),
    text: Option<(usize, usize)>,
    end: usize,
}

/// A cursor over `s` that steps by characters and reports byte offsets.
struct Scan<'a> {
    s: &'a str,
}

impl Scan<'_> {
    fn at(&self, i: usize) -> Option<char> {
        self.s[i..].chars().next()
    }

    fn skip_while(&self, mut i: usize, pred: impl Fn(char) -> bool) -> usize {
        while let Some(c) = self.at(i) {
            if !pred(c) {
                break;
            }
            i += c.len_utf8();
        }
        i
    }

    fn eat(&self, i: usize, c: char) -> Option<usize> {
        (self.at(i) == Some(c)).then(|| i + c.len_utf8())
    }

    fn eat_str(&self, i: usize, lit: &str) -> Option<usize> {
        self.s[i..].starts_with(lit).then(|| i + lit.len())
    }

    fn eat_dot(&self, i: usize) -> Option<usize> {
        let c = self.at(i)?;
        is_dot(c).then(|| i + c.len_utf8())
    }

    fn ws(&self, i: usize) -> usize {
        self.skip_while(i, unicode::is_space)
    }

    fn digits(&self, i: usize) -> Option<(usize, usize)> {
        let end = self.skip_while(i, |c| unicode::decimal_value(c).is_some());
        (end > i).then_some((i, end))
    }

    /// `[^()]*`
    fn plain(&self, i: usize) -> usize {
        self.skip_while(i, |c| c != '(' && c != ')')
    }

    /// The tail of `TOKEN_DEF` after the closing `)` of the traits:
    /// `\s*[･・]\s*AP(\d+)\s*[･・]\s*HP(\d+)(?:\s*[･・]\s*(text))?\)`. The returned match
    /// has empty name and traits spans; the caller fills them in.
    fn tail(&self, i: usize) -> Option<TokenMatch> {
        let i = self.eat_dot(self.ws(i))?;
        let i = self.eat_str(self.ws(i), "AP")?;
        let ap = self.digits(i)?;
        let i = self.eat_dot(self.ws(ap.1))?;
        let i = self.eat_str(self.ws(i), "HP")?;
        let hp = self.digits(i)?;
        let (text, end) = match self.text_group(hp.1) {
            Some((text, end)) => (Some(text), end),
            None => (None, self.eat(hp.1, ')')?),
        };
        Some(TokenMatch {
            name: (0, 0),
            traits: (0, 0),
            ap,
            hp,
            text,
            end,
        })
    }

    /// `\s*[･・]\s*(?P<text>[^()]*(?:\([^()]*\)[^()]*)*)` followed by `\)`; the text span and
    /// the end after the `)`. The greedy `\s*` before the text is maximal in the first
    /// successful attempt (whitespace given back would only move into the text's `[^()]*`).
    fn text_group(&self, i: usize) -> Option<((usize, usize), usize)> {
        let i = self.eat_dot(self.ws(i))?;
        let start = self.ws(i);
        let mut j = self.plain(start);
        while self.at(j) == Some('(') {
            let k = self.plain(j + 1);
            if self.at(k) != Some(')') {
                break;
            }
            j = self.plain(k + 1);
        }
        let end = self.eat(j, ')')?;
        Some(((start, j), end))
    }

    /// `TOKEN_DEF.match(s, p)` (`cards/tokens.py:12-15`).
    fn token_at(&self, p: usize) -> Option<TokenMatch> {
        let name_start = self.eat(p, '[')?;
        let name_end = self.skip_while(name_start, |c| c != '[' && c != ']');
        if name_end == name_start {
            return None;
        }
        let i = self.eat(name_end, ']')?;
        let traits_start = self.eat(self.eat(i, '(')?, '(')?;
        // `[^)]*(?:\)\s*\([^)]*)*` then `\)`: each candidate ends at a `)`; the greedy loop
        // tries the most iterations first.
        let mut ends = Vec::new();
        let first = self.skip_while(traits_start, |c| c != ')');
        if self.at(first) != Some(')') {
            return None;
        }
        ends.push(first);
        loop {
            let last = ends[ends.len() - 1];
            let Some(open) = self.eat(self.ws(last + 1), '(') else {
                break;
            };
            let close = self.skip_while(open, |c| c != ')');
            if self.at(close) != Some(')') {
                break;
            }
            ends.push(close);
        }
        ends.iter().rev().find_map(|&close| {
            self.tail(close + 1).map(|m| TokenMatch {
                name: (name_start, name_end),
                traits: (traits_start, close),
                ..m
            })
        })
    }
}

/// `parse_token_specs(effect)` (`cards/tokens.py:38-52`): every `TOKEN_DEF.finditer` match in
/// order. `Err(())` where `int()` of AP or HP does not fit an `i32`.
pub(crate) fn parse_token_specs(effect: &str) -> Result<Vec<TokenSpec>, ()> {
    let scan = Scan { s: effect };
    let mut out = Vec::new();
    let mut p = 0;
    while p < effect.len() {
        if let Some(m) = scan.token_at(p) {
            let traits_group = format!("({})", &effect[m.traits.0..m.traits.1]);
            let traits: Vec<Box<str>> = delimited(&traits_group, '(', ')', true)
                .iter()
                .map(|&(a, b)| unicode::strip(&traits_group[a..b]))
                .filter(|t| !t.is_empty())
                .map(Into::into)
                .collect();
            let text = m.text.map_or("", |(a, b)| unicode::strip(&effect[a..b]));
            out.push(TokenSpec {
                name: unicode::strip(&effect[m.name.0..m.name.1]).to_owned(),
                traits,
                ap: digits_value(&effect[m.ap.0..m.ap.1]).ok_or(())?,
                hp: digits_value(&effect[m.hp.0..m.hp.1]).ok_or(())?,
                text: if text.is_empty() {
                    String::new()
                } else {
                    normalize_token_text(text)
                },
            });
            p = m.end;
        } else {
            p += effect[p..].chars().next().map_or(1, char::len_utf8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let l = parse_link(Some("[Amuro Ray]")).unwrap().unwrap();
        assert_eq!(&*l.name_parts, [Box::from("Amuro Ray")]);
        assert!(l.traits.is_empty());
        let l = parse_link(Some("(Zeon) Trait")).unwrap().unwrap();
        assert!(l.name_parts.is_empty());
        assert_eq!(&*l.traits, [Box::from("Zeon")]);
        let l = parse_link(Some("[Allelujah Haptism] / [Hallelujah Haptism]"))
            .unwrap()
            .unwrap();
        assert_eq!(l.name_parts.len(), 2);
        assert_eq!(parse_link(Some(" - ")), Ok(None));
        assert_eq!(parse_link(None), Ok(None));
        assert_eq!(parse_link(Some("[] ()")), Err(()));
        let l = parse_link(Some("[A (x)] (B)")).unwrap().unwrap();
        assert_eq!(&*l.traits, [Box::from("B")]);
    }

    #[test]
    fn modifiers_and_pilots() {
        assert_eq!(parse_modifier(Some("+2")), Ok(2));
        assert_eq!(parse_modifier(Some("1")), Ok(1));
        assert_eq!(parse_modifier(Some(" - ")), Ok(0));
        assert_eq!(parse_modifier(None), Ok(0));
        assert_eq!(parse_modifier(Some("x")), Err(()));
        assert_eq!(
            parse_command_pilot_name("【Pilot】 [ Kira Yamato ]・x"),
            Some("Kira Yamato".to_owned())
        );
        assert_eq!(
            parse_command_pilot_name("【Pilot】[] 【Pilot】[B]"),
            Some("B".into())
        );
        assert_eq!(parse_command_pilot_name("【Pilot】x[B]"), None);
    }

    #[test]
    fn token_specs() {
        let specs = parse_token_specs(
            "Deploy 1 [Zaku Ⅱ]((Zeon)･AP1･HP1) token and 1 [Bit / Funnel]((Long-Range \
             Weapon)･AP2･HP2･This Unit can't be paired with a Pilot or attack) token.",
        )
        .unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].key(), "Zaku Ⅱ|Zeon|1|1|");
        assert_eq!(
            specs[1].key(),
            "Bit / Funnel|Long-Range Weapon|2|2|This Unit can't be paired with a Pilot or attack."
        );
        let specs = parse_token_specs("[GOOhN]((ZAFT) (Marine)・AP１・HP1・a・b (c) d..)").unwrap();
        assert_eq!(specs[0].key(), "GOOhN|ZAFT/Marine|1|1|a･b (c) d.");
        assert!(parse_token_specs("[X]((Y)･AP1･HP)").unwrap().is_empty());
    }

    /// `parse_token_specs` on every card effect with an inline token and on the edge cases of
    /// `export_data.py` (`carddefs.json`): keys, AP and HP equal Python's.
    #[test]
    fn token_spec_vectors_equal_python() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/carddefs.json");
        let text = std::fs::read_to_string(path).expect("carddefs.json (export_data.py)");
        let fx: serde_json::Value = serde_json::from_str(&text).expect("carddefs.json parses");
        let rows = fx["token_specs"].as_array().expect("token_specs");
        assert!(rows.len() > 40);
        for row in rows {
            let effect = row[0].as_str().expect("effect");
            let got: Vec<serde_json::Value> = parse_token_specs(effect)
                .expect("AP/HP fit")
                .iter()
                .map(|s| serde_json::json!([s.key(), s.ap, s.hp]))
                .collect();
            assert_eq!(serde_json::Value::from(got), row[1], "{effect:?}");
        }
    }

    #[test]
    fn unit_variant_and_quotes() {
        assert_eq!(
            as_unit("you may deploy it as an (AP3･HP3) Unit"),
            Some(("3", "3"))
        );
        assert_eq!(
            as_unit("deploy it as a (AP03・HP1) Unit"),
            Some(("03", "1"))
        );
        assert_eq!(as_unit("deploy it as an (AP3･HP3) unit"), None);
        assert_eq!(first_quoted(r#"a "" b "c""#), Some(" b "));
        assert_eq!(first_quoted(r#"x "y""#), Some("y"));
        assert_eq!(first_quoted("none"), None);
    }
}
