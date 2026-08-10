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
