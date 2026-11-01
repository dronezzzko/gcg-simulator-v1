//! `argparse.HelpFormatter` of CPython 3.15.1 (`Lib/argparse.py:172-822`) without colour,
//! for the actions the parser supports.
//!
//! Python formats in two passes: `add_argument` records the widest invocation, then the
//! deferred section items render with it. Here [`Formatter::help`] does the same in that
//! order. Widths follow `HelpFormatter.__init__`: `width = columns - 2` and
//! `max_help_position = min(24, max(width - 20, 4))`.

use super::textwrap;
use super::{Action, ActionKind, Nargs, Parser};

/// Indentation step (`indent_increment=2`).
const INDENT: i64 = 2;

/// Spaces for a `'%*s' % (n, '')` field (`n` is never negative here).
fn spaces(n: i64) -> String {
    " ".repeat(usize::try_from(n).unwrap_or(0))
}

fn len(s: &str) -> i64 {
    i64::try_from(s.chars().count()).unwrap_or(i64::MAX)
}

/// One row of the help: an action, or one subcommand line of a subparsers action
/// (`_ChoicesPseudoAction`, `argparse.py:1344-1352`).
enum Row<'a> {
    Action(&'a Action),
    Choice {
        name: &'static str,
        help: Option<&'static str>,
    },
}

pub(super) struct Formatter {
    prog: String,
    width: i64,
    max_help_position: i64,
    current_indent: i64,
    action_max_length: i64,
}

impl Formatter {
    /// `HelpFormatter(prog)` for a terminal of `columns` (`argparse.py:180-207`).
    pub(super) fn new(prog: &str, columns: i64) -> Self {
        let width = columns.saturating_sub(2);
        Self {
            prog: prog.to_owned(),
            width,
            max_help_position: 24.min((width.saturating_sub(20)).max(INDENT * 2)),
            current_indent: 0,
            action_max_length: 0,
        }
    }

    /// `_metavar_formatter(action, default)(1)` (`argparse.py:658-669`): the metavar, else
    /// `{a,b}` for choices, else the default.
    fn metavar(action: &Action, default: &str) -> String {
        match action.choices() {
            Some(choices) => format!("{{{}}}", choices.join(",")),
            None => default.to_owned(),
        }
    }

    /// `_format_args` (`argparse.py:671-697`) for `nargs=None` and `PARSER`.
    fn format_args(action: &Action, default: &str) -> String {
        let metavar = Self::metavar(action, default);
        match action.nargs() {
            Nargs::Parser => format!("{metavar} ..."),
            Nargs::One | Nargs::Zero => metavar,
        }
    }

    /// `_format_action_invocation` (`argparse.py:618-656`).
    fn invocation(row: &Row<'_>) -> String {
        match row {
            Row::Choice { name, .. } => (*name).to_owned(),
            Row::Action(action) if action.option_strings.is_empty() => {
                Self::metavar(action, action.dest.unwrap_or(""))
            }
            Row::Action(action) if action.nargs() == Nargs::Zero => {
                action.option_strings.join(", ")
            }
            Row::Action(action) => {
                let default = action.dest.unwrap_or("").to_uppercase();
                format!(
                    "{} {}",
                    action.option_strings.join(", "),
                    Self::format_args(action, &default)
                )
            }
        }
    }

    fn subrows(action: &Action) -> Vec<Row<'_>> {
        match &action.kind {
            ActionKind::SubParsers(commands) => commands
                .iter()
                .map(|c| Row::Choice {
                    name: c.name,
                    help: c.help,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `add_argument` (`argparse.py:284-298`): the widest invocation, sub-rows included.
    fn add_argument(&mut self, action: &Action) {
        let mut longest = len(&Self::invocation(&Row::Action(action))) + self.current_indent;
        for sub in Self::subrows(action) {
            longest = longest.max(len(&Self::invocation(&sub)) + self.current_indent + INDENT);
        }
        self.action_max_length = self.action_max_length.max(longest);
    }

    /// `_get_actions_usage_parts` (`argparse.py:442-537`) without groups: the optionals in
    /// order, then the positionals, and the index where the positionals start.
    fn usage_parts(actions: &[Action]) -> (Vec<String>, usize) {
        let mut parts = Vec::new();
        for action in actions.iter().filter(|a| !a.option_strings.is_empty()) {
            let option = action.option_strings[0];
            let part = if action.nargs() == Nargs::Zero {
                option.to_owned()
            } else {
                let default = action.dest.unwrap_or("").to_uppercase();
                format!("{option} {}", Self::format_args(action, &default))
            };
            parts.push(if action.required {
                part
            } else {
                format!("[{part}]")
            });
        }
        let pos_start = parts.len();
        for action in actions.iter().filter(|a| a.option_strings.is_empty()) {
            parts.push(Self::format_args(action, action.dest.unwrap_or("")));
        }
        (parts, pos_start)
    }

    /// `_format_usage` (`argparse.py:337-438`) with the default `usage: ` prefix.
    fn format_usage(&self, actions: &[Action]) -> String {
        let prefix = "usage: ";
        let prog = self.prog.as_str();
        let (parts, pos_start) = Self::usage_parts(actions);
        let mut usage = std::iter::once(prog.to_owned())
            .chain(parts.iter().cloned())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let text_width = self.width - self.current_indent;
        if len(prefix) + len(&usage) > text_width {
            let opt_parts = &parts[..pos_start];
            let pos_parts = &parts[pos_start..];
            let get_lines = |parts: &[String], indent: &str, prefix: Option<&str>| -> Vec<String> {
                let mut lines: Vec<String> = Vec::new();
                let mut line: Vec<&str> = Vec::new();
                let indent_length = len(indent);
                let mut line_len = prefix.map_or(indent_length - 1, |p| len(p) - 1);
                for part in parts {
                    let part_len = len(part);
                    if line_len + 1 + part_len > text_width && !line.is_empty() {
                        lines.push(format!("{indent}{}", line.join(" ")));
                        line.clear();
                        line_len = indent_length - 1;
                    }
                    line.push(part);
                    line_len += part_len + 1;
                }
                if !line.is_empty() {
                    lines.push(format!("{indent}{}", line.join(" ")));
                }
                if prefix.is_some()
                    && let Some(first) = lines.first_mut()
                {
                    *first = first.chars().skip(indent.chars().count()).collect();
                }
                lines
            };
            let prog_len = len(prog);
            // len(prefix) + prog_len <= 0.75 * text_width, exactly in integers.
            let lines = if 4 * (len(prefix) + prog_len) <= 3 * text_width {
                let indent = spaces(len(prefix) + prog_len + 1);
                let with_prog = |rest: &[String]| -> Vec<String> {
                    std::iter::once(prog.to_owned())
                        .chain(rest.iter().cloned())
                        .collect()
                };
                if !opt_parts.is_empty() {
                    let mut lines = get_lines(&with_prog(opt_parts), &indent, Some(prefix));
                    lines.extend(get_lines(pos_parts, &indent, None));
                    lines
                } else if !pos_parts.is_empty() {
                    get_lines(&with_prog(pos_parts), &indent, Some(prefix))
                } else {
                    vec![prog.to_owned()]
                }
            } else {
                let indent = spaces(len(prefix));
                let all: Vec<String> = parts.clone();
                let mut lines = get_lines(&all, &indent, None);
                if lines.len() > 1 {
                    lines = get_lines(opt_parts, &indent, None);
                    lines.extend(get_lines(pos_parts, &indent, None));
                }
                std::iter::once(prog.to_owned()).chain(lines).collect()
            };
            usage = lines.join("\n");
        }
        format!("{prefix}{usage}\n\n")
    }

    /// `_format_text` (`argparse.py:539-546`) and `_fill_text` (`:809-814`).
    fn format_text(&self, text: &str) -> String {
        let text_width = (self.width - self.current_indent).max(11);
        let indent = spaces(self.current_indent);
        let collapsed = collapse_whitespace(text);
        let lines = textwrap::fill_lines(
            &collapsed,
            usize::try_from(text_width).unwrap_or(11),
            &indent,
            &indent,
        );
        format!("{}\n\n", lines.join("\n"))
    }

    /// `_format_action` (`argparse.py:563-616`).
    fn format_action(&mut self, row: &Row<'_>) -> String {
        let help_position = (self.action_max_length + 2).min(self.max_help_position);
        let help_width = (self.width - help_position).max(11);
        let action_width = help_position - self.current_indent - 2;
        let header = Self::invocation(row);
        let help = match row {
            Row::Action(action) => action.help.as_deref(),
            Row::Choice { help, .. } => *help,
        };
        let help = help.filter(|h| !h.is_empty());
        let indent = spaces(self.current_indent);
        let (first, indent_first) = if help.is_none() {
            // no help; start on same line and add a final newline
            (format!("{indent}{header}\n"), 0)
        } else if len(&header) <= action_width {
            // short action name; start on the same line and pad two spaces
            let pad = spaces(action_width - len(&header));
            (format!("{indent}{header}{pad}  "), 0)
        } else {
            // long action name; start on the next line
            (format!("{indent}{header}\n"), help_position)
        };
        let mut parts = vec![first];
        match help {
            Some(text) if !text.trim().is_empty() => {
                let lines = textwrap::wrap(
                    &collapse_whitespace(text),
                    usize::try_from(help_width).unwrap_or(11),
                );
                for (i, line) in lines.iter().enumerate() {
                    let indent = if i == 0 { indent_first } else { help_position };
                    parts.push(format!("{}{line}\n", spaces(indent)));
                }
            }
            _ => {
                if !parts.last().is_some_and(|h| h.ends_with('\n')) {
                    parts.push("\n".to_owned());
                }
            }
        }
        if let Row::Action(action) = row {
            self.current_indent += INDENT;
            for sub in Self::subrows(action) {
                parts.push(self.format_action(&sub));
            }
            self.current_indent -= INDENT;
        }
        parts.concat()
    }

    /// A `_Section` (`argparse.py:236-271`): indented items under `title:`, or nothing.
    fn section(&mut self, title: &str, actions: &[&Action]) -> String {
        self.current_indent += INDENT;
        let items: String = actions
            .iter()
            .map(|a| self.format_action(&Row::Action(a)))
            .collect();
        self.current_indent -= INDENT;
        if items.is_empty() {
            return String::new();
        }
        format!("\n{}{title}:\n{items}\n", spaces(self.current_indent))
    }

    /// `HelpFormatter.format_help` (`argparse.py:316-321`): runs of three or more newlines
    /// become two, outer newlines go, one is added.
    fn finish(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut newlines = 0_usize;
        for c in text.chars() {
            if c == '\n' {
                newlines += 1;
                continue;
            }
            out.push_str(&"\n".repeat(newlines.min(2)));
            newlines = 0;
            out.push(c);
        }
        out.push_str(&"\n".repeat(newlines.min(2)));
        format!("{}\n", out.trim_matches('\n'))
    }

    /// `ArgumentParser.format_usage` (`argparse.py:2843-2848`).
    pub(super) fn usage_only(&self, parser: &Parser) -> String {
        Self::finish(&self.format_usage(&parser.actions))
    }

    /// `ArgumentParser.format_help` (`argparse.py:2850-2872`): usage, description, the
    /// positional arguments and the options.
    pub(super) fn help(&mut self, parser: &Parser) -> String {
        let positionals: Vec<&Action> = parser
            .actions
            .iter()
            .filter(|a| a.option_strings.is_empty())
            .collect();
        let optionals: Vec<&Action> = parser
            .actions
            .iter()
            .filter(|a| !a.option_strings.is_empty())
            .collect();
        // add_argument runs inside each section, one indent level in.
        self.current_indent += INDENT;
        for action in positionals.iter().chain(&optionals) {
            self.add_argument(action);
        }
        self.current_indent -= INDENT;
        let mut text = self.format_usage(&parser.actions);
        if let Some(description) = parser.description {
            text.push_str(&self.format_text(description));
        }
        text.push_str(&self.section("positional arguments", &positionals));
        text.push_str(&self.section("options", &optionals));
        Self::finish(&text)
    }
}

/// `_whitespace_matcher.sub(' ', text).strip()` with `re.ASCII` `\s`.
fn collapse_whitespace(text: &str) -> String {
    text.split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
