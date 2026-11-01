//! An argparse-compatible parser for the fixed structure of `gcg-sim` (docs/port/PLAN.md
//! §10.4 E, A30, T-072, T-152), shared by `gcg-sim` (`crates/gcg-cli/src/cli.rs` builds its
//! parser) and `gcg-eval`: the parsing algorithm, messages, usage and help of CPython
//! 3.15.1's `argparse` (`Lib/argparse.py`) for the features `cli.py:68-95` uses, and nothing
//! else: long options with one value (`type=`, `choices=`, `required=`, `default=`),
//! `store_true` flags, the automatic `-h/--help`, positionals with one value, and required
//! subcommands (`add_subparsers(dest=..., required=True)`), with `allow_abbrev`,
//! `suggest_on_error` and `exit_on_error` at their defaults. There are no mutually exclusive
//! groups, `nargs` other than the default, `fromfile_prefix_chars`, deprecated arguments or
//! parser-level defaults, so their code paths are not ported.
//!
//! Output is byte-identical to Python's for a non-TTY stdout at any `COLUMNS` (transcripts of
//! `port/oracle/export_cli.py`). Two documented differences remain on a terminal: Python
//! colours help and errors (3.15's `color=True`) and, without `COLUMNS`, wraps to the
//! terminal's width; this port never colours and wraps to 80 columns, because reading the
//! terminal size needs an `ioctl`, which safe Rust cannot make.
//!
//! The module depends on no other `gcg-*` crate except `gcg-core::lookup`. It lives in
//! `gcg-runner`, because `gcg-eval` cannot depend on the `gcg-sim` crate
//! (docs/port/contracts.md, open item O11).

pub mod difflib;
mod format;
pub mod pyint;
pub mod pypath;
pub mod pystr;
pub mod textwrap;
mod unicode;
pub mod unicode_tables;

use std::cmp::Ordering;

pub use pyint::PyInt;
pub use pystr::PyStr;

/// A converted argument value, as it lands in the namespace.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Value {
    /// `None` (a positional or option that was not given and has no default).
    None,
    Bool(bool),
    Str(PyStr),
    /// `pathlib.Path(text)`, held as `str(path)` (the text Python prints and opens).
    Path(PyStr),
    Int(PyInt),
    /// `--workers auto` (the additive Rust value, PLAN A5).
    Auto,
}

/// The `type=` callables of the `gcg-sim` parser (`cli.py:40-65`), plus the Rust `--workers`
/// converter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Converter {
    /// No `type=`: the text itself.
    Str,
    /// `type=Path`.
    Path,
    /// `_positive_int` (`cli.py:40-44`).
    PositiveInt,
    /// `_non_negative_int` (`cli.py:47-51`).
    NonNegativeInt,
    /// `_seed` (`cli.py:54-58`).
    Seed,
    /// `auto`, or `_positive_int` (PLAN A5; numeric failures read exactly as Python's).
    Workers,
}

/// Why a converter rejected a text (`argparse.py:2790-2812`).
enum ConvError {
    /// `ArgumentTypeError(message)`.
    Type(PyStr),
    /// `ValueError` (here only `str()` of an integer above 4300 digits): argparse reports
    /// `invalid {type.__name__} value: {text!r}`.
    Value,
}

impl Converter {
    /// `type.__name__` (`argparse.py:2806`).
    const fn type_name(self) -> &'static str {
        match self {
            Self::Str => "str",
            Self::Path => "Path",
            Self::PositiveInt | Self::Workers => "_positive_int",
            Self::NonNegativeInt => "_non_negative_int",
            Self::Seed => "_seed",
        }
    }

    /// `_int` (`cli.py:61-65`): `int(text, 0)`, else `not an integer: {text!r}`.
    fn int(text: &PyStr) -> Result<PyInt, ConvError> {
        pyint::parse_int(text, 0).ok_or_else(|| {
            let mut msg = PyStr::from("not an integer: ");
            msg.push_py(&text.repr());
            ConvError::Type(msg)
        })
    }

    /// `ArgumentTypeError(f"{prefix}{value}")`, or the `ValueError` of `str(value)`.
    fn range_error(prefix: &str, value: &PyInt) -> ConvError {
        match value.py_str() {
            Some(digits) => ConvError::Type(PyStr::from(format!("{prefix}{digits}").as_str())),
            None => ConvError::Value,
        }
    }

    fn positive_int(text: &PyStr) -> Result<Value, ConvError> {
        let value = Self::int(text)?;
        if value.cmp_i64(1) == Ordering::Less {
            return Err(Self::range_error("must be at least 1, got ", &value));
        }
        Ok(Value::Int(value))
    }

    fn convert(self, text: &PyStr) -> Result<Value, ConvError> {
        match self {
            Self::Str => Ok(Value::Str(text.clone())),
            Self::Path => Ok(Value::Path(pypath::path_str(text))),
            Self::PositiveInt => Self::positive_int(text),
            Self::Workers if text.eq_str("auto") => Ok(Value::Auto),
            Self::Workers => Self::positive_int(text),
            Self::NonNegativeInt => {
                let value = Self::int(text)?;
                if value.is_negative() {
                    return Err(Self::range_error("must be 0 or more, got ", &value));
                }
                Ok(Value::Int(value))
            }
            Self::Seed => {
                let value = Self::int(text)?;
                if value.to_u64().is_none() {
                    return Err(Self::range_error("must be in [0, 2**64), got ", &value));
                }
                Ok(Value::Int(value))
            }
        }
    }
}

/// What an action does when it is reached.
#[derive(Clone, Debug)]
pub enum ActionKind {
    /// `_HelpAction` (`argparse.py:1290-1310`): print the help and exit 0.
    Help,
    /// `_StoreTrueAction` (`argparse.py:1160-1177`).
    StoreTrue,
    /// `_StoreAction` (`argparse.py:1098-1133`) with its `type=` and `choices=`.
    Store {
        convert: Converter,
        choices: Option<Vec<&'static str>>,
    },
    /// `_SubParsersAction` (`argparse.py:1342-1458`), in `add_parser` order.
    SubParsers(Vec<SubCommand>),
}

/// One `add_parser(name, help=...)` (`argparse.py:1379-1418`).
#[derive(Clone, Debug)]
pub struct SubCommand {
    pub name: &'static str,
    pub help: Option<&'static str>,
    pub parser: Parser,
}

/// One `add_argument` (or the automatic help, or `add_subparsers`).
#[derive(Clone, Debug)]
pub struct Action {
    /// Empty for a positional.
    pub option_strings: Vec<&'static str>,
    /// `None` is `SUPPRESS` (the help action).
    pub dest: Option<&'static str>,
    pub kind: ActionKind,
    pub required: bool,
    pub help: Option<String>,
    /// `None` is `SUPPRESS`: nothing is stored before parsing.
    pub default: Option<Value>,
}

/// `nargs` of the actions this port supports.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Nargs {
    /// `nargs=None`: one value.
    One,
    /// `nargs=0`: flags and help.
    Zero,
    /// `nargs=PARSER`: a subcommand name and everything after it.
    Parser,
}

impl Action {
    /// A positional argument with one value (`add_argument(dest, type=...)`).
    #[must_use]
    pub const fn positional(dest: &'static str, convert: Converter, help: Option<String>) -> Self {
        Self {
            option_strings: Vec::new(),
            dest: Some(dest),
            kind: ActionKind::Store {
                convert,
                choices: None,
            },
            required: true,
            help,
            default: Some(Value::None),
        }
    }

    /// A long option with one value; `dest` derives from the option string as
    /// `_get_optional_kwargs` does (`argparse.py:1795-1831`): `--ai-preset` stores `ai_preset`.
    #[must_use]
    pub fn option(
        option: &'static str,
        dest: &'static str,
        convert: Converter,
        choices: Option<Vec<&'static str>>,
        default: Value,
        help: String,
    ) -> Self {
        Self {
            option_strings: vec![option],
            dest: Some(dest),
            kind: ActionKind::Store { convert, choices },
            required: false,
            help: Some(help),
            default: Some(default),
        }
    }

    /// A `store_true` flag (default `False`).
    #[must_use]
    pub fn flag(option: &'static str, dest: &'static str, help: String) -> Self {
        Self {
            option_strings: vec![option],
            dest: Some(dest),
            kind: ActionKind::StoreTrue,
            required: false,
            help: Some(help),
            default: Some(Value::Bool(false)),
        }
    }

    /// `add_subparsers(dest=dest, required=True)` (`argparse.py:2090-2125`); no help.
    #[must_use]
    pub const fn subcommands(dest: &'static str, commands: Vec<SubCommand>) -> Self {
        Self {
            option_strings: Vec::new(),
            dest: Some(dest),
            kind: ActionKind::SubParsers(commands),
            required: true,
            help: None,
            default: Some(Value::None),
        }
    }

    const fn nargs(&self) -> Nargs {
        match self.kind {
            ActionKind::Help | ActionKind::StoreTrue => Nargs::Zero,
            ActionKind::Store { .. } => Nargs::One,
            ActionKind::SubParsers(_) => Nargs::Parser,
        }
    }

    /// The `choices` as strings (subcommand names for a subparsers action).
    fn choices(&self) -> Option<Vec<&'static str>> {
        match &self.kind {
            ActionKind::Store { choices, .. } => choices.clone(),
            ActionKind::SubParsers(commands) => Some(commands.iter().map(|c| c.name).collect()),
            ActionKind::Help | ActionKind::StoreTrue => None,
        }
    }

    /// `_get_action_name` (`argparse.py:898-918`): `-h/--help`, `--matches`, `deck`,
    /// `command`.
    fn name(&self) -> PyStr {
        if self.option_strings.is_empty() {
            PyStr::from(self.dest.unwrap_or(""))
        } else {
            PyStr::from(self.option_strings.join("/").as_str())
        }
    }
}

/// An `ArgumentParser` (`argparse.py:1983-2066`) with `add_help=True`: the help action is
/// always `actions[0]`.
#[derive(Clone, Debug)]
pub struct Parser {
    pub prog: String,
    pub description: Option<&'static str>,
    pub actions: Vec<Action>,
}

impl Parser {
    /// A parser with only `-h/--help` (`argparse.py:2053-2058`).
    #[must_use]
    pub fn new(prog: &str, description: Option<&'static str>) -> Self {
        Self {
            prog: prog.to_owned(),
            description,
            actions: vec![Action {
                option_strings: vec!["-h", "--help"],
                dest: None,
                kind: ActionKind::Help,
                required: false,
                help: Some("show this help message and exit".to_owned()),
                default: None,
            }],
        }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// The parser of a subcommand, if `self` has one named `name`.
    #[must_use]
    pub fn subcommand(&self, name: &str) -> Option<&Self> {
        self.actions.iter().find_map(|a| match &a.kind {
            ActionKind::SubParsers(commands) => {
                commands.iter().find(|c| c.name == name).map(|c| &c.parser)
            }
            _ => None,
        })
    }

    /// `_option_string_actions`, in insertion order (`argparse.py:1708-1727`).
    fn option_strings(&self) -> impl Iterator<Item = (&'static str, usize)> + '_ {
        self.actions
            .iter()
            .enumerate()
            .flat_map(|(i, a)| a.option_strings.iter().map(move |s| (*s, i)))
    }

    fn lookup_option(&self, s: &PyStr) -> Option<usize> {
        self.option_strings()
            .find(|(os, _)| s.eq_str(os))
            .map(|(_, i)| i)
    }

    /// `format_usage()` (`argparse.py:2843-2848`).
    #[must_use]
    pub fn format_usage(&self, columns: i64) -> String {
        format::Formatter::new(&self.prog, columns).usage_only(self)
    }

    /// `format_help()` (`argparse.py:2850-2872`).
    #[must_use]
    pub fn format_help(&self, columns: i64) -> String {
        format::Formatter::new(&self.prog, columns).help(self)
    }

    /// `error(message)` (`argparse.py:2942-2958`): the usage, then `{prog}: error:
    /// {message}`, on stderr, exit 2.
    #[must_use]
    pub fn error(&self, message: &PyStr, columns: i64) -> Exit {
        let mut stderr = self.format_usage(columns).into_bytes();
        stderr.extend_from_slice(format!("{}: error: ", self.prog).as_bytes());
        stderr.extend_from_slice(&message.to_stderr_bytes());
        stderr.push(b'\n');
        Exit {
            status: 2,
            stdout: Vec::new(),
            stderr,
        }
    }

    /// `parse_args(args)` (`argparse.py:2148-2156`): the namespace, or the exit that
    /// argparse's `SystemExit` stands for (help, or a usage error).
    ///
    /// # Errors
    /// The [`Exit`] with the bytes Python writes to stdout and stderr.
    pub fn parse_args(&self, args: Vec<PyStr>, columns: i64) -> Result<Namespace, Exit> {
        let (ns, extras) = self.parse_known_args(args, Namespace::default(), columns)?;
        if extras.is_empty() {
            return Ok(ns);
        }
        let mut msg = PyStr::from("unrecognized arguments: ");
        for (i, extra) in extras.iter().enumerate() {
            if i > 0 {
                msg.push_str(" ");
            }
            msg.push_py(extra);
        }
        Err(self.error(&msg, columns))
    }

    /// `parse_known_args` / `_parse_known_args2` (`argparse.py:2158-2197`).
    fn parse_known_args(
        &self,
        args: Vec<PyStr>,
        mut ns: Namespace,
        columns: i64,
    ) -> Result<(Namespace, Vec<PyStr>), Exit> {
        for action in &self.actions {
            if let (Some(dest), Some(default)) = (action.dest, &action.default)
                && !ns.has(dest)
            {
                ns.set(dest, default.clone());
            }
        }
        match self.parse_known_inner(&args, &mut ns, columns) {
            Ok(mut extras) => {
                if let Some(unrecognized) = ns.unrecognized.take() {
                    extras.extend(unrecognized);
                }
                Ok((ns, extras))
            }
            Err(Flow::Error(err)) => Err(self.error(&err.text(), columns)),
            Err(Flow::Exit(exit)) => Err(exit),
        }
    }

    /// `_parse_optional` (`argparse.py:2575-2619`).
    fn parse_optional(&self, arg: &PyStr) -> Result<Option<Vec<OptionTuple>>, Flow> {
        if arg.at(0) != Some(u32::from('-')) {
            return Ok(None);
        }
        if let Some(action) = self.lookup_option(arg) {
            return Ok(Some(vec![OptionTuple {
                action: Some(action),
                option_string: arg.clone(),
                sep_eq: false,
                explicit_arg: None,
            }]));
        }
        if arg.len() == 1 {
            return Ok(None);
        }
        let (option_string, explicit) = arg.partition_eq();
        if let Some(explicit) = &explicit
            && let Some(action) = self.lookup_option(&option_string)
        {
            return Ok(Some(vec![OptionTuple {
                action: Some(action),
                option_string,
                sep_eq: true,
                explicit_arg: Some(explicit.clone()),
            }]));
        }
        let tuples = self.option_tuples(arg)?;
        if !tuples.is_empty() {
            return Ok(Some(tuples));
        }
        // `_negative_number_matcher` (`-\.?\d`); this parser has no negative-number-like
        // options, so such an argument is positional.
        let digit = |i: usize| arg.at(i).is_some_and(|cp| unicode::decimal(cp).is_some());
        if digit(1) || (arg.at(1) == Some(u32::from('.')) && digit(2)) {
            return Ok(None);
        }
        if arg.contains_char(' ') {
            return Ok(None);
        }
        Ok(Some(vec![OptionTuple {
            action: None,
            option_string: arg.clone(),
            sep_eq: false,
            explicit_arg: None,
        }]))
    }

    /// `_get_option_tuples` (`argparse.py:2621-2663`) with `allow_abbrev=True`.
    fn option_tuples(&self, arg: &PyStr) -> Result<Vec<OptionTuple>, Flow> {
        let dash = Some(u32::from('-'));
        let mut out = Vec::new();
        let (option_prefix, explicit) = arg.partition_eq();
        if arg.at(0) == dash && arg.at(1) == dash {
            for (os, action) in self.option_strings() {
                if PyStr::from(os).starts_with(&option_prefix) {
                    out.push(OptionTuple {
                        action: Some(action),
                        option_string: PyStr::from(os),
                        sep_eq: explicit.is_some(),
                        explicit_arg: explicit.clone(),
                    });
                }
            }
        } else if arg.at(0) == dash {
            let short_prefix = arg.slice(0, 2);
            let short_explicit = arg.slice(2, arg.len());
            for (os, action) in self.option_strings() {
                if short_prefix.eq_str(os) {
                    out.push(OptionTuple {
                        action: Some(action),
                        option_string: PyStr::from(os),
                        sep_eq: false,
                        explicit_arg: Some(short_explicit.clone()),
                    });
                } else if PyStr::from(os).starts_with(&option_prefix) {
                    out.push(OptionTuple {
                        action: Some(action),
                        option_string: PyStr::from(os),
                        sep_eq: explicit.is_some(),
                        explicit_arg: explicit.clone(),
                    });
                }
            }
        } else {
            let mut msg = PyStr::from("unexpected option string: ");
            msg.push_py(arg);
            return Err(Flow::Error(ArgError {
                argument: None,
                message: msg,
            }));
        }
        Ok(out)
    }

    /// `_match_argument` (`argparse.py:2536-2556`) for an option.
    fn match_argument(&self, action: usize, pattern: &[u8]) -> Result<usize, Flow> {
        match self.actions[action].nargs() {
            Nargs::Zero => Ok(0),
            Nargs::One if pattern.first() == Some(&b'A') => Ok(1),
            Nargs::One | Nargs::Parser => Err(Flow::Error(ArgError {
                argument: Some(self.actions[action].name()),
                message: PyStr::from("expected one argument"),
            })),
        }
    }

    /// `_get_values` (`argparse.py:2743-2788`) for the supported `nargs`.
    fn get_values(&self, action: usize, args: &[PyStr]) -> Result<Values, Flow> {
        let a = &self.actions[action];
        match (&a.kind, args) {
            (ActionKind::Store { convert, .. }, [arg]) => {
                let value = self.get_value(action, *convert, arg)?;
                self.check_value(action, &value)?;
                Ok(Values::One(value))
            }
            (ActionKind::SubParsers(_), [first, ..]) => {
                self.check_value(action, &Value::Str(first.clone()))?;
                Ok(Values::Many(args.to_vec()))
            }
            _ => Ok(Values::Many(Vec::new())),
        }
    }

    /// `_get_value` (`argparse.py:2790-2812`).
    fn get_value(&self, action: usize, convert: Converter, arg: &PyStr) -> Result<Value, Flow> {
        convert.convert(arg).map_err(|e| {
            let message = match e {
                ConvError::Type(msg) => msg,
                ConvError::Value => {
                    let mut msg =
                        PyStr::from(format!("invalid {} value: ", convert.type_name()).as_str());
                    msg.push_py(&arg.repr());
                    msg
                }
            };
            Flow::Error(ArgError {
                argument: Some(self.actions[action].name()),
                message,
            })
        })
    }

    /// `_check_value` (`argparse.py:2814-2841`), with the `difflib` suggestion.
    fn check_value(&self, action: usize, value: &Value) -> Result<(), Flow> {
        let a = &self.actions[action];
        let Some(choices) = a.choices() else {
            return Ok(());
        };
        let Value::Str(text) = value else {
            return Ok(());
        };
        if choices.iter().any(|c| text.eq_str(c)) {
            return Ok(());
        }
        let listed = choices
            .iter()
            .map(|c| PyStr::from(*c).repr().to_string_opt().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ");
        let choice_cps: Vec<Vec<u32>> = choices
            .iter()
            .map(|c| PyStr::from(*c).code_points().to_vec())
            .collect();
        let refs: Vec<&[u32]> = choice_cps.iter().map(Vec::as_slice).collect();
        let mut msg = PyStr::from("invalid choice: ");
        msg.push_py(&text.repr());
        if let Some(closest) = difflib::close_match(text.code_points(), &refs) {
            msg.push_str(", maybe you meant ");
            msg.push_py(&PyStr::from_code_points(closest.to_vec()).repr());
            msg.push_str("?");
        }
        msg.push_str(&format!(" (choose from {listed})"));
        Err(Flow::Error(ArgError {
            argument: Some(a.name()),
            message: msg,
        }))
    }

    /// `take_action` (`argparse.py:2247-2265`) and the action's `__call__`.
    fn take_action(
        &self,
        action: usize,
        args: &[PyStr],
        st: &mut ParseState<'_>,
    ) -> Result<(), Flow> {
        st.seen[action] = true;
        let values = self.get_values(action, args)?;
        let a = &self.actions[action];
        match (&a.kind, values) {
            (ActionKind::Help, _) => Err(Flow::Exit(Exit {
                status: 0,
                stdout: self.format_help(st.columns).into_bytes(),
                stderr: Vec::new(),
            })),
            (ActionKind::StoreTrue, _) => {
                st.ns.set(a.dest.unwrap_or(""), Value::Bool(true));
                Ok(())
            }
            (ActionKind::Store { .. }, Values::One(value)) => {
                st.ns.set(a.dest.unwrap_or(""), value);
                Ok(())
            }
            (ActionKind::SubParsers(commands), Values::Many(values)) => {
                // _SubParsersAction.__call__ (argparse.py:1423-1458)
                let Some((name, rest)) = values.split_first() else {
                    return Ok(());
                };
                if let Some(dest) = a.dest {
                    st.ns.set(dest, Value::Str(name.clone()));
                }
                let Some(command) = commands.iter().find(|c| name.eq_str(c.name)) else {
                    let mut msg = PyStr::from("unknown parser ");
                    msg.push_py(&name.repr());
                    return Err(Flow::Error(ArgError {
                        argument: Some(a.name()),
                        message: msg,
                    }));
                };
                let (sub_ns, extras) = command
                    .parser
                    .parse_known_args(rest.to_vec(), Namespace::default(), st.columns)
                    .map_err(Flow::Exit)?;
                for (key, value) in sub_ns.entries {
                    st.ns.set(key, value);
                }
                if !extras.is_empty() {
                    st.ns
                        .unrecognized
                        .get_or_insert_with(Vec::new)
                        .extend(extras);
                }
                Ok(())
            }
            (ActionKind::Store { .. } | ActionKind::SubParsers(_), _) => Ok(()),
        }
    }

    /// `consume_optional` (`argparse.py:2267-2365`).
    fn consume_optional(&self, start: usize, st: &mut ParseState<'_>) -> Result<usize, Flow> {
        let tuples = st.option_tuples[start].clone().unwrap_or_default();
        if tuples.len() > 1 {
            let options = tuples
                .iter()
                .map(|t| t.option_string.clone())
                .collect::<Vec<_>>();
            let mut msg = PyStr::from("ambiguous option: ");
            msg.push_py(&st.args[start]);
            msg.push_str(" could match ");
            for (i, o) in options.iter().enumerate() {
                if i > 0 {
                    msg.push_str(", ");
                }
                msg.push_py(o);
            }
            return Err(Flow::Error(ArgError {
                argument: None,
                message: msg,
            }));
        }
        let Some(first) = tuples.into_iter().next() else {
            return Ok(start + 1);
        };
        let OptionTuple {
            mut action,
            mut option_string,
            mut sep_eq,
            mut explicit_arg,
        } = first;
        let mut action_tuples: Vec<(usize, Vec<PyStr>)> = Vec::new();
        let stop;
        loop {
            let Some(act) = action else {
                st.extras.push(st.args[start].clone());
                return Ok(start + 1);
            };
            if let Some(explicit) = explicit_arg.take() {
                let arg_count = self.match_argument(act, b"A")?;
                let dash = u32::from('-');
                if arg_count == 0 && option_string.at(1) != Some(dash) && !explicit.is_empty() {
                    if sep_eq || explicit.at(0) == Some(dash) {
                        return Err(ignored_explicit(&self.actions[act], &explicit));
                    }
                    action_tuples.push((act, Vec::new()));
                    let mut next = PyStr::from_code_points(vec![dash]);
                    next.push_py(&explicit.slice(0, 1));
                    option_string = next;
                    if let Some(found) = self.lookup_option(&option_string) {
                        action = Some(found);
                        let rest = explicit.slice(1, explicit.len());
                        if rest.is_empty() {
                            sep_eq = false;
                            explicit_arg = None;
                        } else if rest.at(0) == Some(u32::from('=')) {
                            sep_eq = true;
                            explicit_arg = Some(rest.slice(1, rest.len()));
                        } else {
                            sep_eq = false;
                            explicit_arg = Some(rest);
                        }
                    } else {
                        let mut extra = PyStr::from_code_points(vec![dash]);
                        extra.push_py(&explicit);
                        st.extras.push(extra);
                        stop = start + 1;
                        break;
                    }
                } else if arg_count == 1 {
                    stop = start + 1;
                    action_tuples.push((act, vec![explicit]));
                    break;
                } else {
                    return Err(ignored_explicit(&self.actions[act], &explicit));
                }
            } else {
                let first_arg = start + 1;
                let arg_count = self.match_argument(act, &st.pattern[first_arg..])?;
                stop = first_arg + arg_count;
                action_tuples.push((act, st.args[first_arg..stop].to_vec()));
                break;
            }
        }
        for (act, args) in action_tuples {
            self.take_action(act, &args, st)?;
        }
        Ok(stop)
    }

    /// `consume_positionals` (`argparse.py:2371-2403`).
    fn consume_positionals(&self, start: usize, st: &mut ParseState<'_>) -> Result<usize, Flow> {
        let kinds: Vec<Nargs> = st
            .positionals
            .iter()
            .map(|&i| self.actions[i].nargs())
            .collect();
        let counts = match_arguments_partial(&kinds, &st.pattern[start..]);
        let mut index = start;
        for (k, &count) in counts.iter().enumerate() {
            let action = st.positionals[k];
            let mut args = st.args[index..index + count].to_vec();
            let dashes = st.pattern[index..index + count].contains(&b'-');
            if self.actions[action].nargs() == Nargs::Parser {
                if st.pattern.get(index) == Some(&b'-') && !args.is_empty() {
                    args.remove(0);
                }
            } else if dashes && let Some(pos) = args.iter().position(|a| a.eq_str("--")) {
                args.remove(pos);
            }
            index += count;
            self.take_action(action, &args, st)?;
        }
        st.positionals.drain(..counts.len());
        Ok(index)
    }

    /// `_parse_known_args` (`argparse.py:2199-2504`) without intermixing.
    fn parse_known_inner(
        &self,
        args: &[PyStr],
        ns: &mut Namespace,
        columns: i64,
    ) -> Result<Vec<PyStr>, Flow> {
        let mut option_tuples: Vec<Option<Vec<OptionTuple>>> = vec![None; args.len()];
        let mut pattern: Vec<u8> = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            if arg.eq_str("--") {
                pattern.push(b'-');
                pattern.extend(std::iter::repeat_n(b'A', args.len() - i - 1));
                break;
            }
            match self.parse_optional(arg)? {
                None => pattern.push(b'A'),
                Some(tuples) => {
                    option_tuples[i] = Some(tuples);
                    pattern.push(b'O');
                }
            }
        }
        let positionals: Vec<usize> = self
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| a.option_strings.is_empty())
            .map(|(i, _)| i)
            .collect();
        let max_option = option_tuples.iter().rposition(Option::is_some);
        let mut st = ParseState {
            args,
            pattern,
            option_tuples,
            positionals,
            seen: vec![false; self.actions.len()],
            extras: Vec::new(),
            ns,
            columns,
        };
        let mut start = 0;
        if let Some(max_option) = max_option {
            while start <= max_option {
                let mut next_option = start;
                while next_option <= max_option && st.option_tuples[next_option].is_none() {
                    next_option += 1;
                }
                if start != next_option {
                    let end = self.consume_positionals(start, &mut st)?;
                    if end > start {
                        start = end;
                        continue;
                    }
                    start = end;
                }
                if st.option_tuples[start].is_none() {
                    st.extras.extend(args[start..next_option].iter().cloned());
                    start = next_option;
                }
                start = self.consume_optional(start, &mut st)?;
            }
        }
        let stop = self.consume_positionals(start, &mut st)?;
        st.extras.extend(args[stop..].iter().cloned());
        // Required actions (argparse.py:2463-2489). The string-default conversion of the
        // same loop applies `type=` to str defaults; every str default here has no `type=`,
        // so it changes nothing.
        let missing: Vec<PyStr> = self
            .actions
            .iter()
            .zip(&st.seen)
            .filter(|(a, seen)| a.required && !**seen)
            .map(|(a, _)| a.name())
            .collect();
        if !missing.is_empty() {
            let mut msg = PyStr::from("the following arguments are required: ");
            for (i, name) in missing.iter().enumerate() {
                if i > 0 {
                    msg.push_str(", ");
                }
                msg.push_py(name);
            }
            return Err(Flow::Error(ArgError {
                argument: None,
                message: msg,
            }));
        }
        Ok(st.extras)
    }
}

fn ignored_explicit(action: &Action, explicit: &PyStr) -> Flow {
    let mut msg = PyStr::from("ignored explicit argument ");
    msg.push_py(&explicit.repr());
    Flow::Error(ArgError {
        argument: Some(action.name()),
        message: msg,
    })
}

/// The group lengths of `_match_arguments_partial` (`argparse.py:2558-2573`) for the
/// longest prefix of `kinds` whose `_get_nargs_pattern`s (`argparse.py:2665-2709`: `(-*A-*)`
/// for one value, `(-*A[-AO]*)` for a subcommand) match at the start of `pattern`, with the
/// backtracking order of Python's `re` (greedy first).
fn match_arguments_partial(kinds: &[Nargs], pattern: &[u8]) -> Vec<usize> {
    for i in (1..=kinds.len()).rev() {
        let mut counts = Vec::with_capacity(i);
        if let Some(end) = match_groups(&kinds[..i], pattern, 0, &mut counts) {
            if end < pattern.len() && pattern[end] == b'O' {
                while counts.last() == Some(&0) {
                    counts.pop();
                }
            }
            return counts;
        }
    }
    Vec::new()
}

fn match_groups(
    kinds: &[Nargs],
    pattern: &[u8],
    pos: usize,
    counts: &mut Vec<usize>,
) -> Option<usize> {
    let Some((kind, rest)) = kinds.split_first() else {
        return Some(pos);
    };
    let leading = pattern[pos..].iter().take_while(|&&c| c == b'-').count();
    let a = pos + leading;
    if pattern.get(a) != Some(&b'A') {
        return None;
    }
    let last = match kind {
        Nargs::Parser => pattern.len(),
        Nargs::One | Nargs::Zero => {
            a + 1 + pattern[a + 1..].iter().take_while(|&&c| c == b'-').count()
        }
    };
    for end in (a + 1..=last).rev() {
        counts.push(end - pos);
        if let Some(done) = match_groups(rest, pattern, end, counts) {
            return Some(done);
        }
        counts.pop();
    }
    None
}

/// One interpretation of an option string (`argparse.py:2575-2663`): the action (`None` for
/// an unknown option), the option string, whether a `=` separated it, and the attached
/// argument.
#[derive(Clone, Debug)]
struct OptionTuple {
    action: Option<usize>,
    option_string: PyStr,
    sep_eq: bool,
    explicit_arg: Option<PyStr>,
}

/// The values `_get_values` produced.
enum Values {
    One(Value),
    Many(Vec<PyStr>),
}

/// The locals of `_parse_known_args` that its nested functions share.
struct ParseState<'a> {
    args: &'a [PyStr],
    pattern: Vec<u8>,
    option_tuples: Vec<Option<Vec<OptionTuple>>>,
    positionals: Vec<usize>,
    seen: Vec<bool>,
    extras: Vec<PyStr>,
    ns: &'a mut Namespace,
    columns: i64,
}

/// `ArgumentError` (`argparse.py:921-938`).
struct ArgError {
    argument: Option<PyStr>,
    message: PyStr,
}

impl ArgError {
    fn text(&self) -> PyStr {
        match &self.argument {
            None => self.message.clone(),
            Some(name) => {
                let mut out = PyStr::from("argument ");
                out.push_py(name);
                out.push_str(": ");
                out.push_py(&self.message);
                out
            }
        }
    }
}

/// How parsing stopped early.
enum Flow {
    /// An `ArgumentError`, reported by the parser that is parsing.
    Error(ArgError),
    /// A finished exit (help, or an error a subparser already reported).
    Exit(Exit),
}

/// What `SystemExit` stands for: the exit status and the bytes Python wrote to stdout and
/// stderr (stdout `surrogateescape`, stderr `backslashreplace`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Exit {
    pub status: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// An `argparse.Namespace`: `dest -> value` in insertion order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Namespace {
    entries: Vec<(&'static str, Value)>,
    /// `_UNRECOGNIZED_ARGS_ATTR` (`argparse.py:1452-1458`).
    unrecognized: Option<Vec<PyStr>>,
}

impl Namespace {
    fn has(&self, dest: &str) -> bool {
        self.entries.iter().any(|(k, _)| *k == dest)
    }

    fn set(&mut self, dest: &'static str, value: Value) {
        match self.entries.iter_mut().find(|(k, _)| *k == dest) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((dest, value)),
        }
    }

    /// `getattr(namespace, dest)`.
    #[must_use]
    pub fn get(&self, dest: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(k, _)| *k == dest)
            .map(|(_, v)| v)
    }

    /// `vars(namespace).items()`.
    #[must_use]
    pub fn entries(&self) -> &[(&'static str, Value)] {
        &self.entries
    }
}
