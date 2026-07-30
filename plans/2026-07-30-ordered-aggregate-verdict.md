# Ordered aggregate verdict

## Receipts

The V5 source receipt is `src/lower.rs:1016-1025`: the emitted expression is
`json_group_array(arg ORDER BY arg)`, with decoded text input and optional
symbol interning at the head column. `src/ast.rs:439-484` contains
`JsonGroupArray` and `JsonGroupObject`; V5 has no `group_concat` aggregate
spelling in the source.

The runnable lab receipts are in the lab's README and companion scripts.
Lab files died on landing per protocol; the last full copy is at commit
`c45e3a46` (`git show c45e3a46:v6/prolog/labs/ordered_aggregate/README.md`
recovers the receipt doc, likewise probe.mjs / oracle_ordering.pl /
nesting_probe.dl6 / nesting_schedule.json and the .out files). `node probe.mjs`
exits 0. The Prolog ordering script exits 0. The nesting oracle exits 0.

Coordinator re-verification: all three re-run clean. The agent's `bop check`
exit 1 was an environment gap (sprefa-store/js had no node_modules in the
fresh worktree); after `pnpm install` there the same command prints
`refusal: unsupported_construct(aggregate_head(json_array(_)))` and exits 2 —
the honest current-HEAD receipt that the compiler refuses json aggregate
heads by name, which is exactly what the wiring arc removes. Observation for
the json-potholes lane: the nesting oracle renders json payload cells as
`#{a:2,z:1}` text, not canonical JSON; that mapping is potholes territory
and may legitimately change under its landing.

## Slots

| slot | answer | evidence |
| --- | --- | --- |
| `slot_order_axis` | both spellings | Value sorting gives V5 parity with `json_group_array(item_name ORDER BY item_name)`. Explicit stream order uses `json_group_array(item_name ORDER BY ordinal)`. One spelling cannot select both order sources without changing the meaning of its `ORDER BY` expression. |
| `slot_string_join_spelling` | own aggregate: `group_concat(value, separator ORDER BY ordinal)` | SQLite 3.45.1 accepts and executes the inner `ORDER BY`. The mermaid demand needs text assembly directly, so a second aggregate avoids parsing an intermediate JSON array. |
| `slot_empty_group` | absent head row | The min/max generated SQL uses `HAVING count(*) > 0`; the ordered-array draft follows the same condition. `[]` requires a separately present group row, while an empty input group has no grouped SQL row. |
| `slot_nested_array` | composition works through `json(payload)` | The SQL probe returns canonical object values inside the array. The Prolog oracle accepts `json_array(Payload)` over a `json` column and emits a nested array. The tsv2 compiler receipt remains pending because the checked command exits during package resolution. |
| `slot_incremental_tier` | group-scoped recompute | The scoped delete and scoped grouped insert use the touched group key. `EXPLAIN QUERY PLAN` reports indexed `SEARCH`; statement count is 2 for both 10 and 1000 groups. |

## Recommended spelling

Use `json_group_array(value)` for value-sort parity and
`json_group_array(value, ordinal)` for the explicit ordinal form, with the
lowering placing `ORDER BY value` or `ORDER BY ordinal` inside the SQL
aggregate. Use `group_concat(value, separator ORDER BY ordinal)` for direct
string assembly. These names use the existing SQL vocabulary and the
existing rxjs and Prolog vocabulary used by the receipts.

## Four-sighting census

| sighting | dl6 program it wants | Q1 axis | pure-rxjs lowering |
| --- | --- | --- | --- |
| self-map mermaid assembly | `rel mermaid_text(file_name: text, lines: text). mermaid_text(FileName, group_concat(LineText, "\\n" ORDER BY LineOrdinal)) <- mermaid_line(FileName, LineOrdinal, LineText).` | ordinal | `combineLatest([mermaidLine$]).pipe(groupBy(row => row.fileName), mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, joinOrdinalLines)))))` |
| V5 collect parity | `rel group_rels(group_name: text, names_json: text). group_rels(GroupName, json_group_array(RelationName)) <- rel_catalog(RelationName, GroupName, ColumnText, DocumentationText).` | value | `relCatalog$.pipe(groupBy(row => row.groupName), mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, collectValueSortedJson)))))` |
| json aggregate heads refused in tsv2 | `rel group_rels(group_name: text, names_json: text). group_rels(GroupName, json_group_array(RelationName)) <- rel_catalog(RelationName, GroupName, ColumnText, DocumentationText).` | value | `relCatalog$.pipe(groupBy(row => row.groupName), mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, collectValueSortedJson)))))` |
| extract-t2 round-trip | `rel fragment_text(fragment_name: text, lines: text). fragment_text(FragmentName, group_concat(LineText, "\\n" ORDER BY LineOrdinal)) <- fragment_line(FragmentName, LineOrdinal, LineText).` | ordinal | `fragmentLine$.pipe(groupBy(row => row.fragmentName), mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, joinOrdinalLines)))))` |

The census records the wanted forms. Current compiler refusal for JSON
aggregate heads is a wiring-arc item, while the oracle and SQL legs already
run.

## Q5 nesting snippet and lowering

The probe is:

```dl6
rel child(group_name: text, payload: json).
rel nested(group_name: text, payloads: json).
nested(GroupName, json_array(Payload)) <- child(GroupName, Payload).
```

Its pure-rxjs lowering is:

```ts
child$.pipe(
  groupBy(row => row.groupName),
  mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, collectJsonArray)))),
)
```

## Q6 minus-delta grading

For `north` rows `(1, pear)`, `(2, orange)`, `(3, apple)`, a minus delta for
`(1, pear)` seeds `north`, deletes its old array head, and inserts
`["orange","apple"]` under the ordinal spelling. A minus delta for the last
remaining row deletes the head and inserts no row. A later add for that group
creates a fresh array head. The corresponding pure-rxjs lowering is:

```ts
delta$.pipe(
  groupBy(delta => delta.storeName),
  mergeMap(group$ => group$.pipe(toArray(), map(rows => reduce(rows, applyMinusAndRebuild)))),
)
```

## Wiring arc

1. Add parser and registry rows for the selected aggregate spellings.
2. Add lowering for value and ordinal order expressions, including text
   decoding and head-column handling.
3. Emit the scoped delete, scope seed, and ordered grouped insert beside the
   existing min/max aggregate plan.
4. Add canonical tick-log fixtures for value order, ordinal order, empty
   groups, nested JSON, and minus deltas.
5. Resolve the `rxjs` package installation in the tsv2 compile-check
   environment, then rerun the nesting compiler receipt.
6. Add the four sighting programs to compiler and oracle coverage without
   editing the lab receipts.
