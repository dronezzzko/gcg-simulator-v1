//! `textwrap.wrap(text, width)` and `textwrap.fill(...)` of CPython 3.15.1
//! (`Lib/textwrap.py:17-391`) with the default options argparse uses: `expand_tabs`,
//! `replace_whitespace`, `break_long_words`, `drop_whitespace` and `break_on_hyphens` on,
//! `fix_sentence_endings` off, no `max_lines`.
//!
//! The input is the parser's own help text, which is ASCII, so the `\w`, `\d` and letter
//! classes of `wordsep_re` are their ASCII forms here (a test checks that every help string
//! of the parser is ASCII). Checked against `textwrap.wrap` vectors exported by
//! `oracle/export_cli.py`.

/// `textwrap._whitespace` (`textwrap.py:15`).
const fn is_ws(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ')
}

/// `\w` for ASCII text.
const fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `word_punct = r'[\w!"\'&.,?]'`.
const fn is_word_punct(c: char) -> bool {
    is_word(c) || matches!(c, '!' | '"' | '\'' | '&' | '.' | ',' | '?')
}

/// `letter = r'[^\d\W]'`: a word character that is not a digit.
const fn is_letter(c: char) -> bool {
    is_word(c) && !c.is_ascii_digit()
}

/// The end of the `wordsep_re` match that starts at `i` (`textwrap.py:83-100`). Every
/// position starts a match (a whitespace run, an em-dash, or a non-whitespace run that ends
/// at a hyphenation point, before whitespace or the end, or before an em-dash), so
/// `re.split` yields exactly these matches.
fn token_end(s: &[char], i: usize) -> usize {
    let n = s.len();
    let at = |k: usize| s.get(k).copied();
    // any whitespace: %(ws)s+
    if is_ws(s[i]) {
        let mut j = i;
        while j < n && is_ws(s[j]) {
            j += 1;
        }
        return j;
    }
    // em-dash between words: (?<=%(wp)s) -{2,} (?=\w)
    if i > 0 && is_word_punct(s[i - 1]) && s[i] == '-' {
        let mut j = i;
        while j < n && s[j] == '-' {
            j += 1;
        }
        // Fewer hyphens would leave a '-' for the lookahead, which is not \w.
        if j - i >= 2 && at(j).is_some_and(is_word) {
            return j;
        }
    }
    // word, possibly hyphenated: %(nws)s+? (?: hyphen | end of word | em-dash )
    let mut k = i + 1;
    loop {
        // -(?: (?<=%(lt)s{2}-) | (?<=%(lt)s-%(lt)s-)) (?= %(lt)s -? %(lt)s)
        if at(k) == Some('-') {
            let behind = (k >= 2 && is_letter(s[k - 2]) && is_letter(s[k - 1]))
                || (k >= 3 && is_letter(s[k - 3]) && s[k - 2] == '-' && is_letter(s[k - 1]));
            let ahead = at(k + 1).is_some_and(is_letter)
                && (at(k + 2).is_some_and(is_letter)
                    || (at(k + 2) == Some('-') && at(k + 3).is_some_and(is_letter)));
            if behind && ahead {
                return k + 1;
            }
        }
        // (?=%(ws)s|\z)
        match at(k) {
            None => return k,
            Some(c) if is_ws(c) => return k,
            Some(_) => {}
        }
        // (?<=%(wp)s) (?=-{2,}\w)
        if is_word_punct(s[k - 1]) && at(k) == Some('-') && at(k + 1) == Some('-') {
            let mut j = k;
            while j < n && s[j] == '-' {
                j += 1;
            }
            if at(j).is_some_and(is_word) {
                return k;
            }
        }
        // s[k] is not whitespace: extend the lazy run.
        k += 1;
    }
}

/// `_munge_whitespace` (`textwrap.py:143-155`): `expandtabs(8)`, then every
/// `_whitespace` character becomes a space.
fn munge(text: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut column = 0_usize;
    for c in text.chars() {
        match c {
            '\t' => {
                let spaces = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(' ');
                column = 0;
            }
            c if is_ws(c) => {
                out.push(' ');
                column += 1;
            }
            c => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

/// `_split` (`textwrap.py:158-180`): the chunks of `text`, in order.
fn split(text: &[char]) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let end = token_end(text, i);
        chunks.push(text[i..end].iter().collect());
        i = end;
    }
    chunks
}

fn is_blank(chunk: &str) -> bool {
    chunk.trim().is_empty()
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// `_handle_long_word` (`textwrap.py:198-235`).
fn handle_long_word(
    reversed_chunks: &mut Vec<String>,
    cur_line: &mut Vec<String>,
    cur_len: usize,
    width: isize,
) {
    let space_left = if width < 1 {
        1
    } else {
        usize::try_from(width).unwrap_or(0).saturating_sub(cur_len)
    };
    if space_left > 0 {
        let Some(chunk) = reversed_chunks.last() else {
            return;
        };
        let chars: Vec<char> = chunk.chars().collect();
        let mut end = space_left;
        if chars.len() > space_left {
            // break after the last hyphen, but only if there are non-hyphens before it
            let hyphen = chars[..space_left].iter().rposition(|&c| c == '-');
            if let Some(h) = hyphen
                && h > 0
                && chars[..h].iter().any(|&c| c != '-')
            {
                end = h + 1;
            }
        }
        let end = end.min(chars.len());
        cur_line.push(chars[..end].iter().collect());
        let rest: String = chars[end..].iter().collect();
        if let Some(last) = reversed_chunks.last_mut() {
            *last = rest;
        }
    } else if cur_line.is_empty()
        && let Some(chunk) = reversed_chunks.pop()
    {
        cur_line.push(chunk);
    }
}

/// `_wrap_chunks` (`textwrap.py:237-339`) without `max_lines`.
fn wrap_chunks(
    mut chunks: Vec<String>,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    chunks.reverse();
    while !chunks.is_empty() {
        let mut cur_line: Vec<String> = Vec::new();
        let mut cur_len = 0_usize;
        let indent = if lines.is_empty() {
            initial_indent
        } else {
            subsequent_indent
        };
        let line_width = width as isize - char_len(indent) as isize;
        if !lines.is_empty() && chunks.last().is_some_and(|c| is_blank(c)) {
            chunks.pop();
        }
        while let Some(last) = chunks.last() {
            let l = char_len(last);
            if (cur_len + l) as isize <= line_width {
                cur_len += l;
                if let Some(chunk) = chunks.pop() {
                    cur_line.push(chunk);
                }
            } else {
                break;
            }
        }
        if chunks
            .last()
            .is_some_and(|c| char_len(c) as isize > line_width)
        {
            // Python then recomputes cur_len, which only max_lines reads.
            handle_long_word(&mut chunks, &mut cur_line, cur_len, line_width);
        }
        // A trailing whitespace chunk is dropped (Python also lowers cur_len, which only
        // max_lines reads).
        if cur_line.last().is_some_and(|c| is_blank(c)) {
            cur_line.pop();
        }
        if !cur_line.is_empty() {
            lines.push(format!("{indent}{}", cur_line.concat()));
        }
    }
    lines
}

/// `textwrap.wrap(text, width)` (`textwrap.py:342-353`, `:373-383`).
///
/// # Panics
/// For `width == 0`, where Python raises `ValueError` (argparse never passes less than 11).
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    fill_lines(text, width, "", "")
}

/// The lines of `textwrap.fill(text, width, initial_indent=..., subsequent_indent=...)`
/// (`textwrap.py:356-364`, `:385-391`).
///
/// # Panics
/// For `width == 0`.
#[must_use]
pub fn fill_lines(
    text: &str,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) -> Vec<String> {
    assert!(width > 0, "invalid width {width} (must be > 0)");
    let chunks = split(&munge(text));
    wrap_chunks(chunks, width, initial_indent, subsequent_indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_matches_the_docstring() {
        // textwrap.py:79-82 and :163-168
        assert_eq!(
            split(&munge("Hello there -- you goof-ball, use the -b option!")),
            [
                "Hello", " ", "there", " ", "--", " ", "you", " ", "goof-", "ball,", " ", "use",
                " ", "the", " ", "-b", " ", "option!"
            ]
        );
        assert_eq!(split(&munge("x--y")), ["x", "--", "y"]);
    }

    #[test]
    fn wrap_basics() {
        assert_eq!(
            wrap("show this help message and exit", 56),
            ["show this help message and exit"]
        );
        assert_eq!(wrap("aaa bbb ccc", 7), ["aaa bbb", "ccc"]);
        assert!(wrap("", 10).is_empty());
    }
}
