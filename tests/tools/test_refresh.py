"""Refresh skill core (``gcg_sim.tools.refresh``): SPEC criterion 9.

A dry run against the pinned snapshot changes nothing; a synthetic new card in a temporary copy
is detected by the diff, fails the coverage gate, and passes it once a binding and a card test
exist; the banned/restricted list validates against the cached card data.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from gcg_sim.effects.registry import get_registry
from gcg_sim.tools import refresh
from gcg_sim.tools.marks import scan, values
from gcg_sim.tools.sources import load_fragments

ROOT = Path(__file__).resolve().parents[2]
PACKAGED = ROOT / "src" / "gcg_sim" / "data" / "gcgapi"
SKILL_SCRIPTS = ROOT / ".claude" / "skills" / "gcg-refresh-data" / "scripts"
NEW_NUMBER = "GD99-001"
NEW_EFFECT = "【Deploy】Draw 1 for each friendly Unit in play, up to a maximum of 2."
VANILLA_UNIT = "GD01-060"


def _tree_hashes(*roots: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for root in roots:
        for path in sorted(root.rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                out[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def _read_ndjson(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _write_ndjson(path: Path, records: list[dict[str, Any]]) -> None:
    text = "".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) + "\n" for r in records)
    path.write_text(text, encoding="utf-8")


def _template(number: str) -> dict[str, Any]:
    for rec in _read_ndjson(PACKAGED / "cards.ndjson"):
        if rec["product_id"] == number:
            return rec
    raise AssertionError(f"{number} not in the packaged data")


def _new_card(
    product_id: str = NEW_NUMBER, effect: str = NEW_EFFECT, **fields: Any
) -> dict[str, Any]:
    rec = dict(_template(VANILLA_UNIT))
    rec.update(
        {
            "product_id": product_id,
            "card_number": product_id.split("_", maxsplit=1)[0],
            "name": "Refresh Test Gundam",
            "set_code": "GD99",
            "set_name": "Refresh Test Set",
            "color": "Blue",
            "level": 3,
            "cost": 2,
            "ap": 3,
            "hp": 3,
            "ap_raw": "3",
            "hp_raw": "3",
            "effect": effect,
            "keyword_effects": [],
            "timing_markers": ["Deploy"],
            "where_to_get": "Refresh Test Set [GD99]",
        }
    )
    rec.update(fields)
    return rec


# ---------------------------------------------------------------------------------------------
# dry run


def test_dry_run_against_pinned_snapshot_changes_nothing(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    before = _tree_hashes(ROOT / "src" / "gcg_sim" / "data", ROOT / "data")
    out_json = tmp_path / "diff.json"

    assert refresh.main(["diff", "--new-data", str(PACKAGED), "--json", str(out_json)]) == 0
    report = json.loads(out_json.read_text(encoding="utf-8"))
    assert report["summary"]["has_changes"] is False
    assert all(f["status"] == "unchanged" for f in report["files"].values())
    for section, counts in report["summary"]["counts"].items():
        assert counts == {"added": 0, "removed": 0, "changed": 0}, section
    assert report["printings"]["cosmetic_field_changes"] == {}

    assert refresh.main(["apply", "--new-data", str(PACKAGED), "--dry-run"]) == 0
    assert refresh.main(["banlist"]) == 0
    out = capsys.readouterr().out
    assert "files: all unchanged" in out
    assert "no changes: the packaged gcg-api snapshot and its lock entries are current" in out
    assert "banlist OK" in out

    for args in (["check"], ["fragment", "--dry-run"]):
        res = subprocess.run(
            [sys.executable, str(SKILL_SCRIPTS / "official_normalize.py"), *args],
            capture_output=True,
            text=True,
            check=False,
        )
        assert res.returncode == 0, res.stdout + res.stderr
    assert _tree_hashes(ROOT / "src" / "gcg_sim" / "data", ROOT / "data") == before


def test_diff_output_is_deterministic(tmp_path: Path) -> None:
    first, second = tmp_path / "a.json", tmp_path / "b.json"
    assert refresh.main(["diff", "--new-data", str(PACKAGED), "--json", str(first)]) == 0
    assert refresh.main(["diff", "--new-data", str(PACKAGED), "--json", str(second)]) == 0
    assert first.read_bytes() == second.read_bytes()


# ---------------------------------------------------------------------------------------------
# diff: synthetic changes in a temporary copy


def _changed_copy(tmp_path: Path) -> Path:
    new = tmp_path / "gcgapi"
    shutil.copytree(PACKAGED, new)
    records = _read_ndjson(new / "cards.ndjson")
    by_id = {r["product_id"]: r for r in records}
    by_id["GD01-002"]["effect"] = by_id["GD01-002"]["effect"].replace("0 Lv.", "1 Lv.")
    apostrophe = next(
        r for r in records if r["product_id"] == r["card_number"] and "'" in (r["effect"] or "")
    )
    apostrophe["effect"] = apostrophe["effect"].replace("'", "’")
    by_id["GD01-008"]["ap"] = 9
    records = [r for r in records if r["card_number"] != "ST01-005"]
    alt = dict(by_id["ST01-001"])
    alt["product_id"] = "ST01-001_p99"
    records += [alt, _new_card(), _new_card(f"{NEW_NUMBER}_p1")]
    _write_ndjson(new / "cards.ndjson", records)

    rulings = json.loads((new / "rulings.json").read_text(encoding="utf-8"))
    rulings[0]["answer"] = "Changed answer."
    rulings.append(
        {
            "card_number": NEW_NUMBER,
            "num": "Q9999",
            "date": "October 1, 2026",
            "question": "Does it count itself?",
            "answer": "Yes, it does.",
        }
    )
    (new / "rulings.json").write_text(json.dumps(rulings), encoding="utf-8")
    faq = json.loads((new / "rules-faq.json").read_text(encoding="utf-8"))
    (new / "rules-faq.json").write_text(json.dumps(faq[1:]), encoding="utf-8")
    errata = json.loads((new / "errata.json").read_text(encoding="utf-8"))
    errata.append(
        {
            "card_number": "ST12-001",
            "field": "effect",
            "before": "【During Pair･Lv.5 or Higher Pilot】",
            "after": "【During Pair･Lv.5 or Higher Pilot】【Once per Turn】",
            "date": "2026-09-04",
            "status": "applied",
        }
    )
    (new / "errata.json").write_text(json.dumps(errata), encoding="utf-8")
    sets = json.loads((new / "sets" / "en" / "index.json").read_text(encoding="utf-8"))
    sets.append({"set_code": "GD99", "set_name": "Refresh Test Set", "card_count": 2})
    (new / "sets" / "en" / "index.json").write_text(json.dumps(sets), encoding="utf-8")
    manifest = json.loads((new / "manifest.json").read_text(encoding="utf-8"))
    manifest["dataset_version"] = "26-synthetic"
    manifest["card_count"] = len(records)
    (new / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    return new


def test_diff_detects_synthetic_new_card_and_changes(tmp_path: Path) -> None:
    new = _changed_copy(tmp_path)
    report = refresh.diff_snapshots(refresh.load_snapshot(PACKAGED), refresh.load_snapshot(new))
    cards = report["cards"]
    assert [c["card_number"] for c in cards["added"]] == [NEW_NUMBER]
    assert cards["added"][0]["product_ids"] == [NEW_NUMBER, f"{NEW_NUMBER}_p1"]
    assert [c["card_number"] for c in cards["removed"]] == ["ST01-005"]

    changed = {c["card_number"]: c for c in cards["changed"]}
    wording = changed["GD01-002"]["changes"][0]
    assert wording["field"] == "effect"
    assert wording["effect_change"]["kind"] == "wording"
    assert wording["effect_change"]["old_text_hash"] != wording["effect_change"]["new_text_hash"]
    typography = [
        ch["effect_change"]["kind"]
        for c in changed.values()
        for ch in c["changes"]
        if ch["field"] == "effect" and c["card_number"] != "GD01-002"
    ]
    assert typography == ["typography"]
    assert changed["GD01-008"]["changes"] == [
        {"field": "ap", "old": _template("GD01-008")["ap"], "new": 9}
    ]

    printings = report["printings"]
    added = {p["product_id"]: p["new_card_number"] for p in printings["added"]}
    assert added == {"ST01-001_p99": False, NEW_NUMBER: True, f"{NEW_NUMBER}_p1": True}
    assert {p["card_number"] for p in printings["removed"]} == {"ST01-005"}

    assert [r["key"] for r in report["rulings"]["added"]] == [f"{NEW_NUMBER}:Q9999"]
    assert len(report["rulings"]["changed"]) == 1
    assert report["rulings"]["changed"][0]["changes"][0]["field"] == "answer"
    assert len(report["rules_faq"]["removed"]) == 1
    assert [e["key"] for e in report["errata"]["added"]] == ["ST12-001:effect:2026-09-04"]
    assert [s["key"] for s in report["sets"]["added"]] == ["GD99"]
    assert report["manifest"]["dataset_version"]["changed"] is True
    assert report["manifest"]["schema_version"]["changed"] is False
    assert report["summary"]["has_changes"] is True
    assert report["summary"]["official_errata_missing_from_gcgapi"] == []
    assert report["files"]["products.json"]["status"] == "unchanged"
    assert report["files"]["cards.ndjson"]["note"] == "record changes"

    lines = refresh.render_diff(report)
    assert any(line.startswith(f"  NEW {NEW_NUMBER} Refresh Test Gundam") for line in lines)
    assert any(line.startswith("  CHANGED GD01-002") and "(wording)" in line for line in lines)


def test_diff_accepts_a_clone_layout(tmp_path: Path) -> None:
    clone = tmp_path / "clone"
    for local, upstream in refresh.PACKAGED_FILES.items():
        (clone / upstream).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(PACKAGED / local, clone / upstream)
    snap = refresh.load_snapshot(clone)
    assert snap.layout == "clone"
    report = refresh.diff_snapshots(refresh.load_snapshot(PACKAGED), snap)
    assert report["summary"]["has_changes"] is False


# ---------------------------------------------------------------------------------------------
# apply


def _git(repo: Path, *args: str) -> str:
    env = {
        **os.environ,
        "GIT_AUTHOR_NAME": "refresh-test",
        "GIT_AUTHOR_EMAIL": "refresh-test@example.invalid",
        "GIT_COMMITTER_NAME": "refresh-test",
        "GIT_COMMITTER_EMAIL": "refresh-test@example.invalid",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_NOSYSTEM": "1",
    }
    res = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=True, env=env
    )
    return res.stdout.strip()


def _fake_clone(tmp_path: Path, records: list[dict[str, Any]]) -> tuple[Path, str]:
    clone = tmp_path / "gcg-api"
    for local, upstream in refresh.PACKAGED_FILES.items():
        (clone / upstream).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(PACKAGED / local, clone / upstream)
    _write_ndjson(clone / "data" / "cards.ndjson", records)
    (clone / "data" / "cards.json").write_text(json.dumps(records), encoding="utf-8")
    per_set: dict[str, list[dict[str, Any]]] = {}
    for r in records:
        per_set.setdefault(str(r["set_code"]).lower(), []).append(r)
    (clone / "data" / "cards" / "en").mkdir(parents=True)
    for code, recs in per_set.items():
        (clone / "data" / "cards" / "en" / f"{code}.json").write_text(
            json.dumps(recs), encoding="utf-8"
        )
    manifest = json.loads((clone / "data" / "manifest.json").read_text(encoding="utf-8"))
    manifest.update({"dataset_version": "26-synthetic", "built_at": "2026-10-01T00:00:00.000Z"})
    (clone / "data" / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    _git(clone, "init", "--quiet")
    _git(clone, "add", "-A")
    _git(clone, "commit", "--quiet", "--no-gpg-sign", "-m", "data: weekly refresh (run 26)")
    return clone, _git(clone, "rev-parse", "HEAD")


def _temp_checkout(tmp_path: Path) -> Path:
    root = tmp_path / "checkout"
    (root / "data" / "sources.d").mkdir(parents=True)
    shutil.copyfile(ROOT / refresh.FRAGMENT_REL, root / refresh.FRAGMENT_REL)
    shutil.copytree(PACKAGED, root / refresh.DEST_REL)
    return root


def test_apply_installs_a_clone_and_rewrites_the_lock_fragment(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    records = [*_read_ndjson(PACKAGED / "cards.ndjson"), _new_card()]
    clone, commit = _fake_clone(tmp_path, records)
    root = _temp_checkout(tmp_path)
    stamp = "2026-10-01T09:00:00Z"
    common = ["--new-data", str(clone), "--root", str(root), "--retrieved-at", stamp]

    before = _tree_hashes(root)
    assert refresh.main(["apply", *common, "--dry-run"]) == 0
    assert _tree_hashes(root) == before
    assert "dry run: nothing was written" in capsys.readouterr().out

    assert refresh.main(["apply", *common]) == 0
    out = capsys.readouterr().out
    assert "uv run python -m gcg_sim.tools.sources --write" in out
    dest = root / refresh.DEST_REL
    for local, upstream in refresh.PACKAGED_FILES.items():
        assert (dest / local).read_bytes() == (clone / upstream).read_bytes(), local
    fragment = {e["id"]: e for e in json.loads((root / refresh.FRAGMENT_REL).read_text("utf-8"))}
    assert fragment["gcgapi:commit"]["url"].endswith(commit)
    assert "data: weekly refresh (run 26)" in fragment["gcgapi:commit"]["notes"]
    cards = fragment["gcgapi:data/cards.ndjson"]
    assert cards["sha256"] == refresh.sha256_file(clone / "data" / "cards.ndjson")
    assert cards["url"] == (
        f"https://raw.githubusercontent.com/yzRobo/gcg-api/{commit}/data/cards.ndjson"
    )
    assert {e["retrieved_at"] for e in fragment.values()} == {stamp}
    assert {e["version"] for e in fragment.values()} == {"26-synthetic"}
    assert "same multiset" in fragment["gcgapi:data/cards/en/*.json"]["notes"].lower()
    _, problems = load_fragments(root)
    assert problems == []

    assert refresh.main(["apply", *common]) == 0
    assert "no changes" in capsys.readouterr().out


def test_apply_refuses_changed_data_that_is_not_a_git_clone(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    new = _changed_copy(tmp_path)
    root = _temp_checkout(tmp_path)
    assert refresh.main(["apply", "--new-data", str(new), "--root", str(root), "--dry-run"]) == 2
    assert "fetch_gcgapi.sh" in capsys.readouterr().err


def test_apply_refuses_a_schema_change_without_the_flag(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    clone, _ = _fake_clone(tmp_path, _read_ndjson(PACKAGED / "cards.ndjson"))
    manifest = json.loads((clone / "data" / "manifest.json").read_text(encoding="utf-8"))
    manifest["schema_version"] = 2
    (clone / "data" / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    root = _temp_checkout(tmp_path)
    args = ["apply", "--new-data", str(clone), "--root", str(root), "--dry-run"]
    assert refresh.main(args) == 2
    assert "--allow-schema-change" in capsys.readouterr().err
    assert refresh.main([*args, "--allow-schema-change"]) == 0


# ---------------------------------------------------------------------------------------------
# coverage


def test_coverage_lists_unimplemented_changed_and_untested_cards(tmp_path: Path) -> None:
    golden_dir = tmp_path / "golden"
    shutil.copytree(ROOT / "tests" / "effects" / "golden", golden_dir)
    gd01 = json.loads((golden_dir / "GD01.json").read_text(encoding="utf-8"))
    gd01["GD01-001"]["text_hash"] = "0" * 16
    del gd01["GD01-002"]
    (golden_dir / "GD01.json").write_text(json.dumps(gd01), encoding="utf-8")

    report = refresh.coverage_report(ROOT / "tests", golden_dir)
    reg = get_registry()
    expected = sorted(
        c.card_number for c in reg.db.real_cards() if reg.cards[c.def_id].script is None
    )
    assert sorted(u["card_number"] for u in report["unimplemented"]) == expected
    assert [t["card_number"] for t in report["text_changed"]] == ["GD01-001"]
    assert report["not_in_golden"] == ["GD01-002"]
    assert "GD01-001" not in report["untested"]  # tagged in tests/cards
    assert report["unknown_card_tags"] == []
    assert set(report["needs_work"]) >= {"GD01-001", "GD01-002", *expected}
    assert report["by_set"]["GD01"]["text_changed"] == 1

    out_json = tmp_path / "coverage.json"
    args = ["coverage", "--golden-dir", str(golden_dir), "--json", str(out_json)]
    assert refresh.main(args) == 0
    assert refresh.main([*args, "--fail-on-gaps"]) == 1
    assert json.loads(out_json.read_text(encoding="utf-8"))["counts"] == report["counts"]


# ---------------------------------------------------------------------------------------------
# banlist


def test_banlist_check_passes_on_the_cached_files(capsys: pytest.CaptureFixture[str]) -> None:
    banlist = json.loads(refresh.DEFAULT_BANLIST.read_text(encoding="utf-8"))
    report = refresh.check_banlist(banlist, refresh.load_snapshot(PACKAGED))
    assert report.problems == []
    assert report.warnings == []
    assert any("identical to the enumerated list" in line for line in report.info)
    assert refresh.main(["banlist", "--banlist", str(refresh.DEFAULT_BANLIST)]) == 0
    assert "banlist OK" in capsys.readouterr().out


def test_banlist_check_reports_bad_entries(tmp_path: Path) -> None:
    banlist = json.loads(refresh.DEFAULT_BANLIST.read_text(encoding="utf-8"))
    banlist["banned"][0]["name"] = "Not Anksha"
    banlist["banned_pairs"][0]["cards"][1] = "ST05-999"
    rule = banlist["attribute_pair_rules"][0]
    rule["enumerated_members"] = rule["enumerated_members"][1:]
    report = refresh.check_banlist(banlist, refresh.load_snapshot(PACKAGED))
    text = "\n".join(report.problems)
    assert "GD01-020 is named 'Not Anksha'" in text
    assert "ST05-999 is not in the card data" in text
    assert "only the predicate matches ['GD01-035']" in text
    assert "predicate_equals_enumeration is True but the check says False" in text


def test_banlist_check_flags_a_new_vanilla_pair_card(tmp_path: Path) -> None:
    new = tmp_path / "gcgapi"
    shutil.copytree(PACKAGED, new)
    records = _read_ndjson(new / "cards.ndjson")
    records.append(_new_card("GD99-002", effect="-", level=2, cost=1, ap=2, hp=2))
    _write_ndjson(new / "cards.ndjson", records)
    code = refresh.main(["banlist", "--data", str(new)])
    assert code == 1
    banlist = json.loads(refresh.DEFAULT_BANLIST.read_text(encoding="utf-8"))
    report = refresh.check_banlist(banlist, refresh.load_snapshot(new))
    assert any("only the predicate matches ['GD99-002']" in p for p in report.problems)
    assert any("predicate_matches_in_card_data is stale" in p for p in report.problems)


def test_predicate_language() -> None:
    card = {
        "card_number": "GD01-035",
        "name": "Zaku Ⅱ",
        "card_type": "UNIT",
        "color": "Green",
        "level": 2,
        "cost": 1,
        "ap": 2,
        "hp": 2,
        "traits": ["Zeon"],
        "has_effect": False,
    }
    assert refresh.matches({"all_of": [{"level": 2}, {"trait": "Zeon"}]}, card)
    assert refresh.matches({"any_of": [{"cost": 9}, {"name_contains": "Zaku"}]}, card)
    assert not refresh.matches({"not": {"card_number": "$matched"}}, card, bound="GD01-035")
    assert refresh.matches({"not": {"card_number": "$matched"}}, card, bound="ST01-005")
    with pytest.raises(refresh.RefreshError):
        refresh.matches({"rarity": "C"}, card)
    with pytest.raises(refresh.RefreshError):
        refresh.matches({"level": 2, "cost": 1}, card)


# ---------------------------------------------------------------------------------------------
# skill helpers


def test_html_extraction_reproduces_the_cached_text() -> None:
    sys.path.insert(0, str(SKILL_SCRIPTS))
    try:
        from html_to_text import html_to_text
    finally:
        sys.path.remove(str(SKILL_SCRIPTS))
    pages = sorted((ROOT / "data" / "official_raw").glob("*.html"))
    assert pages
    for page in pages:
        cached = page.with_suffix(".txt").read_text(encoding="utf-8")
        assert html_to_text(page.read_bytes()) == cached, page.name


def test_official_check_fails_on_a_quote_missing_from_the_source(tmp_path: Path) -> None:
    root = tmp_path / "checkout"
    for rel in ("data/official_raw", "data/sources.d", "src/gcg_sim/data/official"):
        shutil.copytree(ROOT / rel, root / rel)
    rules = "src/gcg_sim/data/rules/gundam-card-game-comprehensive-rules.md"
    (root / rules).parent.mkdir(parents=True)
    shutil.copyfile(ROOT / rules, root / rules)
    bo3 = root / "src/gcg_sim/data/official/bo3_match_rules.json"
    data = json.loads(bo3.read_text(encoding="utf-8"))
    data["sources"][0]["quote"] = "Matches are played to five wins."
    bo3.write_text(json.dumps(data), encoding="utf-8")
    res = subprocess.run(
        [
            sys.executable,
            str(SKILL_SCRIPTS / "official_normalize.py"),
            "--root",
            str(root),
            "check",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert res.returncode == 1
    assert "bo3_match_rules.json.sources[0]: quote not found" in res.stdout


# ---------------------------------------------------------------------------------------------
# end to end: coverage gate in a temporary copy of the repository

BINDING = '''"""Bindings for the synthetic GD99 set of the refresh end-to-end test."""

from __future__ import annotations

from gcg_sim.cards.model import CardDef
from gcg_sim.effects import dsl as d
from gcg_sim.effects.bindings import card


@card("GD99-001")
def gd99_001(c: CardDef) -> d.CardScript:
    """【Deploy】Draw 1 for each friendly Unit in play, up to a maximum of 2."""
    units = d.Count(d.Sel(d.Side.FRIENDLY, d.Loc.BATTLE))
    deploy = d.Triggered(d.Trigger(d.Ev.DEPLOYED), (d.Draw(d.MinOf((units, 2))),))
    return d.CardScript(c.card_number, abilities=(deploy,), source="binding")
'''

CARD_TEST = '''"""GD99-001 Refresh Test Gundam."""

from __future__ import annotations

import pytest

from gcg_sim.engine.types import Zone
from gcg_sim.testkit import Scenario, play


@pytest.mark.card("GD99-001")
@pytest.mark.parametrize(("others", "drawn"), [(0, 1), (1, 2), (3, 2)])
def test_gd99_001_deploy_draws_per_friendly_unit_up_to_two(others: int, drawn: int) -> None:
    sc = Scenario()
    sc.resources(0, 5)
    for _ in range(others):
        sc.add(0, "GD01-060")
    card = sc.add(0, "GD99-001", Zone.HAND)
    st = sc.start()
    before = len(st.zones[0][Zone.HAND])
    play(st, card)
    assert len(st.zones[0][Zone.HAND]) == before - 1 + drawn
'''


def _run(copy: Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith("PYTEST_")}
    env.update({"PYTHONPATH": str(copy / "src"), "PYTHONDONTWRITEBYTECODE": "1"})
    return subprocess.run(
        [sys.executable, *args], cwd=copy, env=env, capture_output=True, text=True, check=False
    )


def _pytest(copy: Path, *tests: str) -> subprocess.CompletedProcess[str]:
    return _run(copy, "-m", "pytest", "-q", "-p", "no:cacheprovider", "-p", "no:xdist", *tests)


def test_new_card_fails_the_coverage_gate_until_implemented(tmp_path: Path) -> None:
    copy = tmp_path / "repo"
    ignore = shutil.ignore_patterns("__pycache__")
    shutil.copytree(ROOT / "src", copy / "src", ignore=ignore)
    shutil.copytree(ROOT / "tests" / "effects", copy / "tests" / "effects", ignore=ignore)
    shutil.copyfile(ROOT / "pyproject.toml", copy / "pyproject.toml")
    probe = _run(copy, "-c", "import gcg_sim; print(gcg_sim.__file__)")
    assert Path(probe.stdout.strip()).is_relative_to(copy / "src"), probe.stdout + probe.stderr

    # Baseline: the cards whose gate status does not depend on tests outside tests/effects
    # (vanilla cards, plus implemented cards that the copied tests tag).
    reg = get_registry()
    tagged = values(scan(copy / "tests", ("card",)), "card")
    keep = {
        c.card_number
        for c in reg.db.real_cards()
        if (s := reg.cards[c.def_id].script) is not None
        and (s.source == "vanilla" or c.card_number in tagged)
    }
    assert VANILLA_UNIT in keep
    data = copy / "src" / "gcg_sim" / "data" / "gcgapi"
    baseline = [r for r in _read_ndjson(data / "cards.ndjson") if r["card_number"] in keep]
    _write_ndjson(data / "cards.ndjson", baseline)
    baseline_dir = tmp_path / "baseline"
    shutil.copytree(data, baseline_dir)
    gate = "tests/effects/test_card_coverage.py"
    res = _pytest(copy, gate)
    assert res.returncode == 0, res.stdout[-3000:] + res.stderr[-3000:]

    # A refresh brings a new card whose effect the compiler cannot handle.
    _write_ndjson(data / "cards.ndjson", [*baseline, _new_card()])
    report = refresh.diff_snapshots(
        refresh.load_snapshot(baseline_dir), refresh.load_snapshot(data)
    )
    assert [c["card_number"] for c in report["cards"]["added"]] == [NEW_NUMBER]
    res = _pytest(copy, gate)
    assert res.returncode == 1, res.stdout[-3000:]
    assert "test_every_card_is_implemented" in res.stdout
    assert "test_every_non_vanilla_card_has_a_behaviour_test" in res.stdout
    assert f"{NEW_NUMBER}: " in res.stdout
    assert f"'{NEW_NUMBER}'" in res.stdout
    cov_json = tmp_path / "coverage.json"
    cov = _run(
        copy,
        "-m",
        "gcg_sim.tools.refresh",
        "coverage",
        "--tests",
        "tests",
        "--golden-dir",
        "tests/effects/golden",
        "--json",
        str(cov_json),
    )
    assert cov.returncode == 0, cov.stderr
    coverage = json.loads(cov_json.read_text(encoding="utf-8"))
    assert [u["card_number"] for u in coverage["unimplemented"]] == [NEW_NUMBER]
    assert coverage["untested"] == [NEW_NUMBER]
    assert NEW_NUMBER in coverage["not_in_golden"]

    # Implement it: a binding module (auto-discovered) and a @pytest.mark.card test.
    (copy / "src" / "gcg_sim" / "effects" / "bindings" / "gd99.py").write_text(BINDING, "utf-8")
    (copy / "tests" / "effects" / "test_gd99_001.py").write_text(CARD_TEST, "utf-8")
    res = _pytest(copy, gate, "tests/effects/test_gd99_001.py")
    assert res.returncode == 0, res.stdout[-3000:] + res.stderr[-3000:]
    golden = _run(copy, "-m", "gcg_sim.tools.golden", "--write", "--prefix", "GD99")
    assert golden.returncode == 0, golden.stderr
    cov = _run(
        copy,
        "-m",
        "gcg_sim.tools.refresh",
        "coverage",
        "--tests",
        "tests",
        "--golden-dir",
        "tests/effects/golden",
        "--json",
        str(cov_json),
    )
    assert cov.returncode == 0, cov.stderr
    coverage = json.loads(cov_json.read_text(encoding="utf-8"))
    assert coverage["unimplemented"] == []
    assert coverage["untested"] == []
    assert NEW_NUMBER not in coverage["not_in_golden"]
    gd99 = json.loads((copy / "tests" / "effects" / "golden" / "GD99.json").read_text("utf-8"))
    assert gd99[NEW_NUMBER]["binding"] == "gd99"
