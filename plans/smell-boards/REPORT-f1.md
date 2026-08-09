# smell scan f1

Read-only scan of `v6/prolog/` for repeated statements. Every claim carries a
`file:line`. The type-shaped duplication was verified by running the compiler
on `conformance/fixtures/10_list_elements.pl`; the relplan dump below is that
run's actual output, not an inference.

## 1. Findings table

Ranked by estimated lines removed, most first.

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | lower.pl | 746-905 | catalog_rows/5, catalog_rel_rows/8, catalog_column_rows/8, catalog_column_type_id/7, resolve_declared_column_type/6, declared_column_type/5 | catalog resolves a column's type id from the relplan it already holds; Decls+Types thread dropped | ~45 |
| 2 | 0_type_plane.pl, lower.pl, emit_ts.pl | 158-164, 511-516, 633-635 / 380-385 / 892-895 / 802-805 | relation_columns_and_types/5, position_column_name/4, ref_column_names/4, relation_column_types/4, declared_column_type/5, declared_column_types/3 | one declared-schema accessor with a full-coverage flag | ~30 |
| 3 | analyze.pl, 0_coalesce_expand.pl, 0_relation_edge_expand.pl, 0_match_expand.pl, print_dl.pl | 61-83 / 72-110 / 34-43 / 91-98,125-128 / 348-377 | rule_is_edge+rule_is_level, rule_head_ref, rule_head, rule_body, edge_headed_refs, level_headed_refs, expand_rule, build_rule, present_goal, expand_rule_relation_edges, expand_match_arm, arm_head_ref, rule_line, print_match_arm | one arrow(Arrow) dispatcher per file | ~35 |
| 4 | emit_ts.pl | 818-824, 826-843 | gate_column_type/3, boundary_column_type/3 | one dispatch, both collapse list(_) to json | ~4 (drift, not clean merge) |

## 2. The type_decl / relplan / type_def question

The hypothesis is **partly confirmed, with one correction that flips the key
mechanism**.

**Verified, by running the compiler:**

```
relplan(carry/2,set,[id,rows],none,[int,list(list(text))])
relplan(grid/2,set,[id,rows],none,[int,list(list(text))])
```

`relplan` ColumnTypes holds `list(list(text))` uncollapsed. The collapse to
`json` lives only at the two emit seams `emit_ts.pl:823 gate_column_type` and
`emit_ts.pl:842 boundary_column_type`. The code is ahead of its comments:
`lower.pl:746` and `lower.pl:866` both claim a relplan reports a list column as
json, which the run falsifies.

**Which fields each shape holds that the others do not:**

| shape | site | unique fields |
| --- | --- | --- |
| type_decl(Name, [col(C,T)...]) | 0_type_plane.pl:64 | the authoring sugar, pairs in one term |
| type_def(Name, Columns, ColumnTypes) | 0_type_plane.pl:63 | projection of type_decl, no unique field |
| col_type(Ref, Column, Type) | compile.pl:127-134 | projection of type_decl, no unique field |
| relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes) | compile.pl:213-219 | **Ref, Kind (log/set), KeyOrNone**; ColumnTypes are storage kinds, uncollapsed in current code |

So `type_def` and `col_type` are pure projections of `type_decl` with nothing
of their own. `relplan` is not a literal third shape: it adds Kind and
KeyOrNone, and its ColumnTypes are the storage projection. A naive merge of all
four into one record is wrong, because the runtime genuinely needs the
`json`-collapsed storage view (`rel_column_types` in the emitted module) while
the catalog and the declared gate need the declared list view. One record can
carry both, but it is not a pure dedup.

**Where relplan holds a collapsed type while type_decl holds the declared one:**
nowhere in the record itself. The collapse moved to the seam. The sites that
still thread Decls because of the stale assumption, and so consult both for a
split that no longer exists:

- lower.pl:747 catalog_rows (type_definitions at 750)
- lower.pl:775-786 catalog_list_types (`relation_columns_and_types` at 778)
- lower.pl:841-842, 848-863 catalog_rel_rows / catalog_column_rows
- lower.pl:867-874 catalog_column_type_id (the declared_column_type consult)
- lower.pl:875-884 resolve_declared_column_type
- lower.pl:892-895 declared_column_type
- emit_ts.pl:793-805 rel_declared_column_types_lines / declared_column_types

**Smallest merged record that serves every current reader:** extend nothing
structurally. `relplan` already carries the declared type. The catalog path
only needs one added clause so a `ref(Name)` storage kind resolves straight to
its rel id, and then every Decls/Types re-derivation inside 746-905 is dead.

**How many of the 113 + 102 sites change:** the 102 `type_decl` sites do not
change; type_decl stays the authoring sugar and the materialize step
(compile.pl:127-134) keeps producing col_type from it. On the relplan side,
only the catalog sub-block (lower.pl:775-905, the `relplan` reads
catalog_list_types 777, catalog_rel_id_map 824, catalog_rel_rows 834 and the
Decls consult chain 867-895) changes, roughly 8 predicates and 4 call sites,
not all 113. The honest count is the catalog block, not the totals.

**The counterexample, stated plainly:** the strong form of the hypothesis is
false. Merging type_decl and relplan into one record does not remove the
Decls/RelPlans threading by itself, because the runtime and the catalog need
two different type projections and relplan already supplies both. The threading
is removable, but because the code already keeps list types in relplan, not
because a merge collapses two records into one.

## 3. The top three

### 3.1 Catalog re-derives Decls for a collapse that no longer exists

Current, lower.pl:867-895:

```prolog
catalog_column_type_id(Decls, Types, Ref, ColumnName, ColumnType,
                       RelIdMap, ListIdMap, TypeId) :-
    (   declared_column_type(Decls, Types, Ref, ColumnName, DeclaredType)
    ->  resolve_declared_column_type(Decls, Types, DeclaredType, ColumnType,
                                     RelIdMap, ListIdMap, TypeId)
    ;   catalog_type_id(ColumnType, TypeId)
    ).

declared_column_type(Decls, Types, Ref, ColumnName, DeclaredType) :-
    relation_columns_and_types(Decls, Types, Ref, DeclaredColumns, DeclaredColumnTypes),
    nth1(Position, DeclaredColumns, ColumnName),
    nth1(Position, DeclaredColumnTypes, DeclaredType).
```

ColumnType is already the relplan storage kind, verified to carry
`list(list(text))` and `ref(Name)`. One added clause removes the Decls consult:

```prolog
catalog_column_type_id(_, _, _, _, ColumnType, RelIdMap, ListIdMap, TypeId) :-
    (   list(Element) = ColumnType
    ->  list_row_id(ListIdMap, list(Element), TypeId)
    ;   ref(Name) = ColumnType
    ->  rel_row_id(RelIdMap, Name, TypeId)
    ;   catalog_type_id(ColumnType, TypeId)
    ).
```

Drops the Decls+Types args from catalog_rows, catalog_rel_rows,
catalog_column_rows, catalog_type_id, and deletes declared_column_type and the
relation_columns_and_types call inside the catalog. The two stale comments
(lower.pl:746, lower.pl:866) are corrected at the same edit.

### 3.2 Five accessors read the same declared-type truth

Current, three of the five:

```prolog
relation_column_types(_, Types, Name/Arity, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    length(Columns, Arity), !.
relation_column_types(Decls, _, Ref, ColumnTypes) :-
    findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes).
% 0_type_plane.pl:380-385

ref_column_names(Decls, Ref, Arity, Columns) :-
    findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
    length(Columns, Arity).
% 0_type_plane.pl:633-635

declared_column_types(Decls, Ref, Types) :-
    Ref = _/Arity,
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    length(Types, Arity).
% emit_ts.pl:802-805
```

`ref_column_names` and `declared_column_types` are the same predicate with the
Columns swapped for Types. One reader:

```prolog
relation_declared_schema(Decls, Types, Ref, Arity, Columns, ColumnTypes) :-
    (   type_definition(Types, Ref, Columns, ColumnTypes), length(Columns, Arity), !
    ;   findall(Column-Type, member(col_type(Ref, Column, Type), Decls), Pairs),
        pairs_keys_values(Pairs, Columns, ColumnTypes),
        length(Columns, Arity)
    ).
```

`relation_column_types`, `ref_column_names`, `declared_column_types`,
`position_column_name` (0_type_plane.pl:511-516) become calls into it; the
decl-lookups that have drifted apart stop being independent facts.

### 3.3 The <- / <+ arrow family

Current, two of many copies:

```prolog
rule_is_edge((_ <+ _)).
rule_is_level((_ <- _)).
% analyze.pl:61-62

expand_rule((Head <- Body), Clauses) :-
    !, refuse_coalesce_in_head(Head), expand_clause(level, Head, Body, Clauses).
expand_rule((Head <+ Body), Clauses) :-
    !, refuse_coalesce_in_head(Head), expand_clause(edge, Head, Body, Clauses).
% 0_coalesce_expand.pl:72-79
```

Merged, per file:

```prolog
arrow((_ <- _), level).
arrow((_ <+ _), edge).
rule_is_kind(Rule, Kind) :- arrow(Rule, Kind).
```

The head/body accessors, the present-goal and build-rule dispatchers, and the
edge/level headed-ref mirrors all reduce to one arrow/2 union per file. The
drift hazard is real here: nothing stops a future arrow from gaining a case in
one of the five files and not the others.

## 4. What you checked and found clean

- strat.pl: one stratum computation, no duplicate (whole file reviewed).
- 1_expansion.pl: order is a single facts table, no mirror.
- 0_enum_expand.pl and 0_match_expand.pl: the duplication that used to sit
  there was already removed; 0_match_expand.pl:134-137 documents it.
- 0_dot_expand.pl: no relplan/type/decl access; clean per the counts.
- The `rule_is_edge`/`rule_is_level` dispatch in strat.pl and compile.pl goes
  through analyze.pl's single definition; it is not re-implemented.
- emit_ts.ts:2636+ catalog rows calls `lower:catalog_rows` directly
  (emit_ts.pl:763-764), so the catalog is shared, not doubled.
- The canonical-JSON encoder in 0_type_plane.pl:687-705 mirrors
  conformance/ticklog.pl:value_json by documented choice (0_type_plane.pl:678-685),
  outside the named scope; flagged as a deliberate, test-pinned boundary, not a
  clean-dedup candidate.
- type_cycle_witness (0_type_plane.pl:228) has a single caller and js_float_text
  is imported by the ticklog script; both are single-call-site but justified, so
  counted as clean rather than dead.

## 5. The two d2 numbers

- viewBox: `0 0 1728 1364` (height < width)
- shape count (grep): 22
