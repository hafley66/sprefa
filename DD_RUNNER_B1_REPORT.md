# DD runner B1 report

## JSON shape

`fixture_dd_plan_json_text/3` emits one compact JSON object with these fields:

| field | source |
|---|---|
| `ddl` | `lowered/8` DDL strings, unchanged |
| `rels` | DD-plan relation name, columns, and lowered `select_all` snapshot SQL |
| `rules` | each head-writing map's ordered delete and insert SQL |
| `initial` | fixture initial rows |
| `schedule` | fixture schedule rows, each with relation, values, and signed arrival |
| `tick_order` | emitted plan phases |

`json_write_dict/3` writes the JSON twin. The compiler test emits every JSON
twin twice and compares the strings before comparing against the checked-in
golden.

## Crate

`v6/dd-runner` is a standalone Cargo workspace and is excluded from the root
workspace. `src/main.rs` is 180 lines. Dependencies: `rusqlite` with bundled
SQLite, `serde`, and `serde_json`.

The runner applies the emitted DDL in an in-memory connection, seeds initial
rows, runs map-owned level SQL once for boot and once per scheduled tick,
compares emitted relation snapshots, and prints the canonical JSONL envelope.

## Grade output

`v6/dd-runner/grade.sh` output:

```text
retraction_only_tick_retracts_level_view: byte-diff clean
float_exact_join_has_no_epsilon: byte-diff clean
float_avg_is_grouped: byte-diff clean
```

Compiler gate: `swipl -q -g run_tests -t halt v6/prolog/compile/test/plunit_tests.pl`
completed through `[571/571] schema_parity_goldens:... passed`.

`v6/tsv2/scripts/sweep.sh` reached stage 3 and exited because Node could not
resolve package `rxjs` from `v6/tsv2/scripts/sweep.ts`. Its generated deletion
of `v6/prolog/compile/out/pokeapi_shape.ts` was restored; the two compile output
directories were clean afterwards.

## SQLite/oracle disagreement

The first mirror diff diverged at byte 3: `serde_json` ordered the outer object
as `{"deltas":...,"tick":1}` while the oracle emits
`{"tick":1,"deltas":...}`. The runner now writes the envelope in the oracle's
field order. The three byte diffs above are clean.

## Leak fix pass

Changed goldens: `v6/prolog/compile/test/dd/float_exact_join_has_no_epsilon.dd.pl`.
The join arrangements now key `[name,value]` and carry `[]` values on both
inputs. Its JSON twin is byte-identical because arrangements are not rendered
there.

`shared_head_positions/4` now emits every shared positive body variable as a
join key. The DD test derives equalities from `body_ref_uses/2` and checks their
columns against emitted arrangements for all golden fixtures.

The JSON renderer throws `unsupported_construct(edgestmt)` for a map payload it
cannot render. The test drives a minimal edge-rule fixture through
`fixture_dd_plan_json_text/3`.

An empty `RuleOrder` uses the program rules; nonempty orders retain their exact
sequence. Mutual recursion raises `unsupported_construct(mutual_recursion(Ref))`.
The emitter tests cover both paths.

`pnpm install` ran in `v6/tsv2` and `v6/sprefa-store/js`. Sweep deleted
`v6/prolog/compile/out/pokeapi_shape.ts`; it was restored with
`git checkout -- v6/prolog/compile/out/pokeapi_shape.ts`.

Gate outputs:

```text
% [69/575] emit_dd_plan:retraction_only_tick_retracts_level_view ... passed
% [70/575] emit_dd_plan:float_exact_join_has_no_epsilon ... passed
% [71/575] emit_dd_plan:float_avg_is_grouped ... passed
% [72/575] emit_dd_plan:json_twins_are_deterministic ... passed
% [76/575] emit_dd_plan:description_join_keys_cover_body_argument_equalities ... passed
% [77/575] emit_dd_plan:json_rejects_edge_statement_payload ... passed
% [78/575] emit_dd_plan:empty_rule_order_falls_back_to_program_rules ... passed
% [79/575] emit_dd_plan:mutual_recursion_is_rejected ... passed
% [575/575] schema_parity_goldens:... passed

retraction_only_tick_retracts_level_view: byte-diff clean
float_exact_join_has_no_epsilon: byte-diff clean
float_avg_is_grouped: byte-diff clean

RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=247 final_identical=246 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
```
