"""``summary.md``: a human-readable view of ``results.json`` (no wall-clock data)."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

TOP_BENCH_CARDS = 10


def pct(x: float | None) -> str:
    return "n/a" if x is None else f"{100 * x:.1f}%"


def num(x: float | None, digits: int = 1) -> str:
    return "-" if x is None else f"{x:.{digits}f}"


def points(x: float | None) -> str:
    return "-" if x is None else f"{100 * x:+.1f} pp"


def rate_text(r: dict[str, Any]) -> str:
    if r["rate"] is None:
        return "n/a (n=0)"
    lo, hi = r["ci95"]
    return f"{pct(r['rate'])} [{pct(lo)}, {pct(hi)}] (n={r['n']})"


def _table(header: Sequence[str], rows: Sequence[Sequence[object]]) -> list[str]:
    lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
    lines += ["| " + " | ".join(str(c) for c in row) + " |" for row in rows]
    return lines


def _outcome_row(label: str, o: dict[str, Any]) -> list[object]:
    return [label, o["n"], o["dut_wins"], o["bench_wins"], o["draws"], rate_text(o["dut_win_rate"])]


OUTCOME_HEADER = ("", "n", "DUT wins", "Bench wins", "Draws", "DUT win rate [95% CI]")


def _headline(res: dict[str, Any], command: str) -> list[str]:
    d, c, ai = res["decks"], res["config"], res["ai"]
    return [
        f"# {d['deck_under_test']['name']} vs {d['benchmark']['name']}",
        "",
        f"Deck under test (DUT) **{d['deck_under_test']['name']}** against benchmark "
        f"**{d['benchmark']['name']}**: {c['matches']} {c['format'].upper()} matches, master seed "
        f"{res['seeds']['master']}, AI preset `{ai['preset']}` (agents `{ai['dut_agent']}` / "
        f"`{ai['bench_agent']}`, factory `{ai['factory']}`).",
        "",
        f"Reproduce: `{command}`",
        "",
    ]


def _results(res: dict[str, Any]) -> list[str]:
    r, s = res["results"], res["splits"]
    rows = [_outcome_row("Matches", r["matches"]), _outcome_row("Games", r["games"])]
    split_rows = [
        _outcome_row("On the play", s["on_play"]),
        _outcome_row("On the draw", s["on_draw"]),
    ]
    split_rows += [_outcome_row(f"Game {k}", v) for k, v in sorted(s["by_game_number"].items())]
    return [
        "## Result",
        "",
        *_table(OUTCOME_HEADER, rows),
        "",
        "## Splits",
        "",
        *_table(OUTCOME_HEADER, split_rows),
        "",
    ]


def _endings(res: dict[str, Any]) -> list[str]:
    t, dr = res["game_length"]["turns"], res["draws"]
    rows = [_outcome_row(e["reason"], e) for e in res["end_reasons"]]
    by = res["game_length"]["by_result"]
    return [
        "## How games end",
        "",
        *_table(("Reason", *OUTCOME_HEADER[1:]), rows),
        "",
        f"Game length (turns): mean {num(t['mean'])}, median {t['median']}, p10 {t['p10']}, "
        f"p90 {t['p90']}, range {t['min']}-{t['max']}. Median by DUT result: win "
        f"{num(by['win']['median'], 0)}, loss {num(by['loss']['median'], 0)}, draw "
        f"{num(by['draw']['median'], 0)}.",
        "",
        f"Draws: {dr['games']} ({dr['both_defeated']} simultaneous defeat, {dr['turn_limit']} "
        f"turn limit); {dr['scored_for_match_by_turn_player_rule']} scored for BO3 by the "
        "turn-player-loses rule (TRM 5.2).",
        "",
    ]


def _mulligan(res: dict[str, Any]) -> list[str]:
    m = res["mulligan"]
    return [
        "## Redraws",
        "",
        f"DUT redraw rate {rate_text(m['dut']['redraw_rate'])}; DUT win rate after a redraw "
        f"{rate_text(m['dut']['win_rate_after_redraw'])}, after keeping "
        f"{rate_text(m['dut']['win_rate_after_keep'])}. Benchmark redraw rate "
        f"{rate_text(m['bench']['redraw_rate'])}.",
        "",
    ]


def _card_row(c: dict[str, Any]) -> list[object]:
    played, held = c["played"], c["held_never_played"]
    return [
        f"{c['card_number']} {c['name']}",
        c["copies"],
        f"{c['drawn']['games']} ({pct(c['drawn']['game_rate'])})",
        played["games"],
        num(played["own_turn"]["mean"]),
        rate_text(c["win_rate_when_drawn"]),
        rate_text(c["win_rate_when_not_drawn"]),
        rate_text(c["win_rate_when_played"]),
        f"{held['copies']} ({pct(held['rate_of_drawn_copies'])})",
        pct(c["mulligan"]["rate"]),
    ]


def _cards(res: dict[str, Any]) -> list[str]:
    header = (
        "Card",
        "Copies",
        "Drawn games",
        "Played games",
        "Mean own turn played",
        "Win rate when drawn",
        "Win rate when not drawn",
        "Win rate when played",
        "Drawn, never played",
        "Redrawn when in initial hand",
    )
    return [
        "## Deck-under-test cards",
        "",
        *_table(header, [_card_row(c) for c in res["cards"]]),
        "",
    ]


def _bench(res: dict[str, Any]) -> list[str]:
    top = [b for b in res["benchmark_cards"] if b["rank_eligible"]][:TOP_BENCH_CARDS]
    header = ("Benchmark card", "Seen games", "DUT loss rate when seen", "when not seen", "Lift")
    rows = [
        [
            f"{b['card_number']} {b['name']}",
            b["seen_games"],
            rate_text(b["dut_loss_rate_when_seen"]),
            rate_text(b["dut_loss_rate_when_not_seen"]),
            points(b["loss_rate_lift"]),
        ]
        for b in top
    ]
    lines = ["## Benchmark cards most associated with DUT losses", ""]
    if not rows:
        need = res["hypothesis_thresholds"]["min_games"]
        return [*lines, f"No benchmark card has {need}+ games both seen and unseen yet.", ""]
    return [*lines, *_table(header, rows), ""]


def _hypotheses(res: dict[str, Any]) -> list[str]:
    lines = [
        "## Tuning hypotheses (to test, not conclusions)",
        "",
    ]
    t = res["hypothesis_thresholds"]
    gate = (
        f"at least {t['min_games']} games per side and a two-proportion z-test significant at "
        f"{t['alpha']} after a {t['multiple_comparisons'].capitalize()} correction"
    )
    if not res["hypotheses"]:
        return [*lines, f"No difference cleared the noise gate ({gate}). Run more games.", ""]
    lines += [f"Signals shown cleared the noise gate ({gate}).", ""]
    lines += [f"- {h['statement']} {_evidence_note(h)}" for h in res["hypotheses"]]
    return [*lines, ""]


def _evidence_note(h: dict[str, Any]) -> str:
    ev = h["evidence"]
    if "p_value" not in ev:
        return f"(sample: {h['sample_size']})"
    return (
        f"(sample: {h['sample_size']}; p = {ev['p_value']}, "
        f"{ev.get('comparisons', 1)} comparison(s) of this kind)"
    )


def _provenance(res: dict[str, Any]) -> list[str]:
    v = res["versions"]
    lines = [
        "## Versions",
        "",
        f"gcg-sim {v['package']}; results schema {v['results_schema']}; card data "
        f"{v['dataset_version']} (built {v['data_built_at']}); Comprehensive Rules "
        f"{v['rules_version']} ({v['rules_effective_date']}); Banned & Restricted list "
        f"{v['banlist_effective_date']}; BO3 rules {v['bo3_rules_effective_date']}.",
        "",
        "## Conflict resolutions affecting these decks",
        "",
    ]
    conflicts = res["conflict_resolutions"]
    lines += [
        f"- `{c['conflict_id']}` ({c['card_number']} {c['name']}; {', '.join(c['decks'])}): "
        f"{c['decision']}"
        for c in conflicts
    ] or ["- none"]
    lines += ["", "## Replays", ""]
    lines += [
        f"- `{r['file']}`: {r['result']}, match {r['match_index']} game {r['game_number']}, "
        f"{r['turns']} turns, {r['end_reason']}"
        for r in res["replays"]
    ] or ["- none"]
    lines += ["", "## Notes", "", *(f"- {n}" for n in res["notes"]), ""]
    return lines


def render_summary(res: dict[str, Any], command: str) -> str:
    sections = [
        _headline(res, command),
        _results(res),
        _endings(res),
        _mulligan(res),
        _cards(res),
        _bench(res),
        _hypotheses(res),
        _provenance(res),
    ]
    return "\n".join(line for section in sections for line in section)
