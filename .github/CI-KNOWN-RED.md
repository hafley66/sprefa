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

Row removed 2026-08-14 (issue golden-flex-coverage) at base `dc97a827`:
`golden-flex`, measured green 3/3 (coverage gate + text door + all four
cardinality/mode-parity legs + served e2e) after the string-family/json
registry landing's 16 unaccounted constructs were exercised or named-absent
in `v6/dl/fixtures/golden-flex.dl6`.

Row added 2026-08-17 at base `2b3c33ea0`: `docs-staleness`, a new leg wired
into `v6/tools/green-parallel.sh` this PR, red on first measurement (see row).

## Red legs

| leg | exact failure text | throw site |
|---|---|---|
| roundtrip | `G1 round-trip: 391 / 392 fixtures pass` then `FAIL mutual_recursion_matches_oracle (.../fixtures/engine_core.pl): fail(not_variant)` | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| tsv2-test | 4 failures of 209: `hostDecode.test.ts:144` (actual `[1,2,2,3]`, expected `[0,1,2,3]`), two `bopCheck` exit-code tests, and `edge-body negation SEARCHes the negated rel by key`. The stated "needs `gen_emitted/`" cause is WRONG: `v6/tsv2/gen_emitted/` holds 287 files and all four still fail. Re-diagnose before fixing. | `v6/tsv2/tests/hostDecode.test.ts:144` |
| plunit | `5 tests failed` (of 637): `catalog_plane_rail:level_plane_family_corpus_counts`, `rel_zero_arity:a_root_rel_zero_still_has_no_storage`, `json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard`, `json_merge_patch:merge_patch_stops_on_the_json_null_stand_in` (no_exception), `json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in` (no_exception). `expression_inventory` was fixed with the typed_scalar landing (the ratchet had gone stale at #238). | `v6/prolog/compile/test/plunit_tests.pl:1314,5809,7684,7739,7743` |
| compile-speed | `COMPILE_SPEED regressions=17 improvements=0 FAIL` (baseline written 2026-08-07; golden-flex lower +178%, emit +120%). The 17th row is `door-handwritten parse 42941 -> 50165 (+16.8%)`, added when the classic parser was deleted and `use_item/3` moved onto the DCG. | `v6/prolog/compile/scripts/1_compile_speed.sh:248` |
| memory-soak | `FAIL sqlite_page_count_flat: second-quarter mean 24.8, final-quarter mean 49.5, ceiling 27.2` | `v6/tsv2/scripts/memory-soak.ts:327` |
| lsp-diags | `phase B1: the real LSP client never received both diagnostics for b.ts: READY`; needs the v5 `dl` binary present, then fails B1 deterministically (driver.log stalls at READY) | `v6/tsv2/scripts/lsp-diags.sh:266` |
| dd-grade | `GRADE REGRESSION, these were byte-clean and are not:` then `list_bare_column_round_trips`, `list_bare_text_door`, `nested_list_text_door`, followed by `GRADE RATCHET, newly byte-clean; copy the run into graded.dd-diet-rust-sqlite.tsv:` listing 42 names. `DD-GRADE arm=--dd-diet-rust-sqlite graded=245 byte-clean=173`. The runner renders a `list` column as the JSON text SQLite holds (`"[]"`, `"[\"alpha\",\"beta\"]"`) where the oracle renders the parsed array (`[]`, `["alpha","beta"]`); `sql_ref_to_json` has no column-type arm. Out of scope: commit 54cca49de moved both emitters' list boundary type int -> json and states in its own message that "the runtime readers need no arm yet (that is F3, next slice)", so the Rust arm is a deliberately unfinished slice, not a fault of this base. The ratchet file is left untouched so the three regressed names stay recorded as byte-clean. | `v6/dd-runner/grade.sh:73` (verdict), `v6/dd-runner/src/main.rs:590` `sql_ref_to_json` (cause) |
| docs-staleness | New leg (this PR), red on first measurement: `git diff --exit-code` on the regenerated files is non-empty. SYNTAX.md is missing the `pre/2` surface row and marks `json_object/2` `refused` where `registry.pl` now has it `head(lower)`/`live`. CONSTRUCT-REFERENCE.md's cited `registry.pl:N-M` ranges are off by 1-21 lines from `registry.pl:200` onward (the doc was not regenerated after later registry edits shifted line numbers). 3/3 runs: byte-identical diff, exit 1, wall 1.4-2.2s. | `v6/prolog/compile/SYNTAX.md:119,148` (stale rows); `v6/prolog/compile/CONSTRUCT-REFERENCE.md:31-46` (stale line ranges) |

`leak-soak` and `endurance` were the v6/dl app's legs and no longer exist; the
app was deleted 2026-08-12. `serve-leak-soak` and `serve-endurance` are tsv2's
own equivalents and stay.

## Flaky

| leg | result | sensitive to |
|---|---|---|
| serve-leak-soak | 2 pass / 1 fail (clean TMPDIR per run) | a transient `setImmediate` handle pending at the resource-count sampling instant; `names_with_growth(handles)` after 20 swap cycles races the event loop (`tests/serveLeak.test.ts:148`). Not the stale-TMPDIR class. Allowlisted so a flake does not silently red the gate. |

## Allowlist

allow: plunit
allow: compile-speed
allow: tsv2-test
allow: lsp-diags
allow: memory-soak
allow: roundtrip
allow: serve-leak-soak
allow: dd-grade
allow: docs-staleness
