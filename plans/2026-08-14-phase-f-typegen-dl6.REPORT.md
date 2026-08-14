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
| type-name collision via module prefix (`type_name/2` at `7_emit_ts_types.pl:61-64`) | not implemented; no collisions in the pinned set, so no row exercises it |
| generic-rel emission (`ts_generic_text`, type parameters/constraints) | not implemented; no pinned fixture emits a generic_rel |
| empty interface `export interface X {}` | not implemented; every pinned rel has at least one column |
| list nesting deeper than 2 (`Array<Array<Array<...>>>`) | not implemented; dl6 refuses positive recursion in a stratum, so list_type unrolls to depth 2 |
| option of a list (`Array<X> | null`) | not implemented; pinned option elements are primitives only |
| `not(contains(Name, '__'))` compiler-helper filter | expressed as `instr(Name, '__') > 0` via `minted_rel/1`; there is no `contains/2` builtin in this dl6 |

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
bash v6/prolog/compile/test/typegen_golden.sh     # the new gate
  PASS  generic_expansion_end_to_end
  PASS  nested_list_of_text_round_trips
  PASS  list_of_json_documents_round_trips
  PASS  split_initcap_and_fold_render_pascal_case
  TYPEGEN GOLDEN: HOLDS
```

Baselines (run after this arc, byte-identical to 91da6781):

| gate | baseline | after this arc |
|---|---|---|
| conformance | 421 PASS / 0 FAIL | 421 PASS / 0 FAIL |
| tsv2 sweep | RUN total=317 identical=314 wrong=0 rejection=3 | unchanged |

This arc adds no fixture and touches no existing compiler file, so conformance,
plunit, and grade came back unchanged.
