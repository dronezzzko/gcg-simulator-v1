//! Canonical move keys and dominated-move pruning for the search tree (`ai/actions.py:31-277`).
//!
//! Two legal actions are the same move when they differ only in which copy of a card in a
//! private or public pile is used, or in how many EX Resources pay for them. Keys name
//! off-field cards the decider knows by definition and field cards by instance, so a key means
//! the same move in every determinization of the decider's information set. The public-move
//! merge (`merge_equivalent`) is in `fingerprint.rs`.

use gcg_effects::{Ev, Node, P, Registry, Side, Timing};
use gcg_engine::observe::determinize;
use gcg_engine::procedures::{
    action_step_options, activated_list, command_playable, declare_attack, set_rested_for_trial,
};
use gcg_engine::view::{ap_of, derived, same_values, trigger_index};
use gcg_engine::{
    ALT_PLAY_BASE, Action, ActionKind, DecisionKind, DecisionView, EngineError, GameState, NO_ARG,
    Uid, Zone,
};

use crate::agent::uid_argument;
use crate::{MoveKey, PruneFlags};

/// `_BY_DEFINITION` (`ai/actions.py:34-36`): zones whose cards a key names by definition.
const BY_DEFINITION: [Zone; 6] = [
    Zone::Hand,
    Zone::Trash,
    Zone::Deck,
    Zone::Removal,
    Zone::Shield,
    Zone::ResourceDeck,
];
/// `_DEF_OFFSET = 1 << 24` (`ai/actions.py:37`).
const DEF_OFFSET: i64 = 1 << 24;
/// `_PAID_KINDS` (`ai/actions.py:38`).
const PAID_KINDS: [ActionKind; 5] = [
    ActionKind::PlayUnit,
    ActionKind::PlayBase,
    ActionKind::Pair,
    ActionKind::PlayCommand,
    ActionKind::Activate,
];
/// `_TRIAL_WORLD_SEED = 0` (`ai/actions.py:136`): the trial determinisations never touch the
/// search generator.
const TRIAL_WORLD_SEED: u64 = 0;
/// `_IN_PLAY` (`ai/actions.py:202`).
const IN_PLAY: [Zone; 3] = [Zone::Battle, Zone::Base, Zone::Paired];

/// `uid` as an index of `st.cards` when `0 <= uid < len(st.cards)`.
pub(crate) fn card_index(st: &GameState, uid: i32) -> Option<Uid> {
    let u = usize::try_from(uid).ok()?;
    if u < st.cards().len() {
        Uid::try_from(u).ok()
    } else {
        None
    }
}

/// `ex_used(action)` (`ai/actions.py:41-45`): EX Resources an action spends.
#[must_use]
pub fn ex_used(a: Action) -> i32 {
    if !PAID_KINDS.contains(&a.kind) || a.c < 0 {
        return 0;
    }
    a.c % ALT_PLAY_BASE
}

/// `_ident(st, uid, decider)` (`ai/actions.py:48-54`).
fn ident(st: &GameState, uid: i32, decider: u8) -> i64 {
    let Some(u) = card_index(st, uid) else {
        return i64::from(uid);
    };
    let c = st.card(u);
    if BY_DEFINITION.contains(&c.zone) && c.known & (1 << decider) != 0 {
        return DEF_OFFSET + i64::from(c.def_id.0);
    }
    i64::from(uid)
}

/// `action_key(st, dec, action)` (`ai/actions.py:57-69`), with the decider `dec.player`.
#[must_use]
pub fn action_key(st: &GameState, dec: &DecisionView<'_>, a: Action) -> MoveKey {
    let k = a.kind;
    let p = dec.hdr.player;
    if k == ActionKind::Select {
        let first = if uid_argument(dec) {
            ident(st, a.a, p)
        } else {
            i64::from(a.a)
        };
        return MoveKey {
            kind: k,
            a: first,
            b: i64::from(a.b),
            variant: NO_ARG,
        };
    }
    if PAID_KINDS.contains(&k) {
        // `c // ALT_PLAY_BASE` only runs on `c >= 0`.
        let variant = if a.c >= 0 {
            a.c / ALT_PLAY_BASE
        } else {
            NO_ARG
        };
        let b = if k == ActionKind::Activate {
            i64::from(a.b)
        } else {
            ident(st, a.b, p)
        };
        return MoveKey {
            kind: k,
            a: ident(st, a.a, p),
            b,
            variant,
        };
    }
    if matches!(k, ActionKind::Attack | ActionKind::Block) {
        return MoveKey {
            kind: k,
            a: i64::from(a.a),
            b: i64::from(a.b),
            variant: NO_ARG,
        };
    }
    MoveKey {
        kind: k,
        a: i64::from(a.a),
        b: i64::from(a.b),
        variant: a.c,
    }
}

/// `canonical(st, dec)` (`ai/actions.py:72-85`): one representative per distinct key, in order
/// of first appearance; a later action replaces it only when it spends strictly fewer EX.
#[must_use]
pub fn canonical(st: &GameState, dec: &DecisionView<'_>) -> Vec<(MoveKey, Action)> {
    let mut out: Vec<(MoveKey, Action)> = Vec::with_capacity(dec.options.len());
    for &action in dec.options {
        let key = action_key(st, dec, action);
        match out.iter_mut().find(|(k, _)| *k == key) {
            None => out.push((key, action)),
            Some(slot) => {
                if ex_used(action) < ex_used(slot.1) {
                    slot.1 = action;
                }
            }
        }
    }
    out
}

/// `_free_card_burst(st)` (`ai/actions.py:91-103`): the resolving 【Burst】 is exactly
/// `(AddToHand(ThisCard()),)`, so accepting costs nothing.
fn free_card_burst(st: &GameState) -> bool {
    let Some(f) = st.frames().last() else {
        return false;
    };
    let Some(uid) = card_index(st, f.card_uid) else {
        return false;
    };
    let reg = st.registry();
    let entry = reg.card(st.card(uid).def_id);
    let Some(aid) = entry.burst_aid else {
        return false;
    };
    let Node::Burst(burst) = reg.node(reg.ability(aid).ability.node()) else {
        return false;
    };
    let [step] = &*burst.steps else {
        return false;
    };
    match reg.node(step.node()) {
        Node::AddToHand(add) => {
            !add.reveal && matches!(reg.node(add.r#ref.node()), Node::ThisCard(_))
        }
        _ => false,
    }
}

/// `_attack_can_matter(st, player, attacker)` (`ai/actions.py:106-120`): an attack by a Unit
/// with AP 0 or less can still matter through attack triggers or a trick in its action step.
///
/// # Errors
/// The engine's error from the view or the playability checks.
pub fn attack_can_matter(st: &GameState, player: u8, attacker: Uid) -> Result<bool, EngineError> {
    let dv = derived(st)?;
    if ap_of(st, &dv, attacker) > 0 {
        return Ok(true);
    }
    if trigger_index(st, &dv)
        .get(Ev::Attacks)
        .iter()
        .any(|&(host, _)| st.card(host).owner == player)
    {
        return Ok(true);
    }
    if !activated_list(st, player, Timing::Action)?.is_empty() {
        return Ok(true);
    }
    for &uid in st.zone(player, Zone::Hand) {
        if command_playable(st, player, uid, Timing::Action)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `_attack_matters(world, player, attacker, target)` (`ai/actions.py:123-133`): declares the
/// attack in a clone of `world`.
fn attack_matters(
    world: &GameState,
    player: u8,
    attacker: i32,
    target: i32,
) -> Result<bool, EngineError> {
    let mut trial = world.clone();
    let queued = trial.pending_triggers().len();
    let attacker = card_index(&trial, attacker)
        .ok_or_else(|| EngineError::Index(format!("list index out of range: {attacker}")))?;
    declare_attack(&mut trial, attacker, target)?;
    let new = trial.pending_triggers().get(queued..).unwrap_or(&[]);
    if new.iter().any(|t| t.controller == player) {
        return Ok(true);
    }
    Ok(!action_step_options(&trial, player)?.is_empty())
}

/// `_drop_idle_attacks(st, player, moves)` (`ai/actions.py:139-150`).
fn drop_idle_attacks(
    st: &GameState,
    player: u8,
    moves: &[(MoveKey, Action)],
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let dv = derived(st)?;
    let mut zero: Vec<MoveKey> = Vec::new();
    for &(key, a) in moves {
        if a.kind == ActionKind::Attack {
            let attacker = card_index(st, a.a)
                .ok_or_else(|| EngineError::Index(format!("list index out of range: {}", a.a)))?;
            if ap_of(st, &dv, attacker) <= 0 && !zero.contains(&key) {
                zero.push(key);
            }
        }
    }
    if zero.is_empty() {
        return Ok(moves.to_vec());
    }
    let world = determinize(st, player, TRIAL_WORLD_SEED)?;
    let mut kept = Vec::with_capacity(moves.len());
    for &(key, a) in moves {
        if !zero.contains(&key) || attack_matters(&world, player, a.a, a.b)? {
            kept.push((key, a));
        }
    }
    Ok(kept)
}

/// What a Python repr scan found: the class names called in it, with the first string
/// argument of `Var(name=...)`.
#[derive(Default)]
struct ReprScan {
    var_names: Vec<String>,
    reads_did: bool,
}

/// Decodes the Python string literal at the start of `text` (`'...'` or `"..."`, the escapes
/// `repr(str)` writes); returns the value and the bytes consumed.
fn string_literal(text: &str) -> Option<(String, usize)> {
    let quote = text.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        if c == quote {
            return Some((out, i + c.len_utf8()));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let (_, e) = chars.next()?;
        let hex = |chars: &mut std::iter::Skip<std::str::CharIndices<'_>>, n: usize| {
            let mut v = 0u32;
            for _ in 0..n {
                v = v * 16 + chars.next()?.1.to_digit(16)?;
            }
            char::from_u32(v)
        };
        match e {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'x' => out.push(hex(&mut chars, 2)?),
            'u' => out.push(hex(&mut chars, 4)?),
            'U' => out.push(hex(&mut chars, 8)?),
            other => out.push(other),
        }
    }
    None
}

/// Scans a CPython dataclass repr (`Registry::repr`): every identifier directly followed by
/// `(` outside string literals is a class constructor. Enum members (`<P.YOU: 'you'>`), ints
/// and strings never are.
fn scan_repr(text: &str) -> ReprScan {
    let mut scan = ReprScan::default();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut prev_ident = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' || b == b'"' {
            let Some((_, used)) = string_literal(&text[i..]) else {
                break;
            };
            i += used;
            prev_ident = false;
            continue;
        }
        let ident_start = (b.is_ascii_alphabetic() || b == b'_') && !prev_ident;
        if ident_start {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let after_dot = start > 0 && bytes[start - 1] == b'.';
            if !after_dot && bytes.get(i) == Some(&b'(') {
                let name = &text[start..i];
                if name == "DidLast" || name == "JumpIfNotDid" {
                    scan.reads_did = true;
                }
                if name == "Var"
                    && let Some(rest) = text[i + 1..].strip_prefix("name=")
                    && let Some((value, _)) = string_literal(rest)
                {
                    scan.var_names.push(value);
                }
            }
            prev_ident = false;
            continue;
        }
        prev_ident = b.is_ascii_alphanumeric() || b == b'_';
        i += 1;
    }
    scan
}

/// `_chosen_only_to_rest(st)` (`ai/actions.py:170-199`). Python's `_contains` walks tuples and
/// dataclass fields (no lists, dicts or strings occur in the IR) for a `d.Var` named `ins.var`
/// or a `pr.JumpIfNotDid`/`d.DidLast`; the walk here scans each instruction's CPython repr,
/// which prints exactly those fields. Python memoises the result per `(program, pc)`; it is a
/// pure function of the registry, recomputed here (contracts O10).
fn chosen_only_to_rest(st: &GameState) -> bool {
    let Some(f) = st.frames().last() else {
        return false;
    };
    if !f.waiting {
        return false;
    }
    if f.once_key.is_some_and(|k| !k.0.is_empty()) && !f.acted {
        return false;
    }
    let reg: &Registry = st.registry();
    let instrs = &reg.program(f.program).instrs;
    let pc = usize::from(f.pc);
    let Some(&at) = instrs.get(pc) else {
        return false;
    };
    let Node::Choose(choose) = reg.node(at) else {
        return false;
    };
    let enemy = matches!(reg.node(choose.sel.node()), Node::Sel(sel) if sel.side == Side::Enemy);
    if !enemy || choose.chooser != P::You {
        return false;
    }
    let var = reg.var_name(choose.var);
    let uses: Vec<usize> = (pc + 1..instrs.len())
        .filter(|&i| {
            scan_repr(&reg.repr(instrs[i]))
                .var_names
                .iter()
                .any(|n| n == var)
        })
        .collect();
    let [only] = uses[..] else {
        return false;
    };
    matches!(reg.node(instrs[only]), Node::Rest(_))
        && !instrs[only + 1..]
            .iter()
            .any(|&i| scan_repr(&reg.repr(i)).reads_did)
}

/// `_resting_can_trigger(st)` (`ai/actions.py:205-211`).
fn resting_can_trigger(st: &GameState) -> Result<bool, EngineError> {
    if !st.delayed().is_empty() {
        return Ok(true);
    }
    let dv = derived(st)?;
    Ok(trigger_index(st, &dv)
        .get(Ev::Rested)
        .iter()
        .any(|&(host, _)| IN_PLAY.contains(&st.card(host).zone)))
}

/// `_resting_changes_more_than_the_unit(st, dec)` (`ai/actions.py:214-232`), judged in the
/// trial determinisation.
fn resting_changes_more_than_the_unit(
    st: &GameState,
    dec: &DecisionView<'_>,
) -> Result<bool, EngineError> {
    let world = determinize(st, dec.hdr.player, TRIAL_WORLD_SEED)?;
    let before = derived(&world)?;
    for o in dec.options {
        if o.kind != ActionKind::Select {
            continue;
        }
        let Some(uid) = card_index(&world, o.a) else {
            continue;
        };
        let c = world.card(uid);
        if c.owner == dec.hdr.player || c.zone != Zone::Battle || c.rested {
            continue;
        }
        let mut rested = world.clone();
        set_rested_for_trial(&mut rested, uid, true);
        if !same_values(&*derived(&rested)?, &before) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `_rests_a_rested_enemy(st, dec, action)` (`ai/actions.py:235-239`).
fn rests_a_rested_enemy(st: &GameState, dec: &DecisionView<'_>, a: Action) -> bool {
    if a.kind != ActionKind::Select {
        return false;
    }
    let Some(uid) = card_index(st, a.a) else {
        return false;
    };
    let c = st.card(uid);
    c.owner != dec.hdr.player && c.zone == Zone::Battle && c.rested
}

/// Which branch of the Rest-target prune decides a SELECT ([`rest_prune`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestPrune {
    /// The prune drops the moves that rest an already rested enemy Unit.
    Pruned,
    /// `_resting_can_trigger` keeps them: a delayed effect, or a Rested trigger of a card in
    /// play (`ai/actions.py:205-211`).
    RestTrigger,
    /// The "while rested" exception keeps them: resting an active enemy candidate changes the
    /// derived view in the trial determinisation (`_resting_changes_more_than_the_unit`,
    /// `ai/actions.py:214-232`; W-13).
    WhileRested,
}

/// The Rest-target prune of [`prune_dominated`] (`ai/actions.py:250-258`) at `dec` over
/// `moves`, as the decision trace counts it (W-13): `None` when the prune does not apply (not a
/// SELECT of uids for an effect whose chosen enemy Unit is only rested) or would change nothing
/// (no move rests a rested enemy Unit, or every move does, which `kept or moves` keeps);
/// otherwise the branch that decides, with its exceptions checked in the prune's order.
/// Analysis only: no agent calls it, and it computes exactly what the prune computes, so a
/// trace that calls it leaves every decision unchanged.
///
/// # Errors
/// The engine's error from the view or the trial determinisation.
pub fn rest_prune(
    st: &GameState,
    dec: &DecisionView<'_>,
    moves: &[(MoveKey, Action)],
) -> Result<Option<RestPrune>, EngineError> {
    if dec.hdr.kind != DecisionKind::Select || !uid_argument(dec) || !chosen_only_to_rest(st) {
        return Ok(None);
    }
    let dropped = moves
        .iter()
        .filter(|m| rests_a_rested_enemy(st, dec, m.1))
        .count();
    if dropped == 0 || dropped == moves.len() {
        return Ok(None);
    }
    Ok(Some(if resting_can_trigger(st)? {
        RestPrune::RestTrigger
    } else if resting_changes_more_than_the_unit(st, dec)? {
        RestPrune::WhileRested
    } else {
        RestPrune::Pruned
    }))
}

/// `prune_dominated(st, dec, moves, strict_attacks=, rest_targets=)` (`ai/actions.py:242-277`):
/// the first matching branch decides, and at least one move is always kept (`kept or moves`).
/// The conditions are evaluated in Python's order, so the view calls on `st` happen exactly
/// where Python's do.
///
/// # Errors
/// The engine's error from the view, a trial determinisation or a trial attack.
pub fn prune_dominated(
    st: &GameState,
    dec: &DecisionView<'_>,
    moves: Vec<(MoveKey, Action)>,
    f: PruneFlags,
) -> Result<Vec<(MoveKey, Action)>, EngineError> {
    let kind = dec.hdr.kind;
    let kept = if kind == DecisionKind::Burst && free_card_burst(st) {
        moves
            .iter()
            .copied()
            .filter(|m| m.1.kind == ActionKind::Yes)
            .collect()
    } else if f.rest_targets
        && kind == DecisionKind::Select
        && uid_argument(dec)
        && chosen_only_to_rest(st)
        && !resting_can_trigger(st)?
        && !resting_changes_more_than_the_unit(st, dec)?
    {
        moves
            .iter()
            .copied()
            .filter(|m| !rests_a_rested_enemy(st, dec, m.1))
            .collect()
    } else if kind == DecisionKind::Main && f.strict_attacks {
        drop_idle_attacks(st, dec.hdr.player, &moves)?
    } else if kind == DecisionKind::Main {
        let mut kept = Vec::with_capacity(moves.len());
        for &(key, a) in &moves {
            let keep = if a.kind == ActionKind::Attack {
                let attacker = card_index(st, a.a).ok_or_else(|| {
                    EngineError::Index(format!("list index out of range: {}", a.a))
                })?;
                attack_can_matter(st, dec.hdr.player, attacker)?
            } else {
                true
            };
            if keep {
                kept.push((key, a));
            }
        }
        kept
    } else {
        return Ok(moves);
    };
    Ok(if kept.is_empty() { moves } else { kept })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ex_used_reads_the_payment_digits() {
        assert_eq!(ex_used(Action::new(ActionKind::PlayUnit, 3, -1, 201)), 1);
        assert_eq!(ex_used(Action::new(ActionKind::PlayUnit, 3, -1, -1)), 0);
        assert_eq!(ex_used(Action::new(ActionKind::Attack, 3, -2, 5)), 0);
        assert_eq!(ex_used(Action::new(ActionKind::Activate, 3, 0, 2)), 2);
    }

    #[test]
    fn repr_scan_finds_constructors_outside_strings() {
        let s = scan_repr(
            "Rest(ref=Var(name='t'), cond=Text(text=\"Var(name='u') DidLast()\"), \
             who=<P.YOU: 'you'>)",
        );
        assert_eq!(s.var_names, ["t"]);
        assert!(!s.reads_did);
        let s = scan_repr("JumpIfNotDid(target=4)");
        assert!(s.reads_did && s.var_names.is_empty());
        let s = scan_repr("If(cond=DidLast(), then=(Damage(ref=Var(name='a\\'b'), amount=1),))");
        assert!(s.reads_did);
        assert_eq!(s.var_names, ["a'b"]);
        assert!(scan_repr("BindVar(var='t')").var_names.is_empty());
        assert!(scan_repr("XVar(name='t')").var_names.is_empty());
    }
}
