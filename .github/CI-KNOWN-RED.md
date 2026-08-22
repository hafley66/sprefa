# CI known-red allowlist

Re-measured 2026-08-19 on branch `fix/ci-red-legs-green` (base `c554db778`) in
a fresh worktree, three back-to-back `just green-all` runs at base `50eb5f919` and two more after the rebase (`476s`, `478s`, identical failing set), plus a per-leg run
of every row below. The CI job runs the gate and uploads the raw log as an
artifact; the job's own pass/fail is decided by the `allow:` lines at the
bottom, so only a leg that fails and is NOT listed here turns the job red.

Each red leg carries the exact failure text seen at this measurement and the
site the failure comes out of. Do not edit this list as a way to make CI green;
edit it only when the underlying defect is fixed and the leg measured green.

**Partial re-measurement 2026-08-20** on `fix/test-estate-green` (base
`67951ea94`), covering groups B and D only. Receipts and the two design forks
that hold the remainder open: `TASKS/test-estate-green.REPORT.md`. Legs whose
numbers moved: sweep (`emitted_crash 30 -> 7`), plunit (`8 -> 7 tests failed`),
rust-grade (exit 1 -> exit 0 locally). Legs re-run and unchanged: conformance
(`433 PASS`, `FAILURES 1`), text-door (`compiled=336 byte_identical=331
failures=5`), roundtrip (`432 / 434`).

**staleness-gate closed 2026-08-20** on `fix/self-map-settle` (base `06c9c5f63`).
Its row read a self-map settle timeout; the cause was `self_map_facts.pl`
loading `compile/parse_dl.pl`, deleted at `81e1cf1bf` on 2026-08-12, so the
`sh` host answering the fourth source exited 2 and `program_rel`/`program_edge`
held zero rows forever. Receipts in `TASKS/self-map-settle.REPORT.md`:
`bash v6/tools/staleness-gate.sh` prints `STALENESS_GATE_OK` in 7.3s and
`just self-map` reads `SELF MAP HOLDS diagrams=4 lines=692` 3/3 at 7.57s,
7.76s, 7.72s with byte-identical output. The row and its `allow:` line are
gone; a red staleness-gate is now a real finding.

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

Rows that closed at this measurement, each measured green 3/3: `docs-staleness`
and `memory-soak`'s two other findings. `roundtrip`,
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

**CLOSED 2026-08-20** on `fix/enum-column-ref` (base `57559f61f`). An
enum-typed or rel-typed column holds a REFERENCE (user's call, same date), so
both runtimes carry the referenced instance's integer id in and out instead of
demanding a tagged object. Sweep reaches `emitted_crash=0` and rust-grade
reaches `runtime-error 0`. Receipts and the per-door diff:
`TASKS/enum-column-ref.REPORT.md`.

| leg | measured after | site |
|---|---|---|
| sweep | `RUN total=335 identical=322 wrong=0 emitted_crash=7` became `identical=329 wrong=0 emitted_crash=0`, no `SWEEP GATE` line | `v6/tsv2/runtime/enumPlane.ts` |
| rust-grade | `byte-clean=322`, `runtime-error 7` became `byte-clean=329`, no `runtime-error` line, exit 0 | `v6/sprefa-engine-rs/src/enum_plane.rs`, `src/driver.rs` |

`sweep` and `rust-grade` stay on the allowlist for group A and group E, which
this arc does not touch.

### C. `golden-flex.dl6` does not compile

Re-measured after rebasing onto `c554db778`, which carries the two `use`
targets (`de8e2c0a2`). With them present the program still fails at
`0_type_plane.pl:151` with `unsupported_construct(column_type_unknown('CodecDocument'))`:
`golden-flex.dl6:28` writes `CodecUse(value: CodecBox(CodecDocument))` and the
type plane does not resolve a rel name used as a template argument. Introduced
with `69ea4a37c feat: preserve interface application bounds end to end`, the
same commit that added the `use` lines. Every row below is that one failure
seen from a different door; the rendered message drops the column name
(`0_unsupported_messages.pl:116`, "rule-index unavailable").

| leg | exact failure text | site |
|---|---|---|
| golden-flex | `PASS  coverage gate` then `FAIL  bop check: ... unsupported: rule-index unavailable: unsupported_construct: compiler refused rule 'column_type_unknown' (column_type_unknown)` | `v6/tsv2/scripts/golden-flex.sh:56`, `v6/prolog/0_type_plane.pl:151` |
| compile-speed | `compile-speed: golden-flex failed to compile` then `ERROR: -g compile_dl6_profiled(...): string_codes/2: Type error: 'character_code' expected, found 'any' (an atom)`. `compile_dl6_profiled/2` now runs `use` resolution (flagship-flow and flagship-callgraph reach emit), so the stop moved from the parse of `golden-flex.dl6:14` into golden-flex's own emit. No inference count is produced, so the ratchet is not reached. | `v6/prolog/compile/scripts/1_compile_speed.sh:100` |
| tsv2-test | `ℹ tests 242 / pass 239 / fail 2` (`232 / 220 / 11` at the old base): `golden-flex served: ...` fails with `dl6 compile failed (swipl exit 2): use_path_unresolved("0_golden-flex-imported.dl6", [<gen_served dir>])` (the served door copies the one file into a scratch dir, so its `use` lines resolve against nothing); `tests/listStoredSnapshot.test.ts` imports `../gen_emitted/golden-flex.ts`, which only `golden-flex.sh` writes, after the compile that fails. | `v6/tsv2/tests/goldenFlexServed.test.ts`, `v6/tsv2/tests/listStoredSnapshot.test.ts:29` |
| typecheck | one error: `tests/listStoredSnapshot.test.ts(29,25): error TS2307: Cannot find module '../gen_emitted/golden-flex.ts' or its corresponding type declarations.` Down from 219 errors at the old base. | `v6/tsv2/tests/listStoredSnapshot.test.ts:29` |
| plunit | of `7 tests failed`, `subscribe_cone:golden_flex_cone_invariants` | `v6/prolog/compile/test/plunit_tests.pl` |

### D. one defect each

| leg | exact failure text | site |
|---|---|---|
| memory-soak | `FAIL sqlite_page_count_flat: second-quarter mean 25.8, final-quarter mean 50.5, ceiling 28.3 (tolerance +10%)`, 3/3 identical; `rss_flat`, `heap_used_flat`, `dbstat_available` and `statements_per_tick_flat` all PASS. The ceiling is right and the growth is real: `page_count` climbs 8 -> 57 monotonically over 101 samples with `freelist_count` 0 at every one. The grower is `__str`, the string dictionary. `TextPlane.intern` runs `INSERT OR IGNORE INTO "__str" ...` for every distinct text value and NOTHING releases a dictionary row: no `DELETE FROM "__str"` exists anywhere in the tree, and the retention prune deletes only from the rel's own table. The soak posts a unique `tag-${tick}` per tick, so 2500 strings accumulate while every rel stays row-bounded. The file's HEALTHY baseline of 10 flat pages was recorded 2026-07-29, before `a07030ba1` landed interning and before `572811745` made `dict` the default mode, and was never re-measured. Fixing it is a dictionary-release design decision (refcount, or a sweep against every dict column), not a soak edit. | `v6/tsv2/scripts/memory-soak.ts:327` (assertion), `v6/tsv2/runtime/textPlane.ts:46-58` (the unbounded write), `v6/prolog/lower.pl:2562,2595,2599` (the emitted intern SQL, no companion delete) |
| roundtrip | `G1 round-trip: 460 / 462 fixtures pass` then `FAIL module_path_option_element_round_trips (.../fixtures/7_module_path_element.pl): fail(not_variant)` and `FAIL mutual_recursion_matches_oracle (.../fixtures/engine_core.pl): fail(not_variant)` | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| text-door | four byte differences beside the plan failure in group A: `TEXT_DOOR_FAIL bounded_template_ground_instance byte_difference`, `two_bounded_parameters_mint_one_instance`, `nested_bounded_template_instance`, `mixed_bounded_and_free_parameters`. All four are template-bound fixtures from the interface-bound arc. NARROWED 2026-08-20: the diff is nine lines, every one an `h_schema:` value on a TYPE-plane `__rel` row (`interface`/`generic_rel`/`type_parameter`/`constraint`/`generic_column`/`concrete_type`); every `rel` row including its `h_id` is byte-identical. On type rows that slot holds no schema hash: `annotate_catalog_row/3` overwrites it with `semantic_type_id_text/2` of `named(ModuleHash, Kind, Name)`. The two doors agree on the module hash seeding RELATION identity and disagree on the one seeding TYPE identity. | `v6/prolog/compile/scripts/text_door_receipt.sh`, `v6/prolog/lower.pl:1728`, `v6/prolog/0_type_ids.pl:19,51` |
| plunit | the remaining four of `7 tests failed`: `catalog_plane_rail:level_plane_family_corpus_counts` and three `json_merge_patch` tests (`json_patch_lowers_with_the_null_stand_in_guard`, `merge_patch_stops_on_the_json_null_stand_in`, `merge_patch_stops_on_a_nested_json_null_stand_in`) | `v6/prolog/compile/test/plunit_tests.pl:1694,9803` |
| scale-floor | `scale-floor: scale bench failed for s2/10000 (sample 1 of 3)` then `LibsqlError: SQLITE_ERROR: no such table: a`. `7_scale-floor.sh` compiles a fresh `s2` fixture through `compile_fixture/4` into `gen/scale_generated.ts`; the emitted boot DDL creates no table for rel `a`. | `v6/tsv2/scripts/7_scale-floor.sh:391` |
| flagship | `FAIL  the corpus MOVED since the v5 golden was captured (golden 9b1b91ad6aa3933ecd113377e7df76c924e4d69c1d2be20a2945647c1f062828, now 39d0cf438a1e173919bcb60e1092b31ea153afb15675b5f66c3f20938039e11c). Grading v6 against a golden from a different corpus is a false green.` The script names its own regeneration command, and that command runs the v5 binary. | `v6/tsv2/scripts/flagship-callgraph.sh:178` |
| dd-grade | `DD-GRADE arm=--dd-diet-rust-sqlite graded=250 byte-clean=1 peak_rss_mb=4 (4544 kB, clean_state_gate_and_exit_zero) ceiling=8`, with the ratchet naming ~240 fixtures that were byte-clean and are not. None of grade.sh's four inputs (`6_isolated_compiler_dd.pl`, `sweep_plans.pl`, `sweep_oracle.pl`, `ticklog.pl`) is touched by this branch. | `v6/dd-runner/grade.sh:73` |
| prolog-lint | `PROLOG_LINT findings=14 baseline=0 FAIL`, every finding `private_cross_module_call`: `generic_expand:compile_type_plane/3`, `generic_expand:compile_type_query/3`, `type_ids:semantic_type_id_encoding/2`, `typegen_export:read_row_lines/2`, `typegen_export:write_row_line/2`, each called from a plunit test module | `v6/prolog/tools/prolog-lint.sh:76` |
| catalog-audit | `catalog-audit rail: probe arrival returned 500` | `v6/tsv2/scripts/catalog-audit-rail.sh:64` |

### E. red on the GitHub runner only

`v6/sprefa-engine-rs/Cargo.toml:25` depends on
`../../../sprefa-v6/0_runtime/1_rust_runtime_host` by path, and
`~/projects/sprefa-v6` is a local repo with no git remote and no GitHub
mirror. No runner can build `sprefa-engine-rs` until that crate is published,
vendored, or the dependency is cut. `hafley-rs` (the other sibling path dep)
is public and ci.yml clones it; this one cannot be cloned.

| leg | exact failure text (CI run 32280953042) | site |
|---|---|---|
| typegen-golden | `error: failed to load manifest for dependency \`sprefa-rust-runtime-host\`` / `Caused by: failed to read /Users/runner/work/sprefa/sprefa-v6/0_runtime/1_rust_runtime_host/Cargo.toml` / `FAIL  runtime product/sum checks` / `TYPEGEN GOLDEN: FAIL`. Green locally every run. | `v6/prolog/compile/test/typegen_golden.sh:210` (`cargo test --lib enum_plane` in sprefa-engine-rs) |
| rust-grade | same manifest error, `exit=101` on the runner; locally it reaches `RUST-GRADE graded=462 byte-clean=335` (group B) | `v6/sprefa-engine-rs/grade.sh` |

## Flaky

| leg | exact failure text | measured |
|---|---|---|
| serve-leak-soak | `✖ receipt (c): 20 program-swap cycles leave no handle, timer, or subscription behind` with `AssertionError: Expected values to be strictly deep-equal: + [ 'Immediate 0->1' ] - []` at `v6/tsv2/tests/serveLeak.test.ts:113` | green 3/3 in the first three whole-gate runs and green 3/3 in isolation every time; red once in one whole-gate run with 6 legs in parallel. One `setImmediate` still pending at the moment the handle count is read, under load only. |

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
allow: serve-leak-soak
allow: sweep
allow: text-door
allow: tsv2-test
allow: typecheck
allow: typegen-golden
