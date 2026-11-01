//! Aggregate game records into outcomes, splits, lengths, mulligan and per-card statistics
//! (`reports/aggregate.py`). Everything is from the deck under test's point of view; win rates
//! count draws as non-wins.

use std::collections::BTreeMap;

use gcg_core::pyjson::Json;
use gcg_data::Deck;
use gcg_engine::EndReason;

use super::results::ResultsData;
use super::stats::{Distribution, Rate, ratio, rnd};
use crate::{GameRecord, MatchRecord, Outcome};

/// A game result from the deck under test's view: `WIN`, `LOSS` or `DRAW`
/// (`reports/aggregate.py:20-25`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DutResult {
    Win,
    Loss,
    Draw,
}

impl DutResult {
    /// `RESULTS = (WIN, LOSS, DRAW)` (`reports/aggregate.py:21`): the order of every
    /// per-result iteration.
    pub const ALL: [Self; 3] = [Self::Win, Self::Loss, Self::Draw];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Loss => "loss",
            Self::Draw => "draw",
        }
    }

    /// `dut_result(g)` (`reports/aggregate.py:24-25`).
    #[must_use]
    pub const fn of(g: &GameRecord) -> Self {
        match g.winner {
            Outcome::Dut => Self::Win,
            Outcome::Bench => Self::Loss,
            Outcome::Draw => Self::Draw,
        }
    }
}

fn count(n: usize) -> u64 {
    u64::try_from(n).expect("a count fits in u64")
}

/// The block of `_outcomes(winners)` (`reports/aggregate.py:28-36`).
#[derive(Clone, PartialEq, Debug)]
pub struct Outcomes {
    pub n: u64,
    pub dut_wins: u64,
    pub bench_wins: u64,
    pub draws: u64,
    pub dut_win_rate: Rate,
}

impl Outcomes {
    /// `_outcomes(winners)` (`reports/aggregate.py:28-36`).
    pub fn of(winners: impl IntoIterator<Item = Outcome>) -> Self {
        let (mut n, mut dut, mut bench, mut draws) = (0u64, 0u64, 0u64, 0u64);
        for w in winners {
            n += 1;
            match w {
                Outcome::Dut => dut += 1,
                Outcome::Bench => bench += 1,
                Outcome::Draw => draws += 1,
            }
        }
        Self {
            n,
            dut_wins: dut,
            bench_wins: bench,
            draws,
            dut_win_rate: Rate::new(dut, n),
        }
    }

    /// `game_outcomes(games)` (`reports/aggregate.py:39-40`).
    pub fn of_games<'a>(games: impl IntoIterator<Item = &'a GameRecord>) -> Self {
        Self::of(games.into_iter().map(|g| g.winner))
    }

    /// `match_outcomes(matches)` (`reports/aggregate.py:43-44`).
    #[must_use]
    pub fn of_matches(matches: &[MatchRecord]) -> Self {
        Self::of(matches.iter().map(|m| m.winner))
    }

    /// Writes the keys into `o` in Python's order (also used by `end_reasons`, whose entries
    /// start with `reason`).
    fn write_into(&self, o: &mut Json) {
        o.set("n", self.n);
        o.set("dut_wins", self.dut_wins);
        o.set("bench_wins", self.bench_wins);
        o.set("draws", self.draws);
        o.set("dut_win_rate", self.dut_win_rate.to_json());
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        self.write_into(&mut o);
        o
    }
}

/// `splits(games, games_per_match)` (`reports/aggregate.py:47-55`).
#[derive(Clone, PartialEq, Debug)]
pub struct Splits {
    pub on_play: Outcomes,
    pub on_draw: Outcomes,
    /// `"1"..="G"` in order, every game number of the format even with `n == 0`.
    pub by_game_number: Vec<(String, Outcomes)>,
}

impl Splits {
    /// `splits(games, games_per_match)` (`reports/aggregate.py:47-55`).
    #[must_use]
    pub fn of(games: &[&GameRecord], games_per_match: u32) -> Self {
        Self {
            on_play: Outcomes::of_games(games.iter().copied().filter(|g| g.dut_on_play)),
            on_draw: Outcomes::of_games(games.iter().copied().filter(|g| !g.dut_on_play)),
            by_game_number: (0..games_per_match)
                .map(|i| {
                    let o = Outcomes::of_games(games.iter().copied().filter(|g| g.game_index == i));
                    ((i + 1).to_string(), o)
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut by = Json::object();
        for (k, o) in &self.by_game_number {
            by.set(k.as_str(), o.to_json());
        }
        let mut o = Json::object();
        o.set("on_play", self.on_play.to_json());
        o.set("on_draw", self.on_draw.to_json());
        o.set("by_game_number", by);
        o
    }
}

/// `end_reasons(games)` (`reports/aggregate.py:58-62`): one entry per reason that occurs, in
/// value-string order.
#[must_use]
pub fn end_reasons(games: &[&GameRecord]) -> Json {
    let entries = EndReason::SORTED_BY_VALUE
        .into_iter()
        .filter(|r| games.iter().any(|g| g.end_reason == *r))
        .map(|r| {
            let mut o = Json::object();
            o.set("reason", r.as_str());
            Outcomes::of_games(games.iter().copied().filter(|g| g.end_reason == r))
                .write_into(&mut o);
            o
        })
        .collect();
    Json::Array(entries)
}

/// `draws(games)` (`reports/aggregate.py:65-72`).
#[must_use]
pub fn draws(games: &[&GameRecord]) -> Json {
    let drawn: Vec<&GameRecord> = games
        .iter()
        .copied()
        .filter(|g| g.winner == Outcome::Draw)
        .collect();
    let with = |r: EndReason| count(drawn.iter().filter(|g| g.end_reason == r).count());
    let mut o = Json::object();
    o.set("games", count(drawn.len()));
    o.set("both_defeated", with(EndReason::BothDefeated));
    o.set("turn_limit", with(EndReason::TurnLimit));
    o.set(
        "scored_for_match_by_turn_player_rule",
        count(drawn.iter().filter(|g| g.match_point.is_some()).count()),
    );
    o
}

fn turns_of(games: &[&GameRecord]) -> Vec<i64> {
    games.iter().map(|g| i64::from(g.turns)).collect()
}

/// `game_length(games)` (`reports/aggregate.py:75-83`).
#[must_use]
pub fn game_length(games: &[&GameRecord]) -> Json {
    let mut histogram: BTreeMap<i32, u64> = BTreeMap::new();
    for g in games {
        *histogram.entry(g.turns).or_insert(0) += 1;
    }
    let mut by = Json::object();
    for r in DutResult::ALL {
        let of_r: Vec<&GameRecord> = games
            .iter()
            .copied()
            .filter(|g| DutResult::of(g) == r)
            .collect();
        by.set(r.as_str(), Distribution::of(&turns_of(&of_r)).to_json());
    }
    let mut o = Json::object();
    o.set("turns", Distribution::of(&turns_of(games)).to_json());
    o.set("by_result", by);
    o.set(
        "histogram",
        Json::Array(
            histogram
                .into_iter()
                .map(|(t, n)| {
                    let mut h = Json::object();
                    h.set("turns", t);
                    h.set("games", n);
                    h
                })
                .collect(),
        ),
    );
    o
}

/// `mulligan(games)` (`reports/aggregate.py:86-97`).
#[derive(Clone, PartialEq, Debug)]
pub struct Mulligan {
    pub dut_redraw_rate: Rate,
    pub win_rate_after_redraw: Rate,
    pub win_rate_after_keep: Rate,
    pub bench_redraw_rate: Rate,
}

impl Mulligan {
    /// `mulligan(games)` (`reports/aggregate.py:86-97`).
    #[must_use]
    pub fn of(games: &[&GameRecord]) -> Self {
        let wins =
            |gs: &[&GameRecord]| count(gs.iter().filter(|g| g.winner == Outcome::Dut).count());
        let (redrew, kept): (Vec<&GameRecord>, Vec<&GameRecord>) =
            games.iter().copied().partition(|g| g.dut_cards.redrew);
        let bench = count(
            games
                .iter()
                .filter(|g| g.redraws[usize::from(g.bench_seat())])
                .count(),
        );
        let n = count(games.len());
        Self {
            dut_redraw_rate: Rate::new(count(redrew.len()), n),
            win_rate_after_redraw: Rate::new(wins(&redrew), count(redrew.len())),
            win_rate_after_keep: Rate::new(wins(&kept), count(kept.len())),
            bench_redraw_rate: Rate::new(bench, n),
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut dut = Json::object();
        dut.set("redraw_rate", self.dut_redraw_rate.to_json());
        dut.set(
            "win_rate_after_redraw",
            self.win_rate_after_redraw.to_json(),
        );
        dut.set("win_rate_after_keep", self.win_rate_after_keep.to_json());
        let mut bench = Json::object();
        bench.set("redraw_rate", self.bench_redraw_rate.to_json());
        let mut o = Json::object();
        o.set("dut", dut);
        o.set("bench", bench);
        o
    }
}

/// `_CardTally` (`reports/aggregate.py:100-139`).
#[derive(Default)]
struct CardTally {
    drawn_games: u64,
    drawn_wins: u64,
    drawn_copies: u64,
    opening_games: u64,
    played_games: u64,
    played_wins: u64,
    drawn_unplayed_games: u64,
    drawn_unplayed_wins: u64,
    plays: u64,
    own_turns: Vec<i64>,
    never_copies: u64,
    never_games: u64,
    end_hand_copies: u64,
    initial_games: u64,
    redrawn_games: u64,
}

fn copies_of(list: &[Box<str>], number: &str) -> u64 {
    count(list.iter().filter(|n| &***n == number).count())
}

impl CardTally {
    /// `_CardTally.add(g, number)` (`reports/aggregate.py:118-139`); Python's bool arithmetic
    /// counts `True` as 1.
    fn add(&mut self, g: &GameRecord, number: &str) {
        let c = &g.dut_cards;
        let win = g.winner == Outcome::Dut;
        let drawn = copies_of(&c.drawn, number);
        let turns: Vec<i64> = c
            .played
            .iter()
            .filter(|p| &*p.card_number == number)
            .map(|p| i64::from(p.own_turn))
            .collect();
        let played = !turns.is_empty();
        self.drawn_copies += drawn;
        self.drawn_games += u64::from(drawn > 0);
        self.drawn_wins += u64::from(drawn > 0 && win);
        self.opening_games += u64::from(c.opening_hand.iter().any(|n| &**n == number));
        self.played_games += u64::from(played);
        self.played_wins += u64::from(played && win);
        self.drawn_unplayed_games += u64::from(drawn > 0 && !played);
        self.drawn_unplayed_wins += u64::from(drawn > 0 && !played && win);
        self.plays += count(turns.len());
        self.own_turns.extend(turns);
        let never = copies_of(&c.never_played, number);
        self.never_copies += never;
        self.never_games += u64::from(never > 0);
        self.end_hand_copies += copies_of(&c.in_hand_at_end, number);
        if c.initial_hand.iter().any(|n| &**n == number) {
            self.initial_games += 1;
            self.redrawn_games += u64::from(c.redrew);
        }
    }
}

/// One entry of `card_stats` (`_card_entry`, `reports/aggregate.py:142-181`).
#[derive(Clone, PartialEq, Debug)]
pub struct CardStats {
    pub card_number: Box<str>,
    pub name: Box<str>,
    pub card_type: &'static str,
    pub level: i32,
    pub cost: i32,
    pub copies: u128,
    pub drawn_games: u64,
    pub drawn_game_rate: Option<f64>,
    pub drawn_copies: u64,
    pub copies_per_game: Option<f64>,
    pub opening_games: u64,
    pub opening_game_rate: Option<f64>,
    pub played_games: u64,
    pub plays: u64,
    pub rate_when_drawn: Option<f64>,
    pub own_turn: Distribution,
    /// `[{"own_turn": k, "plays": count}]` sorted by `k`.
    pub by_own_turn: Vec<(i64, u64)>,
    pub win_rate_when_drawn: Rate,
    pub win_rate_when_not_drawn: Rate,
    pub win_rate_when_played: Rate,
    pub win_rate_when_drawn_not_played: Rate,
    pub held: HeldNeverPlayed,
    pub in_initial_hand_games: u64,
    pub redrawn_games: u64,
    pub mulligan_rate: Option<f64>,
}

/// The `held_never_played` block (`reports/aggregate.py:170-175`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HeldNeverPlayed {
    pub copies: u64,
    pub rate_of_drawn_copies: Option<f64>,
    pub games: u64,
    pub in_hand_at_end_copies: u64,
}

impl HeldNeverPlayed {
    /// Writes the keys into `o` in Python's order (the dead-card evidence spreads them,
    /// `reports/hypotheses.py:121`).
    pub(crate) fn write_into(&self, o: &mut Json) {
        o.set("copies", self.copies);
        o.set("rate_of_drawn_copies", self.rate_of_drawn_copies);
        o.set("games", self.games);
        o.set("in_hand_at_end_copies", self.in_hand_at_end_copies);
    }
}

fn copies_json(copies: u128) -> Json {
    Json::Int(i128::try_from(copies).expect("a deck's copy count fits in i128"))
}

impl CardStats {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut drawn = Json::object();
        drawn.set("games", self.drawn_games);
        drawn.set("game_rate", self.drawn_game_rate);
        drawn.set("copies", self.drawn_copies);
        drawn.set("copies_per_game", self.copies_per_game);
        let mut opening = Json::object();
        opening.set("games", self.opening_games);
        opening.set("game_rate", self.opening_game_rate);
        let mut played = Json::object();
        played.set("games", self.played_games);
        played.set("plays", self.plays);
        played.set("rate_when_drawn", self.rate_when_drawn);
        played.set("own_turn", self.own_turn.to_json());
        played.set(
            "by_own_turn",
            Json::Array(
                self.by_own_turn
                    .iter()
                    .map(|&(k, n)| {
                        let mut e = Json::object();
                        e.set("own_turn", k);
                        e.set("plays", n);
                        e
                    })
                    .collect(),
            ),
        );
        let mut held = Json::object();
        self.held.write_into(&mut held);
        let mut mulligan = Json::object();
        mulligan.set("in_initial_hand_games", self.in_initial_hand_games);
        mulligan.set("redrawn_games", self.redrawn_games);
        mulligan.set("rate", self.mulligan_rate);
        let mut o = Json::object();
        o.set("card_number", &*self.card_number);
        o.set("name", &*self.name);
        o.set("card_type", self.card_type);
        o.set("level", self.level);
        o.set("cost", self.cost);
        o.set("copies", copies_json(self.copies));
        o.set("drawn", drawn);
        o.set("opening_hand", opening);
        o.set("played", played);
        o.set("win_rate_when_drawn", self.win_rate_when_drawn.to_json());
        o.set(
            "win_rate_when_not_drawn",
            self.win_rate_when_not_drawn.to_json(),
        );
        o.set("win_rate_when_played", self.win_rate_when_played.to_json());
        o.set(
            "win_rate_when_drawn_not_played",
            self.win_rate_when_drawn_not_played.to_json(),
        );
        o.set("held_never_played", held);
        o.set("mulligan", mulligan);
        o
    }
}

/// `card_stats(games, deck)` (`reports/aggregate.py:184-194`): the main deck's cards in
/// card-number order (`Deck::main` is sorted), each tallied over every game.
#[must_use]
pub fn card_stats(games: &[&GameRecord], deck: &Deck, data: &ResultsData) -> Vec<CardStats> {
    let n = count(games.len());
    let wins = count(games.iter().filter(|g| g.winner == Outcome::Dut).count());
    deck.main
        .iter()
        .map(|cc| {
            let number = &*cc.card_number;
            let mut t = CardTally::default();
            for g in games {
                t.add(g, number);
            }
            let facts = data.card(number);
            let mut by_turn: BTreeMap<i64, u64> = BTreeMap::new();
            for &k in &t.own_turns {
                *by_turn.entry(k).or_insert(0) += 1;
            }
            CardStats {
                card_number: cc.card_number.clone(),
                name: facts.name.clone(),
                card_type: facts.card_type.as_str(),
                level: facts.level,
                cost: facts.cost,
                copies: cc.count,
                drawn_games: t.drawn_games,
                drawn_game_rate: ratio(t.drawn_games, n),
                drawn_copies: t.drawn_copies,
                copies_per_game: ratio(t.drawn_copies, n),
                opening_games: t.opening_games,
                opening_game_rate: ratio(t.opening_games, n),
                played_games: t.played_games,
                plays: t.plays,
                rate_when_drawn: ratio(t.played_games, t.drawn_games),
                own_turn: Distribution::of(&t.own_turns),
                by_own_turn: by_turn.into_iter().collect(),
                win_rate_when_drawn: Rate::new(t.drawn_wins, t.drawn_games),
                win_rate_when_not_drawn: Rate::new(wins - t.drawn_wins, n - t.drawn_games),
                win_rate_when_played: Rate::new(t.played_wins, t.played_games),
                win_rate_when_drawn_not_played: Rate::new(
                    t.drawn_unplayed_wins,
                    t.drawn_unplayed_games,
                ),
                held: HeldNeverPlayed {
                    copies: t.never_copies,
                    rate_of_drawn_copies: ratio(t.never_copies, t.drawn_copies),
                    games: t.never_games,
                    in_hand_at_end_copies: t.end_hand_copies,
                },
                in_initial_hand_games: t.initial_games,
                redrawn_games: t.redrawn_games,
                mulligan_rate: ratio(t.redrawn_games, t.initial_games),
            }
        })
        .collect()
}

/// One entry of `bench_card_stats` (`_bench_entry`, `reports/aggregate.py:197-216`, plus
/// `rank_eligible`, `:226-228`).
#[derive(Clone, PartialEq, Debug)]
pub struct BenchCardStats {
    pub card_number: Box<str>,
    pub name: Box<str>,
    pub card_type: &'static str,
    pub copies: u128,
    pub seen_games: u64,
    pub seen_rate: Option<f64>,
    pub dut_loss_rate_when_seen: Rate,
    pub dut_loss_rate_when_not_seen: Rate,
    pub loss_rate_lift: Option<f64>,
    pub rank_eligible: bool,
}

impl BenchCardStats {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::object();
        o.set("card_number", &*self.card_number);
        o.set("name", &*self.name);
        o.set("card_type", self.card_type);
        o.set("copies", copies_json(self.copies));
        o.set("seen_games", self.seen_games);
        o.set("seen_rate", self.seen_rate);
        o.set(
            "dut_loss_rate_when_seen",
            self.dut_loss_rate_when_seen.to_json(),
        );
        o.set(
            "dut_loss_rate_when_not_seen",
            self.dut_loss_rate_when_not_seen.to_json(),
        );
        o.set("loss_rate_lift", self.loss_rate_lift);
        o.set("rank_eligible", self.rank_eligible);
        o
    }
}

/// `bench_card_stats(games, deck, min_games)` (`reports/aggregate.py:219-237`): sorted by
/// `(not rank_eligible, -(lift if lift is not None else -2.0), -seen_games, card_number)`, a
/// stable sort whose float keys compare with IEEE `<` (`-0.0 == 0.0`).
#[must_use]
pub fn bench_card_stats(
    games: &[&GameRecord],
    deck: &Deck,
    min_games: u64,
    data: &ResultsData,
) -> Vec<BenchCardStats> {
    let mut entries: Vec<BenchCardStats> = deck
        .main
        .iter()
        .map(|cc| {
            let number = &*cc.card_number;
            let facts = data.card(number);
            let (seen, unseen): (Vec<&GameRecord>, Vec<&GameRecord>) = games
                .iter()
                .copied()
                .partition(|g| g.bench_seen.iter().any(|n| &**n == number));
            let losses = |gs: &[&GameRecord]| {
                count(gs.iter().filter(|g| g.winner == Outcome::Bench).count())
            };
            let when_seen = Rate::new(losses(&seen), count(seen.len()));
            let when_unseen = Rate::new(losses(&unseen), count(unseen.len()));
            let lift = when_seen
                .rate
                .zip(when_unseen.rate)
                .map(|(s, u)| rnd(s - u));
            BenchCardStats {
                card_number: cc.card_number.clone(),
                name: facts.name.clone(),
                card_type: facts.card_type.as_str(),
                copies: cc.count,
                seen_games: count(seen.len()),
                seen_rate: ratio(count(seen.len()), count(games.len())),
                dut_loss_rate_when_seen: when_seen,
                rank_eligible: count(seen.len()) >= min_games && when_unseen.n >= min_games,
                dut_loss_rate_when_not_seen: when_unseen,
                loss_rate_lift: lift,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        let key = |e: &BenchCardStats| -(e.loss_rate_lift.unwrap_or(-2.0));
        (!a.rank_eligible)
            .cmp(&!b.rank_eligible)
            .then_with(|| key(a).partial_cmp(&key(b)).expect("lifts are never NaN"))
            .then_with(|| b.seen_games.cmp(&a.seen_games))
            .then_with(|| a.card_number.cmp(&b.card_number))
    });
    entries
}
