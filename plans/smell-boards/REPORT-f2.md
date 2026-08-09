# smell scan f2

## 1. Findings table

Ranked by estimated lines removed, most first. Every span cites current code on
this tree (`git merge --ff-only 9b2f8b0f` = already up to date, HEAD `9b2f8b0f`).

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | `v6/prolog/0_type_plane.pl` | 62-72, 209, 229 + `lower.pl:1573` + `0_relation_edge_expand.pl:22,31` | `type_decl/2` (16), `type_def/3` (63), `type_definition/4` (69), `declared_type_name/2` (72) | collapse the three declared-schema spellings onto `col_type/3`; drop `type_def/3` as a stored shape | ~14 |
| 2 | `v6/prolog/lower.pl` | 4591, 4637, 4647 + 4623-4643 | `departure_frontier_ddl/3` (4583), `delta_ddl/3` (4614) | one `phase_stage_ddl/4` helper for the verbatim `_phase`+`_sequence` TEMP row and its index | ~6 |
| 3 | `v6/prolog/0_type_plane.pl` | 380-385 vs 158-164 | `relation_column_types/4` (380), `relation_columns_and_types/5` (158) | delete `relation_column_types/4`; reuse `relation_columns_and_types/5` and stop the extra result | ~6 |
| 4 | `v6/prolog/lower.pl` | 2008-2012 vs 2018-2022 | `arrival_statement/3` log (2006), set (2016) | share the table/columns/placeholders prologue as one helper | ~5 |

## 2. The type_decl / relplan / type_def question

The coordinator hypothesis is **partly falsified**. Three claims, three verdicts.

**Claim A: one record wearing three shapes.** Partly true. `type_decl/2`,
`col_type/3`, and `type_def/3` are three spellings of the same declared column
schema. Evidence: `col_type/3` rows are *materialized* from `type_decl/2` at
`compile.pl:127-134`; `type_def/3` is a parallel-lists projection of the same
decl at `0_type_plane.pl:62-67`. So after materialization, `Decls` carries both
`type_decl(Name,[col(C,T)])` and the derived `col_type(Name/Arity,C,T)` rows:
one truth stored twice, by construction. `type_def/3` is a third copy as a
position-indexed convenience.

**Claim B: relplan is the same record and merging removes the Decls/RelPlans
thread.** **Falsified.** `relplan/5` carries more than the declared schema:

| shape | name | per-column types | kind | key | other |
| --- | --- | --- | --- | --- | --- |
| `type_decl/2` | atom | `[col(C,T)]` declared | :x: | declared key lives in the `rel` decl, not here | :x: |
| `col_type/3` | `Name/Arity` | `Type` declared | :x: | :x: | :x: |
| `type_def/3` | atom | parallel list declared | :x: | :x: | :x: |
| `relplan/5` | `Name/Arity` | parallel list, **inferred** | `log|set` | `key(Ps)|none` | :x: |

The fields only `relplan` holds are `Kind` and `KeyOrNone`, produced at
`compile.pl:213-219`. And its `ColumnTypes` are the *inferred* types from
`analyze.pl:program_column_types/7` (478), not the declared ones. `relplan` is a
storage-plan record, not a schema record. Merging it into the declared-schema
shape is a category error and does not remove the Decls/RelPlans thread: code
that passes both (`lower.pl:736,747,4564,4921`, `emit_ts.pl:793`) needs `Decls`
for declared source types and for non-schema decls (`query`, `keep`, `edge`),
none of which `relplan` carries.

**Claim C (the collapse): relplan ColumnTypes report `list(text)` as `json`
while type_decl holds `list(text)`. Falsified.** The storage decoder keeps the
`list(E)` kind instead of collapsing it. Measured against `column_storage/3`
under SWI-Prolog 10.0.2:

```
col_storage int            -> int
col_storage text           -> text
col_storage json           -> json
col_storage list(text)     -> list(text)
col_storage list(json)     -> list(json)
col_storage list(list(text)) -> list(list(text))
col_storage bool           -> bool
col_storage float          -> float
col_storage place          -> thrown(unsupported_construct(column_type_unknown(place)))
```

The inferrer then keeps that kind: `analyze.pl:403` (`Storage == json ;
Storage = list(_) -> Type = Storage`). Unit tests lock the same behavior,
`compile/test/plunit_tests.pl:6116-6118`. The header comment at
`0_type_plane.pl:111-114` ("Today the storage kind collapses to json") is stale;
it contradicts the clause directly below it (115) and the tests.

So no site has to consult both *because of* a collapsed `json`. Sites that do
consult both consult them for a different reason: `relplan` carries the inferred
type, `Decls.col_type` carries the declared type, and the two differ on struct
columns (`ref(place)` in `relplan` vs `place` in `col_type`, `analyze.pl:397-398`)
and on undeclared columns (inferred with no `col_type` at all,
`analyze.pl:507-535`). The emit reads both on purpose,
`emit_ts.pl:749-759` (inferred) vs `801-804` (declared).

**Smallest merged record that serves every current reader**: keep `col_type/3`
as the one declared-schema spelling and compute parallel lists on demand.
`relplan/5` stays as-is (it is a different record). `type_def/3` disappears as a
stored shape.

**How many of 113 + 102 sites change**: roughly none of the 113 `relplan` sites
(that record is untouched) and only the projection layer among the `type_decl`
sites. Most consumers already read `col_type/3` directly; the in-scope clauses
that must change are `type_definitions/2`, `type_definition/4`,
`declared_type_name/2`, the five `type_def` matches in `0_type_plane.pl:63,70,72,
209,229`, `lower.pl:1573`, and the `type_definition` import in
`0_relation_edge_expand.pl:22,31`: about 8-10 clauses, not 102. The 102 count is
dominated by fixture/test literals of the source spelling, which do not change.

Verdict: the real redundancy is `type_decl` + `col_type` + `type_def` (three
spellings of one schema). The relplan merge is a category error the hypothesis
got wrong, and the "collapsed json" example does not occur on this HEAD.

## 3. The top three

### 3.1 The declared schema triple (`type_decl` / `col_type` / `type_def`)

Current, `compile.pl:127-134` materializes the flattened rows; `type_definitions/2`
(`0_type_plane.pl:62-67`) re-projects them into `type_def/3`:

```prolog
materialize_reference_target_rels(prog(Decls0, Rules), prog(Decls, Rules)) :-
    findall(col_type(Name/Arity, Column, Type),
            ( member(type_decl(Name, Specs), Decls0),
              length(Specs, Arity),
              member(col(Column, Type), Specs),
              \+ memberchk(col_type(Name/Arity, Column, Type), Decls0) ),
            MissingColumns),
    append(Decls0, MissingColumns, Decls).

type_definitions(Decls, Types) :-
    findall(type_def(Name, Columns, ColumnTypes),
            ( member(type_decl(Name, Specs), Decls),
              findall(Column, member(col(Column, _), Specs), Columns),
              findall(Type, member(col(_, Type), Specs), ColumnTypes) ),
            Types).
```

Merged form: one reader over `col_type/3`; ask for parallel lists where needed,
never store `type_def/3`:

```prolog
materialize_reference_target_rels(prog(Decls0, Rules), prog(Decls, Rules)) :-
    findall(col_type(Name/Arity, Column, Type),
            ( member(type_decl(Name, Specs), Decls0),
              length(Specs, Arity),
              member(col(Column, Type), Specs),
              \+ memberchk(col_type(Name/Arity, Column, Type), Decls0) ),
            MissingColumns),
    append(Decls0, MissingColumns, Decls).

% type_definitions/2, type_definition/4, declared_type_name/2 retire;
% their callers read col_type/3 (or a thin getters pair) instead.
```

### 3.2 Repeated stage-table DDL in `lower.pl`

Current, three verbatim copies of the same TEMP row plus index twins:

```prolog
% departure_frontier_ddl, lower.pl:4590-4592
format(atom(TableDdl),
       'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
       [QuotedDepartureTable, ColumnsSql]).
% delta_ddl frontier, lower.pl:4636-4638
format(atom(FrontierDdl),
       'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
       [QuotedFrontierTable, ColumnsSql]).
% delta_ddl next-frontier, lower.pl:4646-4648  (same string)
```

Merged form, one helper plus its index:

```prolog
phase_stage_table(QuotedTable, ColumnsSql, Ddl) :-
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedTable, ColumnsSql]).
% callers: phase_stage_table(QuotedDepartureTable, ColumnsSql, TableDdl)
%          phase_stage_table(QuotedFrontierTable, ColumnsSql, FrontierDdl)
%          phase_stage_table(QuotedNextFrontierTable, ColumnsSql, NextFrontierDdl)
```

### 3.3 superseded `relation_column_types/4`

Current, `0_type_plane.pl:158-164` already answers with columns and types; the
predated projection (header note at 156-157) repeats the same two doors:

```prolog
% 158
relation_columns_and_types(_, Types, Name/Arity, Columns, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    length(Columns, Arity), !.
relation_columns_and_types(Decls, _, Ref, Columns, ColumnTypes) :-
    findall(Column-Type, member(col_type(Ref, Column, Type), Decls), Pairs),
    pairs_keys_values(Pairs, Columns, ColumnTypes).
% 380 - superseded
relation_column_types(_, Types, Name/Arity, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    length(Columns, Arity), !.
relation_column_types(Decls, _, Ref, ColumnTypes) :-
    findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes).
```

Merged form: delete 380-385; the only callers (`0_type_plane.pl:362,372`) call
`relation_columns_and_types/5` and ignore the columns.

## 4. What you checked and found clean

Duplication is absent where I expected it.

- **`lower.pl` relplan accessors are single-sourced.** `relplan_columns/3`,
  `relplan_kind/3`, `relplan_column_types/3` at `lower.pl:269-271` are the only
  field readers; the roughly 25 `relplan_columns` call sites use the accessor,
  none re-match the record inline.
- **The catalog contract has one writer.** `catalog_ddl_contract/2` and
  `catalog_rows/6` (`lower.pl:747`) delegate once; `emit_ts.pl:763-764` forwards,
  no second schema list.
- **expander level/edge pairs differ for real reasons.** `0_coalesce_expand.pl:
  109-110` (`present_goal` level vs edge), `0_relation_edge_expand.pl:34-42`
  (`wrap_latest` only for edge), `0_dot_expand`, `0_seq_expand` split on
  `<-`/`<+` but each arm carries distinct logic. Not rote duplication.
- **`emit_ts` `*_lines`/`*_entry_line` pairs are parallel but not mergable.**
  `rel_columns_entry_line` (743), `rel_column_types_entry_line` (754),
  `snapshot_field_line` (870), `diff_local_line` (2158), `rel_entry_line` (2163)
  share the map-one-row-to-a-line shape, each emitting different text; a generic
  fold would save no real code.
- **No dispatch-table drift found** in the scanned producers. `rel_kind` with
  the `log|set` pair covers every ref (`compile.pl:215`), and `column_storage/3`
  (0_type_plane:77-128) refuses unknown kinds instead of defaulting; the one
  `decl_key` catch-all (`compile.pl:218`) pairs a sibling that never sees a
  declared-key gap.
- **No dead exported predicate** among the `emit_ts`/`lower` surface I checked;
  the single apparently-orphaned projection (`relation_column_types/4`) is
  internal, not exported, and covered by finding 3.

## 5. The two d2 numbers

`viewBox="0 0 1556 746"` (height 746 < width 1556)
shape count **14** (the `grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:'` key count is 23;
it also counts the `vars`/`classes`/`direction` config lines, not shapes).
