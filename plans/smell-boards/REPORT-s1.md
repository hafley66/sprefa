# smell scan s1

Note on scope: the brief lists `v6/prolog/compile/strat.pl` and
`v6/prolog/compile/print_dl.pl`. Both files exist at `v6/prolog/strat.pl` and
`v6/prolog/print_dl.pl` instead (no `compile/` prefix). Read at their real
paths; noted here so the coordinator can fix the brief for the next lane.

## 1. Findings table

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | `v6/prolog/lower.pl` | 1296-1298, 1308-1310, 4587-4589, 4598-4600, 4617-4619, 4672-4675, 4691-4694, 4713-4716 (8 sites) | `rel_ddl/6` (log, set), `departure_frontier_ddl/4`, `pre_ddl/4`, `delta_ddl/3` (relplan clause), `dred_wave_ddl/5`, `expand_wave_ddl/4`, `ref_count_head_ddl/4` | one `column_defs_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql)` helper, called once per site | 12 (27 lines -> 15 lines: helper definition 4 + 8 call-sites 5+6, unchanged PK lines not counted) |
| 2 | `v6/prolog/0_type_plane.pl` | 158-164 vs 380-385 | `relation_columns_and_types/5`, `relation_column_types/4` | `relation_column_types/4` becomes a 2-line wrapper calling `relation_columns_and_types/5` and discarding `Columns` | 4 (6 lines -> 2 lines) |
| 3 | `v6/prolog/lower.pl` | 2006-2015, 2016-2034 | `arrival_statement/2` (log clause), `arrival_statement/2` (set clause) | shared prologue extracted to `arrival_columns_and_placeholders(Ref, Columns, Table, QuotedTable, QuotedColumns, ColumnsSql, PlaceholdersSql)` | 2 (10 lines of shared prologue -> 8) |
| 4 | `v6/prolog/emit_ts.pl` | 818-824 vs 826-843 | `gate_column_type/2`, `boundary_column_type/2` | shared `wire_column_type/2` fact table (6 facts) for the 6 clauses both predicates duplicate, each predicate keeps only its own catch-all | 0 net (11 lines -> 11 lines). Value is not deletion: the `list(_) -> json` collapse rule is currently spelled twice and can drift; one predicate already has a `ref(_)` clause the other lacks. See section 2. |
| 5 | `v6/prolog/lower.pl` | 1332, 1407, 1412, 1418, 1426, 1432 | `relation_render_expr/5`, `relation_render_column_expr/5` | drop the `Types` argument from both predicates' signatures | 0 lines deleted, 1 dead argument removed across 2 predicate definitions + 3 clause heads + 1 call site. See detail below. |

**Finding 5 detail (threading noise, brief class 6).** `rel_ddl/6` at
lower.pl:1332 calls `relation_render_expr(Mode, Types, Columns, ColumnTypes,
RenderExpr)`. `relation_render_expr/5` (lower.pl:1407-1415) passes `Types`
straight into `relation_render_column_expr(Mode, Types, Column, ColumnType,
ValueExpr)` at line 1412 and does nothing else with it. All three clauses of
`relation_render_column_expr/5` (lower.pl:1418, 1426, 1432) bind that argument
to `_`:

```prolog
relation_render_column_expr(_, _, Column, ref(TypeName), Expr) :- ...
relation_render_column_expr(Mode, _, Column, ColumnType, Expr) :- ...
relation_render_column_expr(_, _, Column, _, Expr) :- ...
```

`Types` is threaded through 2 predicate definitions and read by none of them.
The struct-column case is already fully decided by `ColumnType = ref(TypeName)`
(relplan's resolved storage kind), so `Types` was never needed here.

## 2. The type_decl / relplan / type_def question

**Which fields does each shape hold that the others do not?**

- `type_decl(TypeName, Specs)` (`Specs = [col(Column, Type), ...]`,
  parsed fact, 0_program_check.pl and friends read it via `Decls`): the
  **declared** type as the user wrote it, still unresolved (`list(text)`,
  a struct type name, `json`, etc). Nothing else in the compiler carries the
  pre-resolution spelling.
- `type_def(Name, Columns, ColumnTypes)` (0_type_plane.pl:56-66,
  `type_definitions/2`): the same `Specs` pairs unzipped into two parallel
  lists. Computed by one pure pass over `type_decl` facts, zero new
  information over `type_decl`. It exists purely for positional access
  (`nth1(Position, Columns, ...)` / `nth1(Position, ColumnTypes, ...)`).
- `relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes)`
  (lower.pl:7-24 header): three fields neither `type_decl` nor `type_def`
  has an analogue for — `Kind` (log|set, physical write semantics),
  `KeyOrNone` (SQL key positions for `ON CONFLICT` / `WITHOUT ROWID` PK), and
  a `Ref` domain that is **strictly larger** than `type_decl`'s: `type_decl/2`
  is emitted only for a rel reached in column position (0_type_plane.pl:18-19,
  "produced from a `rel` declaration referenced in column position"), while
  113 `relplan(` sites cover every ordinary `rel foo(...)` declaration too.
  `relplan`'s `ColumnTypes` is also the **resolved storage kind**
  (post `column_storage/3`: a struct type name becomes `ref(Name)`), not the
  raw declared type.

**Where does relplan's ColumnTypes hold a COLLAPSED type while type_decl
holds the declared one? List every site that has to consult both.**

The collapse is not inside `relplan` or `type_decl` themselves — `column_storage/3`
(0_type_plane.pl:113 `list(Element)` clause) preserves `list(Element)` as its
own storage kind all the way into `relplan`. The collapse happens at the
TypeScript wire boundary in `emit_ts.pl`, independently, twice:

- `boundary_column_type(list(_), json) :- !.` (emit_ts.pl:842) feeds
  `rel_column_types_entry_line/2` (emit_ts.pl:753-759), which reads
  **relplan's** `ColumnTypes` directly.
- `gate_column_type(list(_), json) :- !.` (emit_ts.pl:823) feeds
  `rel_declared_column_types_lines/3` (emit_ts.pl:793-799) via
  `declared_column_types/3` (emit_ts.pl:802-805), which does **not** go
  through relplan or `type_def` at all — it re-scans `col_type(Ref, _, Type)`
  facts straight out of raw `Decls`.

I found **zero** sites that consult both `type_decl`/`type_def` and `relplan`
for the same column-type question. Instead there is exactly one site,
`declared_column_types/3` (emit_ts.pl:802-805), that reimplements *half* of
0_type_plane.pl's existing `relation_columns_and_types/5` (0_type_plane.pl:
158-164) — the `Decls`-scan fallback clause only — and **omits the
`type_definition/4` (struct) branch** that accessor has. `UNVERIFIED`: whether
this omission is a live bug (a struct's dictionary relplan, built by
`dictionary_relplans/2` at lower.pl:1571-1580, gets no entry in the emitted
`rel_declared_column_types` TS map, since `declared_column_types/3` finds no
`col_type/3` facts for a synthesized `__ref_<TypeName>` table name) or dead
code that never fires because the arrival gate never runs against a
dictionary table. What to run to check: compile a fixture with a struct
column (e.g. one already in `v6/dl/fixtures/`), diff the generated
`rel_declared_column_types` and `rel_column_types` TS object literals for the
struct's `__ref_<TypeName>` key.

**What is the smallest merged record that serves every current reader?**

`type_def` is the only one of the three with zero readers that need
independent storage: eliminate it, and have `type_definition/4` +
`declared_type_name/2` (its 2 exported readers) call `type_definitions/2`
directly, or better, delegate to `relation_columns_and_types/5` the way
`relation_column_types/4` should already (finding 2 above). `type_decl` and
`relplan` cannot merge: `type_decl` is needed pre-resolution (the parser and
`column_storage/3` itself consume it), `relplan` is needed post-resolution
plus `Kind`/`Key`, and `relplan`'s `Ref` domain covers ordinary rels
`type_decl` never touches.

**How many of the 113 + 102 sites would actually have to change?**

Zero of the 113 `relplan(` sites need a shape change. They read
`Columns`/`ColumnTypes`/`Kind`/`Key` and never the raw declared `Specs`, so
folding `type_decl` into `relplan` would be pure threading noise across
sites that do not want the payload — the exact class named in the brief,
applied to the merge itself rather than to something already in the code.
Of the 102 `type_decl(` sites, `type_def`'s **10** call-through sites (they
call `type_definition/4`, not `type_def/3`'s functor) stay textually
unchanged; only the 2-clause producer (`type_definitions/2`, 6 lines) and the
6-line `relation_column_types/4` duplicate shrink. At most 1 site
(`emit_ts.pl:793-805`) changes for real, by rerouting through
`relation_columns_and_types/5` instead of hand-rolling `declared_column_types/3`.

**Verdict: the coordinator's hypothesis is falsified in its strong form.**
`type_decl`, `type_def`, and `relplan` are not one record independently
maintained in three places that can drift from each other — `type_def` is
mechanically derived from `type_decl` by a single function
(`type_definitions/2`), and the struct-only slice of `relplan` is mechanically
derived from `type_def` by a single function (`dictionary_relplans/2`,
lower.pl:1571-1580). Nothing threads `Decls` alongside `RelPlans` "everywhere"
because of this trio; `Decls` is threaded through the compiler for reasons
unrelated to column types (`keep(Ref, count(N))` retention decls, reserved-word
checks in 0_program_check.pl, key decls), so `RelPlans` could never replace
`Decls` regardless of any merge here. The real, much narrower defect is
finding 4 above (two independently-hand-written `list(_) -> json` collapse
tables) plus the one site in this section that skips the shared accessor.

## 3. The top three

### #1 -- lower.pl DDL column-def prologue (8 sites, 12 lines)

Current (2 of 8 sites shown; the other 6 repeat the same 3-4 line shape at
1308, 4587, 4598, 4617, 4672, 4691):

```prolog
% lower.pl:1294-1299, rel_ddl/6 (log)
rel_ddl(Mode, _, _, _, _, relplan(Ref, log, Columns, _, ColumnTypes), Ddls) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),

% lower.pl:4706-4716, ref_count_head_ddl/4
ref_count_head_ddl(Mode, RelPlans, HeadRef, [Ddl, NewDdl, ZeroIndexDdl]) :-
    ref_count_table_name(HeadRef, RefCountTable),
    quote_ident(RefCountTable, QuotedRefCountTable),
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
```

Proposed:

```prolog
column_defs_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql) :-
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql).

rel_ddl(Mode, _, _, _, _, relplan(Ref, log, Columns, _, ColumnTypes), Ddls) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    column_defs_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql),

ref_count_head_ddl(Mode, RelPlans, HeadRef, [Ddl, NewDdl, ZeroIndexDdl]) :-
    ref_count_table_name(HeadRef, RefCountTable),
    quote_ident(RefCountTable, QuotedRefCountTable),
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    column_defs_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
```

### #2 -- 0_type_plane.pl relation_column_types/4 (4 lines)

Its own header comment already names this as a pre-existing duplicate: "A rel
reached in COLUMN position carries its shape in a `type_decl/2` ...
`relation_column_types/4` below is the types-only projection that predates
it" (0_type_plane.pl:152-157).

Current:

```prolog
% 0_type_plane.pl:380-385
relation_column_types(_, Types, Name/Arity, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    length(Columns, Arity),
    !.
relation_column_types(Decls, _, Ref, ColumnTypes) :-
    findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes).
```

Proposed:

```prolog
relation_column_types(Decls, Types, Ref, ColumnTypes) :-
    relation_columns_and_types(Decls, Types, Ref, _Columns, ColumnTypes).
```

### #3 -- lower.pl arrival_statement/2 shared prologue (2 lines)

Current (both clauses repeat lines 2008-2012/2018-2022 verbatim):

```prolog
% lower.pl:2006-2013, log clause
arrival_statement(relplan(Ref, log, Columns, _, _),
                  arrivalstmt(Ref, log, AddSql, none, IncrementalAddSql, none)) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
```

Proposed:

```prolog
arrival_columns_and_placeholders(Ref, Columns, Table, QuotedTable, QuotedColumns,
                                 ColumnsSql, PlaceholdersSql) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql).

arrival_statement(relplan(Ref, log, Columns, _, _),
                  arrivalstmt(Ref, log, AddSql, none, IncrementalAddSql, none)) :- !,
    arrival_columns_and_placeholders(Ref, Columns, Table, QuotedTable, QuotedColumns,
                                     ColumnsSql, PlaceholdersSql),
    format(atom(AddSql), 'INSERT INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
```

## 4. What I checked and found clean

- **`boot_seed_statement/6` (lower.pl:4814-4817)**: the log/set pair is
  already correctly factored -- both clauses call the same
  `boot_rows_statements/8`, differing only in the `Insert` verb atom
  (`'INSERT INTO'` vs `'INSERT OR IGNORE INTO'`). No duplicated logic.
- **`0_program_check.pl` (37 `program_violation(` clauses)**: surveyed as a
  candidate dispatch table. Every clause names a distinct refusal with its
  own real check logic; there is no catch-all clause silently absorbing a
  case (the brief's `foo(a,1). foo(b,2). foo(_,0).` shape). This is the
  correct pattern for a named-refusal ledger, not drift.
- **`relation_reference_target/5` (0_type_plane.pl:358-376)**: two clauses
  share 5 of 7 lines. Looked like a candidate, but they are base-case
  (target found directly) and recursive-case (descend one struct level) of
  one traversal, not independently-maintained copies -- merging would need an
  artificial base call for zero measured line savings. Left alone.
- **`print_dl.pl` `print_body_item/3` vs `print_surface_body_item/4`**: not a
  mirrored family. `print_body_item/3` (387-406) delegates to
  `print_surface_body_item/4` for exactly one of its 5 clauses
  (`body_surface_for_term/6` dispatch, line 401-403); the rest is
  composition, not duplication.
- **`compile/parse_dl.pl`, `strat.pl`, `print_dl.pl` overall**: predicate
  frequency survey (`typed_column_type/7`, `body_item/7`,
  `parse_surface_wrapper/N`, etc.) shows the expected shape for a recursive
  descent parser/pretty-printer -- many small clauses per production, each
  handling a distinct grammar shape. No repeated 3+ clause family found that
  mirrors another predicate's cases.
- **`type_definitions/2` recomputation (21+ call sites across the scope)**:
  every caller recomputes `Types` fresh from `Decls` on its own pass. This is
  a real repeated cost, not a repeated-logic smell (one function, called
  often) -- it is a recompute-guard question, not a duplication question, so
  out of this lab's scope per the brief's finding classes.

## 5. The two d2 numbers

- `viewBox="0 0 2639 1682"` (width 2639 > height 1682, ratio 1.57)
- Shape count per the brief's grep command
  (`grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:' plans/smell-s1.d2`): **32**
  lines match that key-line pattern (includes nested `style.*:` and
  `vars`/`classes` block lines). The actual rendered shape count, counted
  independently via `class="shape"` occurrences in the compiled SVG, is
  **22**, under the 24-shape cap.
