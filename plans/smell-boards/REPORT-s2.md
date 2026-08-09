# smell scan s2

## 1. Findings table

| rank | file | line span | predicates | merged form | lines removed |
| --- | --- | --- | --- | --- | --- |
| 1 | v6/prolog/lower.pl | 1296-1298, 1308-1310, 2917-2920, 2931-2934, 4587-4589, 4598-4600, 4617-4619, 4672-4675, 4691-4694, 4713-4716 | rel_ddl/6 (both clauses), aggregate_scope_ddl/3 (both clauses), departure_frontier_ddl/4, pre_ddl/4, delta_ddl/3 (temp-table clause), dred_wave_ddl/4, expand_wave_ddl/4, ref_count_head_ddl/4 | one columns_def_and_pk_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql, PrimaryKeySql) helper, called once per site | ~20 (35 duplicated lines become 10 one-line calls plus a 5-line helper) |
| 2 | v6/prolog/compile/parse_dl.pl + v6/prolog/0_type_plane.pl | parse_dl.pl:837-872, 0_type_plane.pl:62-67 | normalize_relation_value_decls/2,4, relation_schema/4, type_definitions/2 | drop type_decl/2 as a stored fact; type_definitions/2 reads col_type/3 directly, gated by the already-computed ValueRelationNames set | ~18 (see section 2, production code only) |
| 3 | v6/prolog/emit_ts.pl | 870-872, 2158-2161, 2163-2165 | snapshot_field_line/2, diff_local_line/2, rel_entry_line/2 | one relplan_name(relplan(Ref,_,_,_,_), Name) :- ref_name(Ref, Name). accessor, each site keeps its own format/3 | 3 |
| 4 | v6/prolog/lower.pl | 1436-1437 | dictionary_storage_kind/3 | delete the wrapper; both call sites (1576, 1799) call column_storage/3 directly | 2 |
| 5 | v6/prolog/lower.pl, emit_ts.pl, analyze.pl, print_dl.pl | 16 sites, listed in section 4 | inline memberchk(relplan(Ref,...), RelPlans) / member(relplan(Ref,...), RelPlans) | route every site through the 3 existing accessors (relplan_columns/3, relplan_kind/3, relplan_column_types/3, lower.pl:269-271) plus a new relplan_key/3 | 0 net (robustness finding, not a size finding, see section 4) |

## 2. The type_decl / relplan / type_def question

Which fields does each shape hold that the others do not:

- type_decl(Name, Specs), Specs = [col(Column,Type), ...]: holds nothing col_type/3 lacks. Confirmed at the source: parse_dl.pl:866-872 (relation_schema/4) builds Specs by re-reading col_type(Name/Arity, Column, Type) facts already present in Decls, and parse_dl.pl:848 inserts the result as a second, separate type_decl/2 fact next to the col_type/3 facts it was read from. 0_type_plane.pl:18-21's own header calls it a "legacy compiler IR record."
- type_def(Name, Columns, ColumnTypes): a pure unzip of type_decl's Specs into parallel lists, computed fresh on every call to type_definitions/2 (0_type_plane.pl:62-67). Holds nothing type_decl lacks; it is a second reshape of the same pairs, done because most readers (column_storage/3, type_ref_columns/3, type_shape_error/4, the canonical-JSON renderer) want positional (nth1) access, not a col(Column,Type) pair list. That access-pattern difference is real and earns type_def its keep; it is not the redundant one.
- relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes): Ref is arity-qualified (Name/Arity, not bare Name); Kind (set/log) comes from rel_kind/3, a different decl family (keep/kind/keyed, not col_type); KeyOrNone comes from decl_key/2. Built once in compile.pl:213-219 for every Ref in the program (AllRefs = rule heads/bodies union declared refs union seeded refs), not just names that got a type_decl. relplan's ColumnTypes strictly generalizes type_def's: for a struct's own columns it is the same scalar list; relplan additionally covers rule-derived refs with zero col_type declarations at all (typed by the literal-witness fixpoint, analyze.pl:478-535), which type_decl/type_def never see.
- Every name that gets a type_decl necessarily also gets its own relplan entry: normalize_relation_value_decls/2 (parse_dl.pl:837-843) only fires on a Name that already has col_type(Name/Arity, _, _) facts, and declared_refs/2 (analyze.pl:252-261) puts any Ref with a col_type fact into AllRefs, which compile.pl:213 turns into a relplan. So type_decl/type_def's payload (Columns plus scalar ColumnTypes) is a strict subset of what relplan already carries for the same Ref.

Where does relplan's ColumnTypes hold a COLLAPSED type while type_decl holds the declared one:

FALSIFIED for the specific example given (list(text) reported as json). column_storage/3 (0_type_plane.pl:77-128) has a dedicated clause for json (line 88, column_storage(_, json, json) :- !.) and a separate dedicated clause for list(Element) (lines 115-122, column_storage(Types, list(Element), list(Element)) :- !, ...) that returns list(Element) unchanged. lower.pl:1371-1374's column_def/3 also has its own clause for list(_) (a TEXT column with an array-ness CHECK), distinct from the json clause at lower.pl:1383-1385. Neither collapses list types into json.

One caveat, UNVERIFIED against history: 0_type_plane.pl:111-114's own comment says "Today the storage kind collapses to json," immediately above the clause that does not collapse it. That reads as a stale comment describing a since-fixed state, not a live bug; I did not run git log -p on the file to confirm when the fix landed. Recommend: git log -p -- v6/prolog/0_type_plane.pl | grep -n "collapses to" to date the comment against the list(Element) clause.

Because the premise is false, there is no live "must consult both because of a collapse" site over list types. There is a genuine reason many predicates thread both Types and RelPlans as separate parameters (15+ call sites: expand_relation_pattern_rule/6, rewrite_relation_atom/6, decode_goal_atoms/6, check_edge_rule_relation_values/3, boot_statements/6, etc., lower.pl:1650-1926, 4941, 5008), but it is not the collapse. It is that RelPlans has no "this Ref is a struct type" flag. Testing struct-type membership today only works through Types (declared_type_name/2), because relplan/5 carries Kind in {set, log}, never a third struct tag.

What is the smallest merged record that serves every current reader:

Extend relplan/5 to relplan/6, relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes, TypeTag) where TypeTag is none or struct, and delete type_decl/2 as a stored Decls fact. type_definitions/2 (0_type_plane.pl:62-67) is rewritten to filter relplan/6 on TypeTag = struct and unzip Columns/ColumnTypes directly (no Specs, no col(Column,Type) round trip). type_def/3 (Types) stays exactly as it is today; the positional-access shape is genuinely needed, it just gets built from relplan instead of from type_decl. print_dl.pl's shadowed_by_type_decl/2 (print_dl.pl:221-224) becomes declared_type_name(Types, Name), which already exists.

How many of the 113 + 102 sites would actually have to change:

My own counts over in-scope, non-generated, non-fixture, non-lab .pl files (grep -rn under v6/prolog, excluding compile/out/**, labs/**, **/fixtures/**) differ from the brief's supplied numbers, so I report mine and flag the gap: type_decl( 27 sites, type_def( 47 sites, relplan( 114 sites, col_type( 163 sites. The 661/102/10/113 figures in the brief likely include the generated corpus, plunit fixtures, or labs this lane was told to ignore. UNVERIFIED, worth a grep -c over the full tree including those to reconcile.

Of my 27 type_decl( sites: 19 are plunit test fixtures (compile/test/plunit_tests.pl) constructing type_decl(...) directly as literal Decls input, bypassing the parser; these are out of this lane's read-only scope and would need rewriting to col_type/3 facts, real but uncounted churn. The remaining 8 are production code: 0_type_plane.pl:64 (type_definitions/2 itself), print_dl.pl:214,221-222,226,254 (5 sites, the printer's round-trip dedup), compile.pl:129 (the synthesis call site), 0_program_check.pl:760 (1 site). All 8 change to type_definition/4 or declared_type_name/2 calls, both of which already exist; no new predicate needed beyond the relplan/6 extension. relplan( sites do not need to change at all; they gain a 6th argument pattern (a wildcard _TypeTag) at existing relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes) matches, which is a mechanical arity-widening edit, not a logic change, across all 114.

## 3. The top three

### 1. lower.pl: the columns-DDL idiom, 10 sites

Current, pre_ddl/4, lower.pl:4594-4600:

```prolog
pre_ddl(Mode, RelPlans, Ref, Ddl) :-
    memberchk(relplan(Ref, _, Columns, KeyOrNone, ColumnTypes), RelPlans),
    pre_table_name(Ref, PreTable),
    quote_ident(PreTable, QuotedPreTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
```

and ref_count_head_ddl/4, lower.pl:4711-4716:

```prolog
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
```

Proposed:

```prolog
columns_def_and_pk_sql(Mode, Columns, ColumnTypes, QuotedColumns, ColumnsSql, PrimaryKeySql) :-
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql).
```

Each of the 10 sites replaces its local 3 or 4 lines with one call, ignoring PrimaryKeySql where unused (Prolog permits an unused output var). 35 duplicated lines become 10 call lines plus the 5-line helper: 15 total, -20 net.

### 2. the type_decl / type_def / relplan chain

Current, parse_dl.pl:845-855 plus 0_type_plane.pl:62-67, the synthesis and the reshape:

```prolog
normalize_relation_value_decls([Head | Rest], ValueNames, Seen,
                               [type_decl(Name, Specs), Head | More]) :-
    Head = col_type(Name/Arity, _, _),
    memberchk(Name, ValueNames), \+ memberchk(Name, Seen), !,
    relation_schema([Head | Rest], Name, Name/Arity, Specs),
    normalize_relation_value_decls(Rest, ValueNames, [Name | Seen], More).
% ...
type_definitions(Decls, Types) :-
    findall(type_def(Name, Columns, ColumnTypes),
            ( member(type_decl(Name, Specs), Decls),
              findall(Column, member(col(Column, _), Specs), Columns),
              findall(Type, member(col(_, Type), Specs), ColumnTypes) ),
            Types).
```

Proposed, no type_decl/2 fact ever stored; relplan/6 carries the struct tag from the moment compile.pl builds it:

```prolog
findall(relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes, TypeTag),
        ( member(Ref, AllRefs), Ref = Name/_,
          rel_kind(Decls, Ref, Kind),
          memberchk(Ref-Columns, RefColumns), memberchk(Ref-ColumnTypes, RefTypes),
          ( decl_key(Decls, Ref, Positions) -> KeyOrNone = key(Positions) ; KeyOrNone = none ),
          ( declared_column_type_name(Decls, Name) -> TypeTag = struct ; TypeTag = none )
        ), RelPlans),
% ...
type_definitions(RelPlans, Types) :-
    findall(type_def(Name, Columns, ColumnTypes),
            member(relplan(Name/_, _, Columns, _, ColumnTypes, struct), RelPlans),
            Types).
```

Line delta is modest (about 18 lines, section 2) because most of the value here is removing a second store of the same fact, not raw text. The risk this buys back: today a change to how a struct's columns are typed has to stay consistent across three encodings (col_type/3, type_decl/2, relplan.ColumnTypes) by convention, with nothing enforcing agreement: exactly the brief's "dispatch table drifted" class, applied to a whole record rather than a foo(a,1) table.

### 3. emit_ts.pl: the ref_name family

Current, emit_ts.pl:2158-2165, two of three sites:

```prolog
diff_local_line(relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    format(atom(Line), '  const ~w = multiset_diff(before.~w, after.~w);', [Name, Name, Name]).

rel_entry_line(relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    format(atom(Line), '      { rel: "~w", add: ~w.add, del: ~w.del },', [Name, Name, Name]).
```

Proposed:

```prolog
relplan_name(relplan(Ref, _, _, _, _), Name) :- ref_name(Ref, Name).

diff_local_line(RelPlan, Line) :-
    relplan_name(RelPlan, Name),
    format(atom(Line), '  const ~w = multiset_diff(before.~w, after.~w);', [Name, Name, Name]).
```

Small (-3 lines across snapshot_field_line/2, diff_local_line/2, rel_entry_line/2), listed because it is the cleanest, lowest-risk example of the pattern, unlike #1 where PrimaryKeySql usage varies per site.

## 4. What I checked and found clean

- v6/prolog/0_enum_expand.pl, 0_dot_expand.pl, 0_match_expand.pl, 0_relation_edge_expand.pl (full reads): each has a <-/<+/passthrough three-clause dispatch (for example 0_coalesce_expand.pl:72-80 versus 0_relation_edge_expand.pl:34-43). The shape repeats across files but the clause bodies do not; different rewrite logic per file. Not a duplication finding; it is a shared idiom for "handle a level rule, handle an edge rule, pass everything else through," and merging it would need a higher-order combinator taking two rewrite closures, a bigger design change than this scan is asked to propose.
- v6/prolog/lower.pl:269-271 (relplan_columns/3, relplan_kind/3, relplan_column_types/3): these accessors exist and are used at roughly 55 call sites (verified by grep), the majority of RelPlans lookups in the file. The 16 sites that bypass them are drift, not absence of a pattern: print_dl.pl:187, analyze.pl:1060,1068, emit_ts.pl:308,795,1048,1165,1515,1708,1798,1808, lower.pl:403,777,1110,1261,2202,2307,4555,4595. Three of these (lower.pl:2202,4595, emit_ts.pl:1048) need KeyOrNone, which has no accessor; that gap, not laziness, explains why those three hand-write the full 5-tuple match. The other 13 need only Columns or ColumnTypes and could call the existing accessor with no line-count change, only reduced coupling to relplan/5's literal arity.
- v6/prolog/analyze.pl declared_refs/2, program_refs/2, seeded_refs/2 (analyze.pl:240-269): three findalls over different Decls/Rules/Initial shapes feeding one AllRefs union in compile.pl:193. Structurally parallel (each is a findall plus sort) but not reducible; they read three genuinely different sources.
- v6/prolog/compile/parse_dl.pl coltype//3 (823-829) and decl_b_column_type//5 (815-821): read once, no duplication found, but not exhaustively checked against the rest of the 1849-line file. UNVERIFIED beyond the sections cited above.
- v6/prolog/analyze.pl (1742 lines) and v6/prolog/emit_ts.pl (2809 lines): only the sections reachable from relplan/type_decl grep hits and the record-emission family were read closely. The remainder (roughly two-thirds of each file) was not read line by line in this pass. UNVERIFIED for duplication outside what is cited here.
- v6/prolog/0_program_check.pl (940 lines), v6/prolog/strat.pl (114 lines), v6/prolog/1_expansion.pl (86 lines): sampled at the type_decl/col_type grep hits only, not read in full. UNVERIFIED beyond the one 0_program_check.pl:760 site cited in section 2.

## 5. The two d2 numbers

- viewBox="0 0 3292 1107"
- shape-line count (grep -cE '^[[:space:]]*[A-Za-z0-9_.-]+:' plans/smell-s2.d2): 22
- actual shape count, manually verified: 16 (1 title plus 6 in the type-plane container plus 9 in the column-def container), under the 24 max.
