# Phase F: typegen as a dl6 program — REPORT

## What this arc delivered

The type plane now has a second door beside `7_emit_ts_types.pl`: a prolog
export dumps the semantic type rows as JSONL, and a checked-in dl6 program
(`render_ts.dl6`) renders TS interfaces from those rows on the real tsv2
runtime. A golden gate (`typegen_golden.sh`) pins the dl6-rendered text for a
four-fixture set and diffs it against committed goldens. File writing stays out
of scope (the fs-effects door is a separate queued arc); rendered text rests in
the derived `rendered_type` rel and is asserted by the golden.

New files:

| path | role |
|---|---|
| `v6/prolog/compile/typegen_export.pl` | `dump_type_rows/2`, `dump_fixture_rows/3`; row -> `type_row/7` JSONL arrivals |
| `v6/dl/typegen/render_ts.dl6` | EDB `type_row/7` + `field_line`/`body_text`/`rendered_type` IR |
| `v6/prolog/compile/test/typegen_golden.sh` | compile -> dump -> render -> assemble -> diff gate |
| `v6/prolog/compile/test/typegen_golden/*.types.ts` | committed dl6-rendered goldens (4) |
| `plans/2026-08-14-phase-f-typegen-dl6.REPORT.md` | this file |

No existing file was edited. Conformance and sweep ran byte-identical to the
91da6781 baselines (see Validation).

## Parity report (dl6 vs prolog, pinned set)

| fixture | identical | first differing line | cause |
|---|---|---|---|
| `generic_expansion_end_to_end` | yes | none | |
| `nested_list_of_text_round_trips` | yes | none | |
| `list_of_json_documents_round_trips` | yes | none | |
| `split_initcap_and_fold_render_pascal_case` | yes | none | |

All four pinned fixtures render byte-identical to the corresponding
`compile/out/<name>.types.ts`. The dl6 output equals the prolog output on
interfaces, option columns (`| null`), single-level and two-level list columns
(`Array<...>`), and PascalCase casing (`replace(initcap(name), '_', '')`).

## Feature gaps (named constructs, none forced this arc)

Byte parity holds because the pinned set stays inside the renderer's scope.
Each gap names the construct, not "unsupported":

| construct | status |
|---|---|
| type-name collision via module prefix (`type_name/2` at `7_emit_ts_types.pl:61-64`) | CLOSED round 2, `shape_module_prefix_collision` |
| generic-rel emission (`ts_generic_text`, type parameters/constraints) | CLOSED round 2, `shape_generic_rel` |
| empty interface `export interface X {}` | CLOSED round 2, `shape_interface_declaration` |
| list nesting deeper than 2 (`Array<Array<Array<...>>>`) | CLOSED round 2 to depth 4, `shape_list_nesting_depth` |
| option of a list (`Array<X> | null`) | CLOSED round 2, `shape_option_of_list` |
| `not(contains(Name, '__'))` compiler-helper filter | expressed as `instr(Name, '__') > 0` via `minted_rel/1`; there is no `contains/2` builtin in this dl6 |

Round 2 closed each gap against a checked-in row set under
`compile/test/typegen_golden/shape_*.type_rows.jsonl`, because the type-plane
door mints none of these rows for any conformance fixture today. One golden
per shape is judged twice: the dl6 render and `7_emit_ts_types` reading the
same JSONL back through `write_prolog_types/2`. Named and still open:

| construct | status |
|---|---|
| list nesting past 4 | one stratum per level; a fifth needs a fifth rel |
| option of option, list of (option of list) | the unrolled grammar covers leaf, list-of-leaf, option-of-leaf, option-of-list, list-of-option-of-leaf |
| module name in camelCase or with a separator outside `_ . - /` | `module_type_stem` is initcap plus a fixed replace set; the prolog `module_type_name/2` maps every non-alnum |
| a minted `__` rel readmitted by a `concrete_type` child row | prolog `renderable_rel/2` admits it, `minted_rel/1` does not |

Measured, not assumed: a self-recursive `list_type` LOADS on tsv2 (`{"loaded":true}`)
and derives one nesting level in the arrival tick, the rest on the next tick.
The round-1 note "dl6 refuses positive recursion in a stratum" is wrong about
the load; the reason to unroll is that a one-shot render must settle in one tick.

## Renderer notes

The 3-rule IR is the proven probe shape (`chat_log/20260813.2`):

```mermaid
flowchart LR
  A[type_row/7 EDB arrivals] --> B[leaf_type / list_type / option_type]
  B --> C[type_of]
  C --> D[field_line]
  D --> E[body_text: group_concat sep newline ord]
  E --> F[rendered_type: concat wrap]
```

- Type resolution (`type_of`) is the union of three mutually exclusive arms
  keyed on kind: `leaf_type` (primitive / rel ref), `list_type` (json_list
  unrolled to two levels), `option_type` (element plus `| null`).
- `field_line`'s line text is `  Name: Type;` with no trailing newline; the
  aggregate separator and the concat wrap place the closing brace on its own
  line, byte-matching the prolog emitter.
- `minted_rel/1` filters the `__` compiler namespace; `instr(Name, '__') > 0`
  is the marker test because `contains/2` is not a builtin.

## Validation

```
bash v6/prolog/compile/test/typegen_golden.sh     # the gate; round 2 = 9 PASS
  PASS  generic_expansion_end_to_end
  PASS  nested_list_of_text_round_trips
  PASS  list_of_json_documents_round_trips
  PASS  split_initcap_and_fold_render_pascal_case
  PASS  shape_interface_declaration
  PASS  shape_generic_rel
  PASS  shape_module_prefix_collision
  PASS  shape_list_nesting_depth
  PASS  shape_option_of_list
  TYPEGEN GOLDEN: HOLDS
```

Baselines (run after this arc, byte-identical to 91da6781):

| gate | baseline | after this arc |
|---|---|---|
| conformance | 421 PASS / 0 FAIL | 421 PASS / 0 FAIL |
| tsv2 sweep | RUN total=317 identical=314 wrong=0 rejection=3 | unchanged |

This arc adds no fixture and touches no existing compiler file, so conformance,
plunit, and grade came back unchanged.
