# Data sources and URLs

The simulator reads only packaged copies; the refresh skill is the only network client. Each
source's provenance entry (URL, version, effective date, retrieval time, sha256, local path) is
in one of three `data/sources.d/*.json` fragments. The skill merges them into
`data/SOURCES.lock.json` and `docs/SOURCES.md` with
`uv run python -m gcg_sim.tools.sources --write`.

## gcg-api (card data)

| What | Where |
| --- | --- |
| Repository | https://github.com/yzRobo/gcg-api (weekly `data: weekly refresh (run N)` commits) |
| Raw file at a commit | `https://raw.githubusercontent.com/yzRobo/gcg-api/<commit>/<path>` |
| Commit page | `https://github.com/yzRobo/gcg-api/commit/<commit>` |
| Licence | ODbL v1.0 (`LICENSE-DATA`, packaged); attribution is in `docs/SOURCES.md` |
| Fetch | `scripts/fetch_gcgapi.sh [--ref REF] --dest DIR`: partial sparse clone of `data/`, `schema.sql`, `LICENSE-DATA` |
| Provenance fragment | `data/sources.d/gcgapi.json`, written by `python -m gcg_sim.tools.refresh apply` |

Packaged file set (`src/gcg_sim/data/gcgapi/` ← clone path):

| packaged | clone | content |
| --- | --- | --- |
| `cards.ndjson` | `data/cards.ndjson` | one record per printing (`product_id`; alt printings `_pN`) |
| `rulings.json` | `data/rulings.json` | card rulings, keyed `card_number` + `num` (Q-number) |
| `rules-faq.json` | `data/rules-faq.json` | rules FAQ, keyed `num` |
| `errata.json` | `data/errata.json` | errata ledger (`field`, `before`, `after`, `date`, `status`) |
| `products.json` | `data/products.json` | products and release dates |
| `manifest.json` | `data/manifest.json` | `schema_version`, `dataset_version` (`<run>-<code sha>`), `built_at`, counts |
| `sets/en/index.json` | `data/sets/en/index.json` | set codes, names and card counts |
| `schema.sql` | `schema.sql` | upstream SQL schema (reference only) |
| `LICENSE-DATA` | `LICENSE-DATA` | ODbL text |

`data/cards.json` and `data/cards/en/*.json` are not packaged. Their hashes are in the lock, and
`apply` checks that they hold the same records as `cards.ndjson`. `dataset_version`'s sha is
the code commit that produced the data, not the data commit that the lock pins.

## Official English sites

The US site is `https://www.gundam-gcg.com/en/` and the Asia site is
`https://www.gundam-gcg.com/asia-en/`. `scripts/fetch_official.py fetch` reads the full list
from `data/sources.d/official.json` and derives each file slug from the cached `local_path`
(`data/official_raw/<slug>.html|pdf`, next to `<slug>.txt`).

| Topic | US URL (path under the site) | Normalized into |
| --- | --- | --- |
| Rules hub: CR "Updated" date, PDF link, B&R links, BO3 | `/en/rules/` | `rules_version.json`, `banlist.json` |
| Comprehensive Rules PDF (`?NNNNNN` cache-buster on the link) | `/en/pdf/comprehensiverules_en.pdf` | `rules_version.json`, rules markdown |
| Current banned/restricted list | `/en/news/01_279.html` | `banlist.json` |
| B&R announcements (July 2026, April 2026) | `/en/news/01_277.html`, `/en/news/01_234.html` | `banlist.json` |
| BO3 match rules | `/en/news/best-of-three.html` | `bo3_match_rules.json` |
| Language / edition usage rules | `/en/news/lang_card_rule.html` | `edition_language.json` |
| Deck-building guide, play guide | `/en/news/decks-build.html`, `/en/welcome/playguide.php` | `deck_construction.json` |
| Preparing to Play FAQ (Q1–Q12) | `/en/rules/faqs/list.php?sub_category=Preparing+to+Play` | `deck_construction.json`, `floor_rules.json` |
| FAQ index (per-product FAQ dates) | `/en/rules/faqs/` | review only |
| Tournament Rules Manual (TRM) PDF | `/en/pdf/floor_rule_en.pdf` | `floor_rules.json`, `bo3_match_rules.json` |
| Sanctioned Tournament Floor Rules PDF | `/en/pdf/tournament-rules_en.pdf` | `floor_rules.json` |
| Errata notices (filed under NEWS) | `/en/news/01_204.html`, `/en/news/02_157.html`, `/en/news/02_193.html` | `edition_language.json` → `errata.official_notices`; `overrides.json` |
| Card database page (for errata checks) | `/en/cards/detail.php?detailSearch=<CARD>` | review only |
| News listing (all; RULES-tagged) | `/en/news/`, `/en/news/?subcategory=all&tag=RULES&page=1` | `fetch_official.py news` |
| Edition Beta pages | `/en/news/003.html`, `/en/products/limitedbox-beta.html`, `/en/events/WkndBetaBattle-2025.html` | `edition_language.json` |

English (Asia) twins are `/asia-en/rules/`, `/asia-en/news/01_279.html`, `01_277`, `01_234`,
`best-of-three`, `lang_card_rule` and the RULES listing, plus
`/asia-en/pdf/comprehensiverules_asia-en.pdf` (byte-identical to the US PDF so far). The Asia
"Floor Rules" link (`https://dcd.sc/bcg10`, which redirects to
`https://www.carddass.com/bcg/en/pdf/floor-rule-forasia.pdf`) is the multi-title BANDAI CARD
GAMES Floor Rules. That PDF is larger than 3 MB, so only its text extraction is kept.

Known US/Asia differences:
- B&R effective dates can differ by a day: July 2026 was 2026-07-24 in the US and 2026-07-25
  in Asia.
- The Asia notices omit US-only paragraphs (Regionals, deck resets).
- The Asia floor rules end a time-out tiebreak with rock-paper-scissors.

## Fetch etiquette

- One request at a time, with a pause (`--delay`, default 1.5 s).
- The User-Agent names the skill.
- HTML text: `scripts/html_to_text.py`, stdlib. It reproduces the cached `.txt` files byte for
  byte.
- PDF text: `scripts/pdf_to_text.py`, which needs pypdf. Run it with
  `uv run --no-project --with pypdf`, so pypdf never becomes a project dependency.
- Never fetch card images (`image_url`) or anything else a page merely links to.
