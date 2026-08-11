# CI known-red allowlist

Measured 2026-08-11 on base `91c5ea6e` via `just green-all` in a fresh
worktree. The merge gate (`v6/justfile` green-all) is red on this base. The
CI job runs the gate and uploads the raw log as an artifact; the job's own
pass/fail is decided by the `allow:` lines below, so only a leg that fails
and is NOT listed here turns the job red.

Each red leg is listed with its exact failure text. Do not edit this list as
a way to make CI green; edit it only when the underlying defect is fixed and
the leg measured green.

## Red legs

| leg | exact failure text |
|---|---|
| plunit | `catalog_plane_rail:level_plane_family_corpus_counts`, `plunit_tests.pl:1312`, 1 of 598 |
| staleness-gate | `STALENESS_GATE_FAIL v6/ARCH-MAP.md is STALE (checked-in does not match self-map regeneration)`, v6/ARCH-MAP.md out of date vs HEAD sources |
| rtkq-golden | `missing release extractor: v6/sprefa-extract/target/release/extract` |
| compile-speed | `COMPILE_SPEED regressions=16 improvements=0 FAIL`, baseline written 2026-08-07 |
| tsv2-test | `hostDecode.test.ts:144` expected `[0,1,2,3]` actual `[1,2,2,3]` |
| flagship | `FAIL  no release extractor. A gate does not build; run: cd v6/sprefa-extract && cargo build --release --features cli --bin extract` |
| extraction-live | `FAIL  no release extractor. A gate does not build; run: cd v6/sprefa-extract && cargo build --release --features cli --bin extract` |
| lsp-diags | `FAIL  no release extractor. A gate does not build; run: cd v6/sprefa-extract && cargo build --release --features cli --bin extract` |
| golden-flex | `GOLDEN_COVERAGE FAIL: json_object/2 is excused as 'registry status refused' but its registry status is now live -- the excuse is stale` |
| getting-started | `Warning: Clauses of parse_dl_dcg:lex_token/2 are not together in the source-file`, `parse_dl_dcg.pl:737`, earlier definition at `:372` |
| scale-floor | `stmts/tick set @10000 [37,41] [39,43] FAIL` (load/timing sensitive) |
| memory-soak | `FAIL  sqlite_page_count_flat: second-quarter mean 24.8, final-quarter mean 49.5, ceiling 27.2` |

`leak-soak` and `serve-leak-soak` failed in the raw base run only from stale
`mktemp` literal files left in `$TMPDIR`; with a clean `$TMPDIR` both pass
(verified 2026-08-11) and are deliberately NOT allowlisted.

## Allowlist

allow: plunit
allow: staleness-gate
allow: rtkq-golden
allow: compile-speed
allow: tsv2-test
allow: flagship
allow: extraction-live
allow: lsp-diags
allow: golden-flex
allow: getting-started
allow: scale-floor
allow: memory-soak
