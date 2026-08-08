# Lane catalogtype - REPORT

PASS 1 of 2. Makes `__rel.type_id` describe non-primitive columns: a ref column
carries its target rel's `rel_id`; a list column points at a synthetic `list`
row whose own `type_id` is the element's id.

## 1. The exact diff, file by file

### `v6/prolog/lower.pl` (the owned implementation)

- Export list: `catalog_rows/4` -> `catalog_rows/5` (Decls is now threaded in).
- Import list from `0_type_plane`: added `relation_columns_and_types/5`
  (`column_storage/3` was already imported).
- `catalog_row_ddl/5`: signature gained `Decls`; passes it to `catalog_rows/5`.
- `lower_program`: the `catalog_row_ddl(Name, Decls, Rules, RelPlans, ...)`
  call site now supplies `Decls`.
- `catalog_rows/5`: two-pass rewrite. Pass A walks `RelPlans` in declaration
  order to build the `rel_id` map (`catalog_rel_id_map/4`) and the list row map
  (`catalog_list_rows/6`). Pass B emits rows (`catalog_rel_rows/11`) resolving
  every column `type_id` from those maps. Layout: primitives, then synthetic
  list rows, then the module row, then each rel and its columns.
- New helpers: `catalog_list_types/4` (distinct list types in depth order),
  `list_subtypes/2`, `distinct_order/3`, `list_type_depth/2`,
  `catalog_list_rows/6`, `list_row_id/3`, `list_element_type_id/3`,
  `catalog_rel_id_map/4`, `rel_row_id/3`, `catalog_column_type_id/8`,
  `resolve_declared_column_type/7`, `primitive_type/1`,
  `declared_column_type/5`.
- `catalog_rel_rows/11` and `catalog_column_rows/11` gained `RelIdMap`,
  `ListIdMap`, `Decls`, `Types`; a column's `type_id` now goes through
  `catalog_column_type_id/8` instead of the bare `catalog_type_id/2`.
- `catalog_type_id/2` is unchanged in shape (primitive ids 1..5, else 0); its
  comment now states ref/list are resolved upstream.

### `v6/prolog/emit_ts.pl` (necessary 1-predicate follow)

The exported `catalog_rows/4` -> `/5` signature change forces the single
caller to thread `Decls`. `program_catalog_rows/4` (lines 751-752) now binds
`prog(Decls, Rules)` and calls `lower:catalog_rows(Name, Decls, Rules, RelPlans,
Rows)`. Without this line the sweep's stage 1 would fail to emit every module.

### `v6/prolog/compile/test/plunit_tests.pl` (the owned tests)

- Import list for `../../lower`: added `catalog_rows/5`.
- New block `catalog_type_ids` with four tests (`catalog_ref_column_carries_
  target_rel_id`, `catalog_list_column_carries_element_typed_row`,
  `catalog_nested_list_emits_inner_before_outer`,
  `catalog_no_ref_no_list_ids_unchanged`).

No other file was touched. `v6/prolog/compile/out/*.ts` are sweep-generated
artifacts and differ only in the catalog rows (see item 3).

## 2. Validation output, verbatim

### Command 1: plunit unit tests

```
cd v6/prolog && swipl -q -g "consult('compile/test/plunit_tests.pl'), run_tests, halt" -t 'halt(1)'
```

Run result: `[457/457] ... passed`, exit status 0, **0 failures**. (457 = 453
pre-existing + 4 new; the plunit summary is emitted as one `[N/457]` line per
test; the last line is the final count and no `failed` line appears.)

Tail, verbatim:

```
% [455/457] use_module_system..xactly_one_solution .. passed (0.000 sec)
% [456/457] use_module_system.._carrying_an_escape .. passed (0.000 sec)
% [457/457] use_module_system..n_its_own_file_line .. passed (0.000 sec)
```

The four new tests, verbatim:

```
% [116/457] catalog_type_ids:..rries_target_rel_id .. passed (0.000 sec)
% [117/457] catalog_type_ids:..s_element_typed_row .. passed (0.001 sec)
% [118/457] catalog_type_ids:.._inner_before_outer .. passed (0.000 sec)
% [119/457] catalog_type_ids:.._list_ids_unchanged .. passed (0.000 sec)
```

### Command 2: the two-implementation agreement sweep

```
cd ../tsv2 && bash scripts/sweep.sh
```

Exit status 0. Stage 1 and the acceptance lines, verbatim:

```
=== stage 1: compile sweep ===
SWEEP total=309 compiled=212 unsupported=97 crash=0
  UNSUPPORTED enum_decl_variant_name_collision_is_refused enum_variant_name_collision(page)
  UNSUPPORTED match_enum_nonexhaustive_is_refused match_nonexhaustive(body,redirect)
  ... (unsupported fixtures unchanged, refusal reasons identical to HEAD)
=== stage 3: copy compiled modules into gen_emitted/, run the diff ===
RUN total=212 identical=211 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=212 final_identical=211 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff

=== stage 4: refusal-reason diff vs HEAD (informational) ===
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
```

Acceptance reading:

| gate | requirement | result |
|---|---|---|
| `wrong` | 0 | 0 |
| `final_wrong` | 0 | 0 |
| `MANIFEST_REASON_DIFF` | restated=0 args=0 bucket_moved=0 added=0 removed=0 | all zero |
| `identical` | expected to drop | **211** (see item 3) |

## 3. The `identical` count and why it did not drop

`RUN identical=211` (and `final_identical=211`). The brief predicted this count
would drop because catalog ids shift. It did not.

Mechanism: the id shift lives in the emitted `rel_catalog` constant inside each
emitted module, and the sweep grades the tick log against the oracle row by
row. No fixture's tick schedule reads a catalog row in a way that changes any
user-rel row, so the id shift in `rel_catalog` never reaches the compared
surface. Entering the run there were already 212 fixtures with 1
`log_retraction_rejected` (a fixture whose rejection the oracle matches, so it
has no comparable schedule): 211 compilable fixtures, all identical. The count
`211` is exactly that 211.

The catalog content did change, exactly as the brief intended, and the change is
visible in the generated `v6/prolog/compile/out/*.ts` diffs. Representative
diff, verbatim (`struct_column_renders_canonical_json.ts`):

```
-  { rel_id: 9, parent_id: 7, ordinal: 2, local_name: "at", kind: "column", type_id: 0, ... }
+  { rel_id: 9, parent_id: 7, ordinal: 2, local_name: "at", kind: "column", type_id: 10, ... }
```

`type_id` went 0 -> 10 (the `span` rel's id). The per-tick rows the sweep diffs
are unchanged, so `identical` holds at 211 while the catalog bytes move.

## 4. Printed catalog dump (ref column and list column)

### A ref column: `struct_column_renders_canonical_json` (finding/2 `at` -> span)

```
{ rel_id: 6, ..., local_name: "struct_column_renders_canonical_json", kind: "module", ..., module_id: 6, ... }
{ rel_id: 7, parent_id: 6, ..., local_name: "finding", kind: "rel", type_id: 0, arity: 2, module_id: 6, ... }
{ rel_id: 8, parent_id: 7, ordinal: 1, local_name: "path", kind: "column", type_id: 1, ... }
{ rel_id: 9, parent_id: 7, ordinal: 2, local_name: "at",   kind: "column", type_id: 10, ... }
{ rel_id: 10, parent_id: 6, ..., local_name: "span", kind: "rel", type_id: 0, arity: 2, module_id: 6, ... }
```

The `finding.at` column `type_id=10` is `span`'s `rel_id`.

### A list column: `list_column_fans_out_through_spread` (repo/2 `tags` -> list(text))

```
{ rel_id: 6, parent_id: 0, ordinal: 0, local_name: "list(text)", kind: "list", type_id: 1, arity: 0, module_id: 0, ... }
{ rel_id: 7, ..., local_name: "list_column_fans_out_through_spread", kind: "module", ..., module_id: 7, ... }
{ rel_id: 8, parent_id: 7, ..., local_name: "repo", kind: "rel", type_id: 0, arity: 2, module_id: 7, ... }
{ rel_id: 9,  parent_id: 8, ordinal: 1, local_name: "name", kind: "column", type_id: 1, ... }
{ rel_id: 10, parent_id: 8, ordinal: 2, local_name: "tags", kind: "column", type_id: 6, ... }
```

The synthetic `list(text)` row (id 6) carries `type_id=1` (text's id); the
`tags` column carries `type_id=6`, the list row's id.

## 5. What the brief told me that turned out to be wrong

1. **List column type is not recoverable from `RelPlans`.** The brief implied
   the catalog works off relplan column types carrying `list(Element)`.
   Measured: a `list(text)` column resolves to `json` in the relplan
   `ColumnTypes` (`repo/2` -> `[text, json]`). The declared `col_type` carries
   `list(text)`. Recovering list-ness and the element therefore requires the
   program `Decls`, which `catalog_rows/4` did not receive. This forced
   `catalog_rows/4` -> `/5` and the one-line `emit_ts.pl` caller change. That
   line is outside the two owned files; it is documented in item 1 and is the
   only external edit.
2. **Nested lists never reach the catalog through normal compilation.**
   `0_type_plane` `column_storage/3` refuses `list(list(text))`
   (`list_element_not_scalar`), so a nested list is a compile-time refusal and
   no real fixture carries one into `catalog_rows`. The nested path is
   exercised by the plunit test, which calls `catalog_rows/5` directly. The
   catalog code does support nesting; the checker simply never lets one in.
3. **The predicted `identical` drop did not happen.** The id shift is confined
   to the emitted `rel_catalog` constant, which the sweep never compares to the
   oracle. `RUN identical` stayed at 211. See item 3.

## 6. Edge cases the brief did not enumerate

1. **A ref target must be `type_decl`'d.** `column_storage/3` throws
   `column_type_unknown` for a name that is not a declared type. Every real ref
   column's target is type-declared (a bare identifier in column-type position
   is refused otherwise), so primitives are routed before `column_storage` via
   `primitive_type/1` and the final clause falls back to `catalog_type_id/2`'s 0
   for anything unresolved, matching the old catch-all behavior.
2. **A list type shared across several columns or rel-plans** dedupes to one
   synthetic row (`distinct_order/3` keeps the first occurrence). Without
   dedup, two `list(text)` columns would mint two rows and desync the shared
   `ListIdMap`.
3. **Synthetic list rows are ordered by nesting depth, then first appearance.**
   `keysort` on the depth is stable in SWI for equal keys, so two independent
   same-depth lists (e.g. `list(text)` and `list(int)`) keep declaration order;
   the inner of a nested pair is guaranteed to precede its outer, which is what
   lets the outer row resolve the inner's id before emitting itself.
