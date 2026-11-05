//! The subset of Python `re` that the report code needs: the results schema's `pattern`s
//! (`re.search`, jsonschema 4.26) and the stale-replay matcher `REPLAY_FILE` (`re.match`,
//! `reports/writer.py:20`). No regex crate is in the dependency list (PLAN A8).
//!
//! Supported: literals, `\` escapes of punctuation, `\d` (any Unicode decimal digit, Python's
//! `str` semantics), `.` (any character but `\n`), classes `[...]` / `[^...]` with ranges and
//! `\d`, groups `(...)` / `(?:...)`, `|`, the greedy quantifiers `*`, `+`, `?`, `{n}`, `{n,}`
//! and `{n,m}`, and the anchors `^` (string start) and `$` (string end, or before a final
//! `\n`). Anything else is a compile error, so an unsupported pattern can never be matched
//! with different semantics. Matching is backtracking over `char`s, like Python's engine.

use std::fmt;

/// Python's `\d` for `str` patterns: the Unicode decimal digits (category `Nd`), as Python
/// 3.15's `re` matches them (checked against `stats.json` `digit_ranges`, exported by
/// `oracle/export_records.py`).
pub const DIGIT_RANGES: [(u32, u32); 72] = [
    (0x30, 0x39),
    (0x660, 0x669),
    (0x6F0, 0x6F9),
    (0x7C0, 0x7C9),
    (0x966, 0x96F),
    (0x9E6, 0x9EF),
    (0xA66, 0xA6F),
    (0xAE6, 0xAEF),
    (0xB66, 0xB6F),
    (0xBE6, 0xBEF),
    (0xC66, 0xC6F),
    (0xCE6, 0xCEF),
    (0xD66, 0xD6F),
    (0xDE6, 0xDEF),
    (0xE50, 0xE59),
    (0xED0, 0xED9),
    (0xF20, 0xF29),
    (0x1040, 0x1049),
    (0x1090, 0x1099),
    (0x17E0, 0x17E9),
    (0x1810, 0x1819),
    (0x1946, 0x194F),
    (0x19D0, 0x19D9),
    (0x1A80, 0x1A89),
    (0x1A90, 0x1A99),
    (0x1B50, 0x1B59),
    (0x1BB0, 0x1BB9),
    (0x1C40, 0x1C49),
    (0x1C50, 0x1C59),
    (0xA620, 0xA629),
    (0xA8D0, 0xA8D9),
    (0xA900, 0xA909),
    (0xA9D0, 0xA9D9),
    (0xA9F0, 0xA9F9),
    (0xAA50, 0xAA59),
    (0xABF0, 0xABF9),
    (0xFF10, 0xFF19),
    (0x104A0, 0x104A9),
    (0x10D30, 0x10D39),
    (0x10D40, 0x10D49),
    (0x11066, 0x1106F),
    (0x110F0, 0x110F9),
    (0x11136, 0x1113F),
    (0x111D0, 0x111D9),
    (0x112F0, 0x112F9),
    (0x11450, 0x11459),
    (0x114D0, 0x114D9),
    (0x11650, 0x11659),
    (0x116C0, 0x116C9),
    (0x116D0, 0x116E3),
    (0x11730, 0x11739),
    (0x118E0, 0x118E9),
    (0x11950, 0x11959),
    (0x11BF0, 0x11BF9),
    (0x11C50, 0x11C59),
    (0x11D50, 0x11D59),
    (0x11DA0, 0x11DA9),
    (0x11DE0, 0x11DE9),
    (0x11F50, 0x11F59),
    (0x16130, 0x16139),
    (0x16A60, 0x16A69),
    (0x16AC0, 0x16AC9),
    (0x16B50, 0x16B59),
    (0x16D70, 0x16D79),
    (0x1CCF0, 0x1CCF9),
    (0x1D7CE, 0x1D7FF),
    (0x1E140, 0x1E149),
    (0x1E2F0, 0x1E2F9),
    (0x1E4F0, 0x1E4F9),
    (0x1E5F1, 0x1E5FA),
    (0x1E950, 0x1E959),
    (0x1FBF0, 0x1FBF9),
];

/// `\d` on one character.
#[must_use]
pub fn is_decimal_digit(c: char) -> bool {
    let cp = u32::from(c);
    DIGIT_RANGES.iter().any(|&(lo, hi)| lo <= cp && cp <= hi)
}

/// A pattern outside the supported subset, or a malformed one.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PatternError {
    pub pattern: String,
    pub message: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported pattern {:?}: {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for PatternError {}

#[derive(Clone, Debug)]
enum ClassItem {
    Range(char, char),
    Digit,
}

#[derive(Clone, Debug)]
enum Node {
    Char(char),
    Any,
    Digit,
    Class {
        items: Vec<ClassItem>,
        negated: bool,
    },
    Start,
    End,
    Concat(Vec<Self>),
    Alt(Vec<Self>),
    Repeat {
        node: Box<Self>,
        min: u32,
        max: Option<u32>,
    },
}

/// A compiled pattern.
#[derive(Clone, Debug)]
pub struct Regex {
    root: Node,
}

struct Parser<'a> {
    pattern: &'a str,
    chars: Vec<char>,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> PatternError {
        PatternError {
            pattern: self.pattern.to_owned(),
            message: format!("{message} at offset {}", self.pos),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn alternation(&mut self) -> Result<Node, PatternError> {
        let mut alts = vec![self.concat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            alts.push(self.concat()?);
        }
        Ok(if alts.len() == 1 {
            alts.remove(0)
        } else {
            Node::Alt(alts)
        })
    }

    fn concat(&mut self) -> Result<Node, PatternError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quantified(atom)?);
        }
        Ok(Node::Concat(items))
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse().ok()
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, PatternError> {
        let (min, max) = match self.peek() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => {
                let save = self.pos;
                self.pos += 1;
                let Some(min) = self.number() else {
                    return Err(self.error("a literal '{' or a '{,m}' quantifier"));
                };
                let max = if self.peek() == Some(',') {
                    self.pos += 1;
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(
                            self.number()
                                .ok_or_else(|| self.error("a bad {n,m} quantifier"))?,
                        )
                    }
                } else {
                    Some(min)
                };
                if self.peek() != Some('}') {
                    self.pos = save;
                    return Err(self.error("an unterminated {n,m} quantifier"));
                }
                if max.is_some_and(|m| m < min) {
                    return Err(self.error("a {n,m} quantifier with m < n"));
                }
                (min, max)
            }
            _ => return Ok(atom),
        };
        self.pos += 1;
        if matches!(self.peek(), Some('?' | '+' | '*' | '{')) {
            return Err(self.error("a lazy, possessive or repeated quantifier"));
        }
        if matches!(atom, Node::Start | Node::End) {
            return Err(self.error("a quantified anchor"));
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
        })
    }

    fn escape(&mut self) -> Result<Node, PatternError> {
        let Some(c) = self.peek() else {
            return Err(self.error("a trailing backslash"));
        };
        self.pos += 1;
        if c == 'd' {
            return Ok(Node::Digit);
        }
        if c.is_ascii_alphanumeric() {
            return Err(self.error("an escape class or sequence"));
        }
        Ok(Node::Char(c))
    }

    fn class(&mut self) -> Result<Node, PatternError> {
        let negated = self.peek() == Some('^');
        if negated {
            self.pos += 1;
        }
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                return Err(self.error("an unterminated class"));
            };
            if c == ']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            self.pos += 1;
            let lo = if c == '\\' {
                let Some(e) = self.peek() else {
                    return Err(self.error("a trailing backslash"));
                };
                self.pos += 1;
                if e == 'd' {
                    items.push(ClassItem::Digit);
                    continue;
                }
                if e.is_ascii_alphanumeric() {
                    return Err(self.error("an escape inside a class"));
                }
                e
            } else if c == '[' {
                return Err(self.error("a nested class or set operation"));
            } else {
                c
            };
            if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|&n| n != ']') {
                self.pos += 1;
                let Some(hi) = self.peek() else {
                    return Err(self.error("an unterminated range"));
                };
                self.pos += 1;
                if hi == '\\' || hi < lo {
                    return Err(self.error("a bad range"));
                }
                items.push(ClassItem::Range(lo, hi));
            } else {
                items.push(ClassItem::Range(lo, lo));
            }
        }
        Ok(Node::Class { items, negated })
    }

    fn atom(&mut self) -> Result<Node, PatternError> {
        let c = self.peek().ok_or_else(|| self.error("an empty atom"))?;
        self.pos += 1;
        match c {
            '^' => Ok(Node::Start),
            '$' => Ok(Node::End),
            '.' => Ok(Node::Any),
            '\\' => self.escape(),
            '[' => self.class(),
            '(' => {
                if self.peek() == Some('?') {
                    if self.chars.get(self.pos + 1) == Some(&':') {
                        self.pos += 2;
                    } else {
                        return Err(self.error("a group extension"));
                    }
                }
                let inner = self.alternation()?;
                if self.peek() != Some(')') {
                    return Err(self.error("an unbalanced parenthesis"));
                }
                self.pos += 1;
                Ok(inner)
            }
            ')' => Err(self.error("an unbalanced parenthesis")),
            '*' | '+' | '?' | '{' => Err(self.error("nothing to repeat")),
            _ => Ok(Node::Char(c)),
        }
    }
}

fn class_matches(items: &[ClassItem], negated: bool, c: char) -> bool {
    let hit = items.iter().any(|item| match *item {
        ClassItem::Range(lo, hi) => lo <= c && c <= hi,
        ClassItem::Digit => is_decimal_digit(c),
    });
    hit != negated
}

fn match_node(node: &Node, text: &[char], pos: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
    let one = |ok: bool, k: &mut dyn FnMut(usize) -> bool| ok && k(pos + 1);
    match node {
        Node::Char(c) => one(text.get(pos) == Some(c), k),
        Node::Any => one(text.get(pos).is_some_and(|&c| c != '\n'), k),
        Node::Digit => one(text.get(pos).is_some_and(|&c| is_decimal_digit(c)), k),
        Node::Class { items, negated } => one(
            text.get(pos)
                .is_some_and(|&c| class_matches(items, *negated, c)),
            k,
        ),
        Node::Start => pos == 0 && k(pos),
        Node::End => (pos == text.len() || (pos + 1 == text.len() && text[pos] == '\n')) && k(pos),
        Node::Concat(items) => match_seq(items, text, pos, k),
        Node::Alt(alts) => alts.iter().any(|a| match_node(a, text, pos, k)),
        Node::Repeat { node, min, max } => match_repeat(node, *min, *max, 0, text, pos, k),
    }
}

fn match_seq(items: &[Node], text: &[char], pos: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
    match items.split_first() {
        None => k(pos),
        Some((first, rest)) => match_node(first, text, pos, &mut |p| match_seq(rest, text, p, k)),
    }
}

/// Greedy repetition: one more iteration first, then stop once `min` is reached. An
/// iteration that consumes nothing ends the loop, as in Python's engine.
fn match_repeat(
    node: &Node,
    min: u32,
    max: Option<u32>,
    done: u32,
    text: &[char],
    pos: usize,
    k: &mut dyn FnMut(usize) -> bool,
) -> bool {
    if max.is_none_or(|m| done < m) {
        let more = match_node(node, text, pos, &mut |p| {
            if p == pos && done >= min {
                return false;
            }
            match_repeat(node, min, max, done + 1, text, p, k)
        });
        if more {
            return true;
        }
    }
    done >= min && k(pos)
}

impl Regex {
    /// Compiles `pattern`.
    ///
    /// # Errors
    /// [`PatternError`] for syntax outside the supported subset.
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        let mut p = Parser {
            pattern,
            chars: pattern.chars().collect(),
            pos: 0,
        };
        let root = p.alternation()?;
        if p.pos != p.chars.len() {
            return Err(p.error("an unbalanced parenthesis"));
        }
        Ok(Self { root })
    }

    /// `re.search(pattern, text) is not None`.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        (0..=chars.len()).any(|start| match_node(&self.root, &chars, start, &mut |_| true))
    }

    /// `re.match(pattern, text) is not None` (anchored at the start only).
    #[must_use]
    pub fn is_match_at_start(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        match_node(&self.root, &chars, 0, &mut |_| true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_patterns() {
        let hex = Regex::new("^[0-9a-f]{64}$").unwrap();
        let ok = "0123456789abcdef".repeat(4);
        assert!(hex.is_match(&ok));
        assert!(hex.is_match(&format!("{ok}\n")));
        assert!(!hex.is_match(&format!("{ok}\n\n")));
        assert!(!hex.is_match(&ok.to_uppercase()));
        let file = Regex::new(r"^replays/(win|loss|draw)-m[0-9]{4,}-g[1-3]\.json$").unwrap();
        assert!(file.is_match("replays/loss-m12345-g3.json"));
        assert!(!file.is_match("replays/win-m000-g1.json"));
    }

    #[test]
    fn unsupported_syntax_is_rejected() {
        for p in [
            r"\w+", "a*?", "(?=x)", "[[a]]", r"\1", "a{2,1}", "(a", "a)", "*a",
        ] {
            assert!(Regex::new(p).is_err(), "{p}");
        }
    }

    #[test]
    fn search_and_match_differ() {
        let r = Regex::new("b+").unwrap();
        assert!(r.is_match("abbb"));
        assert!(!r.is_match_at_start("abbb"));
        assert!(Regex::new("a|").unwrap().is_match_at_start(""));
        assert!(Regex::new("(a*)*$").unwrap().is_match("aaa"));
    }
}
