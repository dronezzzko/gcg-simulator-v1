//! `gcg-diff api-snapshot` (docs/port/WEAKNESSES.md W-129): a checked listing of the public API
//! of the library crates, read from their sources.
//!
//! Rustdoc's JSON output needs a nightly toolchain, so the listing is read from the sources with
//! comments removed. It holds every `pub` item (exactly `pub`, never `pub(crate)` and the like)
//! of every module that the crate root reaches through `mod` declarations, outside
//! `#[cfg(test)]`. Items of private modules are listed too, because the crate roots re-export
//! most of them with `pub use`: the listing over-approximates the public API and never misses a
//! `pub` item. Per item:
//!
//! - functions and methods: the signature, without the body;
//! - structs and unions: the header and the `pub` fields, with `..` for private fields (a private
//!   tuple field is `_`);
//! - enums: every variant, with its fields and discriminant;
//! - traits: every associated item; `{ .. }` marks a provided method, `= ..` a default;
//! - constants and statics: name and type, without the value; type aliases and `pub use` in full;
//! - inherent `impl` blocks: their `pub` items. Trait `impl` blocks: the header only. Impls of a
//!   type that the crate declares only privately are left out;
//! - `macro_rules!` definitions: the name and the SHA-256 of the body, because a macro may define
//!   items; item-level invocations of the crate's own macros, of `include!`, and of any macro
//!   whose arguments say `pub`, with their arguments.
//!
//! The `derive`, `cfg`, `cfg_attr`, `repr`, `non_exhaustive`, `macro_export` and `doc(hidden)`
//! attributes of an item are kept. Whitespace is normalised, so rustfmt's line breaking never
//! changes the listing. The crates listed are the workspace members that another member depends
//! on ([`api_crates`]). The binaries' crates (`gcg-cli`, `gcg-eval`, `gcg-diff`) are interfaces
//! through their command lines, which `scripts/cli_help_snapshot.py` snapshots.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::json::sha256_hex;
use crate::lints::{strip_comments, strip_comments_and_strings};

/// The committed listing, relative to the repository root.
pub const SNAPSHOT_FILE: &str = "docs/port/api-snapshot.txt";

const HEADER: &str = "\
# Public API of the gcg-sim library crates (docs/port/WEAKNESSES.md W-129).
# Generated from the sources by `gcg-diff api-snapshot --write`; `gcg-diff api-snapshot --check`
# (scripts/verify_all_rust.sh, crates/gcg-diff/tests/api_snapshot.rs) fails on any difference,
# so a changed signature cannot go unrecorded. What is listed: crates/gcg-diff/src/apisnapshot.rs.
";

const KEPT_ATTRS: [&str; 6] = [
    "derive",
    "cfg",
    "cfg_attr",
    "repr",
    "non_exhaustive",
    "macro_export",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Vis {
    Pub,
    Restricted,
    Private,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Fn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    Mod,
    Type,
    Const,
    Static,
    Use,
    ExternCrate,
    ExternBlock,
    MacroRules,
    MacroCall,
}

/// One item: byte offsets into its [`Source`].
struct Item {
    attrs: Vec<String>,
    vis: Vis,
    kind: Kind,
    /// The first byte after the attributes.
    start: usize,
    /// After the kind keyword, or after the `!` of a macro.
    kw_end: usize,
    /// The `{` or `;` that ends the header, or the end of a macro invocation.
    head_end: usize,
    /// Inside the braces (or the delimiters of a macro invocation).
    body: Option<(usize, usize)>,
}

/// Where an item's header ends, its body, and the offset after it.
struct Extent {
    head_end: usize,
    body: Option<(usize, usize)>,
    end: usize,
}

/// A source file in two views with equal offsets: `code` without comments and literals (the
/// structure is read from it) and `shown` without comments (the listed text comes from it).
struct Source {
    rel: String,
    code: Vec<u8>,
    shown: String,
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Whitespace runs become one space, with none just inside brackets, and trailing commas go
/// (before a closing bracket or at the end, as after a vertical `where` clause), so a rustfmt
/// line break never changes the text.
fn norm(text: &str) -> String {
    let mut s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        ("( ", "("),
        (" )", ")"),
        ("[ ", "["),
        (" ]", "]"),
        ("{ ", "{"),
        (" }", "}"),
        ("< ", "<"),
        (", >", ">"),
        (",)", ")"),
        (",]", "]"),
        (",}", "}"),
        (",>", ">"),
    ] {
        while s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s.trim_end_matches(',').to_owned()
}

/// The name of an attribute's path (`derive` for `#[derive(Clone)]`).
fn attr_name(attr: &str) -> &str {
    let inner = attr.trim_start_matches("#[");
    let end = inner
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(inner.len());
    &inner[..end]
}

fn kept_attrs(attrs: &[String]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| {
            let name = attr_name(a);
            KEPT_ATTRS.contains(&name) || (name == "doc" && a.contains("hidden"))
        })
        .cloned()
        .collect()
}

/// The top-level comma-separated parts of `text` (brackets of all three kinds nest).
fn split_commas(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut from = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&text[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[from..]);
    out
}

/// Whether an item with these (normalised) attributes exists only in test builds.
fn is_test_only(attrs: &[String]) -> bool {
    attrs.iter().any(|a| {
        let Some(inner) = a.strip_prefix("#[cfg(").and_then(|r| r.strip_suffix(")]")) else {
            return false;
        };
        inner == "test"
            || inner
                .strip_prefix("all(")
                .and_then(|r| r.strip_suffix(')'))
                .is_some_and(|args| split_commas(args).iter().any(|x| x.trim() == "test"))
    })
}

fn prefixed(attrs: &[String], text: &str) -> String {
    let kept = kept_attrs(attrs);
    if kept.is_empty() {
        text.to_owned()
    } else {
        format!("{} {text}", kept.join(" "))
    }
}

impl Source {
    fn new(rel: &str, text: &str) -> Self {
        Self {
            rel: rel.to_owned(),
            code: strip_comments_and_strings(text).into_bytes(),
            shown: strip_comments(text),
        }
    }

    fn len(&self) -> usize {
        self.code.len()
    }

    fn line(&self, at: usize) -> usize {
        self.code[..at.min(self.code.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1
    }

    fn err(&self, at: usize, what: &str) -> String {
        format!("{}:{}: {what}", self.rel, self.line(at))
    }

    fn shown(&self, lo: usize, hi: usize) -> &str {
        self.shown.get(lo..hi).unwrap_or("")
    }

    fn skip_ws(&self, mut p: usize, hi: usize) -> usize {
        while p < hi && self.code[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }

    fn starts(&self, p: usize, hi: usize, pat: &[u8]) -> bool {
        self.code.get(p..hi).is_some_and(|s| s.starts_with(pat))
    }

    /// The identifier at `p` (a raw identifier without its `r#`) and the offset after it.
    fn ident(&self, p: usize, hi: usize) -> Option<(&str, usize)> {
        let s = if self.starts(p, hi, b"r#") { p + 2 } else { p };
        let mut q = s;
        while q < hi && is_ident_byte(self.code[q]) {
            q += 1;
        }
        if q == s || self.code[s].is_ascii_digit() {
            return None;
        }
        std::str::from_utf8(&self.code[s..q]).ok().map(|w| (w, q))
    }

    fn word_at(&self, p: usize, hi: usize, word: &str) -> bool {
        let end = p + word.len();
        end <= hi
            && &self.code[p..end] == word.as_bytes()
            && (p == 0 || !is_ident_byte(self.code[p - 1]))
            && self.code.get(end).is_none_or(|&c| !is_ident_byte(c))
    }

    /// The bracket that closes the one at `open`.
    fn matching(&self, open: usize) -> Result<usize, String> {
        let (l, r) = match self.code.get(open) {
            Some(b'(') => (b'(', b')'),
            Some(b'[') => (b'[', b']'),
            Some(b'{') => (b'{', b'}'),
            _ => return Err(self.err(open, "expected a bracket")),
        };
        let mut depth = 0usize;
        for (i, &c) in self.code.iter().enumerate().skip(open) {
            if c == l {
                depth += 1;
            } else if c == r {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
        }
        Err(self.err(open, "unclosed bracket"))
    }

    /// Whether the `>` at `i` closes an angle bracket (it is not the end of `->` or `=>`).
    fn closes_angle(&self, i: usize) -> bool {
        self.code[i] == b'>' && (i == 0 || !matches!(self.code[i - 1], b'-' | b'='))
    }

    /// The top-level comma-separated spans of `lo..hi`; with `angles`, `<` and `>` nest too.
    fn split_top(&self, lo: usize, hi: usize, angles: bool) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut from = lo;
        for i in lo..hi {
            match self.code[i] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'<' if angles => depth += 1,
                b'>' if angles && self.closes_angle(i) => depth -= 1,
                b',' if depth == 0 => {
                    out.push((from, i));
                    from = i + 1;
                }
                _ => {}
            }
        }
        out.push((from, hi));
        out.into_iter()
            .filter(|&(a, b)| self.code[a..b].iter().any(|c| !c.is_ascii_whitespace()))
            .collect()
    }

    /// The first top-level `=` of `lo..hi` (not part of `==`, `=>`, `<=`, `>=` or `!=`).
    fn top_eq(&self, lo: usize, hi: usize) -> Option<usize> {
        let mut depth = 0i32;
        for i in lo..hi {
            match self.code[i] {
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'>' if self.closes_angle(i) => depth -= 1,
                b'=' if depth == 0
                    && self
                        .code
                        .get(i + 1)
                        .is_none_or(|&c| !matches!(c, b'=' | b'>'))
                    && !matches!(self.code[i - 1], b'=' | b'<' | b'>' | b'!') =>
                {
                    return Some(i);
                }
                _ => {}
            }
        }
        None
    }

    /// Skips the outer attributes at `p`.
    fn skip_attrs(&self, mut p: usize, hi: usize) -> Result<usize, String> {
        p = self.skip_ws(p, hi);
        while self.starts(p, hi, b"#[") {
            p = self.skip_ws(self.matching(p + 1)? + 1, hi);
        }
        Ok(p)
    }

    fn visibility(&self, p: usize, hi: usize) -> Result<(Vis, usize), String> {
        let Some(("pub", after)) = self.ident(p, hi) else {
            return Ok((Vis::Private, p));
        };
        let q = self.skip_ws(after, hi);
        if self.code.get(q) == Some(&b'(') {
            let close = self.matching(q)?;
            let inner = norm(self.shown(q + 1, close));
            if matches!(inner.as_str(), "crate" | "self" | "super") || inner.starts_with("in ") {
                return Ok((Vis::Restricted, close + 1));
            }
        }
        Ok((Vis::Pub, after))
    }

    /// The kind of the item whose keywords start at `q`, and the offset after its keyword.
    fn kind(&self, q: usize, hi: usize) -> Option<(Kind, usize)> {
        let mut p = q;
        loop {
            let (w, after) = self.ident(p, hi)?;
            let next = self.skip_ws(after, hi);
            let kind = match w {
                "async" | "unsafe" | "default" | "safe" => {
                    p = next;
                    continue;
                }
                "extern" => match self.ident(next, hi) {
                    Some(("crate", a)) => return Some((Kind::ExternCrate, a)),
                    Some(_) => {
                        p = next;
                        continue;
                    }
                    None if self.code.get(next) == Some(&b'{') => {
                        return Some((Kind::ExternBlock, next));
                    }
                    None => return None,
                },
                "const" => match self.ident(next, hi) {
                    Some(("fn" | "unsafe" | "async" | "extern", _)) => {
                        p = next;
                        continue;
                    }
                    _ => Kind::Const,
                },
                "fn" => Kind::Fn,
                "struct" => Kind::Struct,
                "enum" => Kind::Enum,
                "union" if self.ident(next, hi).is_some() => Kind::Union,
                "trait" => Kind::Trait,
                "impl" => Kind::Impl,
                "mod" => Kind::Mod,
                "type" => Kind::Type,
                "static" => Kind::Static,
                "use" => Kind::Use,
                "macro_rules" if self.code.get(next) == Some(&b'!') => {
                    return Some((Kind::MacroRules, next + 1));
                }
                _ => {
                    let mut e = after;
                    while self.starts(e, hi, b"::") {
                        e = self.ident(e + 2, hi)?.1;
                    }
                    let bang = self.skip_ws(e, hi);
                    return (self.code.get(bang) == Some(&b'!'))
                        .then_some((Kind::MacroCall, bang + 1));
                }
            };
            return Some((kind, after));
        }
    }

    /// The extent of an item whose keyword ends at `kw_end`.
    fn extent(&self, kind: Kind, kw_end: usize, hi: usize) -> Result<Extent, String> {
        let ends_at = |p: usize| Extent {
            head_end: p,
            body: None,
            end: p + 1,
        };
        match kind {
            Kind::Const | Kind::Static | Kind::Type | Kind::Use | Kind::ExternCrate => {
                let mut depth = 0i32;
                for p in kw_end..hi {
                    match self.code[p] {
                        b'(' | b'[' | b'{' => depth += 1,
                        b')' | b']' | b'}' => depth -= 1,
                        b';' if depth == 0 => return Ok(ends_at(p)),
                        _ => {}
                    }
                }
                Err(self.err(kw_end, "item without `;`"))
            }
            Kind::MacroRules | Kind::MacroCall => {
                let mut open = self.skip_ws(kw_end, hi);
                if kind == Kind::MacroRules {
                    let (_, a) = self
                        .ident(open, hi)
                        .ok_or_else(|| self.err(open, "macro_rules! without a name"))?;
                    open = self.skip_ws(a, hi);
                }
                let close = self.matching(open)?;
                let mut end = close + 1;
                let s = self.skip_ws(end, hi);
                if self.code.get(s) == Some(&b';') {
                    end = s + 1;
                }
                Ok(Extent {
                    head_end: close + 1,
                    body: Some((open + 1, close)),
                    end,
                })
            }
            _ => {
                let mut depth = 0i32;
                for p in kw_end..hi {
                    match self.code[p] {
                        b'(' | b'[' => depth += 1,
                        b')' | b']' => depth -= 1,
                        b';' if depth == 0 => return Ok(ends_at(p)),
                        b'{' if depth == 0 => {
                            let close = self.matching(p)?;
                            return Ok(Extent {
                                head_end: p,
                                body: Some((p + 1, close)),
                                end: close + 1,
                            });
                        }
                        _ => {}
                    }
                }
                Err(self.err(kw_end, "item without a body or `;`"))
            }
        }
    }

    /// The items of `lo..hi` (a file, or the inside of a module, impl or trait).
    fn items(&self, lo: usize, hi: usize) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        let mut p = lo;
        loop {
            p = self.skip_ws(p, hi);
            if p >= hi {
                return Ok(out);
            }
            if self.code[p] == b';' {
                p += 1;
                continue;
            }
            if self.starts(p, hi, b"#![") {
                p = self.matching(p + 2)? + 1;
                continue;
            }
            let mut attrs = Vec::new();
            while self.starts(p, hi, b"#[") {
                let close = self.matching(p + 1)?;
                attrs.push(norm(self.shown(p, close + 1)));
                p = self.skip_ws(close + 1, hi);
            }
            let start = p;
            let (vis, q) = self.visibility(p, hi)?;
            let q = self.skip_ws(q, hi);
            let (kind, kw_end) = self
                .kind(q, hi)
                .ok_or_else(|| self.err(start, "unrecognised item"))?;
            let ext = self.extent(kind, kw_end, hi)?;
            out.push(Item {
                attrs,
                vis,
                kind,
                start,
                kw_end,
                head_end: ext.head_end,
                body: ext.body,
            });
            p = ext.end;
        }
    }

    /// The name after an item's keyword.
    fn name(&self, it: &Item) -> Result<String, String> {
        let p = self.skip_ws(it.kw_end, it.head_end);
        self.ident(p, it.head_end)
            .map(|(w, _)| w.to_owned())
            .ok_or_else(|| self.err(it.start, "item without a name"))
    }

    fn head(&self, it: &Item) -> String {
        norm(self.shown(it.start, it.head_end))
    }

    /// The header up to the value of a constant, static or associated constant.
    fn without_value(&self, it: &Item) -> (String, bool) {
        match self.top_eq(it.kw_end, it.head_end) {
            Some(eq) => (norm(self.shown(it.start, eq)), true),
            None => (self.head(it), false),
        }
    }

    /// Whether an impl is a trait impl, and the last path segment of its self type.
    fn impl_target(&self, it: &Item) -> (bool, Option<String>) {
        let hi = it.head_end;
        let mut p = self.skip_ws(it.kw_end, hi);
        if self.code.get(p) == Some(&b'<') {
            let mut depth = 0i32;
            while p < hi {
                match self.code[p] {
                    b'<' => depth += 1,
                    b'>' if self.closes_angle(p) => depth -= 1,
                    _ => {}
                }
                p += 1;
                if depth == 0 {
                    break;
                }
            }
        }
        let (mut for_at, mut where_at) = (None, hi);
        let mut depth = 0i32;
        for q in p..hi {
            match self.code[q] {
                b'<' | b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b'>' if self.closes_angle(q) => depth -= 1,
                _ if depth == 0 && for_at.is_none() && self.word_at(q, hi, "for") => {
                    for_at = Some(q);
                }
                _ if depth == 0 && self.word_at(q, hi, "where") => {
                    where_at = q;
                    break;
                }
                _ => {}
            }
        }
        let mut r = self.skip_ws(for_at.map_or(p, |f| f + 3), where_at);
        loop {
            if self.code.get(r) == Some(&b'&') {
                r = self.skip_ws(r + 1, where_at);
            } else if self.code.get(r) == Some(&b'\'') {
                r = self.skip_ws(self.ident(r + 1, where_at).map_or(r + 1, |x| x.1), where_at);
            } else if let Some(("mut" | "dyn", a)) = self.ident(r, where_at) {
                r = self.skip_ws(a, where_at);
            } else {
                break;
            }
        }
        let mut last = None;
        while let Some((w, a)) = self.ident(r, where_at) {
            last = Some(w.to_owned());
            if !self.starts(a, where_at, b"::") {
                break;
            }
            r = a + 2;
        }
        (for_at.is_some(), last)
    }
}

/// A module reached from a crate root.
struct Module {
    path: String,
    exported: bool,
    attrs: Vec<String>,
    src: usize,
    lo: usize,
    hi: usize,
    inline: bool,
}

/// The names a crate declares (module-level structs, enums, unions, traits and type aliases,
/// and its macros).
#[derive(Default)]
struct Names {
    public: BTreeSet<String>,
    private: BTreeSet<String>,
    macros: BTreeSet<String>,
}

impl Names {
    fn private_only(&self, name: &str) -> bool {
        self.private.contains(name) && !self.public.contains(name)
    }
}

struct Crate {
    sources: Vec<Source>,
    modules: Vec<Module>,
}

fn rel_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

impl Crate {
    fn load(root: &Path, krate: &str) -> Result<Self, String> {
        let mut c = Self {
            sources: Vec::new(),
            modules: Vec::new(),
        };
        let dir = PathBuf::from("crates").join(krate).join("src");
        let src = c.add_source(root, &dir.join("lib.rs"))?;
        let hi = c.sources[src].len();
        c.visit(
            root,
            Module {
                path: krate.replace('-', "_"),
                exported: true,
                attrs: Vec::new(),
                src,
                lo: 0,
                hi,
                inline: false,
            },
            &dir,
        )?;
        Ok(c)
    }

    fn from_text(rel: &str, text: &str) -> Result<Self, String> {
        let src = Source::new(rel, text);
        let hi = src.len();
        let mut c = Self {
            sources: vec![src],
            modules: Vec::new(),
        };
        c.visit(
            Path::new(""),
            Module {
                path: "fixture".to_owned(),
                exported: true,
                attrs: Vec::new(),
                src: 0,
                lo: 0,
                hi,
                inline: false,
            },
            Path::new(""),
        )?;
        Ok(c)
    }

    fn add_source(&mut self, root: &Path, rel: &Path) -> Result<usize, String> {
        let p = root.join(rel);
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        self.sources.push(Source::new(&rel_string(rel), &text));
        Ok(self.sources.len() - 1)
    }

    /// Records `m` and, depth first in declaration order, the modules it declares.
    fn visit(&mut self, root: &Path, m: Module, dir: &Path) -> Result<(), String> {
        let (src, lo, hi) = (m.src, m.lo, m.hi);
        let (parent_path, parent_exported) = (m.path.clone(), m.exported);
        self.modules.push(m);
        let mut children = Vec::new();
        {
            let s = &self.sources[src];
            for it in s.items(lo, hi)? {
                if it.kind != Kind::Mod || is_test_only(&it.attrs) {
                    continue;
                }
                children.push((s.name(&it)?, it.vis, kept_attrs(&it.attrs), it.body));
            }
        }
        for (name, vis, attrs, body) in children {
            let child = Module {
                path: format!("{parent_path}::{name}"),
                exported: parent_exported && vis == Vis::Pub,
                attrs,
                src,
                lo: 0,
                hi: 0,
                inline: body.is_some(),
            };
            let child_dir = dir.join(&name);
            if let Some((blo, bhi)) = body {
                self.visit(
                    root,
                    Module {
                        lo: blo,
                        hi: bhi,
                        ..child
                    },
                    &child_dir,
                )?;
                continue;
            }
            let file = dir.join(format!("{name}.rs"));
            let nested = child_dir.join("mod.rs");
            let rel = match (root.join(&file).is_file(), root.join(&nested).is_file()) {
                (true, false) => file,
                (false, true) => nested,
                (both, _) => {
                    return Err(format!(
                        "{}: module {name}: {}",
                        rel_string(dir),
                        if both {
                            "both name.rs and name/mod.rs exist"
                        } else {
                            "no source file"
                        }
                    ));
                }
            };
            let s = self.add_source(root, &rel)?;
            let hi = self.sources[s].len();
            self.visit(
                root,
                Module {
                    src: s,
                    hi,
                    ..child
                },
                &child_dir,
            )?;
        }
        Ok(())
    }

    fn names(&self) -> Result<Names, String> {
        let mut n = Names::default();
        for m in &self.modules {
            let s = &self.sources[m.src];
            for it in s.items(m.lo, m.hi)? {
                if is_test_only(&it.attrs) {
                    continue;
                }
                match it.kind {
                    Kind::Struct | Kind::Enum | Kind::Union | Kind::Trait | Kind::Type => {
                        let name = s.name(&it)?;
                        if it.vis == Vis::Pub {
                            n.public.insert(name);
                        } else {
                            n.private.insert(name);
                        }
                    }
                    Kind::MacroRules => {
                        n.macros.insert(s.name(&it)?);
                    }
                    _ => {}
                }
            }
        }
        Ok(n)
    }

    fn listing(&self, out: &mut Vec<String>) -> Result<(), String> {
        let names = self.names()?;
        for m in &self.modules {
            let s = &self.sources[m.src];
            let mut lines = Vec::new();
            for it in s.items(m.lo, m.hi)? {
                if !is_test_only(&it.attrs) {
                    item_lines(s, &it, &names, "", &mut lines)?;
                }
            }
            if !m.exported && lines.is_empty() {
                continue;
            }
            let mut heading = format!("## {}", m.path);
            if !m.exported {
                heading.push_str(" (private)");
            }
            for a in &m.attrs {
                heading.push(' ');
                heading.push_str(a);
            }
            heading.push_str(&format!(
                "  [{}{}]",
                if m.inline { "inline in " } else { "" },
                s.rel
            ));
            out.push(String::new());
            out.push(heading);
            out.extend(lines);
        }
        Ok(())
    }
}

fn macro_lines(s: &Source, it: &Item, indent: &str, out: &mut Vec<String>) {
    let Some((lo, hi)) = it.body else {
        return;
    };
    let name = norm(s.shown(it.start, it.kw_end));
    let (open, close) = (s.shown(lo - 1, lo), s.shown(hi, hi + 1));
    let text = s.shown(lo, hi);
    if text.contains('\n') {
        let gap = if open == "{" { " " } else { "" };
        out.push(format!("{indent}{name}{gap}{open}"));
        out.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| format!("{indent}    {l}")),
        );
        out.push(format!("{indent}{close}"));
    } else {
        out.push(format!("{indent}{name}{open}{}{close}", norm(text)));
    }
}

/// Whether an item-level macro invocation can define listed items.
fn listed_call(s: &Source, it: &Item, names: &Names) -> bool {
    let name = norm(s.shown(it.start, it.kw_end));
    let last = name.trim_end_matches('!').rsplit("::").next().unwrap_or("");
    let says_pub = it.body.is_some_and(|(lo, hi)| {
        let body = &s.code[lo..hi];
        body.windows(3).enumerate().any(|(i, w)| {
            w == b"pub"
                && (i == 0 || !is_ident_byte(body[i - 1]))
                && body.get(i + 3).is_none_or(|&c| !is_ident_byte(c))
        })
    });
    names.macros.contains(last) || last == "include" || says_pub
}

fn struct_lines(s: &Source, it: &Item, out: &mut Vec<String>) -> Result<(), String> {
    let Some((lo, hi)) = it.body else {
        let mut depth = 0i32;
        let mut open = None;
        for p in it.kw_end..it.head_end {
            match s.code[p] {
                b'<' => depth += 1,
                b'>' if s.closes_angle(p) => depth -= 1,
                b'(' if depth == 0 => {
                    open = Some(p);
                    break;
                }
                _ => {}
            }
        }
        let Some(open) = open else {
            out.push(prefixed(&it.attrs, &s.head(it)));
            return Ok(());
        };
        let close = s.matching(open)?;
        let mut fields = Vec::new();
        for (a, b) in s.split_top(open + 1, close, true) {
            let p = s.skip_attrs(a, b)?;
            fields.push(if s.visibility(p, b)?.0 == Vis::Pub {
                norm(s.shown(p, b))
            } else {
                "_".to_owned()
            });
        }
        let text = format!(
            "{}({}){}",
            norm(s.shown(it.start, open)),
            fields.join(", "),
            norm(s.shown(close + 1, it.head_end))
        );
        out.push(prefixed(&it.attrs, text.trim_end()));
        return Ok(());
    };
    let fields = s.split_top(lo, hi, true);
    let head = prefixed(&it.attrs, &s.head(it));
    if fields.is_empty() {
        out.push(format!("{head} {{}}"));
        return Ok(());
    }
    out.push(format!("{head} {{"));
    let mut private = false;
    for (a, b) in fields {
        let p = s.skip_attrs(a, b)?;
        if s.visibility(p, b)?.0 == Vis::Pub {
            out.push(format!("    {},", norm(s.shown(p, b))));
        } else {
            private = true;
        }
    }
    if private {
        out.push("    ..".to_owned());
    }
    out.push("}".to_owned());
    Ok(())
}

fn enum_lines(s: &Source, it: &Item, out: &mut Vec<String>) -> Result<(), String> {
    let head = prefixed(&it.attrs, &s.head(it));
    let Some((lo, hi)) = it.body else {
        return Err(s.err(it.start, "enum without a body"));
    };
    let variants = s.split_top(lo, hi, false);
    if variants.is_empty() {
        out.push(format!("{head} {{}}"));
        return Ok(());
    }
    out.push(format!("{head} {{"));
    for (a, b) in variants {
        let p = s.skip_attrs(a, b)?;
        out.push(format!("    {},", norm(s.shown(p, b))));
    }
    out.push("}".to_owned());
    Ok(())
}

/// The listed lines of the associated items of a trait (`all`) or of an inherent impl or
/// extern block (`pub` items only).
fn member_lines(
    s: &Source,
    it: &Item,
    names: &Names,
    all: bool,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let Some((lo, hi)) = it.body else {
        return Ok(());
    };
    for m in s.items(lo, hi)? {
        if is_test_only(&m.attrs) {
            continue;
        }
        if m.kind == Kind::MacroCall {
            if listed_call(s, &m, names) {
                macro_lines(s, &m, "    ", out);
            }
            continue;
        }
        if !(all || m.vis == Vis::Pub) {
            continue;
        }
        let text = match m.kind {
            Kind::Fn if all && m.body.is_some() => format!("{} {{ .. }}", s.head(&m)),
            Kind::Fn | Kind::Type | Kind::Static => s.head(&m),
            Kind::Const => match s.without_value(&m) {
                (head, true) if all => format!("{head} = .."),
                (head, _) => head,
            },
            _ => return Err(s.err(m.start, "unexpected associated item")),
        };
        out.push(format!("    {}", prefixed(&m.attrs, &text)));
    }
    Ok(())
}

fn item_lines(
    s: &Source,
    it: &Item,
    names: &Names,
    indent: &str,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let public = it.vis == Vis::Pub;
    match it.kind {
        Kind::Mod => {}
        Kind::Fn | Kind::Type | Kind::Use | Kind::ExternCrate if public => {
            out.push(format!("{indent}{}", prefixed(&it.attrs, &s.head(it))));
        }
        Kind::Const | Kind::Static if public => {
            out.push(format!(
                "{indent}{}",
                prefixed(&it.attrs, &s.without_value(it).0)
            ));
        }
        Kind::Struct | Kind::Union if public => struct_lines(s, it, out)?,
        Kind::Enum if public => enum_lines(s, it, out)?,
        Kind::Trait if public => {
            let mut members = Vec::new();
            member_lines(s, it, names, true, &mut members)?;
            let head = prefixed(&it.attrs, &s.head(it));
            if members.is_empty() {
                out.push(format!("{head} {{}}"));
            } else {
                out.push(format!("{head} {{"));
                out.extend(members);
                out.push("}".to_owned());
            }
        }
        Kind::Impl => {
            let (is_trait, target) = s.impl_target(it);
            if target.as_deref().is_some_and(|t| names.private_only(t)) {
                return Ok(());
            }
            let head = prefixed(&it.attrs, &s.head(it));
            if is_trait {
                out.push(head);
                return Ok(());
            }
            let mut members = Vec::new();
            member_lines(s, it, names, false, &mut members)?;
            if !members.is_empty() {
                out.push(format!("{head} {{"));
                out.extend(members);
                out.push("}".to_owned());
            }
        }
        Kind::ExternBlock => {
            let mut members = Vec::new();
            member_lines(s, it, names, false, &mut members)?;
            if !members.is_empty() {
                out.push(format!("{} {{", prefixed(&it.attrs, &s.head(it))));
                out.extend(members);
                out.push("}".to_owned());
            }
        }
        Kind::MacroRules => {
            let (lo, hi) = it.body.unwrap_or((it.kw_end, it.kw_end));
            let hex = sha256_hex(norm(s.shown(lo, hi)).as_bytes());
            out.push(format!(
                "{indent}{}",
                prefixed(
                    &it.attrs,
                    &format!("macro_rules! {} {{ sha256:{} }}", s.name(it)?, &hex[..16])
                )
            ));
        }
        Kind::MacroCall if listed_call(s, it, names) => macro_lines(s, it, indent, out),
        _ => {}
    }
    Ok(())
}

/// The listing of one source text as the root of a crate named `fixture`, without its `mod`
/// files: the unit tests' view of the extractor.
///
/// # Errors
/// An item the reader does not recognise, or unbalanced brackets.
pub fn listing_of_text(text: &str) -> Result<Vec<String>, String> {
    let c = Crate::from_text("fixture.rs", text)?;
    let mut out = Vec::new();
    c.listing(&mut out)?;
    Ok(out)
}

fn section(text: &str, header: &str) -> Vec<String> {
    let mut lines = text.lines();
    lines.by_ref().find(|l| l.trim() == header);
    lines
        .take_while(|l| !l.trim_start().starts_with('['))
        .map(str::to_owned)
        .collect()
}

/// The workspace members that another member's `[dependencies]` names: the library crates.
///
/// # Errors
/// An unreadable manifest.
pub fn api_crates(root: &Path) -> Result<Vec<String>, String> {
    let read = |rel: &str| {
        let p = root.join(rel);
        std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let workspace = read("Cargo.toml")?;
    let members_text = workspace
        .split_once("members = [")
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(m, _)| m)
        .ok_or("Cargo.toml: no workspace members list")?;
    let members: Vec<String> = members_text
        .split('"')
        .skip(1)
        .step_by(2)
        .filter_map(|m| m.strip_prefix("crates/").map(str::to_owned))
        .collect();
    let mut depended = BTreeSet::new();
    for m in &members {
        let manifest = read(&format!("crates/{m}/Cargo.toml"))?;
        for line in section(&manifest, "[dependencies]") {
            let key = line.split(['.', ' ', '=']).next().unwrap_or("");
            if key != m && members.iter().any(|x| x == key) {
                depended.insert(key.to_owned());
            }
        }
    }
    Ok(members
        .into_iter()
        .filter(|m| depended.contains(m))
        .collect())
}

/// The listing of every crate of [`api_crates`], in workspace order.
///
/// # Errors
/// An unreadable source, a module without its file, or an item the reader does not recognise.
pub fn snapshot(root: &Path) -> Result<String, String> {
    let mut out: Vec<String> = HEADER.lines().map(str::to_owned).collect();
    for krate in api_crates(root)? {
        out.push(String::new());
        out.push(format!("# {krate}"));
        Crate::load(root, &krate)?.listing(&mut out)?;
    }
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

/// Compares the listing with the committed [`SNAPSHOT_FILE`]: the problems found (empty when
/// they are equal).
///
/// # Errors
/// The listing could not be built.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let fresh = snapshot(root)?;
    let path = root.join(SNAPSHOT_FILE);
    let Ok(committed) = std::fs::read_to_string(&path) else {
        return Ok(vec![format!(
            "{SNAPSHOT_FILE} is missing: run `gcg-diff api-snapshot --write`"
        )]);
    };
    if committed == fresh {
        return Ok(Vec::new());
    }
    let (old, new): (Vec<&str>, Vec<&str>) = (committed.lines().collect(), fresh.lines().collect());
    let at = old
        .iter()
        .zip(&new)
        .position(|(a, b)| a != b)
        .unwrap_or(old.len().min(new.len()));
    let only_old = old.iter().filter(|l| !new.contains(l)).count();
    let only_new = new.iter().filter(|l| !old.contains(l)).count();
    Ok(vec![
        format!(
            "{SNAPSHOT_FILE}:{}: committed {:?}, sources {:?}",
            at + 1,
            old.get(at).copied().unwrap_or("<end>"),
            new.get(at).copied().unwrap_or("<end>")
        ),
        format!(
            "{only_old} committed line(s) not in the sources, {only_new} source line(s) not \
             committed; review the change, then run `gcg-diff api-snapshot --write`"
        ),
    ])
}

/// Writes the listing to [`SNAPSHOT_FILE`]; returns its line count.
///
/// # Errors
/// The listing could not be built or written.
pub fn write(root: &Path) -> Result<usize, String> {
    let text = snapshot(root)?;
    let path = root.join(SNAPSHOT_FILE);
    std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text.lines().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(text: &str) -> Vec<String> {
        listing_of_text(text)
            .expect("listing")
            .into_iter()
            .filter(|l| !l.is_empty() && !l.starts_with("## "))
            .collect()
    }

    #[test]
    fn only_exactly_pub_items_outside_test_code_are_listed() {
        let got = list(
            "pub fn a() {}\nfn b() {}\npub(crate) fn c() {}\npub(super) fn d() {}\n\
             #[cfg(test)]\nmod tests { pub fn e() {} }\n#[cfg(all(test, feature = \"x\"))]\n\
             pub fn f() {}\n#[cfg(any(test, feature = \"x\"))]\npub fn g() {}\n",
        );
        assert_eq!(
            got,
            [
                "pub fn a()",
                "#[cfg(any(test, feature = \"x\"))] pub fn g()"
            ]
        );
    }

    #[test]
    fn line_breaks_and_comments_do_not_change_a_signature() {
        let one = list("pub fn f<T: Clone>(a: u8, b: &'static str) -> Option<T> { None }\n");
        let many = list(
            "/// Doc.\npub fn f<\n    T: Clone, // the type\n>(\n    a: u8,\n    b: &'static str,\n\
             ) -> Option<T> {\n    let c = '}';\n    None\n}\n",
        );
        assert_eq!(one, many);
        assert_eq!(
            one,
            ["pub fn f<T: Clone>(a: u8, b: &'static str) -> Option<T>"]
        );
        let flat = list("pub fn g<Q>(q: &Q) where Q: Eq + ?Sized {}\n");
        let tall = list("pub fn g<Q>(q: &Q)\nwhere\n    Q: Eq + ?Sized,\n{\n}\n");
        assert_eq!(flat, tall);
        assert_eq!(flat, ["pub fn g<Q>(q: &Q) where Q: Eq + ?Sized"]);
    }

    #[test]
    fn a_changed_signature_changes_the_listing() {
        let before = list("pub struct S;\nimpl S {\n    pub fn new(x: u8) -> Self { S }\n}\n");
        let after = list("pub struct S;\nimpl S {\n    pub fn new(x: u16) -> Self { S }\n}\n");
        assert_ne!(before, after);
        assert_eq!(
            before,
            [
                "pub struct S",
                "impl S {",
                "    pub fn new(x: u8) -> Self",
                "}"
            ]
        );
    }

    #[test]
    fn structs_list_pub_fields_and_mark_private_ones() {
        let got = list(
            "#[derive(Clone, Debug)]\n#[must_use]\npub struct A<K, V> {\n    pub map: Map<K, V>,\n\
             \n    #[allow(dead_code)]\n    secret: u8,\n    pub(crate) half: u8,\n}\n\
             pub struct B(pub u32, u8);\npub struct C {}\nstruct D { pub x: u8 }\n",
        );
        assert_eq!(
            got,
            [
                "#[derive(Clone, Debug)] pub struct A<K, V> {",
                "    pub map: Map<K, V>,",
                "    ..",
                "}",
                "pub struct B(pub u32, _)",
                "pub struct C {}",
            ]
        );
    }

    #[test]
    fn enums_list_every_variant_and_traits_every_member() {
        let got = list(
            "#[repr(u8)]\npub enum E {\n    #[default]\n    A = 1,\n    B(u8, Vec<(u8, u16)>),\n    \
             C { x: u8 },\n}\npub trait T: Send {\n    const N: usize = 3;\n    type X;\n    \
             fn req(&self) -> u8;\n    fn prov(&self) -> u8 { 1 }\n}\n",
        );
        assert_eq!(
            got,
            [
                "#[repr(u8)] pub enum E {",
                "    A = 1,",
                "    B(u8, Vec<(u8, u16)>),",
                "    C {x: u8},",
                "}",
                "pub trait T: Send {",
                "    const N: usize = ..",
                "    type X",
                "    fn req(&self) -> u8",
                "    fn prov(&self) -> u8 { .. }",
                "}",
            ]
        );
    }

    #[test]
    fn impls_of_private_types_and_constant_values_are_left_out() {
        let got = list(
            "pub struct P;\nstruct Q;\nimpl Clone for P { fn clone(&self) -> Self { P } }\n\
             impl Clone for Q { fn clone(&self) -> Self { Q } }\nimpl Q { pub fn q() {} }\n\
             impl<T: Into<u8>> From<T> for P where T: Copy { fn from(_: T) -> Self { P } }\n\
             pub const K: [u8; 2] = [1, 2];\npub static S: &str = \"x; y\";\n\
             pub use a::{\n    b,\n    c,\n};\n",
        );
        assert_eq!(
            got,
            [
                "pub struct P",
                "impl Clone for P",
                "impl<T: Into<u8>> From<T> for P where T: Copy",
                "pub const K: [u8; 2]",
                "pub static S: &str",
                "pub use a::{b, c}",
            ]
        );
    }

    #[test]
    fn macros_are_listed_by_digest_and_their_item_level_invocations() {
        let def = |body: &str| {
            list(&format!(
                "macro_rules! m {{ ($n:ident) => {{ {body} }}; }}\nm!(Alpha);\n\
                 thread_local! {{ static X: u8 = 0; }}\n"
            ))
        };
        let a = def("pub struct $n;");
        let b = def("pub struct $n(u8);");
        assert_eq!(a.len(), 2, "{a:?}");
        assert!(a[0].starts_with("macro_rules! m { sha256:"), "{a:?}");
        assert_eq!(a[1], "m!(Alpha)");
        assert_ne!(a[0], b[0]);
    }

    #[test]
    fn inline_modules_get_their_own_section() {
        let got = listing_of_text(
            "pub mod inner {\n    pub fn f() {}\n}\nmod hidden {\n    fn g() {}\n}\n",
        )
        .expect("listing");
        assert_eq!(
            got,
            [
                "",
                "## fixture  [fixture.rs]",
                "",
                "## fixture::inner  [inline in fixture.rs]",
                "pub fn f()",
            ]
        );
    }

    #[test]
    fn an_unrecognised_item_is_an_error_with_its_line() {
        let err = listing_of_text("pub fn a() {}\nauto trait X {}\n").expect_err("error");
        assert_eq!(err, "fixture.rs:2: unrecognised item");
    }
}
