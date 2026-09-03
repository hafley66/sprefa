# brief: the v7 loader skips known non-TSI records without a diagnostic

Lane: `fix/v7-loader-foreign-records`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Everything is under `v7/`. Run swipl from the repo root (the fixtures are addressed as `v7/test/fixtures/...`).

## ARCH row

`v6/prolog/ARCH.pl:999` `task(v7_loader_skips_foreign_records_by_name, unbuilt, [tsi_a7_v7_loader])`: `load_tsi_stream/3` (`v7/src/2_comptime/0c_extract_loader.pl:71`) files one `tsi_line(Path, Line, malformed_record(resolved_type_edge))` diagnostic per non-TSI extract row. A `--witness --resolve` stream is mostly `node`, `edge`, `sig`, `site`, `resolved_edge`, `resolved_type_edge` rows, so a hand load drowns in diagnostics that name nothing wrong.

## The record vocabulary (the extract side, do not edit it)

`v6/sprefa-extract/src/types.rs:2975` `#[serde(tag = "record", rename_all = "lowercase")] pub enum FlatFact`, variants in order:
`protocol run fact witness coverage diagnostic node edge dfparam dfarg dffield dflit dfloop dfnest dfallocates sig site const doc doctagout datadocout datavalueout docnodeout specifier methodownerout`
plus the resolve-side records the bin writes: grep `"record"` and `resolved_edge|resolved_type_edge|resolved_call_edge|flow_edge|import|package|scip` in `v6/sprefa-extract/src/bin/extract.rs`, `src/project.rs` and `src/wire.rs` and list every value you find in the PR body with its `file:line`. The TSI six are `protocol run fact witness coverage diagnostic` (`0c_extract_loader.pl:139-171` `decode_record/3`).

## Shape

```prolog
%% foreign_record(?Record) is nondet.
%  An extract record this door does not read. Named so the loader skips it
%  silently; a record outside this list and outside decode_record/3 is
%  malformed_record(Record).
foreign_record(node).  foreign_record(edge).  ... (every value from the list above, one clause each, alphabetical)

decode_known_record(Record, Dict, JsonlPath, LineNumber, Result) :-
    (   decode_record(Record, Dict, Row) -> Result = ok(Row)
    ;   foreign_record(Record)           -> Result = skip
    ;   Result = error(diagnostic(extract, stream(JsonlPath), tsi_line(JsonlPath, LineNumber, malformed_record(Record))))
    ).
```

`combine_line_result(skip, ...)` at `:104` already exists. A TSI record whose body fails `decode_record/3` (say `fact` with no `relation`) stays `malformed_record(fact)`: the skip branch is reached only when the record name is foreign. Keep clause order so a malformed TSI row never falls through to `foreign_record/1`.

## Receipts

1. New fixture `v7/test/fixtures/tsi_invalid/2_foreign_records.jsonl`: a real `extract --witness --resolve --family type --project-root v6/sprefa-extract/tests/fixtures/tsi v6/sprefa-extract/tests/fixtures/tsi/probe.ts` stream trimmed to under 40 lines that still holds at least one of each: `protocol`, `run`, `fact`, `witness`, `coverage`, `node`, `edge`, `resolved_type_edge`, plus one line `{"record":"frobnicate"}` and one `{"record":"fact"}` with no `relation` key. Say in the fixture's first line how it was produced (JSONL has no comments; use a `{"record":"diagnostic","run":0,"relation":"fixture","detail":"<how>"}` row, which the loader accepts).
2. `v7/test/4_extract_loader.test.pl`, new tests after `a_relation_outside_the_registry_is_named_and_skipped`:
   - `foreign_extract_records_are_skipped_without_a_diagnostic`: load the fixture; `Diagnostics == [D1, D2]` where exactly one is `tsi_line(_, _, malformed_record(frobnicate))` and one is `tsi_line(_, _, malformed_record(fact))` (assert by `msort` or `memberchk` plus `length(Diagnostics, 2)`); the accepted rows include the `fact`, `witness`, `coverage` rows and none named `node`/`edge`.
   - `a_malformed_tsi_record_is_still_malformed`: covered by the same fixture's `fact`-without-relation line; keep it a separate test with its own assertion so a regression names the case.
   SABOTAGE RECEIPT comment on each: on the base sha the first test sees one `malformed_record(node)` diagnostic per foreign row.
3. Runner, from the repo root:
   `swipl -q -g "load_files(['v7/test/4_extract_loader.test.pl'],[silent(true)]),run_tests,halt"` all pass, then the whole v7 battery:
   `swipl -q -g "load_files(['v7/test/0_reader.test.pl','v7/test/1_entrypoints.test.pl','v7/test/2_module_system.test.pl','v7/test/3_compiler_trace.test.pl','v7/test/4_extract_loader.test.pl'],[silent(true)]),run_tests,halt"` pasted.
4. Hand load receipt in the PR body: `load_tsi_stream` over a fresh full `--witness --resolve` stream of `probe.ts` returns `Diagnostics == []`.

## Ownership

Owned: `v7/src/2_comptime/0c_extract_loader.pl`, `v7/test/4_extract_loader.test.pl`, `v7/test/fixtures/tsi_invalid/2_foreign_records.jsonl`.
Forbidden: everything under `v6/`, `docs/`, `v6/prolog/ARCH.pl`. Reading `v6/sprefa-extract/src/**` to list the record names is expected; editing it is not.

## Style laws

No em dashes. Comments state only constraints the code cannot show; the `%%` predicate header above is the whole comment budget for the new predicate. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Follow the file's existing style (`%%` headers, `must_be`, `->`/`;` ladders). Commit subject: `v7: the loader skips known non-TSI extract records without a diagnostic`.

## Done

Push, PR against `main` with receipts, then:
`boop beep --no-wait --as fix-v7-loader-foreign-records sprefa-coordinator "v7-foreign PR #<n>: 4_extract_loader <n>/<n>, v7 battery <n>/<n>"`.
