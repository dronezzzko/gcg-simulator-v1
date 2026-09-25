# Conflict policies and how to resolve new conflicts

`uv run python -m gcg_sim.sources.conflicts` writes the report: `data/conflicts.json` and
`docs/CONFLICTS.md`. The report is built from the cached snapshots, the rules markdown,
`src/gcg_sim/data/curated_conflicts.json` and `src/gcg_sim/data/overrides.json`.

`--check` fails when either output is stale, a conflict has no valid resolution, or a curated
quote no longer matches its source. Cached data is never edited. Every conflict gets exactly
one resolution in `overrides.json`.

## Order of authority (highest first)

1. **Official errata.** Verified on every run.
2. **Official card rulings and rules-FAQ answers.** They override literal text. The engine
   implements them, and a `@pytest.mark.ruling` or `@pytest.mark.faq` test pins each one.
3. **Card text over the Comprehensive Rules** (rule 1-3-1), using the canonical printing's
   wording.
4. **The Comprehensive Rules**, with obvious typos and wrong cross-references read as
   intended.
5. **The most defensible reading** for ambiguous text, with the reasoning recorded.

## Policies (`overrides.json` → `policies`)

| Policy | Use for |
| --- | --- |
| `errata_supersedes_print` | Official errata. When gcg-api lacks one, add `field_overrides` with the corrected text; see the example below. |
| `ruling_overrides_text` | A ruling that changes or completes card text or the rules. Needs a `@pytest.mark.ruling` test. |
| `faq_clarifies_rules` | A rules-FAQ answer that fills a gap in the rules. Needs a `@pytest.mark.faq` test. |
| `canonical_printing` | Printings of one card number disagree. Pin `canonical_product_id` (normally the base printing). |
| `edition_beta_superseded` | Edition Beta printings with deviating stats or text. |
| `token_stats_from_effect` | Token stats come from the effect that creates them. |
| `null_stat_is_zero` | A required stat that the data leaves empty is 0. |
| `text_format_correction` | Formatting defects that would make the compiler misread text, such as missing `<>` or an ASCII `:` as the cost separator. Fix with `field_overrides`. |
| `engine_derives_markers` | gcg-api `keyword_effects`/`timing_markers` disagree with the text. The engine reads the text. |
| `rules_obvious_reading`, `rules_xref_intended_target` | Rules typos and wrong cross-references. |
| `banlist_most_defensible` | Ambiguities in the B&R list. |
| `most_defensible_reading` | Anything else genuinely ambiguous. Explain the choice. |

Add a new policy only when none of these fits. A new policy needs `summary` and `rationale`.

## Resolution shape (`overrides.json` → `resolutions[<conflict id>]`)

```json
{
  "policy": "errata_supersedes_print",
  "decision": "What the engine does, in one or two sentences.",
  "rationale": "Why, citing the official source.",
  "card_numbers": ["ST12-001"],
  "field_overrides": {"effect": "<full corrected effect text>"}
}
```

- The allowed keys are `policy`, `decision`, `rationale`, `canonical_product_id`,
  `field_overrides` and `card_numbers`.
- `field_overrides` needs `card_numbers` listing exactly the conflict's card numbers. The
  overridden fields must be gameplay fields.
- `canonical_product_id` must be a printing of the conflict's single card number.

## Failure types after a refresh

| `--check` output | What to do |
| --- | --- |
| `unresolved conflict: divergent:<CARD>` | A new alt printing differs from the base. Classify it with `explain` and the `details` in `data/conflicts.json`; usually `canonical_printing` or `edition_beta_superseded`. |
| `unresolved conflict: markers:<CARD>:*` / `data-null:*` | Resolve with `engine_derives_markers` (keyword/timing marker fields), `text_format_correction` with `field_overrides` (`markers:<CARD>:unbracketed-keyword`, `:ascii-colon`), or `null_stat_is_zero`. |
| `unresolved conflict: banlist:*` | A new ambiguity in `banlist.json`. Resolve with `banlist_most_defensible`. |
| `unresolved conflict: unreviewed:ruling:<CARD>:<Qn>` or `unreviewed:faq:<Qn>` | Read the ruling. If it changes behaviour: add a curated entry to `curated_conflicts.json` (kind `ruling_vs_text` / `faq_vs_rules`, verbatim `evidence` quotes), a `ruling_overrides_text` / `faq_clarifies_rules` resolution keyed by the curated id, engine support, and a tagged test. Otherwise record the printed fingerprint under `reviewed_without_conflict.rulings["<CARD>:<Qn>"]` (or `.faq["<Qn>"]`). Either way, add a tagged test or an N/A reason (`tests/meta/`). |
| `... is changed since it was reviewed` | The ruling's answer changed. Review it again, update the curated entry or the fingerprint, and update the test. |
| `unresolved conflict: errata-unapplied:<CARD>...` | gcg-api lists an erratum its card text does not reflect, or an official notice is missing. Add a `field_overrides` resolution with `errata_supersedes_print`. |
| `unresolved conflict: rules-version:mismatch:...` | The official rules are newer than the markdown. Do SKILL.md step 3; the conflict disappears. |
| `invalid: ...` | The resolution breaks the shape above. Fix it. |
| `stale curated evidence: <id>` | A quoted sentence no longer appears in its source (reworded card, ruling or rule). Review the entry, then update its quotes or delete it with its resolution. |
| `warning: orphaned resolution <id>` / `reviewed:<source>:<ref>` | The conflict or entry no longer exists. Delete the resolution or review record. This is expected when gcg-api finally ships an erratum the override already applied. |

After every edit to `overrides.json` or `curated_conflicts.json`, run these in order:
`python -m gcg_sim.sources.conflicts`, then `--check`, then
`python -m gcg_sim.tools.sources --write`. Both files are also hashed in the lock.

## Existing precedent

- `errata-unapplied:ST12-001:news-02_193`: the 2026-09-04 errata adds 【Once per Turn】.
  gcg-api lacked it at the pinned commit. `refresh diff` reports it under "official errata
  missing from gcg-api errata.json".
- `ruling:ST12-001:Q436` / `Q437`: rulings extended to identical wording on other cards.
- `divergent:GD01-005`: the base printing's wording wins over older promo and Beta wordings.
