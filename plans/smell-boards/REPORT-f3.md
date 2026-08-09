# smell scan f3

## 1. Findings table

Ranked by lines removed, most first. Read-only lane: every number is an estimate of
code that disappears, marked `(est)` where a merge touches threading that spans files.
The named type-record finding (F1) dominates and is itemised in section 2.

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | v6/prolog/0_type_plane.pl, compile.pl, lower.pl, emit_ts.pl | 0_type_plane.pl:62-164, 380-385; compile.pl:127-134; lower.pl:746-895, 4564; emit_ts.pl:749-811 | type_decl/2, type_def/3, col_type/3, relplan/5 | one record `rel(Ref, Kind, [col(Column, DeclaredType)], KeyOrNone)`; storage via column_storage/3 | ~70-90 `(est)` |
| 2 | v6/prolog/lower.pl | 4590-4591 = 4636-4637 = 4646-4647 | departure_frontier_ddl/4, delta_ddl/3 (two clauses in one) | helper `create_temp_rowid(Table, TagCol, ColumnsSql)` | 10 |
| 3 | v6/prolog/emit_ts.pl | 743-811, 818-843, 2158, 2163 | rel_columns_entry_line, rel_column_types_entry_line, rel_catalog_entry_line, rel_declared_types_entry_line, diff_local_line, rel_entry_line | one per-rel entry generator + one derived type map | 18-24 `(est)` |
| 4 | v6/prolog/lower.pl | 1297-1298, 1309-1310, 4588-4589, 4618-4619, 4673-4674, 4692-4693, 4714-4715 | rel_ddl/6, departure_frontier_ddl, delta_ddl/3, dred_wave_ddl, expand_wave_ddl, ref_count_head_ddl | helper `column_defs_sql(Mode, Columns, ColumnTypes, Sql)` | 16 |
| 5 | v6/prolog/lower.pl | 1294/1306, 2006/2016, 4814/4816 | rel_ddl/6, arrival_statement/2, boot_seed_statement/5 | log-vs-set parallel clause prong, no guard forces symmetric growth | 0 removed; structural risk |

## 2. The type_decl / relplan / type_def question

Verdict on the coordinator hypothesis: **partly confirmed, one specific claim falsified.**

The four shapes carry the same column list + types truth.

| shape | payload | fields the others lack | where built |
| --- | --- | --- | --- |
| `col_type(Ref, Column, Type)` | flat rows, Ref carries arity | none beyond the flat spelling | the low-level atom in `Decls` |
| `type_decl(Name, [col(C,T)...])` | grouped, arity implicit | grouped spelling only | parse_dl.pl:848 builds it FROM col_type |
| `type_def(Name, Columns, ColumnTypes)` | de-paired lists | de-paired spelling only | 0_type_plane.pl:62 derives it FROM type_decl |
| `relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes)` | adds kind + key | Kind (log/set), KeyOrNone; ColumnTypes is the STORAGE kind | compile.pl:213 |

Answering the four bullets:

- **Fields each holds that the others do not.** `relplan` alone holds Kind and
  KeyOrNone (compile.pl:213-219). Its ColumnTypes holds STORAGE kinds, which for a
  struct-valued column is `ref(TypeName)` (analyze.pl:397-398) and for a list-column
  is `list(_)` (analyze.pl:403-404), not the declared type. `type_decl`/`col_type`
  hold the DECLARED type. The primitives `int/text/float/bool/json` coincide in both;
  they diverge on struct columns (`Name` vs `ref(Name)`) and on the inferred-vs-declared
  split for undeclared columns. `type_def` and `type_decl` are pure re-spellings of the
  identical truth, no extra fields.

- **Where relplan's ColumnTypes is a collapsed type while type_decl holds the declared.**
  The specific `list(text) reported as json` example is **falsified in current code**:
  `column_type_at_decl` keeps `list(_)` (analyze.pl:403-404), `column_def/3` has a live
  `list(_)` clause (lower.pl:1371-1374), and `boot_column_slot` matches `ColumnType =
  list(_)` (lower.pl:4860). Two stale header comments claim the collapse
  (lower.pl:746 "a relplan reports a list column as json", 866 "a list column's resolved
  type has already collapsed to json") but the code does not do it.
  The REAL collapse that forces `Decls` to be threaded beside `RelPlans` is
  struct-to-ref: emit_ts.pl:309 matches `memberchk(ref(_), ColumnTypes)`. The places
  that must consult BOTH the collapsed (relplan) and the declared (Decls) shape of a
  column:
    * lower.pl:746-895 `catalog_rows` -> `catalog_column_type_id` reads `Decls` for the
      declared type before the storage kind.
    * lower.pl:4564 `retention_statements(Decls, RelPlans, ...)`.
    * lower.pl:4814-4817 `boot_seed_statement(Mode, Decls, Types, relplan(...), ...)`.
    * emit_ts.pl:763 `program_catalog_rows(plan(_, prog(Decls,..),..), RelPlans, ..)`
      and emit_ts.pl:793 `rel_declared_column_types_lines(Decls, RelPlans, ..)`.

- **Smallest merged record serving every current reader.**
  `rel(Ref, Kind, [col(Column, DeclaredType)], KeyOrNone)` carrying the DECLARED type
  as the single authority. Readers needing the storage kind call
  `column_storage(Types, DeclaredType, Storage)` (0_type_plane.pl:77-128), which is
  already the function analyze.pl uses to derive storage from declared (analyze.pl:390).
  This is the smallest record because `Kind`, `KeyOrNone`, the column list, and the
  declared type are the only independent data; storage and the inferred type are both
  functions of the declared type plus witnesses and carry no independent truth.

- **How many of the 113 + 102 sites change.** The coordinator's 113 + 102 count the
  test corpus. Non-test, non-generated site counts I measured: `relplan(` 45,
  `type_decl(` 11, `type_def(` 6, `col_type(` 55 (across `.pl` sources, excluding
  `compile/test`, `conformance/fixtures`, `labs`, `compile/out`, `sweep.pl`). Every
  pattern-match site changes shape. The sites that would be DELETED rather than edited
  are the derivation layer (0_type_plane.pl:62-72, 158-164, 380-385) and the two
  flattened projections (compile.pl:127-134; emit_ts.pl:802-811). Estimate of
  survive-to-edit sites: the 45 relplan readers (unchanged reads, just the new record
  arity) plus the 11 type_decl sites that become reads of the merged record. I marked
  the 113+102 as test-inclusive; they do not represent independent source edits.

**Counterexample showing the falsified claim.** A declared column `list(text)` stays
`list(text)` in relplan ColumnTypes end to end (analyze.pl:403-404 -> lower.pl:1371),
so there is no case where relplan reports `json` for a declared `list(T)`. The collapse
the hypothesis names lives only in two stale comments. The honest merge driver is the
declared-vs-storage split (struct -> `ref(Name)`), not a list->json collapse.

## 3. The top three

### Top 1. Four record shapes of one column truth

Current (derive a fourth shape to re-read the same payload):

```prolog
% 0_type_plane.pl:62  type_def derived from type_decl
type_definitions(Decls, Types) :-
    findall(type_def(Name, Columns, ColumnTypes),
            ( member(type_decl(Name, Specs), Decls),
              findall(Column, member(col(Column, _), Specs), Columns),
              findall(Type, member(col(_, Type), Specs), ColumnTypes) ), Types).

% 0_type_plane.pl:158  readers must answer from either spelling
relation_columns_and_types(_, Types, Name/Arity, Columns, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes), length(Columns, Arity), !.
relation_columns_and_types(Decls, _, Ref, Columns, ColumnTypes) :-
    findall(Column-Type, member(col_type(Ref, Column, Type), Decls), Pairs),
    pairs_keys_values(Pairs, Columns, ColumnTypes).
```

Proposed (one record, storage derived):

```prolog
% one rel/4 holds declared type; column_storage/3 derives the storage kind
rel(R, Kind, [col(C, T)|_], Key) :- RelPlans, memberchk(rel(R, Kind,_,Key),RelPlans).
% readers that want storage call column_storage(Types, Declared, Storage)
```

### Top 2. Three identical working-temp-table DDls

Current (lower.pl:4591, 4637, 4647 identical):

```prolog
format(atom(Ddl),'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
       [QuotedTable, ColumnsSql]).
```

Proposed:

```prolog
create_temp_work(QuotedTable, TagCol, ColumnsSql, Ddl) :-
    format(atom(Ddl), 'CREATE TEMP TABLE ~w ("~w" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedTable, TagCol, ColumnsSql]).
```

Delta table uses `"_sign"` (lower.pl:4621) instead of `"_phase"`; the helper's TagCol
parameter absorbs that too, dropping the delta branch from a separate template to one
argument.

### Top 3. Two emitted column-type maps (declared + storage)

Current (emit_ts.pl:749 and 793 emit two maps from two record shapes):

```prolog
rel_column_types_lines(RelPlans, ... ['const rel_column_types: Record<...> = {' ... ]).
rel_declared_column_types_lines(Decls, RelPlans, ... ['const rel_declared_column_types ...']).
```

Proposed: emit ONE declared-type map; the runtime derives the boundary/gate word through
`column_storage` (declared -> storage), which is exactly what `gate_column_type` and
`boundary_column_type` (emit_ts.pl:818-843) already do at emit time. Removes the second
pass over `Decls` and the duplicated list->json arms (emit_ts.pl:823, 842).

## 4. What you checked and found clean

- `0_type_plane.pl:680-705` `canonical_json_text/2` is a deliberate clause-for-clause
  mirror of `conformance/ticklog.pl:value_json/2`. The header says why: ticklog is a
  script, so the compiler cannot import it without dragging in the oracle; the agreement
  is pinned by the byte-diff grade. Justified duplicate, leave it.
- `0_type_plane.pl:719-845` `js_float_text` rewriting is single-purpose, no mirror.
- `print_dl.pl:250-272` `decl_line/5` has one clause per decl kind; each renders a
  different surface syntax, so the shared `maplist(print_host_column)` motif buys
  nothing to merge.
- `strat.pl` and `compile/parse_dl.pl` I scanned for repetition in the declaration
  parsing and ordering path; `parse_dl.pl:845-864` `normalize_relation_value_decls` is
  the legitimate source of `type_decl` (built from `col_type`), the counterpart to the
  `compile.pl:127` flatten the other direction. `UNVERIFIED`: I did not read all of
  `parse_dl.pl` (1849 lines) or `strat.pl` clause by clause.
- `dred_wave_table_ddl/4` and the expand/ref-count temp tables share the
  `PRIMARY KEY (...) WITHOUT ROWID` template (lower.pl:4685, 4700, 4703) but each is one
  format call; grouping them is covered by Finding 4 rather than its own row.

## 5. The two d2 numbers

- viewBox: `"0 0 1917 834"` (height 834 < width 1917).
- shape/config keys matched by the check: 18 (12 true shapes + 6 config keys).
