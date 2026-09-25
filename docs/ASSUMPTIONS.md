# Assumptions and decisions

Every decision made without asking the user is recorded here with its rationale.

## Preflight (2026-09-25)

| Check | Result | Decision |
| --- | --- | --- |
| Rules file | `gundam-card-game-comprehensive-rules.md` present, 1,197 lines, **Ver. 1.9.0, Updated Sep 11, 2026** | Authoritative; copied verbatim into package data (`src/gcg_sim/data/rules/`). |
| uv | 0.12.5 | Used for all project management. Python pinned to 3.12 (`.python-version`); uv-managed CPython 3.12.14 used. |
| git | 2.55.0; repo existed on `main` (clean) | Work on branch `feat/gcg-sim`; local commits at phase boundaries; no pushes. |
| Network | github.com 200, gundam-gcg.com/en 200, `git ls-remote` gcg-api HEAD = `f57b7c0b0ebc4c13d359649de19750c793ecbefc` | Snapshot pinned at `f57b7c0` (built 2026-09-21T12:11:55Z, dataset_version `25-676f1bf…`). |
| Tool permissions | User default mode `auto`; project `.claude/settings.json` adds allow rules for uv/git read/python/curl and WebFetch on gundam-gcg.com, github.com, gcgapi.com | Background workflow agents do not block on prompts. |
| `claude plugin validate` | Available in Claude Code 2.1.282 | Used for acceptance criterion 9. |

## Data snapshot

- gcg-api files cached in `src/gcg_sim/data/gcgapi/`: `cards.ndjson`, `rulings.json`,
  `rules-faq.json`, `errata.json`, `products.json`, `manifest.json`, `sets/en/index.json`,
  `LICENSE-DATA`, `schema.sql`. `cards.json` and `cards/en/*.json` hold the same records as
  `cards.ndjson` (equivalence checked by the ingestion step) and are omitted from the package to
  keep the wheel small; their hashes are still recorded in `data/SOURCES.lock.json`.
- Figures re-verified at the pinned commit: 1,148 card numbers, 1,912 printings, 368 rulings,
  119 rules-FAQ entries, 2 errata (both `applied`). The prompt's "780 distinct effect texts"
  measures 817 raw distinct `effect` strings (see ingestion report for normalized counts); the
  prompt's "about 40" divergent card numbers measures 43.

## Rules interpretation (engine)

| Topic | Decision | Basis |
| --- | --- | --- |
| Bases and battle damage | Bases deal no battle damage to the attacking Unit (every Base has 0 AP). | 8-5-2-4 describes one-way damage; conflict `rules-internal` resolution; no Base in the data has AP. |
| Simultaneous defeat | Game result is a draw (11-2-1, 1-2-1). In BO3 match scoring the turn player at the end loses (TRM 5.2 top-cut rule), so every match resolves. | Official tournament rules; recorded in `bo3_match_rules.json`. |
| "Exile ... from the game" | Non-token cards go to the removal area (5-12); tokens leave the game (5-17-2-5). | Rules define only "remove"; rulings treat exile as removal area. |
| Multiplayer wording | In 1v1: "each enemy player"/"another player"/"that player" = the opponent; "2 or more enemy players" is false; "number of enemy players" = 1. | Section 12 out of scope. |
| Pilot text routing | 【Burst】 and name-alias lines belong to the Pilot card; all other Pilot text is gained by the paired Unit unless it names hand/trash. | 3-3-9-1/3-3-9-2; the data does not mark which lines are above the name. |
| Resource caps | Placing beyond 15 Resources / 5 EX Resources does nothing (1-3-2). | Section 11 defines no excess management for resources. |
| Lasting "all your Units" effects | Apply only to Units present at resolution (FAQ Q105). | FAQ. |
| Card identity | uids are assigned after the seeded shuffle and cut, so a uid carries no identity information. | Needed for information-set honesty. |
| Simultaneous placement into private zones | Multiple cards placed into a deck at once keep selection order; "randomly" orders use the seeded RNG. | 4-1-6/4-1-7 owner choice modelled as selection order. |
| Turn safety cap | A game that reaches 200 turns or 20,000 decisions ends as a draw with reason `turn_limit`; the robustness suite asserts this never happens. | Termination guarantee; never silent. |
