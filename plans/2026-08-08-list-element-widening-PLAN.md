# list(T) as a real storage kind, and a wider T

Base sha: `60023051`. Gate: `cd v6 && just green-all` (31 legs, ~177s).

## Contents

- [What this is](#what-this-is)
- [Why now](#why-now)
- [The two commits](#the-two-commits)
- [Signatures](#signatures)
- [Site table, every place that matches `json`](#site-table-every-place-that-matches-json)
- [Storage layout](#storage-layout)
- [Refusals, before and after](#refusals-before-and-after)
- [Fixtures](#fixtures)
- [Lane ownership](#lane-ownership)
- [What stays untested and why](#what-stays-untested-and-why)

## What this is

Option B of the two-axis type question (`chat_log/20260808.1`): keep the
fixpoint, widen the element set. Two halves:

- **B1**: the storage kind `list(Element)` survives from `column_storage/3` all
  the way to `lower.pl:column_def/3`, so the array-ness CHECK can be emitted.
  No new expressiveness.
- **B2**: `list_element_type/1` widens from four scalars to also admit `json`
  and a nested `list(_)`, with an element-shape guard at the arrival gate.

Explicitly NOT in scope: `list(RelName)`. That stays refused, for the reason
already stated at `0_type_plane.pl:131-133` (ids in a list would enter the tick
log, breaking print-values-never-ids). Nothing here touches that ruling.

## Why now

`0_type_plane.pl:108-114` states the blocker in its own words:

> SQLite can enforce ARRAY-NESS as a column CHECK (`json_valid(c) AND
> json_type(c) = 'array'`, verified) and CANNOT enforce the ELEMENT type ...
> Today the storage kind collapses to `json`, so neither guard is emitted; the
> array-ness CHECK needs `list(T)` to survive as its own kind all the way to
> `lower.pl:column_def/3`, which widens every place that matches on `json`.

That paragraph is the whole work order. This plan enumerates "every place".

## The two commits

Land in this order inside one branch. Commit 1 must be green on its own.

| commit | change | expected gate movement |
| --- | --- | --- |
| 1 | B1: storage kind survives; array-ness CHECK in DDL | conformance/plunit counts UNCHANGED; emitted DDL gains one CHECK per list column, so `compile/out/*.ts` moves |
| 2 | B2: widen `list_element_type/1` + arrival guard | `list_element_not_scalar` fires only for a rel ref; new fixtures pass |

A broken intermediate is not acceptable. If commit 1 cannot be made green
alone, STOP AND REPORT rather than squashing.

## Signatures

```prolog
%! column_storage(+Types, +DeclaredType, -StorageKind) is det.
%   WAS:  column_storage(Types, list(Element), json)
%   NOW:  column_storage(Types, list(Element), list(Element))
%   The element guard still runs; only the returned kind changes.
column_storage(Types, list(Element), list(Element)) :-
    !,
    (   list_element_type(Types, Element) -> true
    ;   declared_type_name(Types, Element)
    ->  throw(unsupported_construct(list_of_relation_refs(Element)))
    ;   throw(unsupported_construct(list_element_not_scalar(Element)))
    ).

%! list_element_type(+Types, +Element) is semidet.
%   WAS: four ground facts, no Types argument.
%   NOW: four scalars, plus json, plus a nested list whose own element is
%        itself admissible. Types is threaded so the rel-ref arm above keeps
%        its distinct refusal.
%   list_element_type(_, int).      list_element_type(_, text).
%   list_element_type(_, bool).     list_element_type(_, float).
%   list_element_type(_, json).
%   list_element_type(Types, list(Inner)) :- list_element_type(Types, Inner).

%! column_def(+Mode, +QuotedColumn, +Type, -Def) is det.
%   NEW clause, placed IMMEDIATELY BEFORE the existing json clause at :1272.
%   The array-ness predicate is verified on both SQLite builds this repo runs.
column_def(_, QuotedColumn, list(_), Def) :- !,
    format(atom(Def),
           '~w TEXT NOT NULL CHECK (json_valid(~w) AND json_type(~w) = \'array\')',
           [QuotedColumn, QuotedColumn, QuotedColumn]).

%! ir_column_storage(+Mode, +Type, -IrType, -SqlStorage, -Encoding) is det.
%   NEW clause. Physical storage is TEXT, same as json; the IR type is what
%   an executor reads to know the value is an array.
ir_column_storage(_, list(_), list, text, direct) :- !.

%! column_value_shape_error(+Types, +TypeName, +Value, -Reason) is semidet.
%   NEW clause. Runs at arrival, which is the only seam that can see elements;
%   a CHECK constraint cannot, because json_each is a table function and
%   CHECK prohibits subqueries.
%   PSEUDO:
%     value must be a json array            -> field_not_array(Value)
%     every element E must satisfy the
%     element type's own shape check        -> list_element_shape(Index, Reason)
```

## Site table, every place that matches `json`

Measured by grep over `v6/prolog/**.pl`, excluding `compile/out/`, `labs/`,
`ARCH.pl`, `rulings.pl` and fixtures.

| # | file:line | today | action |
| --- | --- | --- | --- |
| 1 | `0_type_plane.pl:115` | `column_storage(Types, list(E), json)` | return `list(E)` |
| 2 | `0_type_plane.pl:134-137` | `list_element_type/1`, four facts | widen to `/2`, add json + nested |
| 3 | `0_type_plane.pl:567+` | no list arm in `column_value_shape_error/4` | new arm, element guard |
| 4 | `lower.pl:1272` | `column_def(_,Q,json,_)` | new `list(_)` clause before it |
| 5 | `lower.pl:3396` | `ir_column_storage(_,json,json,text,direct)` | new `list(_)` clause |
| 6 | `lower.pl:1794` | `nth1(Position, ColumnTypes, json)` decode-source detect | accept `list(_)` as well |
| 7 | `lower.pl:4244` | `json_group_array_value_sql(json,_,_)` | new `list(_)` clause, same body |
| 8 | `analyze.pl:403` | `Storage == json` | `( Storage == json ; Storage = list(_) )` |
| 9 | `analyze.pl:798-800` | `merge_type` json clauses | mirror for `list(_)` |
| 10 | `emit_ts.pl:822-823` | `gate_column_type(list(_), json)` | **already correct, do not touch** |
| 11 | `emit_ts.pl:841` | `boundary_column_type(json, json)` | add `boundary_column_type(list(_), json)` |

Site 10 is the receipt that the emitter already anticipated this: it matches
`list(_)` separately today and maps it down to `json` on purpose.

**Out of bounds for this lane**: `lower.pl:795-800` `catalog_type_id/2` and
every `catalog_*` predicate in `lower.pl:735-800`. Those belong to
`lane/catalogtype`, which is rebasing in parallel. Touching them is a defect.

## Storage layout

Unchanged bytes, new constraint.

| | before | after |
| --- | --- | --- |
| SQLite column type | `TEXT NOT NULL CHECK (json_valid(c))` | `TEXT NOT NULL CHECK (json_valid(c) AND json_type(c) = 'array')` |
| stored value | canonical json text | canonical json text, identical |
| IR type reported | `json` | `list` |
| encoding slot | `direct` | `direct` |

`jsonb` stays banned; the two SQLite builds disagree about whether it exists
(`column_def` comment at `lower.pl:1264-1271`).

Uniqueness condition: the canonical text IS the identity of a list value, which
is unchanged. No new key, no new table, no new index.

## Refusals, before and after

| declared | before | after |
| --- | --- | --- |
| `list(int\|text\|bool\|float)` | accepted | accepted |
| `list(json)` | `list_element_not_scalar(json)` | **accepted** |
| `list(list(text))` | `list_element_not_scalar(list(text))` | **accepted** |
| `list(span)` where span is a rel | `list_of_relation_refs(span)` | `list_of_relation_refs(span)`, unchanged |
| a non-array value arriving at a list column | silently stored | `field_not_array(Value)` |
| an element of the wrong type | silently stored | `list_element_shape(Index, Reason)` |

The last two rows are the point of the arc. Today a `list(text)` column accepts
a bare integer and stores it.

## Fixtures

Own file, `v6/prolog/conformance/fixtures/10_list_elements.pl`, so nothing
collides with the parallel lane. Each fixture states why it exists.

| fixture | proves |
| --- | --- |
| `list_column_ddl_carries_array_check` | commit 1 landed: the emitted DDL has `json_type(c) = 'array'` |
| `list_of_json_documents_round_trips` | `list(json)` is now spellable and survives a tick |
| `nested_list_of_text_round_trips` | `list(list(text))` compiles, stores, and renders byte-identically |
| `non_array_value_at_list_column_is_refused` | the arrival gate fires `field_not_array` |
| `wrong_element_type_is_refused` | the element guard fires; without it the column is untyped |
| `list_of_relation_refs_still_refused` | the identity law did not move |

## Lane ownership

| lane | branch | owns | forbidden |
| --- | --- | --- | --- |
| `listkind` | `lane/list-element-widening` | `0_type_plane.pl`, `lower.pl`, `analyze.pl`, `emit_ts.pl`, `conformance/fixtures/10_list_elements.pl` | `catalog_*` predicates in `lower.pl:735-800`; `0_enum_expand.pl` |
| `variantfield` | `lane/variant-field-storage-type` | `0_enum_expand.pl`, `conformance/fixtures/11_variant_field_types.pl` | everything else |
| coordinator | `main` | the `lane/catalogtype` rebase | the two lane branches |

`listkind` and the catalogtype rebase both edit `lower.pl`, in disjoint line
regions (1250-4250 vs 735-800). Merge order: catalogtype first.

## What stays untested and why

- **Element typing enforced in SQL.** Not possible. CHECK constraints prohibit
  subqueries and `json_each` is a table function. The guard is a checker
  obligation and is tested at the arrival gate only.
- **Deeply nested lists past depth 2.** The recursion in `list_element_type/2`
  is structural; depth 3 exercises no new clause.
- **`list(json)` holding a nested array.** Indistinguishable at the storage
  layer from `list(list(_))`; the declared type is what separates them and both
  are covered.
