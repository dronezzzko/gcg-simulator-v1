//! Tuning signals stated as hypotheses with their sample sizes (`reports/hypotheses.py`).
//!
//! A comparison becomes a hypothesis only when a two-proportion z-test rejects "no difference"
//! at family level [`ALPHA`] after a Holm correction over every comparison of its kind. "Never
//! played" signals are descriptive (no test).

use gcg_core::pyfloat::format_fixed;
use gcg_core::pyjson::Json;

use super::aggregate::{BenchCardStats, CardStats, HeldNeverPlayed, Mulligan, Splits};
use super::stats::{Rate, holm, intervals_overlap, rnd, two_proportion_p};

/// `MIN_GAMES` (`reports/hypotheses.py:18`).
pub const MIN_GAMES: u64 = 20;
/// `MIN_REDRAWS` (`:19`).
pub const MIN_REDRAWS: u64 = 10;
/// `MIN_DELTA` (`:20`).
pub const MIN_DELTA: f64 = 0.05;
/// `MIN_SPLIT_DELTA` (`:21`).
pub const MIN_SPLIT_DELTA: f64 = 0.10;
/// `DEAD_CARD_RATE` (`:22`).
pub const DEAD_CARD_RATE: f64 = 0.5;
/// `PER_DIRECTION` (`:23`).
pub const PER_DIRECTION: usize = 3;
/// `ALPHA` (`:24`).
pub const ALPHA: f64 = 0.05;

/// The `evidence` of a hypothesis.
#[derive(Clone, PartialEq, Debug)]
pub enum Evidence {
    /// `_compare(a, b)` (`reports/hypotheses.py:44-51`) plus the `comparisons` that
    /// `_significant` adds (`:61-62`).
    Compare {
        a: Rate,
        b: Rate,
        delta: f64,
        ci95_separated: bool,
        p_value: f64,
        comparisons: Option<usize>,
    },
    /// `{"drawn_copies": drawn, **held}` (`reports/hypotheses.py:121`).
    Dead {
        drawn_copies: u64,
        held: HeldNeverPlayed,
    },
}

impl Evidence {
    /// `_compare(a, b)` (`reports/hypotheses.py:44-51`); `None` when a rate is `None`, which
    /// Python would raise on (every caller checks the sample sizes first).
    fn compare(a: &Rate, b: &Rate) -> Option<Self> {
        let (ra, rb) = a.rate.zip(b.rate)?;
        Some(Self::Compare {
            a: *a,
            b: *b,
            delta: rnd(ra - rb),
            ci95_separated: !intervals_overlap(a, b),
            p_value: rnd(two_proportion_p(a, b)),
            comparisons: None,
        })
    }

    const fn delta(&self) -> f64 {
        match self {
            Self::Compare { delta, .. } => *delta,
            Self::Dead { .. } => 0.0,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        match self {
            Self::Compare {
                a,
                b,
                delta,
                ci95_separated,
                p_value,
                comparisons,
            } => {
                o.set("a", a.to_json());
                o.set("b", b.to_json());
                o.set("delta", *delta);
                o.set("ci95_separated", *ci95_separated);
                o.set("p_value", *p_value);
                if let Some(c) = comparisons {
                    o.set("comparisons", *c);
                }
            }
            Self::Dead { drawn_copies, held } => {
                o.set("drawn_copies", *drawn_copies);
                held.write_into(&mut o);
            }
        }
        o
    }
}

/// `_hypothesis(hid, kind, statement, sample, evidence)` (`reports/hypotheses.py:31-41`).
#[derive(Clone, PartialEq, Debug)]
pub struct Hypothesis {
    pub id: String,
    pub kind: &'static str,
    pub statement: String,
    pub sample_size: u64,
    pub evidence: Evidence,
}

impl Hypothesis {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("id", self.id.as_str());
        o.set("kind", self.kind);
        o.set("label", "hypothesis");
        o.set("statement", self.statement.as_str());
        o.set("sample_size", self.sample_size);
        o.set("evidence", self.evidence.to_json());
        o
    }
}

/// `_pct(x)` (`reports/hypotheses.py:27-28`): `"n/a"` or `f"{100 * x:.0f}%"` (no decimals).
fn pct(x: Option<f64>) -> String {
    x.map_or_else(
        || "n/a".to_owned(),
        |v| format!("{}%", format_fixed(100.0 * v, 0)),
    )
}

/// `_significant(found, comparisons)` (`reports/hypotheses.py:54-65`): Holm over a family of
/// `max(comparisons, len(found))` tests, padding with `1.0`; every found hypothesis records
/// the family size.
fn significant(mut found: Vec<Hypothesis>, comparisons: usize) -> Vec<Hypothesis> {
    let family = comparisons.max(found.len());
    let mut p_values: Vec<f64> = found
        .iter()
        .map(|h| match h.evidence {
            Evidence::Compare { p_value, .. } => p_value,
            Evidence::Dead { .. } => 1.0,
        })
        .collect();
    p_values.resize(family, 1.0);
    let keep = holm(&p_values, ALPHA);
    for h in &mut found {
        if let Evidence::Compare { comparisons, .. } = &mut h.evidence {
            *comparisons = Some(family);
        }
    }
    found
        .into_iter()
        .zip(keep)
        .filter_map(|(h, ok)| ok.then_some(h))
        .collect()
}

/// `_sampled_cards(cards)` (`reports/hypotheses.py:68-74`).
fn sampled_cards(cards: &[CardStats]) -> usize {
    cards
        .iter()
        .filter(|c| {
            c.win_rate_when_drawn.n >= MIN_GAMES && c.win_rate_when_not_drawn.n >= MIN_GAMES
        })
        .count()
}

/// `_card_delta(card)` (`reports/hypotheses.py:77-100`).
fn card_delta(card: &CardStats) -> Option<Hypothesis> {
    let (drawn, not_drawn) = (&card.win_rate_when_drawn, &card.win_rate_when_not_drawn);
    if drawn.n < MIN_GAMES || not_drawn.n < MIN_GAMES {
        return None;
    }
    let ev = Evidence::compare(drawn, not_drawn)?;
    if ev.delta().abs() < MIN_DELTA {
        return None;
    }
    let label = format!("{} ({})", card.name, card.card_number);
    let advice = if ev.delta() > 0.0 {
        "more copies or more ways to find it may raise the win rate"
    } else {
        "it may underperform in this matchup; test cutting a copy"
    };
    let statement = format!(
        "Games where {label} was drawn were won {} of the time vs {} when it was not \
         (n={}/{}). Hypothesis: {advice}. Correlation only: longer games also draw more cards.",
        pct(drawn.rate),
        pct(not_drawn.rate),
        drawn.n,
        not_drawn.n
    );
    Some(Hypothesis {
        id: format!("card-drawn-delta:{}", card.card_number),
        kind: "card_drawn_win_rate",
        statement,
        sample_size: drawn.n + not_drawn.n,
        evidence: ev,
    })
}

/// `_dead_card(card)` (`reports/hypotheses.py:103-122`).
fn dead_card(card: &CardStats) -> Option<Hypothesis> {
    let held = card.held;
    let drawn = card.drawn_copies;
    let rate = held.rate_of_drawn_copies?;
    if drawn < MIN_GAMES || rate < DEAD_CARD_RATE {
        return None;
    }
    let statement = format!(
        "{} of drawn copies of {} ({}) were never played (n={drawn} copies; {} still in hand at \
         game end). Hypothesis: it is often uncastable or low priority in this matchup; consider \
         fewer copies or a lower curve.",
        pct(Some(rate)),
        card.name,
        card.card_number,
        held.in_hand_at_end_copies
    );
    Some(Hypothesis {
        id: format!("dead-card:{}", card.card_number),
        kind: "held_never_played",
        statement,
        sample_size: drawn,
        evidence: Evidence::Dead {
            drawn_copies: drawn,
            held,
        },
    })
}

/// `_by_delta(h)` order (`reports/hypotheses.py:131-132`): `(-abs(delta), id)`.
fn by_delta(a: &Hypothesis, b: &Hypothesis) -> std::cmp::Ordering {
    (-a.evidence.delta().abs())
        .partial_cmp(&-b.evidence.delta().abs())
        .expect("deltas are never NaN")
        .then_with(|| a.id.cmp(&b.id))
}

/// `_top_by_delta(found)` (`reports/hypotheses.py:125-128`).
fn top_by_delta(found: Vec<Hypothesis>) -> Vec<Hypothesis> {
    let (mut pos, rest): (Vec<Hypothesis>, Vec<Hypothesis>) =
        found.into_iter().partition(|h| h.evidence.delta() > 0.0);
    let mut neg: Vec<Hypothesis> = rest
        .into_iter()
        .filter(|h| h.evidence.delta() < 0.0)
        .collect();
    pos.sort_by(by_delta);
    neg.sort_by(by_delta);
    pos.truncate(PER_DIRECTION);
    neg.truncate(PER_DIRECTION);
    pos.extend(neg);
    pos
}

/// `_play_draw(split)` (`reports/hypotheses.py:135-148`).
fn play_draw(split: &Splits) -> Option<Hypothesis> {
    let (play, draw) = (&split.on_play.dut_win_rate, &split.on_draw.dut_win_rate);
    if play.n < MIN_GAMES || draw.n < MIN_GAMES {
        return None;
    }
    let ev = Evidence::compare(play, draw)?;
    if ev.delta().abs() < MIN_SPLIT_DELTA {
        return None;
    }
    let better = if ev.delta() > 0.0 {
        "going first"
    } else {
        "going second"
    };
    let statement = format!(
        "The deck under test wins {} on the play vs {} on the draw (n={}/{}). Hypothesis: it is \
         built for {better}; its curve and early interaction are the first things to test.",
        pct(play.rate),
        pct(draw.rate),
        play.n,
        draw.n
    );
    Some(Hypothesis {
        id: "play-draw".to_owned(),
        kind: "play_draw_split",
        statement,
        sample_size: play.n + draw.n,
        evidence: ev,
    })
}

/// `_redraw(mull)` (`reports/hypotheses.py:151-164`) on the `dut` block.
fn redraw(mull: &Mulligan) -> Option<Hypothesis> {
    let (after_redraw, after_keep) = (&mull.win_rate_after_redraw, &mull.win_rate_after_keep);
    if after_redraw.n < MIN_REDRAWS || after_keep.n < MIN_REDRAWS {
        return None;
    }
    let ev = Evidence::compare(after_redraw, after_keep)?;
    if ev.delta().abs() < MIN_SPLIT_DELTA {
        return None;
    }
    let statement = format!(
        "After a redraw the deck under test won {} vs {} after keeping (n={}/{}). Hypothesis: \
         opening-hand consistency matters here; the early curve may need smoothing.",
        pct(after_redraw.rate),
        pct(after_keep.rate),
        after_redraw.n,
        after_keep.n
    );
    Some(Hypothesis {
        id: "redraw".to_owned(),
        kind: "redraw_outcome",
        statement,
        sample_size: after_redraw.n + after_keep.n,
        evidence: ev,
    })
}

/// `_bench_threat(card)` (`reports/hypotheses.py:167-184`).
fn bench_threat(card: &BenchCardStats) -> Option<Hypothesis> {
    let lift = card.loss_rate_lift?;
    if !card.rank_eligible || lift < MIN_SPLIT_DELTA {
        return None;
    }
    let (seen, unseen) = (
        &card.dut_loss_rate_when_seen,
        &card.dut_loss_rate_when_not_seen,
    );
    let statement = format!(
        "The deck under test lost {} of games where the benchmark's {} ({}) was seen in play vs \
         {} otherwise (n={}/{}). Hypothesis: answers to it (removal, blockers, or racing it) may \
         be worth testing.",
        pct(seen.rate),
        card.name,
        card.card_number,
        pct(unseen.rate),
        seen.n,
        unseen.n
    );
    Some(Hypothesis {
        id: format!("bench-threat:{}", card.card_number),
        kind: "benchmark_card_threat",
        statement,
        sample_size: seen.n + unseen.n,
        evidence: Evidence::compare(seen, unseen)?,
    })
}

/// `hypotheses(cards, bench_cards, split, mull)` (`reports/hypotheses.py:193-206`): card
/// deltas (top three per direction), dead cards, play/draw, redraw, then up to three
/// benchmark threats.
#[must_use]
pub fn hypotheses(
    cards: &[CardStats],
    bench_cards: &[BenchCardStats],
    split: &Splits,
    mull: &Mulligan,
) -> Vec<Hypothesis> {
    let deltas: Vec<Hypothesis> = cards.iter().filter_map(card_delta).collect();
    let mut out = top_by_delta(significant(deltas, sampled_cards(cards)));
    out.extend(cards.iter().filter_map(dead_card));
    for h in [play_draw(split), redraw(mull)].into_iter().flatten() {
        out.extend(significant(vec![h], 1));
    }
    let eligible = bench_cards.iter().filter(|b| b.rank_eligible).count();
    let threats: Vec<Hypothesis> = bench_cards.iter().filter_map(bench_threat).collect();
    let mut kept = significant(threats, eligible);
    kept.truncate(PER_DIRECTION);
    out.extend(kept);
    out
}
