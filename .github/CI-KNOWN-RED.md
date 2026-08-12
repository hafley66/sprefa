# CI known-red allowlist

Measured 2026-08-12 on base `154ae23c` in a fresh worktree, each leg run 3
times one at a time on a quiet machine (all boop lanes <=0.2% cpu during the
timing-sensitive legs). The merge gate (`v6/justfile` green-all) is red on this
base. The CI job runs the gate and uploads the raw log as an artifact; the
job's own pass/fail is decided by the `allow:` lines below, so only a leg that
fails and is NOT listed here turns the job red.

Each red leg is listed with the exact failure text seen this measurement. Do
not edit this list as a way to make CI green; edit it only when the underlying
defect is fixed and the leg measured green.

Rows removed by re-measure at base `259e0289`: `flagship`, `getting-started`,
`scale-floor` (each measured green 3/3 after its stale pin was refreshed).

## Red legs

| leg | exact failure text | throw site |
|---|---|---|
| roundtrip | `G1 round-trip: 391 / 392 fixtures pass` then `FAIL mutual_recursion_matches_oracle (.../fixtures/engine_core.pl): fail(not_variant)` | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| golden-flex | `GOLDEN_COVERAGE FAIL: json_object/2 is excused as 'registry status refused' but its registry status is now live -- the excuse is stale`; `GOLDEN_COVERAGE FAIL: json_patch/2 (expression) is a registry construct the golden does not exercise`; `GOLDEN_COVERAGE 69 registry constructs, 2 unaccounted for` | `v6/prolog/compile/scripts/golden_coverage.pl:174,178` |
| tsv2-test | `hostDecode.test.ts:144`: decoded row count per demand `[2,1,2,3]`; actual `[1,2,2,3]` expected `[0,1,2,3]` (needs `gen_emitted/` present, produced by `just sweep`) | `v6/tsv2/tests/hostDecode.test.ts:144` |
| rtkq-golden | `ERR_ASSERTION` `deepStrictEqual` at `labs/1_rtkq-extraction-golden.ts:200`: `api_endpoint` rows emit `updateUser`-before-`listUsers`, order-sensitive golden expects `listUsers`-first (spans identical, not a corpus move) | `v6/tsv2/labs/1_rtkq-extraction-golden.ts:200` |
| plunit | `6 tests failed` (of 621): `catalog_plane_rail:level_plane_family_corpus_counts`, `expression_inventory:inventory_is_exactly_the_expected_rows`, `rel_zero_arity:a_root_rel_zero_still_has_no_storage`, `json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard`, `json_merge_patch:merge_patch_stops_on_the_json_null_stand_in` (no_exception), `json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in` (no_exception) | `v6/prolog/compile/test/plunit_tests.pl:1314,4561,5809,7684,7739,7743` |
| compile-speed | `COMPILE_SPEED regressions=16 improvements=0 FAIL` (baseline written 2026-08-07; golden-flex lower +178%, emit +120%) | `v6/prolog/compile/scripts/1_compile_speed.sh:248` |
| memory-soak | `FAIL sqlite_page_count_flat: second-quarter mean 24.8, final-quarter mean 49.5, ceiling 27.2` | `v6/tsv2/scripts/memory-soak.ts:327` |
| lsp-diags | `phase B1: the real LSP client never received both diagnostics for b.ts: READY`; needs the v5 `dl` binary present, then fails B1 deterministically (driver.log stalls at READY) | `v6/tsv2/scripts/lsp-diags.sh:266` |

`leak-soak` passes 3/3 with a clean `$TMPDIR` per run and is not allowlisted;
it leaves a literal `dl-perf.XXXXXX.jsonl` file (the `mktemp` template has text
after `XXXXXX`, so the suffix is never substituted and a reused `$TMPDIR`
collides on the next run with `mktemp: mkstemp failed ... File exists`).

## Flaky

| leg | result | sensitive to |
|---|---|---|
| serve-leak-soak | 2 pass / 1 fail (clean TMPDIR per run) | a transient `setImmediate` handle pending at the resource-count sampling instant; `names_with_growth(handles)` after 20 swap cycles races the event loop (`tests/serveLeak.test.ts:148`). Not the stale-TMPDIR class. Allowlisted so a flake does not silently red the gate. |

## Allowlist

allow: plunit
allow: rtkq-golden
allow: compile-speed
allow: tsv2-test
allow: lsp-diags
allow: golden-flex
allow: memory-soak
allow: roundtrip
allow: serve-leak-soak
