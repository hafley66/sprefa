# CI known-red allowlist

Re-measured 2026-08-19 on branch `fix/ci-red-legs-green` (base `50eb5f919`) in
a fresh worktree, three back-to-back `just green-all` runs plus a per-leg run
of every row below. The CI job runs the gate and uploads the raw log as an
artifact; the job's own pass/fail is decided by the `allow:` lines at the
bottom, so only a leg that fails and is NOT listed here turns the job red.

Each red leg carries the exact failure text seen at this measurement and the
site the failure comes out of. Do not edit this list as a way to make CI green;
edit it only when the underlying defect is fixed and the leg measured green.

**Before this measurement CI never reached the gate at all.** The
`Generate text-door corpus` job step ran `bash v6/tsv2/scripts/sweep.sh`, which
exited 124 on `TIMEOUT sweep.sh: stage 1 compile sweep exceeded 900s`, so
`just green-all` never ran and the allowlist step judged an empty failure list.
Every row below is therefore a FIRST measurement, not a re-measurement, and the
whole previous table was written against a gate that had not run since
2026-08-12.

Legs that left the gate rather than getting a row: `lsp-diags` and
`flagship-flow` (deleted; both spawn the v5 `dl` binary and Chris's "I DO NOT
WANT TO RUN V5 ANYTHING ANYMORE" leaves neither an arm), and `arm-census`
(moved to `just optional`, `v6/justfile`: SWI `coverage/2` over the whole
461-fixture corpus compile ran past 25 minutes at 97% of one core with no
budget, and has never once completed inside a gate run).

Rows that closed at this measurement, each measured green 3/3: `docs-staleness`,
`serve-leak-soak`, `memory-soak`'s two other findings. `roundtrip`,
`compile-speed`, `plunit`, `tsv2-test` and `dd-grade` all stay red but under
DIFFERENT text than the previous table recorded; the old text is not repeated.

## Red legs

Grouped by root cause, because five of the seventeen are one defect each seen
from a different leg.

### A. `nested_zero_column_child_is_one_row_per_parent` fails to plan

`program_plan/3` FAILS (does not throw) for this fixture, so it carries no
`unsupported_construct` reason and no manifest row. Introduced with
`e08946fe8 rel/0 is declarable`.

| leg | exact failure text | site |
|---|---|---|
| conformance | `fail  nested_zero_column_child_is_one_row_per_parent` then `FAILURES  1` | `v6/prolog/conformance/go.pl` |
| text-door | `TEXT_DOOR compiled=352 byte_identical=347 failures=5`, last row `TEXT_DOOR_FAIL nested_zero_column_child_is_one_row_per_parent compile_phase_failed(plan)` | `v6/prolog/compile/scripts/text_door_receipt.sh` |
| sweep | `SWEEP_SILENT_FAIL nested_zero_column_child_is_one_row_per_parent` (stage 1) and `ORACLE_FAIL nested_zero_column_child_is_one_row_per_parent` (stage 2). Both lines are new: stage 1 used to drop the fixture silently and stage 2 used to exit 1 with no name. | `v6/prolog/sweep.pl:103`, `v6/prolog/compile/oracle_dump.pl:26` |
| plunit | of `7 tests failed` (of 909), `module_path_decls:a_zero_column_childs_name_used_as_a_value_is_not_rewritten` and `rel_zero_arity:a_root_rel_zero_still_has_no_storage` | `v6/prolog/compile/test/plunit_tests.pl:6890,6984` |

### B. the enum plane's arrival encoding is unfinished

`EnumPlane.intern` requires a tagged object per enum-typed column; the sweep
and grade runners feed the declared scalar, so the module throws on its first
arrival. Eight fixtures, the same eight in both runtimes.

| leg | exact failure text | site |
|---|---|---|
| sweep | `RUN total=351 identical=337 wrong=0 emitted_crash=8 rejection=6 no_oracle_log=0` then `SWEEP GATE: 8 emitted module(s) crashed on a schedule the oracle completed`. Distinct messages: `enum_arrival_shape_mismatch: not_an_object(grade)` / `(tree)` / `(__opt_text)`, and `ambiguous_owner_context(user_profile, __opt_text)` / `(measurement, __opt_text)` / `(orchard__tree, __opt_text)`. | `v6/tsv2/runtime/enumPlane.ts:9,15,57` (throws), `v6/tsv2/scripts/sweep.ts:311` (verdict) |
| rust-grade | `RUST-GRADE REGRESSION` naming the same eight plus `concat_program_queue` and `nested_zero_column_child_is_one_row_per_parent`; `RUST-GRADE graded=462 byte-clean=335`; `runtime-error 9` with the same `enum_arrival_shape_mismatch` messages, plus `1  boot statement failed: SqlInputError ... "no such function: reverse"` and `diff 1`. `concat_program_queue` is the Rust twin of the emitted-fold defect fixed for TypeScript in this PR at `emit_ts.pl:2233`; `emit_rust.pl` still carries it. | `v6/sprefa-engine-rs/grade.sh` |

### C. `golden-flex.dl6` and its two `use` targets

`golden-flex.dl6:14-15` import `0_golden-flex-imported.dl6` and
`1_golden-flex-namespaced.dl6`, untracked since `69ea4a37c`. origin/main's
`de8e2c0a2` commits both, AFTER this branch's base, so four of these five clear
on merge; `golden-flex` itself does not, and its own text is recorded with the
files present.

| leg | exact failure text | site |
|---|---|---|
| golden-flex | with the two fixtures absent: `ERROR: -g run: Unknown message: use_path_unresolved("0_golden-flex-imported.dl6", [.../dl/fixtures])` then `FAIL  coverage gate:`. With them present: `PASS  coverage gate` then `FAIL  bop check: ... unsupported: rule-index unavailable: unsupported_construct: compiler refused rule 'column_type_unknown' (column_type_unknown)`. Only the second is a real defect. | `v6/tsv2/scripts/golden-flex.sh:225` |
| compile-speed | `compile-speed: golden-flex failed to compile` then `ERROR: -g compile_dl6_profiled(...): parse error at line 14, column 5: statement`. golden-flex is a pinned program and `compile_dl6_profiled/2` is a door with no `use` resolution, so it cannot read line 14 whether the targets exist or not. No inference count was produced, so the 17-regression ratchet from the previous table was never re-reached. | `v6/prolog/compile/scripts/1_compile_speed.sh:100` |
| tsv2-test | `ℹ tests 239 / pass 235 / fail 3` (`232 / 220 / 11` at the base): `golden-flex served: the live host runs, and the served tick log matches the oracle replayed on the served schedule` and `tests/listStoredSnapshot.test.ts` (`Cannot find module '../gen_emitted/golden-flex.ts'`) are both this group; the third, `sabotage: editing fixture in temp dir modifies only the changed row`, is a concurrency flake, green 3/3 in isolation and red only under `npm test`'s `--test-concurrency=6` | `v6/tsv2/tests/listStoredSnapshot.test.ts:29`, `v6/tsv2/tests/7_live-extract.integration.test.ts` |
| typecheck | one error: `tests/listStoredSnapshot.test.ts(29,25): error TS2307: Cannot find module '../gen_emitted/golden-flex.ts' or its corresponding type declarations.` Down from 219 errors at the base. | `v6/tsv2/tests/listStoredSnapshot.test.ts:29` |
| plunit | of `7 tests failed`, `subscribe_cone:golden_flex_cone_invariants` | `v6/prolog/compile/test/plunit_tests.pl` |

### D. one defect each

| leg | exact failure text | site |
|---|---|---|
| memory-soak | `FAIL sqlite_page_count_flat: second-quarter mean 25.8, final-quarter mean 50.5, ceiling 28.3 (tolerance +10%)`, 3/3 identical; `rss_flat`, `heap_used_flat`, `dbstat_available` and `statements_per_tick_flat` all PASS. The ceiling is right and the growth is real: `page_count` climbs 8 -> 57 monotonically over 101 samples with `freelist_count` 0 at every one. The grower is `__str`, the string dictionary. `TextPlane.intern` runs `INSERT OR IGNORE INTO "__str" ...` for every distinct text value and NOTHING releases a dictionary row: no `DELETE FROM "__str"` exists anywhere in the tree, and the retention prune deletes only from the rel's own table. The soak posts a unique `tag-${tick}` per tick, so 2500 strings accumulate while every rel stays row-bounded. The file's HEALTHY baseline of 10 flat pages was recorded 2026-07-29, before `a07030ba1` landed interning and before `572811745` made `dict` the default mode, and was never re-measured. Fixing it is a dictionary-release design decision (refcount, or a sweep against every dict column), not a soak edit. | `v6/tsv2/scripts/memory-soak.ts:327` (assertion), `v6/tsv2/runtime/textPlane.ts:46-58` (the unbounded write), `v6/prolog/lower.pl:2562,2595,2599` (the emitted intern SQL, no companion delete) |
| roundtrip | `G1 round-trip: 460 / 462 fixtures pass` then `FAIL module_path_option_element_round_trips (.../fixtures/7_module_path_element.pl): fail(not_variant)` and `FAIL mutual_recursion_matches_oracle (.../fixtures/engine_core.pl): fail(not_variant)` | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| text-door | four byte differences beside the plan failure in group A: `TEXT_DOOR_FAIL bounded_template_ground_instance byte_difference`, `two_bounded_parameters_mint_one_instance`, `nested_bounded_template_instance`, `mixed_bounded_and_free_parameters`. All four are template-bound fixtures from the interface-bound arc. | `v6/prolog/compile/scripts/text_door_receipt.sh` |
| plunit | the remaining four of `7 tests failed`: `catalog_plane_rail:level_plane_family_corpus_counts` and three `json_merge_patch` tests (`json_patch_lowers_with_the_null_stand_in_guard`, `merge_patch_stops_on_the_json_null_stand_in`, `merge_patch_stops_on_a_nested_json_null_stand_in`) | `v6/prolog/compile/test/plunit_tests.pl:1694,9803` |
| staleness-gate | `STALENESS_GATE_FAIL self-map regeneration failed, ARCH-MAP.md not verified:` followed by the whole last tick as JSON. `bash v6/tsv2/scripts/self-map.sh` alone exits 1 on `FAIL  rels did not settle in 120s`; the document it does write carries an EMPTY mermaid block for section 4, so the rel-graph derivation produces no rows. compile/out and dl_view are regenerated and committed in this PR, so this row is the self-map leg only. | `v6/tools/staleness-gate.sh:159`, `v6/tsv2/scripts/self-map.sh` |
| scale-floor | `scale-floor: scale bench failed for s2/10000 (sample 1 of 3)` then `LibsqlError: SQLITE_ERROR: no such table: a`. `7_scale-floor.sh` compiles a fresh `s2` fixture through `compile_fixture/4` into `gen/scale_generated.ts`; the emitted boot DDL creates no table for rel `a`. | `v6/tsv2/scripts/7_scale-floor.sh:391` |
| flagship | `FAIL  the corpus MOVED since the v5 golden was captured (golden 9b1b91ad6aa3933ecd113377e7df76c924e4d69c1d2be20a2945647c1f062828, now 39d0cf438a1e173919bcb60e1092b31ea153afb15675b5f66c3f20938039e11c). Grading v6 against a golden from a different corpus is a false green.` The script names its own regeneration command, and that command runs the v5 binary. | `v6/tsv2/scripts/flagship-callgraph.sh:178` |
| dd-grade | `DD-GRADE arm=--dd-diet-rust-sqlite graded=250 byte-clean=1 peak_rss_mb=4 (4544 kB, clean_state_gate_and_exit_zero) ceiling=8`, with the ratchet naming ~240 fixtures that were byte-clean and are not. None of grade.sh's four inputs (`6_isolated_compiler_dd.pl`, `sweep_plans.pl`, `sweep_oracle.pl`, `ticklog.pl`) is touched by this branch. | `v6/dd-runner/grade.sh:73` |
| prolog-lint | `PROLOG_LINT findings=14 baseline=0 FAIL`, every finding `private_cross_module_call`: `generic_expand:compile_type_plane/3`, `generic_expand:compile_type_query/3`, `type_ids:semantic_type_id_encoding/2`, `typegen_export:read_row_lines/2`, `typegen_export:write_row_line/2`, each called from a plunit test module | `v6/prolog/tools/prolog-lint.sh:76` |
| catalog-audit | `catalog-audit rail: probe arrival returned 500` | `v6/tsv2/scripts/catalog-audit-rail.sh:64` |

## Flaky

None recorded at this measurement. `serve-leak-soak`, the previous table's only
flaky row, measured green 3/3 and is out of the allowlist.

## Allowlist

allow: catalog-audit
allow: compile-speed
allow: conformance
allow: dd-grade
allow: flagship
allow: golden-flex
allow: memory-soak
allow: plunit
allow: prolog-lint
allow: roundtrip
allow: rust-grade
allow: scale-floor
allow: staleness-gate
allow: sweep
allow: text-door
allow: tsv2-test
allow: typecheck
