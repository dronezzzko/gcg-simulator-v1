# Data provenance notes for the Rust reports (W-88, W-89)

## `dataset_version` names the code commit, not the data commit (W-89)

`results.json` `versions.dataset_version` is `31-ebd08a4b4f8c8898bc407498229839548fb911e9`,
copied from `src/gcg_sim/data/gcgapi/manifest.json` exactly as the Python reports write it
(`reports/versions.py:56-68`). Its sha (`ebd08a4…`, the manifest's `source_commit`) is the gcg-api
**code** commit that produced the snapshot. The **data** commit that the lock pins is
`78a64df3519cb6c773b3d2f4a8ecad14d8096b0e` (`data/SOURCES.lock.json`, source `gcgapi:commit`,
whose notes say the same). Read `dataset_version` as "gcg-api run 31, built by code commit
ebd08a4"; to find the files, use the lock's data commit.

Evidence: `crates/gcg-data/tests/p6k_data_tripwires.rs::w89_dataset_version_names_the_code_commit_not_the_data_commit`
(the Rust value equals the manifest's, its sha equals `source_commit`, and differs from the
lock's data commit) and `crates/gcg-data/tests/status.rs::versions_equal_python` (byte-equal
to Python).

## Which sources are packaged (W-88)

`crates/gcg-data/tests/p6k_data_tripwires.rs::w88_provenance_listing_covers_every_source`
classifies all 49 sources of `data/sources.d/*.json` (equal to the 49 of the lock):

- **packaged**: the cited `local_path` is in the repository, and its SHA-256 equals the lock's;
- **text-only**: only a text extraction is stored; exactly one source, the Asia floor-rules PDF
  (`official-asia-floor-pdf`, 3,046,969 bytes), whose text is
  `data/official_raw/asia_pdf_floor-rule-forasia.txt`;
- **hash-only**: neither the source nor a text extraction is stored (`gcgapi:commit`,
  `cards.json`, `cards/en/*.json`, the rules page and PDF records of `rules.json`).

The listing is the logic of the planned `gcg-eval data-status --sources` (A30); the option
itself is a `gcg-eval` change (see `docs/dev/eval/weakness-status/P6-K.md`, interface notes).
Storing more raw sources is a data-refresh decision outside the port.
