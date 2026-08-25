% lower.pl : rule -> SQL text. Compiles the plan/6 term compile.pl builds
% into a lowered/8 term:
%
%   lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements,
%           DeltaStatements, RelPlans, ArrivalTargets)
%     Ddl              : list of CREATE TABLE SQL strings.
%     RelPlans         : 0_rel_record.pl's rel/5 list, unchanged from the plan;
%                        column_def/4 below is the only reader of its storage.
%     ArrivalStatements: list of arrivalstmt(Ref, Kind, AddSql, DelSqlOrNone,
%                        IncrementalAddSql, IncrementalDelSqlOrNone).
%     EdgeStatements   : list of edgestmt(HeadRef, TriggerRef, HeadColumns,
%                        KeyColumns, ProjectSql, WriteSql, DeltaProjectSql).
%     LevelStatements  : list of levelstmt(HeadRef, DeleteSql, InsertSqls,
%                        DeltaInsertSql, RefCountSql) and
%                        retentionstmt(Ref, Limit, DeleteSql),
%                        already in execution order (strat.pl:sql_rule_order/2).
%     DeltaStatements  : list of deltastmt(Ref, SelectAllSql, DeltaTable,
%                        BoundarySql). SelectAllSql preserves the recompute
%                        referee. DeltaTable and BoundarySql carry P1's
%                        tick-local change stream.
%
% plus boot_statements/7, a SEPARATE list of bootstmt(Rel, Sql, Params) (needs
% Initial, which the plan does not carry, plus LevelStatements for the t=0
% level closure -- PHASE C2 RULING 2, boot_level_recompute_statements/2).
%
% The lowered representation contains SQL text and plain Prolog structures;
% host-language assembly belongs to the emitter.
%
% ── ROUND 2 (reconciliation, tsgo error list): no tick number reaches
% tick() ───────────────────────────────────────────────────────────────────
% The real runtime/types.ts (Phase A, merged) hands `tick(seam, arrivals)`
% ONLY the arrival batch -- tick NUMBERING is owned entirely by the runtime's
% fold (tickLoop.ts), never passed in. Round 1 of this compiler threaded a
% tick number through every statement (Log rels stamped with (tick, seq)
% columns; delta queries filtered `WHERE tick = ?`; edge writes resolved
% "latest occurrence this tick" via a self-join on that stamp) -- ALL of
% that depended on a value the seam does not provide. This is a genuine
% PLAN-TERM change, not a backend rendering fix: DeltaStatements' shape
% changed from a 5-field EXCEPT-query/refresh-table design to a 2-field
% "just read every row" design, EdgeStatements changed from a SQL self-join
% design to a "project one arrival row, resolve keys, upsert" design, and
% Log-rel DDL and its arrival rows dropped their stamp columns. Reported as a
% finding, not silently absorbed.
%
% The replacement strategy matches Phase A's own hand-carved exemplar
% (v6/tsv2/gen/*.ts) exactly, rather than inventing an untested alternative
% under time pressure: read every rel's full row list before a tick's
% writes and again after; the runtime's `multisetDiff` (runtime/diff.ts,
% REUSED per the reuse law, never reimplemented here) computes the add/del
% multiset difference -- ONE algorithm that is a plain set diff for Set/level
% rels and an occurrence-count diff for Log rels (duplicates handled
% correctly, no stamp column needed: engine.pl r7 "Log rels: one +Row per
% new stamp" is exactly "count in next minus count in prev" per distinct row
% value, since Log arrivals are append-only). This eliminates the `__prev`
% shadow tables and their refresh statements too -- DDL is simpler than
% round 1's, not just different.
%
% Edge-rule keyed replace is resolved the same way Phase A's exemplar
% resolves it: in JS, from the raw `arrivals` array directly (the trigger
% rel's fresh rows for this tick ARE the array elements tagged with that
% rel name -- no SQL-side "which rows are this tick's" question to answer at
% all once tick numbering is gone). lower.pl's job for an edge rule shrinks
% to two SQL fragments: ProjectSql (a parameterless-FROM `SELECT
% <head-expr-list>` that turns one arrival row's values, bound to numbered
% placeholders ?1..?N in trigger-argument order, into the head row shape --
% reuses compile_head_expr/head_select_list UNCHANGED, only the Bound source
% differs from round 1) and UpsertSql (`INSERT ... ON CONFLICT(<key cols>)
% DO UPDATE SET <non-key col> = excluded.<non-key col>`, matching the
% reference exemplar's ON CONFLICT idiom exactly). The emitter resolves
% last-write-wins and equal-row no-op in JS (a Map keyed by the head row's
% key-column values, natural overwrite-on-set semantics), matching
% apply_edge_writes/6's "across occurrences the later write wins" rule.
%
% Round 1 also had a LATENT bug `keyed(Ref, Positions)` positions are
% declared against Ref's OWN columns (the keyed rel's own arity), never the
% trigger atom's. Round 1's DELETE/INSERT self-join happened to index into
% TriggerColumns instead of HeadColumns for KeyColumns, which was silently
% correct only because switch_as_keyed_replace's trigger and head both have
% session_id at position 1 by coincidence. Round 2's KeyColumns is indexed
% off HeadColumns, the conceptually right list, fixed while rewriting this
% predicate anyway.
%
% ── representation of a structured column value (UNCHANGED from round 1) ──
% A term column (route_data(RouteId), obj(...), any compound) has no scalar
% SQL type, so it is stored as the SQLite json1 encoding
%   json_object('fn', <functor atom>, 'args', json_array(<arg exprs>))
% A body pattern that DESTRUCTURES a compound (route_view's
% `demanded(route_data(RouteId), _)`) compiles to a functor-tag equality
% (json_extract(col,'$.fn') = 'route_data') plus one json_extract(col,
% '$.args[N]') expression PER sub-argument position, which then binds like
% any other column expression. Phase A's hand-carved exemplar instead
% stores compound values as raw concatenated text (`route_data(settings)`)
% matched back via LIKE + substr with a compile-time functor-length offset
% -- both choices are valid IRow-value TEXT, and the seam does not prefer
% either; this compiler keeps json1 (more general: no per-functor constant
% baked into the matching SQL, verified working with sqlite3 3.43.2).
%
% ── keyed replace applies to edge writes and outside arrivals ───────────────
% engine.pl absorb_set_arrival/5 consults decl_key/3. A changed outside
% arrival into a keyed Set removes the old row with that key and adds the new
% row. The boundary therefore contains -Old followed by +New. A table backing
% a keyed arrival target uses PRIMARY KEY over the key columns, and both
% arrival execution families use a replace insert. The incremental family
% reads the rows at those keys before the write so it can stage the explicit
% minus rows required by the boundary log.
%
% ── acyclic-by-construction level recompute (UNCHANGED from round 1) ───────
% engine.pl re-derives a whole stratum GROUP to a joint fixpoint because
% relax_strata's Gap=0 rule lets two positively-dependent rules land in the
% SAME stratum number. strat.pl verified both target fixtures collapse to
% exactly one such group; sql_rule_order/2 topo-sorts within it. A single
% DELETE-then-INSERT-SELECT pass per rule in that order computes the SAME
% rows a joint fixpoint would for an ACYCLIC chain; a genuine positive cycle
% inside one group is refused at strat.pl:topo_order_group/2.

:- module(lower,
          [ lower_program/2, boot_statements/7,
            % The interning contract's mode vocabulary. compile.pl resolves
            % the compile option into the atom the plan term carries.
            intern_mode/2, interned_column/2, string_dictionary_table/1,
            program_text_intern_plan/3,
            % Both halves of the storage decision, exported so one test can
            % compare the DDL's answer against the IR's on ONE run.
            column_def/4, ir_column_class/4, uniform_text_encoding/1,
            compile_expr/7, compile_comparison/4,
            intern_write_sql/4,
            canonical_column_expr/2, canonical_column_expr/3,
            semantic_generic/4, semantic_generic_instance/4,
            level_ref_count_sql/5, level_dred_plan/5,
            % The departure frontier's table name (TICK PHASE ALIGNMENT target
            % 2). emit_ts.pl renders both the relation-plan field and the
            % departure arm's SELECT, and the name has exactly one definition.
            departure_frontier_table_name/2, departure_read_sql/3,
            % The rule naming every emitter renders into its statement plan,
            % defined once so a second emitter reads it instead of guessing.
            statement_rule_ids/3,
            % frontier(shared): shared frontier tables behind per-rel views,
            % plans/2026-08-19-shared-sqlite-frontier.md.
            frontier_mode/1, with_frontier_mode/2,
            shared_frontier_relation_id/2, shared_frontier_relation_id/3,
            lowered_program_data/2, lowered_program_data/3, write_verb/1,
            % STRUCT-AS-ROWS: the storage plane's own names, exported so the
            % emitter can render the intern plan and the plunit units can pin
            % the exact SQL text.
            dictionary_table_name/2, dictionary_render_expr/3,
            struct_type_plans/3, struct_type_plans/4,
            % The compiler half of the json capture-type table, exported for
            % the unit that pins it equal to body.pl:json_capture_type/2.
            json_capture_json_type/2,
            % The catalog's column contract, read by compile.pl so the ordinary
            % rel path builds the table from real decls instead of caller spellings.
            catalog_ddl_contract/2,
            % The same rows the catalog INSERT renders, read by emit_ts.pl.
            catalog_rows/4,
            catalog_all_rows/10,
            % The type artifact needs declaration rows plus the per-column
            % storage representation that distinguishes a followed ref from
            % an identity-only endpoint.
            catalog_type_rows/6,
            catalog_type_relation_rows/3,
            catalog_type_transport_rows/4,
            % The decl half alone, with the id layout the nesting rail reads.
            catalog_decl_rows/6,
            % Reproduce the RuleLevelStatements input the producer needs, used
            % by the catalog rail to plan rows outside lower_program/2.
            plan_rule_level_statements/2,
            % One number for every fixpoint walk on either door: the wavefront
            % hop cap AND the stratum-group outer-round cap.
            fixpoint_round_cap/1,
            % The `?` order tail's SQL, read by both emitters so the clause
            % they append to final_select has one definition.
            query_order_by_map/3,
            % issues/inner-scan-audit: exported so the plunit unit pins the
            % derived (rel, column) pairs and the DDL text directly.
            audit_scan_index_pairs/5, audit_scan_index_ddls/5,
            audit_scan_index_ddl/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module(analyze).
:- use_module('next/0_parse/use_resolve', [short_hash/2]).
:- use_module('0_rel_record').
:- use_module('next/1_expand/0_generic_expand', [canonical_type_name/2,
                                   type_relation_rows/2,
                                   schema_member_transport_rows/3]).
:- use_module('0_type_ids', [id_kind_name/3, semantic_type_id_text/2]).
:- use_module('0_option_expand', [acyclic_companion/5]).
:- use_module('1_host_expand', [query_decl/3]).
:- use_module('compile/registry', [expression/5, surface/5, body_surface_for_term/6]).
:- use_module('0_type_plane',
              [ type_definition/4, column_storage/3,
                type_topological_order/2, type_canonical_json/4,
                type_field_values/4, declared_type_name/2,
                relation_columns_and_types/5, relation_value_shape/3,
                relation_value_term/4, canonical_json_text/2 ]).
:- use_module('next/1_expand/0_body_walk', [walk_body/3, body_relation_atoms/4]).
:- use_module('conformance/body', [rel_ref/2]).
% run_compile_step/4 lives in 0_trace, never compile.pl: compile.pl imports
% this module, so importing it from there is a cycle.
:- use_module('compile/0_trace', [run_compile_step/4]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ identifiers ═════════════════════════════════════════════════════════════

% The storage map is carried by rel/5 in the compiler IR.  Lowering keeps a
% short-lived thread-local projection because the existing SQL helpers receive
% semantic Ref values, often far below the RelPlans argument that owns the
% map.  It is installed only around lower_program/2 and boot_statements/7;
% direct helper units retain the old Ref -> Name fallback.
:- thread_local physical_storage_name/2.

with_storage_context(RelPlans, Goal) :-
    findall(Ref-StorageName,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, _, _, _),
              relplan_storage_name(RelPlan, StorageName) ),
            Names),
    setup_call_cleanup(
        maplist(assert_storage_name, Names),
        Goal,
        maplist(retract_storage_name, Names)).

assert_storage_name(Ref-StorageName) :- assertz(physical_storage_name(Ref, StorageName)).
retract_storage_name(Ref-StorageName) :- retractall(physical_storage_name(Ref, StorageName)).

:- thread_local frontier_mode_option/1.
:- thread_local shared_frontier_relation_id_fact/2.

:- meta_predicate with_frontier_mode(+, 0).
:- meta_predicate with_shared_frontier_ids(+, 0).

frontier_mode(Mode) :-
    ( frontier_mode_option(Chosen) -> Mode = Chosen ; Mode = per_rel ).

with_frontier_mode(per_rel, Goal) :- !, call(Goal).
with_frontier_mode(shared, Goal) :-
    setup_call_cleanup(
        assertz(frontier_mode_option(shared)),
        Goal,
        retractall(frontier_mode_option(_))).

% Relation ids are RelPlans order; every door numbers the same way.
shared_frontier_relation_id(Ref, RelationId) :-
    shared_frontier_relation_id_fact(Ref, RelationId).

shared_frontier_relation_id(RelPlans, Ref, RelationId) :-
    nth0(RelationId, RelPlans, RelPlan),
    relplan_parts(RelPlan, Ref, _, _, _, _),
    !.

with_shared_frontier_ids(RelPlans, Goal) :-
    ( frontier_mode(shared)
    -> findall(Ref-Id,
               ( nth0(Id, RelPlans, RelPlan),
                 relplan_parts(RelPlan, Ref, _, _, _, _) ),
               Pairs),
       setup_call_cleanup(
           forall(member(Ref-Id, Pairs),
                  assertz(shared_frontier_relation_id_fact(Ref, Id))),
           Goal,
           retractall(shared_frontier_relation_id_fact(_, _)))
    ;  call(Goal)
    ).

shared_frontier_table('__frontier').
shared_next_frontier_table('__next_frontier').

% Plain heaps plus one (relation_id, _phase) index, the shape the per-rel
% tables had; row identity is the durable row's __id.
shared_frontier_ddl(
    [ 'CREATE TEMP TABLE "__frontier" ("relation_id" INTEGER NOT NULL, "_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)',
      'CREATE INDEX "__frontier_rel_phase" ON "__frontier" ("relation_id", "_phase")',
      'CREATE TEMP TABLE "__next_frontier" ("relation_id" INTEGER NOT NULL, "_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)',
      'CREATE INDEX "__next_frontier_rel" ON "__next_frontier" ("relation_id")',
      'CREATE TEMP TABLE "__support_count" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL, PRIMARY KEY ("relation_id", "row_id", "rule_id")) WITHOUT ROWID'
    ]).

shared_support_table('__support_count').

shared_frontier_view_ddl(Ref, Columns, [FrontierView, NextFrontierView]) :-
    shared_frontier_relation_id(Ref, RelationId),
    table_name(Ref, Table),
    quote_ident(Table, QuotedTable),
    frontier_table_name(Ref, FrontierName),
    quote_ident(FrontierName, QuotedFrontierName),
    next_frontier_table_name(Ref, NextName),
    quote_ident(NextName, QuotedNextName),
    findall(Part,
            ( member(Column, Columns),
              quote_ident(Column, QuotedColumn),
              format(atom(Part), 't.~w AS ~w', [QuotedColumn, QuotedColumn]) ),
            Parts),
    atomic_list_concat(Parts, ', ', PayloadSql),
    format(atom(FrontierView),
           'CREATE TEMP VIEW ~w AS SELECT f."_phase" AS "_phase", f."_sequence" AS "_sequence", ~w FROM "__frontier" f JOIN ~w t ON t."__id" = f."row_id" WHERE f."relation_id" = ~w',
           [QuotedFrontierName, PayloadSql, QuotedTable, RelationId]),
    format(atom(NextFrontierView),
           'CREATE TEMP VIEW ~w AS SELECT f."_phase" AS "_phase", f."_sequence" AS "_sequence", ~w FROM "__next_frontier" f JOIN ~w t ON t."__id" = f."row_id" WHERE f."relation_id" = ~w',
           [QuotedNextName, PayloadSql, QuotedTable, RelationId]).

table_name(Ref, Table) :-
    ( physical_storage_name(Ref, Table) -> true ; Ref = Table/_ ).

delta_table_name(Ref, DeltaTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__delta_', Table], DeltaTable).

frontier_table_name(Ref, FrontierTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__frontier_', Table], FrontierTable).

next_frontier_table_name(Ref, NextFrontierTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__next_frontier_', Table], NextFrontierTable).

pre_table_name(Ref, PreTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__pre_', Table], PreTable).

% Last tick's net -delta rows of a rel some rule binds with finalize/1
% (engine.pl tick/7's DepartureCarry). Emitted ONLY for those rels
% (analyze:listened_departure_refs/2), which is what keeps a program with no
% finalize in it byte-identical to what the previous emitter wrote. Same
% column shape as the arrival frontier on purpose: the arm's delta SELECT is
% then the SAME text with one table name swapped, so no new SQL shape enters
% the emitter and the existing EXPLAIN receipts still describe it.
departure_frontier_table_name(Ref, DepartureTable) :-
    table_name(Ref, Table),
    format(atom(DepartureTable), '__departure_frontier_~w', [Table]).

% The runtime fills the departure frontier from the tick's boundary delta,
% whose rows already crossed the decoded text view: characters under any mode.
trigger_read_mode(departure, _, direct) :- !.
trigger_read_mode(ordered_departure, _, direct) :- !.
trigger_read_mode(_, Mode, Mode).

% The naive referee's read of that table: the departed rows in staged order,
% one occurrence each. Built HERE and not in emit_ts.pl because every other
% statement the emitter renders is lowered text it only quotes into a
% template -- the emitter builds identifiers, never SQL.
departure_read_sql(Ref, Columns, Sql) :-
    departure_frontier_table_name(Ref, DepartureTable),
    quote_ident(DepartureTable, QuotedDepartureTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Sql), 'SELECT ~w FROM ~w ORDER BY "_phase", "_sequence"',
           [ColumnsSql, QuotedDepartureTable]).

% ═══ rule identity ══════════════════════════════════════════════════════════
% "<program>:<name>/<arity>#<ordinal>", the name a trace line uses to say WHICH
% rule fired rather than how many statements ran.
%
% Ordinal is 1-based among the LOWERED STATEMENTS sharing a head ref, in
% lowering order, which is not the same as clauses in the source: an edge rule
% lowers to one statement per arm and gets one ordinal each, while a level
% head's clauses fold into a single UNION'd insert (level_statement_group/3
% hands the emitter a LIST under one head) and it is always #1. Separating
% those would be a change to the plan, not to the naming.
%
% Stable under edits elsewhere in the file; it moves when two arms of one head
% are reordered, which is the honest answer, since only their order tells them
% apart. Built here rather than in emit_ts.pl so a second emitter reads the
% numbering instead of reimplementing it.
statement_rule_ids(Program, HeadRefs, RuleIds) :-
    statement_ordinals(HeadRefs, [], Ordinals),
    maplist(rule_id(Program), HeadRefs, Ordinals, RuleIds).

rule_id(Program, Name/Arity, Ordinal, RuleId) :-
    format(atom(RuleId), '~w:~w/~w#~w', [Program, Name, Arity, Ordinal]).

statement_ordinals([], _, []).
statement_ordinals([Ref | Rest], Seen0, [Ordinal | More]) :-
    (   selectchk(Ref-Previous, Seen0, Seen1)
    ->  Ordinal is Previous + 1
    ;   Ordinal = 1, Seen1 = Seen0
    ),
    statement_ordinals(Rest, [Ref-Ordinal | Seen1], More).

ref_count_table_name(Ref, RefCountTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__support_next_', Table], RefCountTable).

% Non-atom names reach this from catalog rows, where write/1's rendering is
% the emitted byte; atomic_list_concat/2 rejects them.
quote_ident(Name, Quoted) :-
    (   atom(Name)
    ->  atomic_list_concat(['"', Name, '"'], Quoted)
    ;   format(atom(Quoted), '"~w"', [Name])
    ).

sql_literal(Atom, Literal) :-
    atomic(Atom),
    ( number(Atom)
    -> ( float(Atom)
       -> float_class(Atom, Class),
          ( memberchk(Class, [normal, subnormal, zero])
          -> format(atom(Literal), '~h', [Atom])
          ; throw(unsupported_construct(non_finite_float_literal(Atom))) )
       ; format(atom(Literal), '~w', [Atom]) )
    ;  sql_text_literal(Atom, Literal) ).
sql_literal(bool_lit(true), '1') :- !.
sql_literal(bool_lit(false), '0') :- !.

% ═══ pattern-argument compiler (level-rule bodies; unchanged from round 1) ══
% Binding = bind | check; a check (negated atom) never introduces a binding, so
% an unbound var there imposes no condition.

% EXPRESSION LIFT: a Bound entry is now typed(Sql, int|text), not bare Sql.
% lower.pl has to know a bound variable's SQL TYPE, not only its text, for
% three reasons the phase-C sweep documented as miscompiles: `/` is integer
% division only when both operands are INTEGER storage class, `mod` needs the
% floored correction only over integers, and a comparison between an
% INTEGER-affinity column and a TEXT one silently applies affinity conversion
% where the oracle's ==/2 is term identity. Carrying the type beside the text
% is what lets each of those be a NAMED unsupported construct instead of a silent answer.
compile_pattern_arg(Mode, Arg, ColumnExpr, ColumnType, Bound0, Bound, WhereParts, Binding) :-
    column_encoding(Mode, ColumnType, Encoding),
    ( var(Arg)
    -> ( bound_lookup(Bound0, Arg, typed(Existing, ExistingType, ExistingEncoding))
       -> join_column_types_agree(ColumnExpr, ColumnType, Existing, ExistingType),
          aligned_pair(Encoding, ColumnExpr, ExistingEncoding, Existing,
                       AlignedColumn, AlignedExisting),
          WhereParts = [pair(AlignedColumn, AlignedExisting)], Bound = Bound0
       ; Binding == bind
       -> WhereParts = [], Bound = [Arg-typed(ColumnExpr, ColumnType, Encoding) | Bound0]
       ; WhereParts = [], Bound = Bound0
       )
    ; Arg = bool_lit(_)
    -> WhereParts = [lit(ColumnExpr, Arg, Encoding)], Bound = Bound0
    ; compound(Arg)
    -> Arg =.. [Functor | SubArgs],
       % json_extract reads the term's characters, so the operand is `value`
       % demand: over a dict column's id every path answers NULL.
       demanded_sql(value, Encoding, ColumnExpr, TermExpr, _TermEncoding),
       FnCheck = pair_lit(TermExpr, Functor),
       compile_sub_args(Mode, SubArgs, TermExpr, 0, Bound0, Bound, MoreWhere, Binding),
       WhereParts = [FnCheck | MoreWhere]
    ; atomic(Arg)
    -> WhereParts = [lit(ColumnExpr, Arg, Encoding)], Bound = Bound0
    ; throw(unsupported_construct(pattern_arg(Arg)))
    ).

% A json_extract-bound variable carries characters while a stored text column
% under dict carries an id; the characters resolve so both sides are ids.
aligned_pair(LeftEncoding, LeftSql, RightEncoding, RightSql, AlignedLeft, AlignedRight) :-
    align_to_encoding(RightEncoding, LeftEncoding, LeftSql, AlignedLeft),
    align_to_encoding(LeftEncoding, RightEncoding, RightSql, AlignedRight).

align_to_encoding(dict, direct, Sql, Aligned) :- !, interned_id_sql(Sql, Aligned).
align_to_encoding(_, _, Sql, Sql).

% A shared variable across two columns of DIFFERENT storage type is the same
% TEXT-collapse hazard as a cross-type comparison, one hop out: engine.pl
% joins by UNIFICATION, where the atom '1' and the integer 1 are distinct
% terms and never join. SQLite applies affinity conversion instead -- with a
% TEXT-affinity column on one side and an INTEGER one on the other, the text
% operand is converted, so `'1' = 1` is TRUE and the two rows join.
%
% MEASURED through the real driver, not assumed (sqlite 3.45.1):
%   SELECT b0."value" FROM "label" b0, "number" b1 WHERE b0."value" = b1."value"
%   with label.value = TEXT '1' and number.value = INTEGER 1
%   -> one row {"v":"1","t0":"text","t1":"integer"}
% where the oracle derives NOTHING. Refused by name; this is the Q4 P1.2 /
% P1.8 assertion from the sqlite_udf verdict ("text `1` and numeric `1` remain
% distinct", "comparison and arithmetic use typed SQLite values, not rendered
% text") turned into a compiler guard, and
% conformance/expressions.pl:text_one_and_numeric_one_never_join is the
% fixture that pins the oracle side of it.
join_column_types_agree(_, ColumnType, _, ExistingType) :-
    ColumnType == ExistingType, !.
% A list column IS its entity id, so the member rel's int `list_id` and the
% column hold one stored value; no affinity conversion is in reach.
join_column_types_agree(_, list(_), _, int) :- !.
join_column_types_agree(_, int, _, list(_)) :- !.
join_column_types_agree(ColumnExpr, ColumnType, Existing, ExistingType) :-
    throw(unsupported_construct(
        join_column_type_mismatch(ColumnExpr, ColumnType, Existing, ExistingType))).

% A destructured sub-argument comes back through json_extract, whose result
% carries no declared column type at all -- typed text, matching the
% inline-flat compound punt (PHASE C2 RULING 1).
compile_sub_args(_, [], _, _, Bound, Bound, [], _).
compile_sub_args(Mode, [SubArg | Rest], ParentExpr, Index, Bound0, Bound, WhereParts, Binding) :-
    format(atom(SubExpr), 'json_extract(~w, \'$.args[~w]\')', [ParentExpr, Index]),
    compile_pattern_arg(direct, SubArg, SubExpr, text, Bound0, Bound1, HereWhere, Binding),
    NextIndex is Index + 1,
    compile_sub_args(Mode, Rest, ParentExpr, NextIndex, Bound1, Bound, MoreWhere, Binding),
    append(HereWhere, MoreWhere, WhereParts).

% NOTE the two distinct names: the head pattern's pair value (PairExpr) must
% stay separate from the output parameter (Expr) -- reusing one name for both
% (round 1's first draft did) unifies the output with the FIRST pair's value
% at head-unification time regardless of which branch fires, so every lookup
% past the first list element fails outright. This is exactly the bug class
% the descriptive-names style law exists to prevent.
bound_lookup([Var-PairExpr | Rest], Target, Expr) :-
    ( Var == Target -> Expr = PairExpr ; bound_lookup(Rest, Target, Expr) ).

where_text(pair(Left, Right), Text) :- format(atom(Text), '~w = ~w', [Left, Right]).
where_text(pair_lit(Left, Functor), Text) :-
    sql_literal(Functor, Quoted),
    format(atom(Text), 'json_extract(~w, \'$.fn\') = ~w', [Left, Quoted]).
where_text(lit(Left, Value, Encoding), Text) :-
    column_literal_sql(Encoding, Value, Resolved),
    format(atom(Text), '~w = ~w', [Left, Resolved]).

% ═══ positive body-atom compilation (level rules only, round 2: edge rules
% no longer use this -- see compile_trigger_bound/2 below) ═══════════════

compile_positive_uses(Mode, RelPlans, Uses, Bound0, Bound, FromParts, WhereTexts) :-
    compile_positive_uses(Mode, RelPlans, Uses, 0, Bound0, Bound, FromParts, WhereParts),
    maplist(where_text, WhereParts, WhereTexts).

compile_positive_uses(_, _, [], _, Bound, Bound, [], []).
compile_positive_uses(Mode, RelPlans,
                      [use(Ref, Args, pos, seeded_pre(Seed)) | Rest], Index,
                      Bound0, Bound, MoreFrom, WhereParts) :-
    compile_seeded_pre_use(Mode, RelPlans, Ref, Args, Seed, Index, Bound0,
                           Bound1, HereWhere),
    NextIndex is Index + 1,
    compile_positive_uses(Mode, RelPlans, Rest, NextIndex, Bound1, Bound,
                          MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).
compile_positive_uses(Mode, RelPlans,
                      [use(Ref, Args, pos, coalesce(Output, Default)) | Rest],
                      Index, Bound0, Bound,
                      [left_join(Join) | MoreFrom], WhereParts) :-
    !,
    compile_coalesced_use(Mode, RelPlans, Ref, Args, Output, Default, current,
                          Index, Bound0, Bound1, Join, HereWhere),
    NextIndex is Index + 1,
    compile_positive_uses(Mode, RelPlans, Rest, NextIndex, Bound1, Bound,
                          MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).
compile_positive_uses(Mode, RelPlans,
                      [use(Ref, Args, pos,
                           old_state(coalesce(Output, Default))) | Rest],
                      Index, Bound0, Bound,
                      [left_join(Join) | MoreFrom], WhereParts) :-
    !,
    compile_coalesced_use(Mode, RelPlans, Ref, Args, Output, Default,
                          old_state(current), Index, Bound0, Bound1, Join,
                          HereWhere),
    NextIndex is Index + 1,
    compile_positive_uses(Mode, RelPlans, Rest, NextIndex, Bound1, Bound,
                          MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).
compile_positive_uses(Mode, RelPlans, [use(Ref, Args, pos, Source) | Rest], Index, Bound0, Bound, [From | MoreFrom], WhereParts) :-
    format(atom(Alias), 'b~w', [Index]),
    positive_use_from(Source, RelPlans, Ref, Alias, From),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    compile_atom_args(Mode, Args, Columns, ColumnTypes, Alias, Bound0, FieldBound, HereWhere),
    bind_reference_target_identity(RelPlans, Ref, Args, Alias,
                                   FieldBound, Bound1),
    NextIndex is Index + 1,
    compile_positive_uses(Mode, RelPlans, Rest, NextIndex, Bound1, Bound, MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

compile_coalesced_use(Mode, RelPlans, Ref, Args, Output, Default, Source, Index,
                      Bound0, Bound, Join, []) :-
    coalesced_relation_sql(RelPlans, Ref, Source, RelationSql),
    format(atom(Alias), 'b~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    compile_coalesced_args(Mode, Args, Columns, ColumnTypes, Alias, Output,
                           Bound0, OnParts, none, OutputColumn, OutputType,
                           OutputEncoding),
    maplist(where_text, OnParts, OnTexts),
    (   OnTexts == []
    ->  OnSql = '1'
    ;   atomic_list_concat(OnTexts, ' AND ', OnSql)
    ),
    compile_expr(Mode, identity, Default, Bound0, DefaultSql, DefaultType,
                 DefaultEncoding),
    join_column_types_agree(OutputColumn, OutputType, DefaultSql, DefaultType),
    align_to_encoding(OutputEncoding, DefaultEncoding, DefaultSql,
                      AlignedDefaultSql),
    format(atom(OutputSql), 'COALESCE(~w, ~w)',
           [OutputColumn, AlignedDefaultSql]),
    Bound = [Output-typed(OutputSql, OutputType, OutputEncoding) | Bound0],
    format(atom(Join), '~w ~w ON ~w', [RelationSql, Alias, OnSql]).

coalesced_relation_sql(RelPlans, Ref, current, QuotedTable) :-
    relplan_storage_name(RelPlans, Ref, Table),
    quote_ident(Table, QuotedTable).
coalesced_relation_sql(RelPlans, Ref, old_state(Source), RelationSql) :-
    old_state_relation_sql(Source, RelPlans, Ref, RelationSql).

compile_coalesced_args(_, [], [], [], _, _, _, OnParts, some(OutputColumn,
                        OutputType, OutputEncoding), OutputColumn, OutputType,
                        OutputEncoding) :-
    !,
    OnParts = [].
compile_coalesced_args(Mode, [Arg | RestArgs], [Column | RestColumns],
                       [ColumnType | RestTypes], Alias, Output, Bound,
                       OnParts, Output0, OutputColumn, OutputType,
                       OutputEncoding) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    (   Arg == Output
    ->  coalesced_output_column(Mode, ColumnExpr, ColumnType, Output0, Output1,
                                HereOn)
    ;   compile_pattern_arg(Mode, Arg, ColumnExpr, ColumnType, Bound, _,
                            HereOn, check),
        Output1 = Output0
    ),
    compile_coalesced_args(Mode, RestArgs, RestColumns, RestTypes, Alias,
                           Output, Bound, MoreOn, Output1, OutputColumn,
                           OutputType, OutputEncoding),
    append(HereOn, MoreOn, OnParts).

coalesced_output_column(Mode, ColumnExpr, ColumnType, none,
                        some(ColumnExpr, ColumnType, Encoding), []) :-
    column_encoding(Mode, ColumnType, Encoding).
coalesced_output_column(_, ColumnExpr, ColumnType,
                        some(FirstExpr, FirstType, Encoding),
                        some(FirstExpr, FirstType, Encoding),
                        [pair(ColumnExpr, FirstExpr)]) :-
    join_column_types_agree(ColumnExpr, ColumnType, FirstExpr, FirstType).

from_parts_sql([left_join(Join) | Rest], Sql) :-
    !,
    format(atom(First), '(SELECT 1) "__coalesce_root" LEFT JOIN ~w', [Join]),
    foldl(append_from_part, Rest, First, Sql).
from_parts_sql([Part | Rest], Sql) :-
    from_part_text(Part, First),
    foldl(append_from_part, Rest, First, Sql).

from_part_text(left_join(Join), Text) :-
    !,
    format(atom(Text), 'LEFT JOIN ~w', [Join]).
from_part_text(Part, Part).

append_from_part(left_join(Join), Before, Sql) :-
    !,
    format(atom(Sql), '~w LEFT JOIN ~w', [Before, Join]).
append_from_part(Part, Before, Sql) :-
    format(atom(Sql), '~w, ~w', [Before, Part]).

positive_use_table(pre, _RelPlans, Ref, Table) :- !, pre_table_name(Ref, Table).
positive_use_table(_, RelPlans, Ref, Table) :-
    relplan_storage_name(RelPlans, Ref, Table).

positive_use_from(old_state(Source), RelPlans, Ref, Alias, From) :-
    !,
    old_state_relation_sql(Source, RelPlans, Ref, RelationSql),
    format(atom(From), '~w ~w', [RelationSql, Alias]).
positive_use_from(Source, RelPlans, Ref, Alias, From) :-
    positive_use_table(Source, RelPlans, Ref, Table),
    quote_ident(Table, QuotedTable),
    format(atom(From), '~w ~w', [QuotedTable, Alias]).

old_state_relation_sql(Source, RelPlans, Ref, RelationSql) :-
    positive_use_table(Source, RelPlans, Ref, Table),
    quote_ident(Table, QuotedTable),
    frontier_table_name(Ref, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    relplan_columns(RelPlans, Ref, Columns),
    qualified_equalities(Columns, old_delta, old_row, FrontierEqualities),
    old_state_projection_columns(Source, RelPlans, Ref, Columns,
                                 ProjectionColumns),
    qualified_column_list(ProjectionColumns, old_row, SelectedColumns),
    old_state_frontier_where(FrontierEqualities, FrontierWhere),
    format(atom(RelationSql),
           '(SELECT ~w FROM ~w old_row GROUP BY ~w HAVING count(*) > (SELECT count(*) FROM ~w old_delta WHERE old_delta."_phase" >= 0 AND ~w))',
           [SelectedColumns, QuotedTable, SelectedColumns,
            QuotedFrontierTable, FrontierWhere]).

old_state_frontier_where([], '1').
old_state_frontier_where(Equalities, Where) :-
    Equalities = [_ | _],
    atomic_list_concat(Equalities, ' AND ', Where).

old_state_projection_columns(pre, _, _, Columns, Columns) :- !.
old_state_projection_columns(_, RelPlans, Ref, Columns, ProjectionColumns) :-
    (   reference_target_ref(RelPlans, Ref)
    ->  ProjectionColumns = ['__id' | Columns]
    ;   ProjectionColumns = Columns
    ).

compile_seeded_pre_use(Mode, RelPlans, Ref, Args, Seed, Index, Bound0, Bound,
                       WhereParts) :-
    pre_table_name(Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'b~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    seeded_pre_args(Mode, Args, Columns, ColumnTypes, Alias, Bound0,
                    KeyWhere, Before, BeforeColumn, BeforeType),
    compile_expr(Mode, value, Seed, Bound0, SeedSql, SeedType, SeedEncoding),
    ( SeedType == BeforeType -> true
    ; throw(unsupported_construct(pre_seed_type_mismatch(Seed, SeedType, BeforeType)))
    ),
    column_encoding(Mode, BeforeType, BeforeEncoding),
    align_to_encoding(BeforeEncoding, SeedEncoding, SeedSql, AlignedSeedSql),
    maplist(where_text, KeyWhere, KeyWhereTexts),
    ( KeyWhereTexts == [] -> WhereSql = ''
    ; atomic_list_concat(KeyWhereTexts, ' AND ', Joined),
      format(atom(WhereSql), ' WHERE ~w', [Joined])
    ),
    format(atom(SelectSql), '(SELECT ~w."~w" FROM ~w ~w~w)',
           [Alias, BeforeColumn, QuotedTable, Alias, WhereSql]),
    format(atom(ValueSql), 'COALESCE(~w, ~w)', [SelectSql, AlignedSeedSql]),
    Bound = [Before-typed(ValueSql, BeforeType, BeforeEncoding) | Bound0],
    WhereParts = [].

seeded_pre_args(_, [], [], [], _, _, _, _, _, _) :-
    throw(unsupported_construct(pre_seed_no_value)).
seeded_pre_args(Mode, [Arg | Args], [Column | Columns], [Type | Types], Alias,
                Bound0, KeyWhere, Before, BeforeColumn, BeforeType) :-
    ( var(Arg), \+ bound_lookup(Bound0, Arg, _)
    -> Args = [], Columns = [], Types = [], KeyWhere = [], Before = Arg,
       BeforeColumn = Column, BeforeType = Type
    ; format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
      compile_pattern_arg(Mode, Arg, ColumnExpr, Type, Bound0, _,
                          HereWhere, check),
      seeded_pre_args(Mode, Args, Columns, Types, Alias, Bound0,
                      RestWhere, Before, BeforeColumn, BeforeType),
      append(HereWhere, RestWhere, KeyWhere)
    ).

% A public relation that appears as another relation's column domain has a
% hidden dense __id. Bind the complete body atom to that endpoint while its
% ordinary fields remain bound independently. A head value with the same
% relation-shaped term can then project the already-joined row identity
% instead of manufacturing JSON or performing a hidden target write.
bind_reference_target_identity(RelPlans, Name/Arity, Args, Alias,
                               Bound0, Bound) :-
    reference_target_ref(RelPlans, Name/Arity),
    !,
    length(Args, Arity),
    Atom =.. [Name | Args],
    format(atom(IdExpr), '~w."__id"', [Alias]),
    Bound = [Atom-typed(IdExpr, ref(Name), direct) | Bound0].
bind_reference_target_identity(_, _, _, _, Bound, Bound).

compile_atom_args(_, [], [], [], _, Bound, Bound, []).
compile_atom_args(Mode, [Arg | RestArgs], [Column | RestColumns], [ColumnType | RestTypes],
                  Alias, Bound0, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Mode, Arg, ColumnExpr, ColumnType, Bound0, Bound1, HereWhere, bind),
    compile_atom_args(Mode, RestArgs, RestColumns, RestTypes, Alias, Bound1, Bound, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

% ═══ negative body-atom compilation (NOT EXISTS; unchanged from round 1) ════

compile_negative_uses(Mode, RelPlans, Uses, Bound, NegTexts) :-
    compile_negative_uses(Mode, RelPlans, Uses, 0, Bound, NegTexts).

compile_negative_uses(_, _, [], _, _, []).
compile_negative_uses(Mode, RelPlans,
                      [use(_, _, neg, coalesce_recount) | Rest], Index, Bound,
                      More) :-
    !,
    compile_negative_uses(Mode, RelPlans, Rest, Index, Bound, More).
compile_negative_uses(Mode, RelPlans, [use(Ref, Args, neg, _) | Rest], Index, Bound, [Text | More]) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'n~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    compile_negative_atom_args(Mode, Args, Columns, ColumnTypes, Alias, Bound, WhereParts),
    maplist(where_text, WhereParts, WhereTexts),
    ( WhereTexts == []
    -> format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w)', [QuotedTable, Alias])
    ; atomic_list_concat(WhereTexts, ' AND ', Joined),
      format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w WHERE ~w)', [QuotedTable, Alias, Joined])
    ),
    NextIndex is Index + 1,
    compile_negative_uses(Mode, RelPlans, Rest, NextIndex, Bound, More).

% The runtime classifies refCount invalidation from NOT EXISTS spans in
% support_sql[1]; an outer-join source changes both its matched and absent rows.
compile_coalesce_recount_markers(RelPlans, Uses, Texts) :-
    compile_coalesce_recount_markers(RelPlans, Uses, 0, Texts).

compile_coalesce_recount_markers(_, [], _, []).
compile_coalesce_recount_markers(RelPlans,
                                 [use(Ref, _, neg, coalesce_recount) | Rest],
                                 Index, [Text | More]) :-
    !,
    table_name(Ref, Table),
    quote_ident(Table, QuotedTable),
    format(atom(Alias), 'c~w', [Index]),
    format(atom(Text),
           'NOT EXISTS (SELECT 1 FROM ~w ~w WHERE 0)',
           [QuotedTable, Alias]),
    NextIndex is Index + 1,
    compile_coalesce_recount_markers(RelPlans, Rest, NextIndex, More).
compile_coalesce_recount_markers(RelPlans, [_ | Rest], Index, More) :-
    compile_coalesce_recount_markers(RelPlans, Rest, Index, More).

compile_negative_atom_args(_, [], [], [], _, _, []).
compile_negative_atom_args(Mode, [Arg | RestArgs], [Column | RestColumns], [ColumnType | RestTypes],
                           Alias, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Mode, Arg, ColumnExpr, ColumnType, Bound, _BoundUnused, HereWhere, check),
    compile_negative_atom_args(Mode, RestArgs, RestColumns, RestTypes, Alias, Bound, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

% ═══ head expression compilation (unchanged from round 1; reused for BOTH
% level rules, via table-alias Bound, and edge rules, via numbered-
% placeholder Bound -- compile_head_expr/3 does not care where a bound
% variable's SQL text came from) ═════════════════════════════════════════

% compile_expr(+Expr, +Bound, -Sql, -Type) : the ONE expression compiler,
% used for head arguments, `:=` right-hand sides, comparison operands and
% aggregate arguments alike. Mirrors engine.pl:eval_expr/2 clause for clause
% (ruling expression_residency: fuse to SQL, deopt to TypeScript only where
% sqlite genuinely lacks the function -- nothing here needs a deopt).
%
% Three places where the naive translation is WRONG and this is not:
%
%  `/`   engine.pl eval_expr's `LeftV // RightV` truncates TOWARD ZERO (SWI's
%        default integer_rounding_function). SQLite's `/` on two INTEGER
%        operands does the same (measured against sqlite 3.45.1 through the
%        real driver: -7/2 = -3, 7/-2 = -3), so the operator maps straight
%        across -- but ONLY while both operands really are INTEGER storage
%        class. A TEXT-affinity operand turns it into float division, which
%        is why a non-int operand is a named unsupported construct below rather than a
%        cast.
%  `mod` engine.pl uses Prolog `mod`, which is FLOORED (sign of the DIVISOR):
%        7 mod -2 = -1, -7 mod 2 = 1. SQLite's `%` is C's (sign of the
%        DIVIDEND): 7 % -2 = 1, -7 % 2 = -1. Emitting `%` directly gets the
%        sign wrong on exactly the two rows
%        division_truncates_toward_zero_mod_follows_divisor_sign grades.
%        The floored correction ((A % B) + B) % B reproduces all four
%        (measured, same run).
%  Int-only  eval_int2/4 THROWS arith_on_non_int the moment a non-integer
%        operand appears. SQLite instead coerces silently ('not_a_number' + 1
%        evaluates to 1), so a text operand would produce a wrong row with no
%        signal at all. Refused by name.

% Demand (contract §5.3): `identity` positions store and compare ids, `value`
% positions read the characters. Encoding says which one Sql came out as.
compile_expr(Mode, Demand, Expr, Bound, Sql, Type, Encoding) :-
    ( var(Expr)
    -> ( bound_lookup(Bound, Expr, typed(BoundSql, Type, BoundEncoding))
       -> demanded_sql(Demand, BoundEncoding, BoundSql, Sql, Encoding)
       ;  throw(unsupported_construct(unbound_head_var(Expr))) )
    ; Expr = bool_lit(_)
    -> sql_literal(Expr, Sql), Type = bool, Encoding = direct
    ; compound(Expr),
      bound_lookup(Bound, Expr, typed(BoundSql, Type, BoundEncoding))
    -> demanded_sql(Demand, BoundEncoding, BoundSql, Sql, Encoding)
    ; integer(Expr)
    -> sql_literal(Expr, Sql), Type = int, Encoding = direct
    ; float(Expr)
    -> sql_literal(Expr, Sql), Type = float, Encoding = direct
    ; atomic(Expr)
    -> text_literal_sql(Mode, Demand, Expr, Sql, Encoding), Type = text
    ; Expr = concat(Parts)
    -> compile_concat_parts(Mode, Parts, Bound, Expr, PartSqls),
       atomic_list_concat(PartSqls, ' || ', Joined),
       format(atom(Sql), '(~w)', [Joined]),
       Type = text, Encoding = direct
    ; text_scalar_expr(Expr, Function, Arguments)
    -> compile_text_operands(Mode, Arguments, Bound, Expr, ArgumentSqls),
       text_scalar_sql(Function, ArgumentSqls, Sql),
       Type = text, Encoding = direct
    ; typed_scalar_expr(Expr, Function, Arguments, OperandTypes, ResultType)
    -> compile_typed_operands(Mode, OperandTypes, Arguments, Bound, Expr, ArgumentSqls),
       typed_scalar_sql(Function, ArgumentSqls, ResultType, Sql, Encoding),
       Type = ResultType
    ; json_scalar_expr(Expr, Function, Arguments)
    -> compile_json_operands(Mode, Arguments, Bound, Expr, ArgumentSqls),
       json_scalar_sql(Function, ArgumentSqls, Sql),
       Type = json, Encoding = direct
    ; arithmetic_expr(Expr, Operator, Left, Right)
    -> compile_numeric_operand(Mode, Operator, Left, Bound, Expr, LeftSql, LeftType),
       compile_numeric_operand(Mode, Operator, Right, Bound, Expr, RightSql, RightType),
       arithmetic_sql(Operator, LeftSql, RightSql, LeftType, RightType, Sql),
       arithmetic_result_type(Operator, LeftType, RightType, Type),
       Encoding = direct
    ; json_value_expr(Expr)
    -> compile_json_document(Mode, Expr, Bound, Sql),
       Type = json, Encoding = direct
    % An aggregate functor reaching value position would fall into the
    % tagged-term door below and store a json literal, silently wrong.
    ; aggregate_expr_functor(Expr, Functor, Arity)
    -> throw(unsupported_construct(aggregate_in_expression_position(Functor/Arity)))
    ; compound(Expr)
    -> Expr =.. [Functor | SubArgs],
       maplist(compile_term_sub_expr(Mode, Bound), SubArgs, SubSqls),
       ( SubSqls == []
       -> format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array())', [Functor])
       ; atomic_list_concat(SubSqls, ', ', Joined),
         format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array(~w))', [Functor, Joined])
       ),
       Type = text, Encoding = direct
    ; throw(unsupported_construct(head_expr(Expr)))
    ).

% Rule ONE's other half: a text COLUMN under `value` demand is an id, and
% concat/norm/regexp/ORDER BY need the characters behind it.
demanded_sql(value, dict, IdSql, Sql, direct) :- !, dictionary_content_sql(IdSql, Sql).
demanded_sql(_, Encoding, Sql, Sql, Encoding).

compile_term_sub_expr(Mode, Bound, Arg, Sql) :- compile_expr(Mode, value, Arg, Bound, Sql, _Type, _Encoding).

aggregate_expr_functor(Expr, Functor, Arity) :-
    compound(Expr),
    functor(Expr, Functor, Arity),
    surface(Functor/Arity, aggregate, _, _, _).

% The operator inventory is registry.pl's expression/5 (rank R5 of
% plans/2026-07-29-prolog-org-review.md), not a local list.
arithmetic_expr(Expr, Operator, Left, Right) :-
    compound(Expr), Expr =.. [Operator, Left, Right],
    expression(Operator/2, arithmetic, _, _, _).

text_scalar_expr(Expr, Function, Arguments) :-
    compound(Expr), Expr =.. [Function | Arguments],
    length(Arguments, Arity),
    expression(Function/Arity, text_scalar, _, _, _).

compile_text_operand(Mode, Operand, Bound, Whole, Sql) :-
    compile_expr(Mode, value, Operand, Bound, Sql, Type, _Encoding),
    ( Type == text
    -> true
    ;  throw(unsupported_construct(text_operand_not_text(Whole, Operand, Type)))
    ).

compile_text_operands(_, [], _, _, []).
compile_text_operands(Mode, [Operand | Rest], Bound, Whole, [Sql | Sqls]) :-
    compile_text_operand(Mode, Operand, Bound, Whole, Sql),
    compile_text_operands(Mode, Rest, Bound, Whole, Sqls).

text_scalar_sql(Function, ArgumentSqls, Sql) :-
    length(ArgumentSqls, Arity),
    expression(Function/Arity, text_scalar, _, Rendering, _),
    text_scalar_rendering(Function, Rendering, ArgumentSqls, Sql).

% SQLite's @libsql seam has lower()/unicode(), but no scalar-function
% registration. The recursive scalar expression preserves V5 `normalize`.
text_scalar_rendering(_, ascii_alnum_lower, [ArgumentSql], Sql) :-
    format(atom(Sql),
           '(WITH RECURSIVE "__norm_chars"("i", "c") AS (SELECT 1, substr(~w, 1, 1) UNION ALL SELECT "i" + 1, substr(~w, "i" + 1, 1) FROM "__norm_chars" WHERE "i" < length(~w)) SELECT coalesce(group_concat(lower("c"), \'\'), \'\') FROM "__norm_chars" WHERE (unicode("c") BETWEEN 48 AND 57) OR (unicode("c") BETWEEN 65 AND 90) OR (unicode("c") BETWEEN 97 AND 122))',
           [ArgumentSql, ArgumentSql, ArgumentSql]).
% Uppercase after a non-alnum boundary or at position 1, lowercase elsewhere;
% group_concat follows the CTE's scan order, the same bet norm already makes.
text_scalar_rendering(_, initcap_words, [ArgumentSql], Sql) :-
    format(atom(Sql),
           '(WITH RECURSIVE "__cap_chars"("i", "c", "p") AS (SELECT 1, substr(~w, 1, 1), \'\' UNION ALL SELECT "i" + 1, substr(~w, "i" + 1, 1), substr(~w, "i", 1) FROM "__cap_chars" WHERE "i" < length(~w)) SELECT coalesce(group_concat(CASE WHEN "p" = \'\' OR NOT ((unicode("p") BETWEEN 48 AND 57) OR (unicode("p") BETWEEN 65 AND 90) OR (unicode("p") BETWEEN 97 AND 122)) THEN upper("c") ELSE lower("c") END, \'\'), \'\') FROM "__cap_chars")',
           [ArgumentSql, ArgumentSql, ArgumentSql, ArgumentSql]).
% Direct SQLite scalar: the rendering IS the function name, so rtrim/2 lowers
% to rtrim(a, b) and replace/3 to replace(a, b, c), no UDF.
text_scalar_rendering(Function, Rendering, ArgumentSqls, Sql) :-
    Rendering == Function,
    atomic_list_concat(ArgumentSqls, ', ', ArgsJoined),
    format(atom(Sql), '~w(~w)', [Function, ArgsJoined]).

typed_scalar_expr(Expr, Function, Arguments, OperandTypes, ResultType) :-
    compound(Expr), Expr =.. [Function | Arguments],
    length(Arguments, Arity),
    expression(Function/Arity, typed_scalar, _, _, typed(OperandTypes, ResultType)).

% float is rejected as an index operand: SQLite would truncate silently, and
% "no coercions" makes that a compile error here instead.
compile_typed_operands(_, [], [], _, _, []).
compile_typed_operands(Mode, [text | Types], [Operand | Rest], Bound, Whole, [Sql | Sqls]) :-
    compile_text_operand(Mode, Operand, Bound, Whole, Sql),
    compile_typed_operands(Mode, Types, Rest, Bound, Whole, Sqls).
compile_typed_operands(Mode, [int | Types], [Operand | Rest], Bound, Whole, [Sql | Sqls]) :-
    compile_expr(Mode, identity, Operand, Bound, Sql, Type, _Encoding),
    (   Type == int
    ->  true
    ;   throw(unsupported_construct(typed_operand_not_int(Whole, Operand, Type)))
    ),
    compile_typed_operands(Mode, Types, Rest, Bound, Whole, Sqls).

typed_scalar_sql(Function, ArgumentSqls, ResultType, Sql, Encoding) :-
    length(ArgumentSqls, Arity),
    expression(Function/Arity, typed_scalar, _, Rendering, _),
    (   ResultType = list(ElementType)
    ->  list_intern_sql(Function, Rendering, ArgumentSqls, ElementType, Sql,
                         Encoding)
    ;   typed_scalar_rendering(Function, Rendering, ArgumentSqls, Sql),
        Encoding = direct
    ).

% The expression answers the interned list id; Encoding carries the intern
% request the caller owes, and Encoding == direct means a plain scalar.
list_intern_sql(split, split_list_intern, [TextSql, SeparatorSql], ElementType,
                Sql, Encoding) :-
    typed_scalar_rendering(split, split_json_array, [TextSql, SeparatorSql],
                           ArraySql),
    list_entity_id_lookup(ElementType, ArraySql, Sql),
    Encoding = list_intern(ElementType, ArraySql).

list_entity_id_lookup(ElementType, ArraySql, Sql) :-
    canonical_type_name(list(ElementType), EntityName),
    table_name(EntityName/1, EntityTable),
    quote_ident(EntityTable, QuotedEntity),
    interned_id_sql(ArraySql, ContentIdSql),
    format(atom(Sql),
           '(SELECT e."__id" FROM ~w e WHERE e."content" = ~w)',
           [QuotedEntity, ContentIdSql]).

% The trailing-separator seed makes the last part ordinary; its NULL row
% filters out. An empty separator never advances instr, so it cannot walk.
typed_scalar_rendering(_, split_json_array, [TextSql, SeparatorSql], Sql) :-
    format(atom(Sql),
           '(CASE WHEN ~w = \'\' THEN json_array(~w) ELSE (WITH RECURSIVE "__split_parts"("rest", "part") AS (SELECT ~w || ~w, NULL UNION ALL SELECT substr("rest", instr("rest", ~w) + length(~w)), substr("rest", 1, instr("rest", ~w) - 1) FROM "__split_parts" WHERE "rest" <> \'\') SELECT json_group_array("part") FROM "__split_parts" WHERE "part" IS NOT NULL) END)',
           [SeparatorSql, TextSql, TextSql, SeparatorSql,
            SeparatorSql, SeparatorSql, SeparatorSql]).
typed_scalar_rendering(Function, Rendering, ArgumentSqls, Sql) :-
    Rendering == Function,
    atomic_list_concat(ArgumentSqls, ', ', ArgsJoined),
    format(atom(Sql), '~w(~w)', [Function, ArgsJoined]).

json_scalar_expr(Expr, Function, Arguments) :-
    compound(Expr), Expr =.. [Function | Arguments],
    length(Arguments, Arity),
    expression(Function/Arity, json_scalar, _, _, _).

% A json operand reads its stored TEXT and re-tags through json(), the same
% carrier json_group_array's aggregate values ride.
compile_json_operand(Mode, Operand, Bound, Whole, Sql) :-
    compile_expr(Mode, value, Operand, Bound, OperandSql, Type, _Encoding),
    (   ( Type == json ; Type = json_list(_) )
    ->  format(atom(Sql), 'json(~w)', [OperandSql])
    ;   throw(unsupported_construct(json_operand_not_json(Whole, Operand, Type)))
    ).

compile_json_operands(_, [], _, _, []).
compile_json_operands(Mode, [Operand | Rest], Bound, Whole, [Sql | Sqls]) :-
    compile_json_operand(Mode, Operand, Bound, Whole, Sql),
    compile_json_operands(Mode, Rest, Bound, Whole, Sqls).

json_scalar_sql(Function, ArgumentSqls, Sql) :-
    length(ArgumentSqls, Arity),
    expression(Function/Arity, json_scalar, _, Rendering, _),
    json_scalar_rendering(Rendering, ArgumentSqls, Sql).

% JSON null IS the atom `none` (decision 2026-08-11), so a patch carrying
% `none` composes to a null-valued key via SQLite json_patch/2 instead of
% stopping the statement. No json_patch_null_unruled guard is emitted.
json_scalar_rendering(json_patch, [TargetSql, PatchSql], Sql) :-
    format(atom(Sql), 'json_patch(~w, ~w)', [TargetSql, PatchSql]).

% Without this guard the generic compound branch below wraps a braces literal
% or a list in the json1 tagged-term encoding, a domain fact's rendering.
json_value_expr(Expr) :- compound(Expr), Expr = {}(_), !.
json_value_expr(Expr) :- is_list(Expr), Expr \== [], !.
json_value_expr(Expr) :- compound(Expr), Expr = [_ | _].
json_value_expr(Expr) :- nonvar(Expr), Expr = json_object(_), !.
json_value_expr(Expr) :- nonvar(Expr), Expr = json_array(_), !.
json_value_expr(Expr) :- Expr == json_null, !.

% Keys sort at COMPILE time: json1 keeps argument order and the log contract
% is sorted keys. A GROUND subtree uses the oracle's own canonicalizer.
compile_json_document(Mode, Expr, Bound, Sql) :-
    (   json_document_dup_key(Expr)
    ->  Sql = 'json(\'json_dup_key\')'
    ;   ground(Expr)
    ->  canonical_json_text(Expr, Text), json_document_text_sql(Text, Sql)
    ;   json_document_pairs(Expr, Pairs)
    ->  keysort(Pairs, Sorted),
        maplist(compile_json_entry(Mode, Bound), Sorted, EntrySqls),
        atomic_list_concat(EntrySqls, ', ', Inner),
        format(atom(Sql), 'json_object(~w)', [Inner])
    ;   Expr = json_array(Values)
    ->  maplist(compile_json_element(Mode, Bound), Values, ElementSqls),
        atomic_list_concat(ElementSqls, ', ', Inner),
        format(atom(Sql), 'json_array(~w)', [Inner])
    ;   Expr == json_null
    ->  Sql = 'json(\'null\')'
    ;   is_list(Expr)
    ->  maplist(compile_json_element(Mode, Bound), Expr, ElementSqls),
        atomic_list_concat(ElementSqls, ', ', Inner),
        format(atom(Sql), 'json_array(~w)', [Inner])
    ;   throw(unsupported_construct(json_value_expression(Expr)))
    ).

json_document_text_sql(Text, Sql) :-
    sql_literal(Text, Quoted),
    format(atom(Sql), 'json(~w)', [Quoted]).

compile_json_entry(Mode, Bound, Key-Raw, Sql) :-
    sql_literal(Key, KeySql),
    compile_json_element(Mode, Bound, Raw, ValueSql),
    format(atom(Sql), '~w, ~w', [KeySql, ValueSql]).

compile_json_element(Mode, Bound, Value, Sql) :-
    (   var(Value)
    ->  compile_json_operand(Mode, Value, Bound, Sql)
    ;   compound(Value), bound_lookup(Bound, Value, _)
    ->  compile_json_operand(Mode, Value, Bound, Sql)
    ;   ( Value = {}(_) ; Value = [_ | _] ; Value = json_object(_) ; Value = json_array(_) ; Value == json_null )
    ->  compile_json_document(Mode, Value, Bound, Sql)
    ;   ground(Value)
    ->  canonical_json_text(Value, Text), json_document_text_sql(Text, Sql)
    ;   compile_json_operand(Mode, Value, Bound, Sql)
    ).

compile_json_operand(Mode, Value, Bound, Sql) :-
    compile_expr(Mode, value, Value, Bound, ValueSql, ValueType, _Encoding),
    json_document_operand_sql(ValueType, ValueSql, Sql).

% A ref column carries the dictionary id; json1 would render that id as a
% number where the oracle renders the struct's document.
json_document_operand_sql(ref(TypeName), _, _) :- !,
    throw(unsupported_construct(json_document_ref_operand(TypeName))).
% SQLite stores a bool column as 0/1 and the tick-log contract is true/false.
json_document_operand_sql(bool, ValueSql, Sql) :- !,
    format(atom(Sql), 'json(CASE WHEN ~w THEN \'true\' ELSE \'false\' END)',
           [ValueSql]).
json_document_operand_sql(Type, ValueSql, Sql) :-
    json_group_array_value_sql(Type, ValueSql, Sql).

% Every level, before any subtree renders: canonical_json_text/2 would
% otherwise throw at COMPILE time where body.pl:json_canon/2 throws at run.
json_document_dup_key(Expr) :-
    nonvar(Expr), Expr = {}(_),
    json_document_pairs(Expr, Pairs),
    (   pairs_keys(Pairs, Keys),
        sort(Keys, Distinct),
        length(Keys, KeyCount), length(Distinct, DistinctCount),
        KeyCount =\= DistinctCount
    ->  true
    ;   member(_-Raw, Pairs), json_document_dup_key(Raw)
    ),
    !.
json_document_dup_key(Expr) :-
    nonvar(Expr),
    Expr = json_object(Pairs),
    (   pairs_keys(Pairs, Keys),
        sort(Keys, Distinct),
        length(Keys, KeyCount), length(Distinct, DistinctCount),
        KeyCount =\= DistinctCount
    ->  true
    ;   member(_-Raw, Pairs), json_document_dup_key(Raw)
    ),
    !.
json_document_dup_key(Expr) :-
    nonvar(Expr),
    Expr = json_array(Values),
    member(Element, Values),
    json_document_dup_key(Element),
    !.
json_document_dup_key(Expr) :-
    is_list(Expr),
    member(Element, Expr),
    json_document_dup_key(Element),
    !.

json_document_pairs(Expr, Pairs) :-
    nonvar(Expr), Expr = {}(Fields),
    json_document_field_pairs(Fields, Pairs).
json_document_pairs(Expr, Pairs) :-
    nonvar(Expr), Expr = json_object(Pairs).

json_document_field_pairs(Fields, _) :- var(Fields), !, fail.
json_document_field_pairs((Left, Right), Pairs) :- !,
    json_document_field_pairs(Left, LeftPairs),
    json_document_field_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
json_document_field_pairs(Key: Raw, [Key-Raw]) :- atomic(Key).

compile_int_operand(Mode, Operand, Bound, Whole, Sql) :-
    compile_expr(Mode, identity, Operand, Bound, Sql, Type, _Encoding),
    ( Type == int
    -> true
    ;  throw(unsupported_construct(arith_operand_not_int(Whole, Operand, Type)))
    ).

compile_numeric_operand(Mode, mod, Operand, Bound, Whole, Sql, int) :-
    !,
    compile_int_operand(Mode, Operand, Bound, Whole, Sql).
compile_numeric_operand(Mode, _, Operand, Bound, Whole, Sql, Type) :-
    compile_expr(Mode, identity, Operand, Bound, Sql, Type, _Encoding),
    ( memberchk(Type, [int, float])
    -> true
    ; throw(unsupported_construct(arith_operand_not_number(Whole, Operand, Type)))
    ).

% Rendering comes from the table's SqlRendering field. sign_corrected_modulo
% is there because SQLite's % takes the sign of the dividend while this
% language's mod follows the divisor.
arithmetic_sql(Operator, LeftSql, RightSql, LeftType, RightType, Sql) :-
    expression(Operator/2, arithmetic, _, Rendering, _),
    arithmetic_rendering(Rendering, LeftSql, RightSql, LeftType, RightType, Sql).

arithmetic_rendering(sign_corrected_modulo, LeftSql, RightSql, _, _, Sql) :- !,
    format(atom(Sql), '(((~w % ~w) + ~w) % ~w)',
           [LeftSql, RightSql, RightSql, RightSql]).
arithmetic_rendering(numeric_division, LeftSql, RightSql, int, int, Sql) :- !,
    format(atom(Sql), '(~w / ~w)', [LeftSql, RightSql]).
arithmetic_rendering(numeric_division, LeftSql, RightSql, _, _, Sql) :- !,
    format(atom(Sql), '(CAST(~w AS REAL) / ~w)', [LeftSql, RightSql]).
arithmetic_rendering(infix(SqlOperator), LeftSql, RightSql, _, _, Sql) :-
    format(atom(Sql), '(~w ~w ~w)', [LeftSql, SqlOperator, RightSql]).

arithmetic_result_type(mod, int, int, int) :- !.
arithmetic_result_type(_, float, Right, float) :-
    memberchk(Right, [int, float]), !.
arithmetic_result_type(_, Left, float, float) :-
    memberchk(Left, [int, float]), !.
arithmetic_result_type(_, int, int, int).

% engine.pl text_piece/2 throws non_display_in_concat on a compound piece;
% an int piece auto-converts (atomic_list_concat), which SQLite's `||` also
% does. Only the compound case needs refusing.
compile_concat_parts(_, Parts, _, Whole, _) :-
    \+ is_list(Parts),
    throw(unsupported_construct(concat_not_a_list(Whole))).
compile_concat_parts(Mode, Parts, Bound, Whole, PartSqls) :-
    is_list(Parts),
    maplist(compile_concat_part(Mode, Bound, Whole), Parts, PartSqls).

compile_concat_part(Mode, Bound, Whole, Part, Sql) :-
    compile_expr(Mode, value, Part, Bound, Sql, Type, _Encoding),
    ( memberchk(Type, [int, text])
    -> true
    ;  throw(unsupported_construct(concat_non_display_piece(Whole, Part)))
    ).

% ═══ guard / bind goals (EXPRESSION LIFT) ═══════════════════════════════════
% Folded LEFT TO RIGHT over the body's guard/bind goals, after every positive
% atom has bound its own columns. engine.pl solve/2 resolves a conjunction
% left to right, so a bind reads what earlier binds introduced; the fold
% order here is the same. A bind whose variable is ALREADY bound is a check,
% not a fresh binding (`solve(Variable := Expr)` ends in `Variable = Value`,
% which is unification, so an already-bound left side filters) and compiles
% to an equality condition.

compile_guard_goals(Mode, Goals, Bound0, Bound, WhereTexts) :-
    foldl(compile_guard_goal(Mode), Goals, Bound0-[], Bound-ReversedTexts),
    reverse(ReversedTexts, WhereTexts).

% The one SQL text now/1 reads. `__tick` is a one-row counter table the
% emitted program advances at the head of every tick, so a scalar subquery
% over it IS the kernel tick read engine.pl performs off ctx(_, _, Tick).
% A subquery, not a bind parameter: the same ProjectSql text runs both as a
% per-arrival prepared statement (naive) and as one set-based delta join
% (incremental), and neither shape has a free parameter slot to spare.
tick_column_sql('(SELECT "n" FROM "__tick")').

tick_table_ddl([ 'CREATE TABLE "__tick" ("n" INTEGER NOT NULL)',
                 % Idempotent on purpose: serve/3_engine.ts re-runs a
                 % program's DDL on every swap and swallows "already exists",
                 % so a bare INSERT would reset (or duplicate) the counter
                 % under a running server.
                 'INSERT INTO "__tick" ("n") SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM "__tick")'
               ]).

% ═══ step g1 SCAFFOLD: the program catalog (rulings.pl:613 catalog_universe) ═
% A column is a CHILD ROW of its rel (ordinal 1-based); a rel's parent_id is the
% module row's, module_id is the module row's own id, h_id the row identity hash.
catalog_ddl_contract('__rel',
                     [ rel_id-int, parent_id-int, ordinal-int,
                       local_name-text, kind-text, type_id-int, arity-int,
                       module_id-int, h_id-text, h_schema-text, h_rule-text ]).

%! catalog_ddl_key(+CatalogName, -KeyPositions) is semidet.
%   rel_id is dense and positional by construction (catalog_rows/N), so the
%   table declares the surrogate the producer already guarantees.
catalog_ddl_key('__rel', [1]).

%! set_rel_pk_sql(+Ref, +KeyOrNone, +EdgeHeadedRefs, +ArrivalTargetRefs,
%!                +Columns, -PkSql) is det.
%   The PK for a set rel: its declared key when edge-headed or an arrival
%   target, its surrogate key when a catalog table, else all columns.

set_rel_pk_sql(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs, Columns,
               PkSql) :-
    set_rel_key_positions(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                          Columns, KeyPositions),
    nth1_list(KeyPositions, Columns, KeyColumns),
    maplist(quote_ident, KeyColumns, QuotedKeyColumns),
    atomic_list_concat(QuotedKeyColumns, ', ', PkSql).

%! set_rel_table_ddl(+QuotedTable, +ColumnsSql, +RefCountColumn, +PkSql,
%!                   -Ddl) is det.
%   The single set-rel table shape: a surrogate `__id INTEGER PRIMARY KEY`
%   plus the content identity as UNIQUE over the columns. At zero columns
%   there is no content, so there is no UNIQUE and no refcount: a 0-ary rel is
%   a proposition and every arrival mints a row.
set_rel_table_ddl(QuotedTable, ColumnsSql, RefCountColumn, PkSql, Ddl) :-
    (   ColumnsSql = ''
    ->  format(atom(Ddl), 'CREATE TABLE ~w ("__id" INTEGER PRIMARY KEY)',
               [QuotedTable])
    ;   format(atom(Ddl),
               'CREATE TABLE ~w ("__id" INTEGER PRIMARY KEY, ~w~w, UNIQUE (~w))',
               [QuotedTable, ColumnsSql, RefCountColumn, PkSql])
    ).

% A keyed set rel's PK is its declared key (when edge-headed or an arrival
% target, or a catalog table), else all columns. Yields the positions so the
% option some-table scope check can reuse the same computation.
set_rel_key_positions(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                      Columns, KeyPositions) :-
    ( set_rel_has_key(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                      KeyPositions)
    -> true
    ;  length(Columns, Arity),
       % numlist/3 FAILS at arity 0 (high below low); a bare failure would
       % drop the whole rel from RelPlans with no message.
       (   Arity =:= 0
       ->  KeyPositions = []
       ;   numlist(1, Arity, KeyPositions)
       )
    ).

% A level-headed keyed rel must keep its all-column PK: that is what
% __refcount dedups against, so only edge, arrival and catalog keys fire.
set_rel_has_key(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                KeyPositions) :-
    ( ( memberchk(Ref, EdgeHeadedRefs) ; memberchk(Ref, ArrivalTargetRefs) ),
      KeyOrNone = key(KeyPositions) ).
set_rel_has_key(Ref, _KeyOrNone, _EdgeHeadedRefs, _ArrivalTargetRefs,
                KeyPositions) :-
    Ref = Name/_,
    catalog_ddl_key(Name, KeyPositions).

% The option enum split's some-table is the id + value payload keyed on the
% value alone (content identity, 0_option_expand.pl variant rewrite). The
% value-direction primary key gives no id-side point read, so the id lookup
% carries its own unique index.
option_some_table(Ref, Columns, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs) :-
    Columns == [id, value],
    set_rel_key_positions(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                          Columns, KeyPositions),
    KeyPositions == [2].

option_some_index_ddl(Table, IndexDdl) :-
    atomic_list_concat([Table, '_id'], IndexName),
    quote_ident(IndexName, QuotedIndexName),
    quote_ident(Table, QuotedTable),
    format(atom(IndexDdl),
           'CREATE UNIQUE INDEX ~w ON ~w ("id")',
           [QuotedIndexName, QuotedTable]).

% The parent-chain guard, default-on for every option(<own rel>) column. It
% rides the DDL because arrival runs one statement and cannot carry a check.
acyclic_guard_ddl(Decls, RelPlans, Ddls) :-
    findall(Ddl,
            ( acyclic_companion(Decls, Ref, Source, OwnerColumn, TargetColumn),
              once(( member(RelPlan, RelPlans),
                     relplan_parts(RelPlan, Ref, _, _, _, _) )),
              acyclic_guard_statement(Ref, Source, OwnerColumn, TargetColumn,
                                      Ddl) ),
            Unsorted),
    sort(Unsorted, Ddls).

% BEFORE INSERT alone covers the upsert too: the arrival's DO UPDATE sets the
% target from excluded, so NEW already carries the row the update would land.
acyclic_guard_statement(Ref, declared_at(RelName, Column), OwnerColumn,
                        TargetColumn, Ddl) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    atom_concat('__acyclic_', Table, TriggerName),
    quote_ident(TriggerName, QuotedTriggerName),
    quote_ident(OwnerColumn, QuotedOwnerColumn),
    quote_ident(TargetColumn, QuotedTargetColumn),
    format(atom(Ddl),
           'CREATE TRIGGER ~w BEFORE INSERT ON ~w WHEN EXISTS (WITH RECURSIVE "__parent_chain" ("__node") AS (SELECT NEW.~w UNION SELECT g.~w FROM ~w g JOIN "__parent_chain" ON g.~w = "__parent_chain"."__node") SELECT 1 FROM "__parent_chain" WHERE "__node" = NEW.~w) BEGIN SELECT RAISE(ABORT, \'parent_cycle(~w, ~w)\'); END',
           [QuotedTriggerName, QuotedTable, QuotedTargetColumn,
            QuotedTargetColumn, QuotedTable, QuotedOwnerColumn,
            QuotedOwnerColumn, RelName, Column]).

% The CREATE TABLE comes from the ordinary rel_ddl/6 path, because compile.pl
% injects the contract's col_type decls; only the child-walk index is minted here.
catalog_table_ddl([
    'CREATE INDEX IF NOT EXISTS "__rel_parent" ON "__rel" ("parent_id", "local_name")']).

%! rel_h_id(+ParentHash, +LocalName, +Arity, -HashText) is det.
%   Under the PARENT's hash: two rels in one module can share a column name.
rel_h_id(ParentHash, LocalName, Arity, HashText) :-
    atomic_list_concat([ParentHash, '/', LocalName, '/', Arity], Key),
    short_hash(Key, HashText).

%! schema_hash(+Columns, +ColumnTypes, +KeyOrNone, -HashText) is det.
schema_hash(Columns, ColumnTypes, KeyOrNone, HashText) :-
    canonical_hash_key(schema(Columns, ColumnTypes, KeyOrNone), Key),
    short_hash(Key, HashText).

%! rule_bodies_map(+Rules, -Map) is det.
%   msort, not sort: a duplicate body counts toward the hash. Head-Body keeps
%   head sharing visible; canonicalize first so msort sorts ground atoms.
rule_bodies_map(Rules, Map) :-
    findall(Ref-Canonical,
            ( member(Rule, Rules),
              rule_head_ref(Rule, Ref),
              rule_head_of(Rule, Head),
              rule_body_of(Rule, Body),
              canonical_hash_key(Head-Body, Canonical) ),
            Pairs0),
    msort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Map).

%! rule_hash(+BodiesMap, +Ref, -HashText) is det.
%   The empty atom for a source rel: no derivation to fingerprint.
rule_hash(BodiesMap, Ref, HashText) :-
    (   memberchk(Ref-Bodies, BodiesMap)
    ->  canonical_hash_key(rules(Bodies), Key),
        short_hash(Key, HashText)
    ;   HashText = ''
    ).

rule_body_of((_Head <- Body), Body).
rule_body_of((_Head <+ Body), Body).

rule_head_of((Head <- _Body), Head).
rule_head_of((Head <+ _Body), Head).

%! canonical_hash_key(+Term, -KeyAtom) is det.
%   numbervars on a COPY: variable identity becomes positional, so the same
%   shape hashes the same across runs and processes.
canonical_hash_key(Term, KeyAtom) :-
    copy_term(Term, Copy),
    numbervars(Copy, 0, _),
    term_to_atom(Copy, KeyAtom).

% Ids are positional for a byte-stable recompile: primitives, list rows,
% module, then each rel and its columns.
catalog_row_ddl(Mode, ModuleName, Rules, RelPlans, DepartureRefs, PreRefs,
                Types, RuleLevelStatements, Decls, [Statement]) :-
    catalog_all_rows(Mode, ModuleName, Rules, RelPlans, DepartureRefs,
                     PreRefs, Types, RuleLevelStatements, Decls, AllRows),
    foldl(catalog_row_part(Mode), AllRows, [], ReversedParts),
    reverse(ReversedParts, Parts),
    atomic_list_concat(Parts, ',', ValuesText),
    format(atom(Statement),
           'INSERT OR IGNORE INTO "__rel" ("rel_id", "parent_id", "ordinal", "local_name", "kind", "type_id", "arity", "module_id", "h_id", "h_schema", "h_rule") VALUES ~w',
           [ValuesText]).

%! plan_rule_level_statements(+Plan, -RuleLevelStatements) is det.
%   Mirror of the lower_program/2 pipeline (dictionaries, the two expands,
%   then level_statement_groups/4) so a caller outside lower_program/2 can plan
%   the same level rows the DDL minted. Faithful because every step is the same
%   predicate.
plan_rule_level_statements(Plan, RuleLevelStatements) :-
    Plan = plan(_, _, _, RelPlans, _, _, _, _, _),
    with_storage_context(RelPlans,
                         plan_rule_level_statements_in_context(Plan,
                                                               RuleLevelStatements)).

plan_rule_level_statements_in_context(
        plan(_Name, prog(_Decls, _Rules), LoweringTypes, RelPlans,
             _ArrivalTargets, RuleOrder, _EdgeRules, _SubscribedRels, Mode),
        RuleLevelStatements) :-
    dictionary_relplans(LoweringTypes, DictionaryRelPlans),
    append(DictionaryRelPlans, RelPlans, BodyRelPlans),
    expand_relation_pattern_rules(LoweringTypes, BodyRelPlans, RuleOrder,
                                  PatternedRuleOrder),
    expand_decode_rules(LoweringTypes, BodyRelPlans, PatternedRuleOrder,
                        DecodedRuleOrder),
    level_statement_groups(Mode, BodyRelPlans, DecodedRuleOrder,
                           RuleLevelStatements).

%! catalog_all_rows(+Mode, +ModuleName, +Rules, +RelPlans, +DepartureRefs,
%!                  +PreRefs, +Types, +RuleLevelStatements, +Decls, -Rows)
%!                  is det.
%   The block the seed renders: the decl rows (catalog_rows/4, byte-stable),
%   then the plane rows (catalog_plane_rows/10: the Rules-derivable families,
%   the level-statement families, and the sh/bind port rows). DepartureRefs,
%   PreRefs, RuleLevelStatements and Decls mirror the lower_program/2 call-site
%   derivations so a plane row exists exactly where its DDL mint site emitted.
% The plane rows name SQLite objects, so this door installs the same storage
% context lower_program/2 does: outside it the catalog described tables the
% DDL never created.
catalog_all_rows(Mode, ModuleName, Rules, RelPlans, DepartureRefs, PreRefs,
                 Types, RuleLevelStatements, Decls, AllRows) :-
    with_storage_context(RelPlans,
        catalog_all_rows_in_context(Mode, ModuleName, Rules, RelPlans,
                                    DepartureRefs, PreRefs, Types,
                                    RuleLevelStatements, Decls, AllRows)).

catalog_all_rows_in_context(Mode, ModuleName, Rules, RelPlans, DepartureRefs,
                            PreRefs, Types, RuleLevelStatements, Decls,
                            AllRows) :-
    catalog_decl_rows(ModuleName, Rules, RelPlans, Decls, DeclRows, Context),
    catalog_plane_rows(Mode, ModuleName, RelPlans, DepartureRefs, PreRefs,
                       Types, RuleLevelStatements, Decls, Context, PlaneRows),
    append(DeclRows, PlaneRows, AllRows).

%! catalog_type_rows(+Mode, +ModuleName, +Rules, +RelPlans, +Decls, -Rows) is det.
% The public type artifacts do not need executable plane rows, but an idref
% and a ref deliberately share the target relation's type_id. Their storage
% child is therefore the semantic discriminator kept beside the type rows.
catalog_type_rows(Mode, ModuleName, Rules, RelPlans, Decls, Rows) :-
    catalog_decl_rows(ModuleName, Rules, RelPlans, Decls, DeclRows,
                      ctx(Modules, RelIdMap, _ListIdMap, StartId)),
    catalog_storage_rows(Mode, RelPlans, RelIdMap, Modules, StartId,
                         StorageRows, _),
    append(DeclRows, StorageRows, Rows).

%! catalog_type_relation_rows(+ModuleName, +Decls, -Rows) is det.
%  Target-independent schema metadata is a parallel catalog stream.  The
%  existing row/11 catalog remains the runtime-artifact stream; callers that
%  need authored roles request these normalized rows explicitly.
catalog_type_relation_rows(_ModuleName, Decls, Rows) :-
    type_relation_rows(Decls, Rows).

%! catalog_type_transport_rows(+ModuleName, +CatalogRows, +Decls, -Rows) is det.
%  The artifact transport view is derived from the target-independent rows
%  and catalog column IDs, without changing the existing catalog rows.
catalog_type_transport_rows(_ModuleName, CatalogRows, Decls, Rows) :-
    type_relation_rows(Decls, RelationRows),
    schema_member_transport_rows(CatalogRows, RelationRows, Rows).

% The plane half is appended after the rels+columns block (plan 4) so no
% existing id moves. Plane ids start at the decl block's FinalId.
%
% Every family below re-derives its existence condition from the SAME
% predicates the DDL mint sites use (any_interned_column, DepartureRefs,
% PreRefs, declared_type_name), so a plane row cannot describe a table the
% lowering did not create. That is the whole point of this step.
catalog_plane_rows(Mode, _ModuleName, RelPlans, DepartureRefs, PreRefs,
                   Types, RuleLevelStatements, Decls,
                   ctx(Modules, RelIdMap, _ListIdMap, StartId),
                   PlaneRows) :-
    catalog_rel_plane_rows(Mode, RelPlans, DepartureRefs, PreRefs, RelIdMap,
                           Modules, StartId, RelPlaneRows,
                           IdAfterRel),
    dict_plane_rows(Mode, RelPlans, Types, RelIdMap, Modules,
                    IdAfterRel, DictRows, IdAfterDict),
    catalog_level_plane_rows(RelPlans, RuleLevelStatements, RelIdMap,
                             Modules, IdAfterDict, LevelRows,
                             IdAfterLevel),
    catalog_port_plane_rows(Decls, RelIdMap, Modules,
                            IdAfterLevel, PortRows, IdAfterPort),
    catalog_storage_rows(Mode, RelPlans, RelIdMap, Modules,
                         IdAfterPort, StorageRows, _IdAfterStorage),
    append([RelPlaneRows, DictRows, LevelRows, PortRows, StorageRows],
           PlaneRows).

% ── per-rel planes ─────────────────────────────────────────────────────────
catalog_rel_plane_rows(_Mode, [], _DepartureRefs, _PreRefs, _RelIdMap,
                       _Modules, Id, [], Id).
catalog_rel_plane_rows(Mode, [RelPlan | Rest], DepartureRefs, PreRefs,
                       RelIdMap, Modules, Id0, Rows, IdFinal) :-
    relplan_parts(RelPlan, Ref, _Kind, Columns, _KeyOrNone, ColumnTypes),
    relplan_storage_name(RelPlan, StorageName),
    Ref = Name/RelArity,
    rel_row_id(RelIdMap, Name, RelId),
    rel_module(Modules, Name, RelHash, RelModuleId),
    rel_h_id(RelHash, Name, RelArity, RelHId),
    catalog_one_rel_planes(Mode, Ref, StorageName, Columns, ColumnTypes, RelId, RelHId,
                           RelModuleId, DepartureRefs, PreRefs, Id0,
                           ThisRows, Id1),
    catalog_rel_plane_rows(Mode, Rest, DepartureRefs, PreRefs, RelIdMap,
                           Modules, Id1, RestRows, IdFinal),
    append(ThisRows, RestRows, Rows).

% The always-on frontier family, the departure frontier where listened, the
% pre table where referenced, and the decode views where the rel is interned.
catalog_one_rel_planes(Mode, Ref, StorageName, Columns, ColumnTypes, RelId, RelHId,
                       ModuleId, DepartureRefs, PreRefs, Id0, Rows, IdFinal) :-
    length(Columns, ColumnCount),
    TagPlus is ColumnCount + 2,
    delta_table_name(Ref, DeltaTable),
    frontier_table_name(Ref, FrontierTable),
    next_frontier_table_name(Ref, NextTable),
    rel_h_id(RelHId, DeltaTable, TagPlus, DeltaHId),
    rel_h_id(RelHId, FrontierTable, TagPlus, FrontierHId),
    rel_h_id(RelHId, NextTable, TagPlus, NextHId),
    schema_hash(['_sign', '_sequence' | Columns], [int, int | ColumnTypes],
                none, DeltaSchema),
    schema_hash(['_phase', '_sequence' | Columns], [int, int | ColumnTypes],
                none, FrontierSchema),
    DeltaRow = row(Id0, RelId, 0, DeltaTable, delta, 0, TagPlus, ModuleId,
                   DeltaHId, DeltaSchema, ''),
    FrontierRow = row(Id1, RelId, 0, FrontierTable, frontier, 0, TagPlus,
                      ModuleId, FrontierHId, FrontierSchema, ''),
    NextRow    = row(Id2, RelId, 0, NextTable, next_frontier, 0, TagPlus,
                     ModuleId, NextHId, FrontierSchema, ''),
    Id1 is Id0 + 1,
    Id2 is Id1 + 1,
    IdAfterNext is Id2 + 1,
    catalog_departure_plane_row(Ref, RelId, RelHId, ModuleId,
                                DepartureRefs, IdAfterNext, Departures,
                                IdAfterDeparture, FrontierSchema, TagPlus),
    catalog_pre_plane_row(Ref, Columns, ColumnTypes, RelId, RelHId,
                          ModuleId, PreRefs, IdAfterDeparture, PreRows,
                          IdAfterPre),
    catalog_view_plane_rows(Mode, StorageName, Columns, ColumnTypes, RelId, RelHId,
                            Id0, ModuleId, IdAfterPre, ViewRows, IdAfterViews),
    append([DeltaRow, FrontierRow, NextRow | Departures], PreRows, Base0),
    append(Base0, ViewRows, Rows),
    IdFinal = IdAfterViews.

% departure: existence mirrors the delta_ddl/3 mint site, which emits the
% frontier only for rels listened_departure_refs/2 named.
catalog_departure_plane_row(Ref, RelId, RelHId, ModuleId,
                            DepartureRefs, Id0, Rows, IdFinal, Schema,
                            TagPlus) :-
    (   memberchk(Ref, DepartureRefs)
    ->  departure_frontier_table_name(Ref, DepartureTable),
        rel_h_id(RelHId, DepartureTable, TagPlus, DepartureHId),
        DepartureRow = row(Id0, RelId, 0, DepartureTable, departure, 0, TagPlus,
                           ModuleId, DepartureHId, Schema, ''),
        IdFinal is Id0 + 1,
        Rows = [DepartureRow]
    ;   Rows = [], IdFinal = Id0
    ).

% pre: existence mirrors pre_ddl/3, called once per level_body_pre_ref/2 ref.
catalog_pre_plane_row(Ref, Columns, ColumnTypes, RelId, RelHId,
                      ModuleId, PreRefs, Id0, Rows, IdFinal) :-
    (   memberchk(Ref, PreRefs)
    ->  length(Columns, ColumnCount),
        pre_table_name(Ref, PreTable),
        rel_h_id(RelHId, PreTable, ColumnCount, PreHId),
        schema_hash(Columns, ColumnTypes, none, PreSchema),
        PreRow = row(Id0, RelId, 0, PreTable, pre, 0, ColumnCount, ModuleId,
                     PreHId, PreSchema, ''),
        IdFinal is Id0 + 1,
        Rows = [PreRow]
    ;   Rows = [], IdFinal = Id0
    ).

% view: existence mirrors text_view_ddls/6, which emits only when the rel has
% an interned column. The delta-table view parents on the delta row (DeltaId),
% giving the two-level plane tree plan 6 describes.
catalog_view_plane_rows(Mode, Name, Columns, ColumnTypes, RelId, RelHId,
                        DeltaId, ModuleId, Id0, Rows, IdFinal) :-
    (   any_interned_column(Mode, ColumnTypes)
    ->  length(Columns, ColumnCount),
        format(atom(RelViewTable), '__txt_~w', [Name]),
        rel_h_id(RelHId, RelViewTable, ColumnCount, RelViewHId),
        schema_hash(Columns, ColumnTypes, none, ViewSchema),
        RelView = row(Id0, RelId, 0, RelViewTable, view, 0, ColumnCount,
                      ModuleId, RelViewHId, ViewSchema, ''),
        Id1 is Id0 + 1,
        format(atom(DeltaViewTable), '__txt___delta_~w', [Name]),
        DeltaViewCount is ColumnCount + 2,
        rel_h_id(RelHId, DeltaViewTable, DeltaViewCount, DeltaViewHId),
        DeltaView = row(Id1, DeltaId, 0, DeltaViewTable, view, 0,
                        DeltaViewCount, ModuleId, DeltaViewHId, ViewSchema, ''),
        IdFinal is Id1 + 1,
        Rows = [RelView, DeltaView]
    ;   Rows = [], IdFinal = Id0
    ).

% ── dictionary planes (per module) ─────────────────────────────────────────
dict_plane_rows(Mode, RelPlans, Types, RelIdMap, Modules, Id0,
                Rows, IdFinal) :-
    Modules = modules(ModuleHash, ModuleId, _),
    (   catalog_has_interned_column(Mode, RelPlans)
    ->  rel_h_id(ModuleHash, '__str', 2, StrHId),
        StrRow = row(Id0, ModuleId, 0, '__str', dictionary, 0, 2, ModuleId,
                     StrHId, '', ''),
        IdAfterStr is Id0 + 1,
        StrRows = [StrRow]
    ;   StrRows = [], IdAfterStr = Id0
    ),
    catalog_ref_dict_rows(Mode, RelPlans, Types, RelIdMap, Modules,
                          IdAfterStr, RefRows, IdFinal),
    append(StrRows, RefRows, Rows).

catalog_has_interned_column(Mode, RelPlans) :-
    member(RelPlan, RelPlans),
    relplan_parts(RelPlan, _, _, _, _, ColumnTypes),
    any_interned_column(Mode, ColumnTypes).

% __ref_<Type> exists once per declared struct type, mirroring the rel_ddl/6
% set-arm that creates the reference view.
catalog_ref_dict_rows(_Mode, [], _Types, _RelIdMap, _Modules, Id, [], Id).
catalog_ref_dict_rows(Mode, [RelPlan | Rest],
                      Types, RelIdMap, Modules, Id0, Rows,
                      IdFinal) :-
    relplan_parts(RelPlan, Name/_, _, Columns, _, ColumnTypes),
    (   declared_type_name(Types, Name)
    ->  rel_row_id(RelIdMap, Name, RelId),
        rel_module(Modules, Name, RelHash, RelModuleId),
        length(Columns, ColumnCount),
        RefCount is ColumnCount + 2,
        relplan_storage_name(RelPlan, StorageName),
        format(atom(RefTable), '__ref_~w', [StorageName]),
        rel_h_id(RelHash, RefTable, RefCount, RefHId),
        schema_hash(Columns, ColumnTypes, none, RefSchema),
        RefRow = row(Id0, RelModuleId, 0, RefTable, dictionary, RelId,
                     RefCount, RelModuleId, RefHId, RefSchema, ''),
        Id1 is Id0 + 1,
        RefRows = [RefRow]
    ;   RefRows = [], Id1 = Id0
    ),
    catalog_ref_dict_rows(Mode, Rest, Types, RelIdMap, Modules,
                          Id1, RestRows, IdFinal),
    append(RefRows, RestRows, Rows).

% ── level-statement planes ────────────────────────────────────────────────
% Per level head: the refcount family (refcount, refcount staging, and the
% recursive-recursion expand/dred waves) and the aggregate family (scope, avg
% accumulator). Existence mirrors ref_count_ddl/2 and aggregate_scope_ddl/2
% clause for clause, keyed off the levelstmt's RefCountSql and AggregateSql.
catalog_level_plane_rows(_RelPlans, [], _RelIdMap, _Modules, Id, [], Id).
catalog_level_plane_rows(RelPlans, [Stmt | Rest], RelIdMap, Modules,
                         Id0, Rows, IdFinal) :-
    Stmt = levelstmt(HeadRef, _, _, _, RefCountSql, AggregateSql, _),
    HeadRef = Name/RelArity,
    rel_row_id(RelIdMap, Name, RelId),
    rel_module(Modules, Name, RelHash, RelModuleId),
    rel_h_id(RelHash, Name, RelArity, RelHId),
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    catalog_one_level_stmt_planes(HeadRef, RelId, RelHId, RelModuleId, Columns,
                                  ColumnTypes, RefCountSql, AggregateSql,
                                  Id0, ThisRows, Id1),
    catalog_level_plane_rows(RelPlans, Rest, RelIdMap, Modules,
                             Id1, RestRows, IdFinal),
    append(ThisRows, RestRows, Rows).

catalog_one_level_stmt_planes(HeadRef, RelId, RelHId, ModuleId, Columns,
                              ColumnTypes, RefCountSql, AggregateSql,
                              Id0, Rows, IdFinal) :-
    (   RefCountSql \== none
    ->  ref_count_table_name(HeadRef, RefCountTable),
        arrival_scratch_table_name(HeadRef, NewTable),
        RefPairs = [refcount-RefCountTable, refcount_staging-NewTable],
        (   RefCountSql = refcountsql(_, _, _, _, _, _, _, _, _, _, _,
                                      ExpandPlan, DredPlan, _, _, _),
            ExpandPlan = expandplan(_, _, _, _, _, _, _, _)
        ->  expand_table_name(HeadRef, a, TableA),
            expand_table_name(HeadRef, b, TableB),
            ExpandPairs = [expand-TableA, expand-TableB],
            (   DredPlan == none
            ->  DredPairs = []
            ;   dred_ping_table_name(HeadRef, PingTable),
                dred_pong_table_name(HeadRef, PongTable),
                dred_cone_table_name(HeadRef, ConeTable),
                DredPairs = [dred-PingTable, dred-PongTable, dred-ConeTable]
            )
        ;   ExpandPairs = [], DredPairs = []
        )
    ;   RefPairs = [], ExpandPairs = [], DredPairs = []
    ),
    append([RefPairs, ExpandPairs, DredPairs], HeadPairs),
    catalog_level_family_rows(HeadPairs, Columns, ColumnTypes, ModuleId,
                              RelId, RelHId, Id0, HeadRows, IdAfterHead),
    aggregate_scope_table_name(HeadRef, ScopeTable),
    (   AggregateSql = aggsql(ScopeColumns, ScopeTypes, _, _, _, _, _)
    ->  ScopePairs = [scope-ScopeTable]
    ;   AggregateSql = avgsql(ScopeColumns, ScopeTypes, _, _, _, _, _)
    ->  avg_accumulator_table_name(HeadRef, AccTable),
        ScopePairs = [scope-ScopeTable, avg_accumulator-AccTable]
    ;   ScopePairs = [], ScopeColumns = [], ScopeTypes = []
    ),
    catalog_level_family_rows(ScopePairs, ScopeColumns, ScopeTypes, ModuleId,
                              RelId, RelHId, IdAfterHead, ScopeRows, IdFinal),
    append(HeadRows, ScopeRows, Rows).

% catalog_level_family(Kind, ExtraArity, SchemaSource): the row shape each
% level-plane family mints, ExtraArity counting columns past the source list.
catalog_level_family(refcount,         1, none).
catalog_level_family(refcount_staging, 1, none).
catalog_level_family(expand,           0, hashed).
catalog_level_family(dred,             0, hashed).
catalog_level_family(scope,            0, hashed).
catalog_level_family(avg_accumulator,  2, none).

% Kind-Table pairs in mint order; ids stride one per pair, so the emission
% order here IS the id order the DDL mint sites walk.
catalog_level_family_rows([], _Columns, _ColumnTypes, _ModuleId, _RelId,
                          _RelHId, Id, [], Id).
catalog_level_family_rows([Kind-Table | Rest], Columns, ColumnTypes, ModuleId,
                          RelId, RelHId, Id0, [Row | More], IdFinal) :-
    catalog_level_family(Kind, ExtraArity, SchemaSource),
    length(Columns, ColumnCount),
    Arity is ColumnCount + ExtraArity,
    rel_h_id(RelHId, Table, Arity, HId),
    (   SchemaSource == hashed
    ->  schema_hash(Columns, ColumnTypes, none, Schema)
    ;   Schema = ''
    ),
    Row = row(Id0, RelId, 0, Table, Kind, 0, Arity, ModuleId, HId, Schema, ''),
    Id1 is Id0 + 1,
    catalog_level_family_rows(Rest, Columns, ColumnTypes, ModuleId, RelId,
                              RelHId, Id1, More, IdFinal).

% ── host port rows ────────────────────────────────────────────────────────
% A sh_decl is an effect: it mints a port row (the demand rel in type_id, the
% declared INPUT count as arity) plus a port_response child (the response rel,
% the declared OUTPUT count).
catalog_port_plane_rows([], _RelIdMap, _Modules, Id, [], Id).
catalog_port_plane_rows([Decl | Rest], RelIdMap, Modules,
                        Id0, Rows, IdFinal) :-
    Modules = modules(ModuleHash, ModuleId, _),
    (   Decl = sh_decl(Name, Inputs, Outputs, template(_)),
        atom_concat('__host_demand_', Name, DemandName),
        atom_concat('__host_response_', Name, ResponseName)
    ->  rel_row_id(RelIdMap, DemandName, DemandId),
        rel_row_id(RelIdMap, ResponseName, ResponseId),
        length(Inputs, InputCount),
        length(Outputs, OutputCount),
        rel_h_id(ModuleHash, Name, InputCount, PortHId),
        rel_h_id(ModuleHash, ResponseName, OutputCount, ResponseHId),
        PortRow = row(Id0, ModuleId, 0, Name, port, DemandId, InputCount,
                      ModuleId, PortHId, '', ''),
        Id1 is Id0 + 1,
        ResponseRow = row(Id1, Id0, 0, ResponseName, port_response,
                          ResponseId, OutputCount, ModuleId, ResponseHId,
                          '', ''),
        Id2 is Id1 + 1,
        ThisRows = [PortRow, ResponseRow]
    ;   ThisRows = [], Id2 = Id0
    ),
    catalog_port_plane_rows(Rest, RelIdMap, Modules, Id2,
                            RestRows, IdFinal),
    append(ThisRows, RestRows, Rows).

% ── storage rows ──────────────────────────────────────────────────────────
% One storage child row per column row: local_name interned_id when the column
% is interned (interned_column/2), else raw_characters. The column j of a rel
% whose rel row is at id R sits at R+j (catalog_column_rows/9's positional id
% assignment), so the storage row parents on that column row's own id.
catalog_storage_rows(_Mode, [], _RelIdMap, _Modules, Id, [], Id).
catalog_storage_rows(Mode, [RelPlan | Rest], RelIdMap, Modules,
                     Id0, Rows, IdFinal) :-
    relplan_parts(RelPlan, Name/RelArity, _, Columns, _, ColumnTypes),
    rel_row_id(RelIdMap, Name, RelId),
    rel_module(Modules, Name, RelHash, RelModuleId),
    rel_h_id(RelHash, Name, RelArity, RelHId),
    catalog_one_rel_storage(Mode, Columns, ColumnTypes, RelHId, RelId, 1,
                            RelModuleId, Id0, RelStorage, IdAfterRel),
    catalog_storage_rows(Mode, Rest, RelIdMap, Modules,
                         IdAfterRel, RestRows, IdFinal),
    append(RelStorage, RestRows, Rows).

catalog_one_rel_storage(_Mode, [], _ColumnTypes, _RelHId, _RelId, _Ordinal,
                        _ModuleId, Id, [], Id).
catalog_one_rel_storage(Mode, [ColumnName | RestColumns], ColumnTypes,
                        RelHId, RelId, Ordinal, ModuleId, Id0, Rows,
                        IdFinal) :-
    nth1(Ordinal, ColumnTypes, ColumnType),
    (   ColumnType = idref(_)
    ->  LocalName = relation_id
    ;   interned_column(Mode, ColumnType)
    ->  LocalName = interned_id
    ;   LocalName = raw_characters
    ),
    ColumnId is RelId + Ordinal,
    rel_h_id(RelHId, ColumnName, 0, ColumnHId),
    rel_h_id(ColumnHId, LocalName, 0, StorageHId),
    StorageRow = row(Id0, ColumnId, Ordinal, LocalName, storage, 0, 0,
                     ModuleId, StorageHId, '', ''),
    NextId is Id0 + 1,
    NextOrdinal is Ordinal + 1,
    catalog_one_rel_storage(Mode, RestColumns, ColumnTypes, RelHId, RelId,
                            NextOrdinal, ModuleId, NextId, RestRows,
                            IdFinal),
    Rows = [StorageRow | RestRows].

%! catalog_rows(+ModuleName, +Rules, +RelPlans, -Rows) is det.
%   The relplan carries each column's full type, ref(_) and json_list(_) included.
%   The decl half only; the plane half is appended by catalog_all_rows/10.
catalog_rows(ModuleName, Rules, RelPlans, AllRows) :-
    catalog_decl_rows(ModuleName, Rules, RelPlans, [], AllRows, _).

%! catalog_decl_rows(+ModuleName, +Rules, +RelPlans, +Decls, -Rows, -Context)
%!                   is det.
%   Rows are the byte-stable decl blocks; Context carries the id layout the
%   plane half needs (module id/hash, the rel and list id maps, and FinalId,
%   the id one past the last decl row).
catalog_decl_rows(ModuleName, Rules, RelPlans, Decls, AllRows, Context) :-
    short_hash(ModuleName, ModuleHash),
    catalog_rel_plans(Decls, RelPlans, CatalogRelPlans, CatalogRelModules),
    rule_bodies_map(Rules, BodiesMap),
    catalog_primitive_rows(1, PrimitiveRows),
    length(PrimitiveRows, PrimitiveCount),
    ListStartId is PrimitiveCount + 1,
    catalog_list_types(CatalogRelPlans, ListTypes),
    catalog_list_id_map(ListTypes, ListStartId, ListIdMap),
    length(ListTypes, ListCount),
    ModuleId is ListStartId + ListCount,
    ModuleRow = row(ModuleId, 0, 0, ModuleName, module, 0, 0, ModuleId, ModuleHash, '', ''),
    SpliceStartId is ModuleId + 1,
    catalog_spliced_module_rows(Decls, ModuleHash, SpliceStartId,
                                SplicedRows, SplicedModules, FirstRelId),
    Modules0 = [mod(ModuleName, ModuleHash, ModuleId) | SplicedModules],
    module_id_by_hash(Modules0, HashIdMap),
    catalog_rel_module_ids(CatalogRelModules, HashIdMap, CatalogRelModulesWithIds),
    rel_module_map(Decls, HashIdMap, RelModuleMap),
    Modules = modules(ModuleHash, ModuleId, RelModuleMap),
    catalog_rel_id_map(CatalogRelPlans, FirstRelId, [], RelIdMap),
    % A relational list element can BE a rel, so the row's element id is only
    % resolvable once the rel ids exist; the id layout came from the count.
    catalog_list_rows(ListTypes, ListIdMap, RelIdMap, ListStartId, ListRowRows),
    catalog_rel_block_end(CatalogRelPlans, FirstRelId, RelBlockEnd),
    catalog_path_tree(Decls, RelIdMap, ModuleId, ModuleHash, RelBlockEnd,
                      NestMap, RoomRows, IdAfterRooms),
    catalog_module_edge_rows(Decls, HashIdMap, IdAfterRooms, EdgeRows,
                             IdAfterEdges),
    catalog_type_metadata_rows(Decls, ModuleId, RelIdMap, ListIdMap,
                               IdAfterEdges, TypeRows, FinalId),
    catalog_rel_rows(CatalogRelPlans, CatalogRelModulesWithIds, BodiesMap, Modules, RelIdMap,
                     ListIdMap, NestMap, FirstRelId, _, RelRows),
    append([PrimitiveRows, ListRowRows, [ModuleRow], SplicedRows, RelRows,
            RoomRows, EdgeRows, TypeRows],
           AllRows),
    Context = ctx(Modules, RelIdMap, ListIdMap, FinalId).

catalog_type_metadata_rows(Decls, ModuleId, RelIdMap, ListIdMap, Id0, Rows,
                           IdFinal) :-
    findall(Name-Parameters,
            member(interface_decl(Name, Parameters), Decls), Interfaces),
    metadata_named_rows(Interfaces, interface, ModuleId, Id0,
                        InterfaceRows, InterfaceMap, Id1),
    semantic_rows_from_decls(Decls, SemanticRows),
    findall(generic(Name, Parameters, Specs),
            semantic_generic(SemanticRows, Name, Parameters, Specs),
            GenericDefinitions),
    findall(Name-Parameters,
            member(generic(Name, Parameters, _), GenericDefinitions), Generics),
    metadata_named_rows(Generics, generic_rel, ModuleId, Id1,
                        GenericRows, GenericMap, Id2),
    metadata_parameter_rows(Interfaces, InterfaceMap, InterfaceMap, Id2,
                            InterfaceParameterRows, Id3),
    metadata_parameter_rows(Generics, GenericMap, InterfaceMap, Id3,
                            GenericParameterRows, Id4),
    metadata_generic_column_rows(GenericDefinitions, GenericMap, RelIdMap, ListIdMap,
                                 GenericParameterRows, Id4,
                                 GenericColumnRows, Id4a),
    findall(instance(Concrete, Generic, Arguments),
            semantic_generic_instance(SemanticRows, Concrete, Generic, Arguments),
            Instances),
    metadata_instance_rows(Instances, GenericMap, RelIdMap, ListIdMap, Id4a,
                           InstanceRows, Id5),
    metadata_anonymous_rows(SemanticRows, RelIdMap, Id5, AnonymousRows, Id6),
    metadata_derived_relation_rows(SemanticRows, RelIdMap, Id6,
                                   DerivedRelationRows, IdFinal),
    append([InterfaceRows, GenericRows, InterfaceParameterRows,
            GenericParameterRows, GenericColumnRows, InstanceRows,
            AnonymousRows, DerivedRelationRows], RawRows),
    annotate_catalog_semantic_ids(RawRows, RawRows, SemanticRows, Rows).

semantic_rows_from_decls(Decls, Rows) :-
    ( member(semantic_type_rows(Rows0), Decls) -> Rows = Rows0 ; Rows = [] ).

%! semantic_generic(+Rows, -Name, -Parameters, -Specs) is nondet.
%   The surface view of one compile-time relation, read back off the graph.
semantic_generic(Rows, Name, Parameters, Specs) :-
    member(declaration(Owner, _, Name, relation, compile_time), Rows),
    findall(Ordinal-Parameter,
            ( member(parameter(ParameterId, Owner, Ordinal, ParameterName), Rows),
              findall(Constraint,
                      ( member(Row, Rows),
                        semantic_constraint_surface(Row, ParameterId,
                                                    Constraint) ),
                      Constraints),
              Parameter = type_parameter(ParameterName, Constraints) ),
            ParameterPairs),
    keysort(ParameterPairs, OrderedParameters),
    pairs_values(OrderedParameters, Parameters),
    findall(Ordinal-column(ColumnName, Type),
            ( member(member(_, Owner, Ordinal, ColumnName, TypeRef), Rows),
              semantic_surface_type(Rows, TypeRef, Type) ),
            SpecPairs),
    keysort(SpecPairs, OrderedSpecs),
    pairs_values(OrderedSpecs, Specs).

semantic_surface_type(Rows, type_ref(parameter(ParameterId)), Name) :-
    member(parameter(ParameterId, _, _, Name), Rows), !.
semantic_surface_type(_, type_ref(primitive(Type)), Type) :- !.
semantic_surface_type(_, type_ref(named(Type)), Type) :- !.
semantic_surface_type(_, type_ref(declaration(DeclId)), Name) :-
    id_kind_name(DeclId, relation, Name), !.
semantic_surface_type(_, Type, Type).

semantic_constraint_surface(constraint(_, ParameterId, InterfaceId), ParameterId,
                            InterfaceName) :-
    id_kind_name(InterfaceId, interface, InterfaceName).
semantic_constraint_surface(constraint(_, ParameterId, InterfaceId, Patterns),
                            ParameterId, Application) :-
    id_kind_name(InterfaceId, interface, InterfaceName),
    Application =.. [InterfaceName | Patterns].

%! semantic_generic_instance(+Rows, -Concrete, -Generic, -Arguments) is nondet.
semantic_generic_instance(Rows, Concrete, Generic, Arguments) :-
    member(derived_from(ConcreteId, ApplicationId), Rows),
    member(application(ApplicationId, ConstructorId), Rows),
    member(declaration(ConcreteId, _, Concrete, relation, materialized), Rows),
    member(declaration(ConstructorId, _, Generic, relation, compile_time), Rows),
    semantic_application_arguments(Rows, ApplicationId, Arguments).

semantic_application_arguments(Rows, ApplicationId, Arguments) :-
    findall(Ordinal-Argument,
            ( member(argument(_, ApplicationId, Ordinal, TypeRef), Rows),
              semantic_argument_type(Rows, TypeRef, Argument) ),
            Pairs),
    keysort(Pairs, Ordered),
    pairs_values(Ordered, Arguments).

semantic_argument_type(_, type_atom(Type), Type).
semantic_argument_type(Rows, type_application(ApplicationId), Application) :-
    member(application(ApplicationId, ConstructorId), Rows),
    member(declaration(ConstructorId, _, Name, relation, _), Rows),
    semantic_application_arguments(Rows, ApplicationId, Arguments),
    Application =.. [Name | Arguments].

annotate_catalog_semantic_ids([], _, _, []).
annotate_catalog_semantic_ids([Row | Rest], AllRows, SemanticRows,
                              [Annotated | AnnotatedRest]) :-
    catalog_semantic_id(Row, AllRows, SemanticRows, SemanticId),
    catalog_semantic_id_text(SemanticId, SemanticText),
    annotate_catalog_row(Row, SemanticText, Annotated),
    annotate_catalog_semantic_ids(Rest, AllRows, SemanticRows, AnnotatedRest).

catalog_semantic_id_text('', '') :- !.
catalog_semantic_id_text(SemanticId, Text) :- semantic_type_id_text(SemanticId, Text).

annotate_catalog_row(row(Id, Parent, Ordinal, Name, Kind, TypeId, Arity,
                         ModuleId, Hash, _Extra, Extra2), SemanticId,
                     row(Id, Parent, Ordinal, Name, Kind, TypeId, Arity,
                         ModuleId, Hash, SemanticId, Extra2)).

catalog_semantic_id(row(_, _, _, Name, interface, _, _, _, _, _, _), _, Rows, Id) :-
    member(declaration(Id, _, Name, interface, _), Rows), !.
catalog_semantic_id(row(_, _, _, Name, generic_rel, _, _, _, _, _, _), _, Rows, Id) :-
    member(declaration(Id, _, Name, relation, compile_time), Rows), !.
catalog_semantic_id(row(_, OwnerId, Ordinal, Name, type_parameter, _, _, _, _, _, _),
                    AllRows, Rows, Id) :-
    semantic_owner_id(OwnerId, AllRows, Rows, Owner),
    member(parameter(Id, Owner, Ordinal, Name), Rows), !.
catalog_semantic_id(row(_, OwnerId, Ordinal, Name, generic_column, _, _, _, _, _, _),
                    AllRows, Rows, Id) :-
    semantic_owner_id(OwnerId, AllRows, Rows, Owner),
    member(member(Id, Owner, Ordinal, Name, _), Rows), !.
catalog_semantic_id(row(_, ParameterId, _, Name, constraint, _, _, _, _, _, Patterns),
                    AllRows, Rows, Id) :-
    semantic_parameter_id(ParameterId, AllRows, Rows, Parameter),
    id_kind_name(Interface, interface, Name),
    ( Patterns == [], member(constraint(Id, Parameter, Interface), Rows)
    ; member(constraint(Id, Parameter, Interface, Patterns), Rows)
    ), !.
catalog_semantic_id(row(_, _, _, Name, concrete_type, _, _, _, _, _, _), _, Rows, Id) :-
    member(declaration(Id, _, Name, relation, materialized), Rows), !.
catalog_semantic_id(_, _, _, '').

semantic_owner_id(CatalogId, AllRows, Rows, SemanticId) :-
    member(row(CatalogId, _, _, Name, generic_rel, _, _, _, _, _, _), AllRows),
    member(declaration(SemanticId, _, Name, relation, compile_time), Rows), !.
semantic_owner_id(CatalogId, AllRows, Rows, SemanticId) :-
    member(row(CatalogId, _, _, Name, interface, _, _, _, _, _, _), AllRows),
    member(declaration(SemanticId, _, Name, interface, _), Rows), !.

semantic_parameter_id(CatalogId, AllRows, Rows, SemanticId) :-
    member(row(CatalogId, OwnerId, Ordinal, Name, type_parameter, _, _, _, _, _, _), AllRows),
    semantic_owner_id(OwnerId, AllRows, Rows, Owner),
    member(parameter(SemanticId, Owner, Ordinal, Name), Rows), !.

metadata_named_rows([], _, _, Id, [], [], Id).
metadata_named_rows([Name-_ | Rest], Kind, ModuleId, Id0,
                    [row(Id0, ModuleId, 0, Name, Kind, 0, 0, ModuleId,
                         '', '', '') | Rows],
                    [Name-Id0 | Map], IdFinal) :-
    Id1 is Id0 + 1,
    metadata_named_rows(Rest, Kind, ModuleId, Id1, Rows, Map, IdFinal).

metadata_parameter_rows([], _, _, Id, [], Id).
metadata_parameter_rows([Name-Parameters | Rest], OwnerMap, InterfaceMap, Id0,
                        Rows, IdFinal) :-
    memberchk(Name-OwnerId, OwnerMap),
    metadata_one_parameter_set(Parameters, OwnerId, InterfaceMap, 1, Id0,
                               ParameterRows, Id1),
    metadata_parameter_rows(Rest, OwnerMap, InterfaceMap, Id1, RestRows,
                            IdFinal),
    append(ParameterRows, RestRows, Rows).

metadata_one_parameter_set([], _, _, _, Id, [], Id).
metadata_one_parameter_set([Parameter | Rest], OwnerId, InterfaceMap, Ordinal,
                           Id0, Rows, IdFinal) :-
    metadata_parameter_parts(Parameter, ParameterName, Constraints),
    ParameterRow = row(Id0, OwnerId, Ordinal, ParameterName, type_parameter,
                       0, 0, 0, '', '', ''),
    Id1 is Id0 + 1,
    metadata_constraint_rows(Constraints, Id0, InterfaceMap, 1, Id1,
                             ConstraintRows, Id2),
    NextOrdinal is Ordinal + 1,
    metadata_one_parameter_set(Rest, OwnerId, InterfaceMap, NextOrdinal, Id2,
                               RestRows, IdFinal),
    append([[ParameterRow], ConstraintRows, RestRows], Rows).

metadata_parameter_parts(type_parameter(Name, Constraints), Name, Constraints) :- !.
metadata_parameter_parts(Name, Name, []).

metadata_constraint_rows([], _, _, _, Id, [], Id).
metadata_constraint_rows([Interface | Rest], ParameterId, InterfaceMap,
                         Ordinal, Id0,
                         [row(Id0, ParameterId, Ordinal, InterfaceName, constraint,
                              InterfaceId, 0, 0, '', '', Patterns) | Rows], IdFinal) :-
    interface_application_parts(Interface, InterfaceName, Patterns),
    ( memberchk(InterfaceName-InterfaceId, InterfaceMap) -> true ; InterfaceId = 0 ),
    Id1 is Id0 + 1,
    NextOrdinal is Ordinal + 1,
    metadata_constraint_rows(Rest, ParameterId, InterfaceMap, NextOrdinal,
                             Id1, Rows, IdFinal).

interface_application_parts(Application, Name, []) :-
    atom(Application), !,
    Name = Application.
interface_application_parts(Application, Name, Arguments) :-
    compound(Application),
    Application =.. [Name | Arguments].

metadata_generic_column_rows([], _, _, _, _, Id, [], Id).
metadata_generic_column_rows([generic(Name, _Parameters, Specs) | Rest], GenericMap, RelIdMap,
                             ListIdMap, ParameterRows, Id0, Rows, IdFinal) :-
    memberchk(Name-GenericId, GenericMap),
    metadata_one_generic_columns(Specs, GenericId, ParameterRows, RelIdMap,
                                 ListIdMap, 1, Id0, ColumnRows, Id1),
    metadata_generic_column_rows(Rest, GenericMap, RelIdMap, ListIdMap,
                                 ParameterRows, Id1, RestRows, IdFinal),
    append(ColumnRows, RestRows, Rows).

metadata_one_generic_columns([], _, _, _, _, _, Id, [], Id).
metadata_one_generic_columns([column(Name, Type) | Rest], GenericId,
                             ParameterRows, RelIdMap, ListIdMap, Ordinal, Id0,
                             [row(Id0, GenericId, Ordinal, Name, generic_column,
                                  TypeId, 0, 0, '', '', '') | Rows], IdFinal) :-
    metadata_generic_type_id(Type, GenericId, ParameterRows, RelIdMap,
                             ListIdMap, TypeId),
    Id1 is Id0 + 1,
    NextOrdinal is Ordinal + 1,
    metadata_one_generic_columns(Rest, GenericId, ParameterRows, RelIdMap,
                                 ListIdMap, NextOrdinal, Id1, Rows, IdFinal).

metadata_generic_type_id(Type, GenericId, ParameterRows, _, _, TypeId) :-
    atom(Type),
    memberchk(row(TypeId, GenericId, _, Type, type_parameter,
                  _, _, _, _, _, _), ParameterRows),
    !.
metadata_generic_type_id(Type, _, _, RelIdMap, ListIdMap, TypeId) :-
    catalog_source_type_id(Type, RelIdMap, ListIdMap, TypeId).

metadata_instance_rows([], _, _, _, Id, [], Id).
metadata_instance_rows([instance(Concrete, Generic, Arguments) | Rest],
                       GenericMap, RelIdMap, ListIdMap, Id0, Rows, IdFinal) :-
    memberchk(Generic-GenericId, GenericMap),
    memberchk(Concrete-ConcreteRelId, RelIdMap),
    InstanceRow = row(Id0, ConcreteRelId, 0, Concrete, concrete_type,
                      GenericId, 0, 0, '', '', ''),
    Id1 is Id0 + 1,
    metadata_argument_rows(Arguments, Id0, RelIdMap, ListIdMap, 1, Id1,
                           ArgumentRows, Id2),
    metadata_instance_rows(Rest, GenericMap, RelIdMap, ListIdMap, Id2,
                           RestRows, IdFinal),
    append([[InstanceRow], ArgumentRows, RestRows], Rows).

metadata_argument_rows([], _, _, _, _, Id, [], Id).
metadata_argument_rows([Argument | Rest], InstanceId, RelIdMap, ListIdMap,
                       Ordinal, Id0,
                       [row(Id0, InstanceId, Ordinal, argument, type_argument,
                            TypeId, 0, 0, '', '', '') | Rows], IdFinal) :-
    catalog_source_type_id(Argument, RelIdMap, ListIdMap, TypeId),
    Id1 is Id0 + 1,
    NextOrdinal is Ordinal + 1,
    metadata_argument_rows(Rest, InstanceId, RelIdMap, ListIdMap, NextOrdinal,
                           Id1, Rows, IdFinal).

% Anonymous product/sum generated types are marked concrete so the catalog and
% type emitters treat them as reachable user types rather than compiler
% internals.  The marker comes from the semantic origin row
% (derived_from(GeneratedId, anonymous(...))), never from the `__` name prefix.
metadata_anonymous_rows(SemanticRows, RelIdMap, Id0, Rows, IdFinal) :-
    findall(Name,
            member(derived_from(named(_, _, Name), anonymous(_, _, _)),
                   SemanticRows),
            Names0),
    sort(Names0, Names),
    metadata_anonymous_rows_(Names, RelIdMap, Id0, Rows, IdFinal).

metadata_anonymous_rows_([], _, Id, [], Id).
metadata_anonymous_rows_([Name | Rest], RelIdMap, Id0,
                         [row(Id0, RelId, 0, Name, concrete_type,
                              0, 0, 0, '', '', '') | Rows],
                         IdFinal) :-
    rel_row_id(RelIdMap, Name, RelId),
    Id1 is Id0 + 1,
    metadata_anonymous_rows_(Rest, RelIdMap, Id1, Rows, IdFinal).
metadata_anonymous_rows_([_Name | Rest], RelIdMap, Id0, Rows, IdFinal) :-
    metadata_anonymous_rows_(Rest, RelIdMap, Id0, Rows, IdFinal).

% A compiler-derived relation is a concrete application whose constructor is
% identified by a return member. Its generated name uses the same helper
% prefix as generic instances, so target renderers need the concrete marker.
metadata_derived_relation_rows(SemanticRows, RelIdMap, Id0, Rows, IdFinal) :-
    findall(Name,
            ( member(derived_from(ConcreteId, ApplicationId), SemanticRows),
              member(application(ApplicationId, ConstructorId), SemanticRows),
              member(declaration(ConcreteId, _, Name, relation, materialized),
                     SemanticRows),
              member(member(ReturnMemberId, ConstructorId, _, return, _),
                     SemanticRows),
              member(member_role(ReturnMemberId, return), SemanticRows) ),
            Names0),
    sort(Names0, Names),
    metadata_derived_relation_rows_(Names, RelIdMap, Id0, Rows, IdFinal).

metadata_derived_relation_rows_([], _, Id, [], Id).
metadata_derived_relation_rows_([Name | Rest], RelIdMap, Id0,
                                [row(Id0, RelId, 0, Name, concrete_type,
                                     0, 0, 0, '', '', '') | Rows],
                                IdFinal) :-
    rel_row_id(RelIdMap, Name, RelId),
    !,
    Id1 is Id0 + 1,
    metadata_derived_relation_rows_(Rest, RelIdMap, Id1, Rows, IdFinal).
metadata_derived_relation_rows_([_ | Rest], RelIdMap, Id0, Rows, IdFinal) :-
    metadata_derived_relation_rows_(Rest, RelIdMap, Id0, Rows, IdFinal).

catalog_source_type_id(json_list(Element), _RelIdMap, ListIdMap, TypeId) :- !,
    ( list_row_id(ListIdMap, json_list(Element), TypeId) -> true ; TypeId = 0 ).
catalog_source_type_id(list(Element), _RelIdMap, ListIdMap, TypeId) :- !,
    ( list_row_id(ListIdMap, list(Element), TypeId) -> true ; TypeId = 0 ).
catalog_source_type_id(Type, RelIdMap, _ListIdMap, TypeId) :-
    atom(Type),
    ( catalog_type_id(Type, PrimitiveId), PrimitiveId =\= 0
    -> TypeId = PrimitiveId
    ; ( rel_row_id(RelIdMap, Type, TypeId) -> true ; TypeId = 0 ) ), !.
catalog_source_type_id(Application, RelIdMap, _ListIdMap, TypeId) :-
    canonical_type_name(Application, Concrete),
    ( rel_row_id(RelIdMap, Concrete, TypeId) -> true ; TypeId = 0 ).

catalog_rel_plans(Decls, RelPlans, CatalogRelPlans, CatalogRelModules) :-
    module_rel_columns(Decls, ModuleRelColumns),
    maplist(catalog_rel_plan(ModuleRelColumns), RelPlans,
            CatalogPlanLists, CatalogModuleLists),
    append(CatalogPlanLists, CatalogRelPlans),
    append(CatalogModuleLists, CatalogRelModules).

module_rel_columns(Decls, ModuleRelColumns) :-
    module_rel_columns(Decls, [], ModuleRelColumns).

module_rel_columns([], _ModuleDecls, []).
module_rel_columns([module_decl(_, _) | Rest], ModuleDecls, ModuleRelColumns) :-
    !,
    reverse(ModuleDecls, OrderedModuleDecls),
    module_column_source(OrderedModuleDecls, ColumnSource),
    take_module_rel_decls(Rest, ColumnSource, ThisModuleRows, More),
    module_rel_columns(More, [], MoreModuleRows),
    append(ThisModuleRows, MoreModuleRows, ModuleRelColumns).
module_rel_columns([Decl | Rest], ModuleDecls0, ModuleRelColumns) :-
    module_rel_columns(Rest, [Decl | ModuleDecls0], ModuleRelColumns).

% One reverse and one grouping pass per module. Every rel plan reaching
% module_rel_declared_columns/3 used to reverse and re-walk the same
% declaration list: pokeapi walked an 848-term list twice for each of 224 rel
% plans, 20.9 ms. A non-ground column reference cannot be a group key without
% changing which declarations member/2's unification reaches, so a module
% carrying one keeps the walk.
module_column_source(OrderedModuleDecls, cols(Groups, OrderedModuleDecls)) :-
    findall(Ref-(Column-Type),
            member(col_type(Ref, Column, Type), OrderedModuleDecls),
            Pairs),
    (   forall(member(GroupRef-_, Pairs), ground(GroupRef))
    ->  keysort(Pairs, Sorted),
        group_pairs_by_key(Sorted, Groups)
    ;   Groups = unkeyed
    ).

take_module_rel_decls([rel_module_decl(Name, Hash) | Rest], ColumnSource,
                     [module_rel(Name, Hash, ColumnSource) | MoreRows], More) :-
    !,
    take_module_rel_decls(Rest, ColumnSource, MoreRows, More).
take_module_rel_decls(Rest, _ColumnSource, [], Rest).

catalog_rel_plan(ModuleRelColumns, RelPlan, CatalogPlans, CatalogModules) :-
    relplan_parts(RelPlan, Name/Arity, Kind, _Columns, KeyOrNone, _ColumnTypes),
    findall(Hash-Columns,
            ( member(module_rel(Name, Hash, ModuleDecls), ModuleRelColumns),
              module_rel_declared_columns(ModuleDecls, Name/Arity, Columns) ),
            ModuleColumns),
    ( ModuleColumns = [_, _ | _]
    -> maplist(catalog_module_rel_plan(Name/Arity, Kind, KeyOrNone),
               ModuleColumns, CatalogPlans, CatalogModules)
    ; CatalogPlans = [RelPlan], CatalogModules = [none]
    ).

module_rel_declared_columns(cols(Groups, OrderedModuleDecls), Ref, Columns) :-
    (   Groups \== unkeyed,
        ground(Ref)
    ->  ( memberchk(Ref-Columns, Groups) -> true ; Columns = [] )
    ;   findall(Column-Type,
                member(col_type(Ref, Column, Type), OrderedModuleDecls),
                Columns)
    ),
    Columns \== [].

catalog_module_rel_plan(Ref, Kind, KeyOrNone, Hash-ColumnTypes,
                        rel(Ref, Kind, Columns, KeyOrNone), module(Hash)) :-
    maplist(catalog_declared_column, ColumnTypes, Columns).

catalog_declared_column(Name-int, col(Name, declared(int), int)).
catalog_declared_column(Name-float, col(Name, declared(float), float)).
catalog_declared_column(Name-text, col(Name, declared(text), text)).
catalog_declared_column(Name-bool, col(Name, declared(bool), bool)).
catalog_declared_column(Name-json, col(Name, declared(json), json)).
catalog_declared_column(Name-json_list(Element),
                        col(Name, declared(json_list(Element)), json_list(Element))).
catalog_declared_column(Name-list(Element),
                        col(Name, declared(list(Element)), list(Element))).
catalog_declared_column(Name-id(Type), col(Name, declared(id(Type)), idref(Type))).
catalog_declared_column(Name-Type, col(Name, declared(Type), ref(Type))).

catalog_rel_module_ids([], _HashIdMap, []).
catalog_rel_module_ids([none | Rest], HashIdMap, [none | More]) :-
    catalog_rel_module_ids(Rest, HashIdMap, More).
catalog_rel_module_ids([module(Hash) | Rest], HashIdMap,
                       [module(Hash, Id) | More]) :-
    memberchk(Hash-Id, HashIdMap),
    catalog_rel_module_ids(Rest, HashIdMap, More).

% One module row per FILE. The entry's row is minted above, so a single-file
% program mints nothing here and its byte layout does not move.
catalog_spliced_module_rows(Decls, ModuleHash, Id0, Rows, SplicedModules,
                            IdEnd) :-
    findall(Name-Hash,
            ( member(module_decl(Name, Hash), Decls), Hash \== ModuleHash ),
            Spliced0),
    sort(Spliced0, Spliced),
    spliced_module_rows(Spliced, Id0, Rows, SplicedModules, IdEnd).

spliced_module_rows([], Id, [], [], Id).
spliced_module_rows([Name-Hash | Rest], Id0, [Row | More],
                    [mod(Name, Hash, Id0) | MoreModules], IdEnd) :-
    Row = row(Id0, 0, 0, Name, module, 0, 0, Id0, Hash, '', ''),
    Id1 is Id0 + 1,
    spliced_module_rows(Rest, Id1, More, MoreModules, IdEnd).

module_id_by_hash(Modules, Map) :-
    findall(Hash-Id, member(mod(_, Hash, Id), Modules), Map).

% Decls carry the used files first, so a rel both a used module and the entry
% declare keys under the module that declared it first.
rel_module_map(Decls, HashIdMap, RelModuleMap) :-
    findall(Name-mod(Hash, Id),
            ( member(rel_module_decl(Name, Hash), Decls),
              memberchk(Hash-Id, HashIdMap) ),
            Pairs),
    first_per_key(Pairs, [], RelModuleMap).

first_per_key([], Acc, Map) :- reverse(Acc, Map).
first_per_key([Name-Module | Rest], Acc, Map) :-
    (   memberchk(Name-_, Acc)
    ->  first_per_key(Rest, Acc, Map)
    ;   first_per_key(Rest, [Name-Module | Acc], Map)
    ).

% A rel keys under the module that DECLARED it; an undeclared rel, and every
% rel of a single-file program, keys under the entry.
rel_module(modules(ModuleHash, ModuleId, RelModuleMap), Name, RelHash,
           RelModuleId) :-
    (   memberchk(Name-mod(Hash, Id), RelModuleMap)
    ->  RelHash = Hash, RelModuleId = Id
    ;   RelHash = ModuleHash, RelModuleId = ModuleId
    ).

% The module graph as ordinary rows: parent_id is the CONSUMER's module row,
% module_id the PRODUCER's, kind use or mount, local_name the alias or name.
catalog_module_edge_rows(Decls, HashIdMap, Id0, Rows, IdEnd) :-
    findall(Kind-LocalName-ConsumerHash-ProducerHash,
            member(module_edge_decl(ConsumerHash, ProducerHash, Kind,
                                    LocalName),
                   Decls),
            Edges0),
    sort(Edges0, Edges),
    module_edge_rows(Edges, HashIdMap, Id0, Rows, IdEnd).

module_edge_rows([], _, Id, [], Id).
module_edge_rows([Edge | Rest], HashIdMap, Id0, [Row | More], IdEnd) :-
    Edge = Kind-LocalName-ConsumerHash-ProducerHash,
    memberchk(ConsumerHash-ConsumerId, HashIdMap),
    memberchk(ProducerHash-ProducerId, HashIdMap),
    module_edge_h_id(ConsumerHash, ProducerHash, Kind, LocalName, EdgeHId),
    Row = row(Id0, ConsumerId, 0, LocalName, Kind, 0, 0, ProducerId, EdgeHId,
              '', ''),
    Id1 is Id0 + 1,
    module_edge_rows(Rest, HashIdMap, Id1, More, IdEnd).

% BOTH endpoints enter the edge identity, so an edge says which resolved
% position it connects and a re-parented consumer mints a different edge.
module_edge_h_id(ConsumerHash, ProducerHash, Kind, LocalName, EdgeHId) :-
    format(atom(Key), '~w/~w/~w/~w',
           [ConsumerHash, Kind, LocalName, ProducerHash]),
    short_hash(Key, EdgeHId).

catalog_primitive_rows(StartId, PrimitiveRows) :-
catalog_primitive_rows(StartId, [text, int, float, bool, json, bytes], [], PrimitiveRows).

catalog_primitive_rows(_, [], Acc, Rows) :- reverse(Acc, Rows).
catalog_primitive_rows(Id, [Name | Rest], Acc, Rows) :-
    NextId is Id + 1,
    catalog_primitive_rows(NextId, Rest, [row(Id, 0, 0, Name, primitive, 0, 0, 0, '', '', '') | Acc], Rows).

% Distinct list column types of both spellings, in first-appearance order, inner
% before outer so a nested list's row id exists before the outer row cites it.
catalog_list_types(RelPlans, OrderedTypes) :-
    findall(ListType,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, _, _, _, _, ColumnTypes),
              member(ColumnType, ColumnTypes),
              list_subtypes(ColumnType, SubTypes),
              member(ListType, SubTypes) ),
            Wide),
    distinct_order(Wide, [], Distinct),
    maplist([L, D-L] >> list_type_depth(L, D), Distinct, Keyed),
    keysort(Keyed, Sorted),
    pairs_values(Sorted, OrderedTypes).

% Every list type a column is or nests, inner-most last in the tail, so a
% nested list reaches the catalog as its own row before the column.
list_subtypes(json_list(Element), [json_list(Element) | More]) :- list_subtypes(Element, More).
list_subtypes(list(Element), [list(Element) | More]) :- list_subtypes(Element, More).
list_subtypes(_, []).

distinct_order([], _, []).
distinct_order([X | Rest], Seen0, Out) :-
    ( memberchk(X, Seen0)
    -> distinct_order(Rest, Seen0, Out)
    ;  distinct_order(Rest, [X | Seen0], OutTail), Out = [X | OutTail]
    ).

list_type_depth(json_list(Inner), Depth) :- !, list_type_depth(Inner, InnerDepth), Depth is InnerDepth + 1.
list_type_depth(list(Inner), Depth) :- !, list_type_depth(Inner, InnerDepth), Depth is InnerDepth + 1.
list_type_depth(_, 0).

% One id per list type, assigned by position so the block's width is known
% before the element ids are resolvable.
catalog_list_id_map([], _Id, []).
catalog_list_id_map([ListType | Rest], Id, [ListType-Id | More]) :-
    NextId is Id + 1,
    catalog_list_id_map(Rest, NextId, More).

% A list row's type_id is the ELEMENT's id: a nested list resolves through the
% list id map, a rel element through the rel id map, anything else through the
% primitive table.
catalog_list_rows([], _ListIdMap, _RelIdMap, _Id, []).
catalog_list_rows([ListType | Rest], ListIdMap, RelIdMap, Id, [Row | MoreRows]) :-
    list_row_kind(ListType, Kind, Element),
    list_element_type_id(Element, ListIdMap, RelIdMap, ElementId),
    format(atom(LocalName), '~w', [ListType]),
    NextId is Id + 1,
    Row = row(Id, 0, 0, LocalName, Kind, ElementId, 0, 0, '', '', ''),
    catalog_list_rows(Rest, ListIdMap, RelIdMap, NextId, MoreRows).

list_row_kind(list(id(Element)), relation_id_list, Element) :- !.
list_row_kind(json_list(Element), json_list, Element).
list_row_kind(list(Element), list, Element).

list_row_id(ListIdMap, ListType, Id) :- memberchk(ListType-Id, ListIdMap).

list_element_type_id(json_list(Inner), ListIdMap, _RelIdMap, TypeId) :- !,
    list_row_id(ListIdMap, json_list(Inner), TypeId).
list_element_type_id(list(Inner), ListIdMap, _RelIdMap, TypeId) :- !,
    list_row_id(ListIdMap, list(Inner), TypeId).
list_element_type_id(id(Element), _ListIdMap, RelIdMap, TypeId) :- !,
    rel_row_id(RelIdMap, Element, TypeId).
list_element_type_id(Element, _ListIdMap, RelIdMap, TypeId) :-
    catalog_type_id(Element, PrimitiveId),
    (   PrimitiveId =\= 0
    ->  TypeId = PrimitiveId
    ;   rel_row_id(RelIdMap, Element, TypeId)
    ->  true
    ;   TypeId = 0
    ).

% Pass A: rel names and their ids, assigned by position, each rel consuming one
% row plus one row per column exactly as pass B emits them.
catalog_rel_id_map([], _Id, Acc, Acc).
catalog_rel_id_map([RelPlan | Rest], Id0, Acc0, Acc) :-
    relplan_parts(RelPlan, Name/RelArity, _, _, _, _),
    IdAfterRel is Id0 + 1 + RelArity,
    ( memberchk(Name-_, Acc0) -> Acc1 = Acc0 ; Acc1 = [Name-Id0 | Acc0] ),
    catalog_rel_id_map(Rest, IdAfterRel, Acc1, Acc).

% Every caller binds Name and takes the first solution; member/2 left the
% catalog build carrying a choicepoint per ref column.
rel_row_id(RelIdMap, Name, Id) :- memberchk(Name-Id, RelIdMap).

% The rel block's width, so the room rows the nesting needs can take ids past
% it without moving a single existing rel or column row.
catalog_rel_block_end([], Id, Id).
catalog_rel_block_end([RelPlan | Rest], Id0, Id) :-
    relplan_parts(RelPlan, _/RelArity, _, _, _, _),
    Id1 is Id0 + 1 + RelArity,
    catalog_rel_block_end(Rest, Id1, Id).

catalog_rel_rows([], [], _BodiesMap, _Modules, _RelIdMap, _ListIdMap,
                 _NestMap, Id, Id, []).
catalog_rel_rows([RelPlan | Rest], [RelModule | ModuleRest], BodiesMap, Modules, RelIdMap,
                 ListIdMap, NestMap, Id0, FinalId, Rows) :-
    relplan_parts(RelPlan, Name/RelArity, _Kind, Columns, KeyOrNone, ColumnTypes),
    catalog_rel_module(Modules, RelModule, Name, RelHash, RelModuleId),
    rel_h_id(RelHash, Name, RelArity, RelHId),
    schema_hash(Columns, ColumnTypes, KeyOrNone, HSchema),
    rule_hash(BodiesMap, Name/RelArity, HRule),
    catalog_rel_scope(NestMap, RelModuleId, Name, ParentId, LocalName),
    RelRow = row(Id0, ParentId, 0, LocalName, rel, 0, RelArity, RelModuleId,
                 RelHId, HSchema, HRule),
    IdAfterRel is Id0 + 1,
    catalog_column_rows(Columns, ColumnTypes,
                        RelIdMap, ListIdMap, RelModuleId, RelHId, Id0, 1,
                        IdAfterRel, IdAfterColumns, ColumnRows),
    catalog_rel_rows(Rest, ModuleRest, BodiesMap, Modules, RelIdMap, ListIdMap,
                     NestMap, IdAfterColumns, FinalId, RestRows),
    append([RelRow | ColumnRows], RestRows, Rows).

catalog_rel_module(_Modules, module(Hash, ModuleId), _Name, Hash, ModuleId) :- !.
catalog_rel_module(Modules, none, Name, RelHash, RelModuleId) :-
    rel_module(Modules, Name, RelHash, RelModuleId).

% A nested rel's local_name is its own SEGMENT, which is what makes the
% (__rel_parent) index on (parent_id, local_name) a per-parent child lookup.
catalog_rel_scope(NestMap, ModuleId, Name, ParentId, LocalName) :-
    (   memberchk(Name-nest(NestedParentId, Segment), NestMap)
    ->  ParentId = NestedParentId,
        LocalName = Segment
    ;   ParentId = ModuleId,
        LocalName = Name
    ).

% ── the containment tree ──────────────────────────────────────────────────

% rel_path_decl/2 is the authority; the `__` join is a NAME, never a structure
% to re-derive by splitting.
catalog_path_tree(Decls, RelIdMap, ModuleId, ModuleHash, StartId,
                  NestMap, RoomRows, EndId) :-
    findall(Segments-Name,
            member(rel_path_decl(Name/_, Segments), Decls),
            Paths0),
    sort(Paths0, Paths),
    (   Paths == []
    ->  NestMap = [], RoomRows = [], EndId = StartId
    ;   path_room_prefixes(Paths, RelIdMap, Rooms),
        room_rows(Rooms, Paths, RelIdMap, ModuleId, ModuleHash, StartId,
                  [], RoomIdMap, RoomRows, EndId),
        path_nest_map(Paths, Paths, RelIdMap, RoomIdMap, ModuleId, NestMap)
    ).

% Shallow first, so a room's own parent room already carries an id.
path_room_prefixes(Paths, RelIdMap, Rooms) :-
    findall(Length-Prefix,
            ( member(Segments-_, Paths),
              proper_path_prefix(Segments, Prefix),
              \+ path_prefix_rel_id(Prefix, Paths, RelIdMap, _),
              length(Prefix, Length) ),
            Keyed0),
    sort(Keyed0, Keyed),
    findall(Prefix, member(_-Prefix, Keyed), Rooms).

proper_path_prefix(Segments, Prefix) :-
    append(Prefix, Tail, Segments),
    Prefix \== [],
    Tail \== [].

% append/3 splitting a bound list leaves a choicepoint the whole catalog build
% would then carry; the split is unique, so it is cut here.
path_split(Segments, ParentPrefix, LocalName) :-
    append(ParentPrefix, [LocalName], Segments),
    !.

path_prefix_rel_id(Prefix, Paths, RelIdMap, Id) :-
    (   memberchk(Prefix-Name, Paths)
    ->  true
    ;   Prefix = [Name]
    ),
    once(rel_row_id(RelIdMap, Name, Id)).

% An interior segment no decl of its own names still needs a row, else a
% child's parent chain stops at nothing.
room_rows([], _, _, _, _, Id, RoomIdMap, RoomIdMap, [], Id).
room_rows([Prefix | Rest], Paths, RelIdMap, ModuleId, ModuleHash, Id0,
          RoomIdMap0, RoomIdMap, [Row | More], EndId) :-
    path_split(Prefix, ParentPrefix, LocalName),
    path_scope_id(ParentPrefix, Paths, RelIdMap, RoomIdMap0, ModuleId,
                  ParentId),
    atomic_list_concat(Prefix, '.', PathAtom),
    rel_h_id(ModuleHash, PathAtom, 0, RoomHId),
    Row = row(Id0, ParentId, 0, LocalName, rel, 0, 0, ModuleId, RoomHId,
              '', ''),
    Id1 is Id0 + 1,
    room_rows(Rest, Paths, RelIdMap, ModuleId, ModuleHash, Id1,
              [Prefix-Id0 | RoomIdMap0], RoomIdMap, More, EndId).

path_scope_id([], _, _, _, ModuleId, ModuleId) :- !.
path_scope_id(Prefix, Paths, RelIdMap, RoomIdMap, _, Id) :-
    (   path_prefix_rel_id(Prefix, Paths, RelIdMap, DeclaredId)
    ->  Id = DeclaredId
    ;   memberchk(Prefix-Id, RoomIdMap)
    ).

path_nest_map([], _, _, _, _, []).
path_nest_map([Segments-Name | Rest], Paths, RelIdMap, RoomIdMap, ModuleId,
              [Name-nest(ParentId, LocalName) | More]) :-
    path_split(Segments, ParentPrefix, LocalName),
    path_scope_id(ParentPrefix, Paths, RelIdMap, RoomIdMap, ModuleId,
                  ParentId),
    path_nest_map(Rest, Paths, RelIdMap, RoomIdMap, ModuleId, More).

catalog_column_rows([], _ColumnTypes, _RelIdMap, _ListIdMap,
                    _ModuleId, _ParentHId, _RelId, _Ordinal, Id, Id, []).
catalog_column_rows([ColumnName | RestColumns], ColumnTypes,
                    RelIdMap, ListIdMap, ModuleId, ParentHId, RelId, Ordinal,
                    Id0, IdFinal, [ColumnRow | MoreRows]) :-
    nth1(Ordinal, ColumnTypes, ColumnType),
    catalog_column_type_id(ColumnType, RelIdMap, ListIdMap, TypeId),
    rel_h_id(ParentHId, ColumnName, 0, ColumnHId),
    NextId is Id0 + 1,
    NextOrdinal is Ordinal + 1,
    catalog_column_rows(RestColumns, ColumnTypes,
                        RelIdMap, ListIdMap, ModuleId, ParentHId, RelId,
                        NextOrdinal, NextId, IdFinal, MoreRows),
    ColumnRow = row(Id0, RelId, Ordinal, ColumnName, column, TypeId, 0, ModuleId,
                    ColumnHId, '', '').

% ref(_) and json_list(_) reach here already resolved by column_storage/3, so the
% relplan column type is the authority; nothing needs the declaration again.
catalog_column_type_id(json_list(Element), _RelIdMap, ListIdMap, TypeId) :- !,
    list_row_id(ListIdMap, json_list(Element), TypeId).
catalog_column_type_id(ref(Name), RelIdMap, _ListIdMap, TypeId) :- !,
    rel_row_id(RelIdMap, Name, TypeId).
catalog_column_type_id(idref(Name), RelIdMap, _ListIdMap, TypeId) :- !,
    rel_row_id(RelIdMap, Name, TypeId).
catalog_column_type_id(list(Element), _RelIdMap, ListIdMap, TypeId) :- !,
    list_row_id(ListIdMap, list(Element), TypeId).
catalog_column_type_id(ColumnType, _RelIdMap, _ListIdMap, TypeId) :-
    catalog_type_id(ColumnType, TypeId).

% ref(_) and json_list(_) are resolved upstream, to a target rel id and a synthetic
% row id; any other boundary resolves to 0.
catalog_type_id(text, 1) :- !.
catalog_type_id(int, 2) :- !.
catalog_type_id(float, 3) :- !.
catalog_type_id(bool, 4) :- !.
catalog_type_id(json, 5) :- !.
catalog_type_id(bytes, 6) :- !.
catalog_type_id(_, 0).

catalog_row_part(Mode, row(RelId, ParentId, Ordinal, Name, Kind, TypeId, Arity,
                           ModuleId, HId, HSchema, HRule), Acc, [Part | Acc]) :-
    catalog_text_sql(Mode, Name, NameLiteral),
    catalog_text_sql(Mode, Kind, KindLiteral),
    catalog_text_sql(Mode, HId, HIdLiteral),
    catalog_text_sql(Mode, HSchema, HSchemaLiteral),
    catalog_text_sql(Mode, HRule, HRuleLiteral),
    format(atom(Part), '(~d,~d,~d,~w,~w,~d,~d,~d,~w,~w,~w)',
           [RelId, ParentId, Ordinal, NameLiteral, KindLiteral, TypeId, Arity,
            ModuleId, HIdLiteral, HSchemaLiteral, HRuleLiteral]).

% The one spelling literal_seed_ddl/3 reads back, so the seed's own strings
% reach "__str" instead of landing raw in an INTEGER column.
catalog_text_sql(Mode, Text, Sql) :-
    (   interned_column(Mode, text)
    ->  interned_literal_sql(Text, Sql)
    ;   sql_text_literal(Text, Sql)
    ).

% SQL string literal: single-quoted, embedded single quotes doubled.
sql_text_literal(Text, Literal) :-
    atom_codes(Text, Codes),
    double_single_quotes(Codes, EscapedCodes),
    append([0''' | EscapedCodes], [0'''], LiteralCodes),
    atom_codes(Literal, LiteralCodes).

double_single_quotes([], []).
double_single_quotes([39 | Rest], [39, 39 | More]) :- !, double_single_quotes(Rest, More).
double_single_quotes([Code | Rest], [Code | More]) :- double_single_quotes(Rest, More).

% TODO(g2): conformance/ticklog.pl needs the same seed only once a FIXTURE derives from a catalog row; a DDL-time seed emits no delta at any tick, so g1 alone cannot diverge from the oracle.
% TODO(g3): __catalog_annotation(target_id, name, value) is the ONLY future DDL statement here, because nesting, generics and column types all land as rows in the table above.

compile_guard_goal(Mode, Goal, Bound0-Texts0, Bound-Texts) :-
    ( regexp_goal(Goal)
    -> compile_regexp_goal(Mode, Goal, Bound0, Text),
       Bound = Bound0, Texts = [Text | Texts0]
    ; tick_goal(Goal, Variable)
    -> tick_column_sql(TickSql),
       ( \+ bound_lookup(Bound0, Variable, _)
       -> Bound = [Variable-typed(TickSql, int, direct) | Bound0], Texts = Texts0
       ;  compile_expr(Mode, identity, Variable, Bound0, VariableSql, _, _Encoding),
          format(atom(Text), '~w = ~w', [VariableSql, TickSql]),
          Bound = Bound0, Texts = [Text | Texts0]
       )
    ;  bind_goal(Goal, Variable, Expr)
    -> compile_expr(Mode, identity, Expr, Bound0, Sql, Type, Encoding),
       ( var(Variable), \+ bound_lookup(Bound0, Variable, _)
       -> Bound = [Variable-typed(Sql, Type, Encoding) | Bound0], Texts = Texts0
       ;  compile_expr(Mode, identity, Variable, Bound0, VariableSql, _VariableType,
                       VariableEncoding),
          aligned_pair(VariableEncoding, VariableSql, Encoding, Sql,
                       AlignedVariable, AlignedValue),
          format(atom(Text), '~w = ~w', [AlignedVariable, AlignedValue]),
          Bound = Bound0, Texts = [Text | Texts0]
       )
    ;  guard_goal(Goal)
    -> compile_comparison(Mode, Goal, Bound0, Text),
       Bound = Bound0, Texts = [Text | Texts0]
    ;  throw(unsupported_construct(guard_goal_shape(Goal)))
    ).

regexp_goal(Goal) :-
    body_surface_for_term(Goal, regexp/2, guard, no_refs,
                          wrapper(expr_pair, lower), _).

compile_regexp_goal(Mode, regexp(Operand, Pattern), Bound, Text) :-
    compile_expr(Mode, value, Operand, Bound, OperandSql, OperandType, _Encoding),
    ( OperandType == text
    -> true
    ;  throw(unsupported_construct(regexp_operand_not_text(Operand, OperandType)))
    ),
    ( string(Pattern)
    -> sql_literal(Pattern, PatternSql)
    ;  throw(unsupported_construct(regexp_pattern_not_literal))
    ),
    format(atom(Text), '(~w REGEXP ~w)', [OperandSql, PatternSql]).

% engine.pl solve_comparison/1: `< =< > >=` run through eval_int2/4, so BOTH
% operands must be integers or the reference engine throws arith_on_non_int
% -- SQLite would instead compare a TEXT-affinity value against an INTEGER
% one under its own affinity rules and answer something. `==`/`\==` are
% eval_expr then Prolog ==/2, term identity, so `1 == '1'` is FALSE there;
% SQLite's `=` between an INTEGER column and a TEXT one applies affinity and
% can answer TRUE. Both cases are refused by name rather than lowered to an
% operator that means something else.
compile_comparison(Mode, Goal, Bound, Text) :-
    Goal =.. [Operator, Left, Right],
    compile_expr(Mode, identity, Left, Bound, LeftSql, LeftType, LeftEncoding),
    compile_expr(Mode, identity, Right, Bound, RightSql, RightType, RightEncoding),
    comparison_operator_sql(Operator, Goal, LeftType, RightType, OperatorSql),
    aligned_pair(LeftEncoding, LeftSql, RightEncoding, RightSql,
                 AlignedLeft, AlignedRight),
    format(atom(Text), '(~w ~w ~w)', [AlignedLeft, OperatorSql, AlignedRight]).

% Family, SQL text and type rule all come from registry.pl's expression/5
% (rank R5). The two type rules are named there: both_int for the ordered
% comparisons (the Int-only law), same_type for the identity ones. The
% unsupported construct terms are unchanged, and an operator with no row still refuses by
% name rather than lowering to something that means something else.
comparison_operator_sql(Operator, Goal, LeftType, RightType, OperatorSql) :-
    expression(Operator/2, Family, _, infix(OperatorSql), TypeRule),
    memberchk(Family, [ordered_comparison, identity_comparison]),
    !,
    check_comparison_types(TypeRule, Goal, LeftType, RightType).
comparison_operator_sql(Operator, Goal, _, _, _) :-
    throw(unsupported_construct(unknown_comparison_operator(Goal, Operator))).

check_comparison_types(both_int, Goal, LeftType, RightType) :-
    (   LeftType == int, RightType == int
    ->  true
    ;   throw(unsupported_construct(
                  comparison_operand_not_int(Goal, LeftType, RightType)))
    ).
check_comparison_types(both_number, Goal, LeftType, RightType) :-
    (   memberchk(LeftType, [int, float]),
        memberchk(RightType, [int, float])
    ->  true
    ;   throw(unsupported_construct(
                  comparison_operand_not_number(Goal, LeftType, RightType)))
    ).
check_comparison_types(same_type, Goal, LeftType, RightType) :-
    (   LeftType == RightType
    ->  true
    ;   throw(unsupported_construct(
                  comparison_type_mismatch(Goal, LeftType, RightType)))
    ).

% BuiltValues is the content SQL of every head position that had to be interned
% on write (contract §5.7): the caller owes each one an intern statement.
% ListInterns is the list(T)-typed head positions whose value is an interned
% list id: the caller owes each one the content + member intern statements.
head_select_list(Mode, ColumnTypes, Head, Bound, ColumnAliases, SelectExprs,
                 BuiltValues, ListInterns) :-
    Head =.. [_ | Args],
    maplist(head_column_expr(Mode, Bound), Args, ColumnTypes, SelectExprs0,
            InternGroups),
    append(InternGroups, Interns),
    partition_head_interns(Interns, BuiltValues, ListInterns),
    ( is_list(ColumnAliases)
    -> maplist(alias_select_expr, SelectExprs0, ColumnAliases, SelectExprs)
    ; SelectExprs = SelectExprs0
    ).

head_column_expr(Mode, Bound, Arg, ColumnType, SelectExpr, Interns) :-
    compile_expr(Mode, identity, Arg, Bound, Sql, _Type, Encoding),
    (   Encoding = list_intern(ElementType, ArraySql)
    ->  SelectExpr = Sql, Interns = [list_intern(ElementType, ArraySql)]
    ;   column_encoding(Mode, ColumnType, dict), Encoding == direct
    ->  interned_id_sql(Sql, SelectExpr), Interns = [built_text(Sql)]
    ;   SelectExpr = Sql, Interns = []
    ).

partition_head_interns([], [], []).
partition_head_interns([built_text(Sql) | Rest], [Sql | Built], ListInterns) :-
    !, partition_head_interns(Rest, Built, ListInterns).
partition_head_interns([list_intern(ElementType, ArraySql) | Rest], Built,
                       [list_intern(ElementType, ArraySql) | ListInterns]) :-
    partition_head_interns(Rest, Built, ListInterns).

intern_write_statements([], _, _, []) :- !.
intern_write_statements(BuiltValues, FromSql, WhereSql, [InternSql]) :-
    intern_write_sql(BuiltValues, FromSql, WhereSql, InternSql).

% Statement one of §5.7.1: every built string the arm will produce, set-based,
% reusing the arm's own FROM and WHERE so the two see identical input.
intern_write_sql(BuiltValues, FromSql, WhereSql, InternSql) :-
    maplist(intern_write_arm(FromSql, WhereSql), BuiltValues, Arms),
    atomic_list_concat(Arms, ' UNION ', ArmsSql),
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(InternSql), 'INSERT OR IGNORE INTO ~w ("content") ~w',
           [QuotedDictionary, ArmsSql]).

intern_write_arm(none, none, ValueSql, Arm) :- !,
    format(atom(Arm), 'SELECT ~w', [ValueSql]).
intern_write_arm(none, WhereSql, ValueSql, Arm) :- !,
    format(atom(Arm), 'SELECT ~w WHERE ~w', [ValueSql, WhereSql]).
intern_write_arm(FromSql, none, ValueSql, Arm) :- !,
    format(atom(Arm), 'SELECT DISTINCT ~w FROM ~w', [ValueSql, FromSql]).
intern_write_arm(FromSql, WhereSql, ValueSql, Arm) :-
    format(atom(Arm), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [ValueSql, FromSql, WhereSql]).

alias_select_expr(Expr, Alias, AliasedExpr) :- format(atom(AliasedExpr), '~w AS "~w"', [Expr, Alias]).

% Statement order is forced: the content text and each member value reach
% "__str" before the entity and member rows that read their ids back out.
list_intern_statements([], _, _, []) :- !.
list_intern_statements(ListInterns, FromSql, WhereSql, Statements) :-
    maplist(list_intern_statement(FromSql, WhereSql), ListInterns, StatementLists),
    append(StatementLists, Statements).

list_intern_statement(FromSql, WhereSql, list_intern(ElementType, ArraySql),
                      Statements) :-
    canonical_type_name(list(ElementType), EntityName),
    table_name(EntityName/1, EntityTable),
    list_member_ref(EntityName, MemberRef),
    table_name(MemberRef, MemberTable),
    quote_ident(EntityTable, QuotedEntity),
    quote_ident(MemberTable, QuotedMember),
    interned_id_sql(ArraySql, ContentIdSql),
    list_intern_from(FromSql, From),
    list_intern_where(WhereSql, Where),
    member_intern_from(FromSql, ArraySql, MemberFrom),
    intern_write_sql([ArraySql], FromSql, WhereSql, ContentInternSql),
    % Order by ArraySql (raw text), never ContentIdSql: the "__str" id orders
    % by string-arrival, not content, and would desync from the oracle's sort.
    format(atom(EntityInternSql),
           'INSERT OR IGNORE INTO ~w ("content") SELECT DISTINCT ~w~w~w ORDER BY ~w',
           [QuotedEntity, ContentIdSql, From, Where, ArraySql]),
    list_member_intern_sql(ElementType, QuotedMember, MemberFrom, QuotedEntity,
                           ContentIdSql, Where, MemberStatements),
    append([ContentInternSql, EntityInternSql], MemberStatements, Statements).

% Relation endpoints are already-local INTEGER identities. Preserve the JSON
% array's order and duplicates in the ordinary indexed member relation without
% routing the endpoint through the string dictionary.
list_member_intern_sql(id(_), QuotedMember, MemberFrom, QuotedEntity,
                       ContentIdSql, Where, [MemberInsertSql]) :- !,
    list_relation_id_where(Where, TypedWhere),
    format(atom(MemberInsertSql),
           'INSERT OR IGNORE INTO ~w ("list_id", "idx", "value") SELECT e."__id", i.key, i.value FROM ~w JOIN ~w e ON e."content" = ~w~w ON CONFLICT ("list_id", "idx") DO NOTHING',
           [QuotedMember, MemberFrom, QuotedEntity, ContentIdSql, TypedWhere]).
list_member_intern_sql(ElementType, QuotedMember, MemberFrom, QuotedEntity,
                       ContentIdSql, Where,
                       [MemberValueInternSql, MemberInsertSql]) :-
    ElementType \= id(_),
    format(atom(MemberValueInternSql),
           'INSERT OR IGNORE INTO "__str" ("content") SELECT DISTINCT i.value FROM ~w~w',
           [MemberFrom, Where]),
    format(atom(MemberInsertSql),
           'INSERT OR IGNORE INTO ~w ("list_id", "idx", "value") SELECT e."__id", i.key, s."__id" FROM ~w JOIN ~w e ON e."content" = ~w JOIN "__str" s ON s."content" = i.value~w ON CONFLICT ("list_id", "idx") DO NOTHING',
           [QuotedMember, MemberFrom, QuotedEntity, ContentIdSql, Where]).

list_relation_id_where('', ' WHERE i.type = ''integer''') :- !.
list_relation_id_where(Where, TypedWhere) :-
    format(atom(TypedWhere), '~w AND i.type = ''integer''', [Where]).

list_intern_from(none, '') :- !.
list_intern_from(FromSql, From) :- format(atom(From), ' FROM ~w', [FromSql]).

list_intern_where(none, '') :- !.
list_intern_where(WhereSql, Where) :- format(atom(Where), ' WHERE ~w', [WhereSql]).

member_intern_from(none, ArraySql, From) :- !,
    format(atom(From), 'json_each(~w) i', [ArraySql]).
member_intern_from(FromSql, ArraySql, From) :-
    format(atom(From), '~w, json_each(~w) i', [FromSql, ArraySql]).

% ═══ interning (plans/2026-08-08-interning-contract.md rev 3) ══════════════
% Threaded, never a flag: a runtime toggle cannot undo a declared column type.

% intern_mode(+Options, -Mode) is det.
%   A compile input, defaulted here and recorded in the emitted artifact.
intern_mode(Options, Mode) :-
    ( memberchk(intern(Requested), Options) -> Mode = Requested ; Mode = dict ).

% interned_column(+Mode, +DeclaredType)
%   json stays TEXT: json1 reads it in place.
interned_column(dict, text).

string_dictionary_table('__str').

% rowid + UNIQUE, not WITHOUT ROWID: `__id` is read once per boundary render
% per column, and that read is `SEARCH s USING INTEGER PRIMARY KEY`.
intern_ddl(dict, [ 'CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)' ]) :- !.
intern_ddl(_, []).

% column_encoding(+Mode, +DeclaredType, -Encoding)
%   dict: the SQL holds an "__str" id. direct: it holds the characters.
column_encoding(Mode, ColumnType, dict) :- interned_column(Mode, ColumnType), !.
column_encoding(_, _, direct).

any_interned_column(Mode, ColumnTypes) :-
    member(ColumnType, ColumnTypes),
    interned_column(Mode, ColumnType),
    !.

program_intern_ddl(Mode, RelPlans, Ddl) :-
    (   member(RelPlan, RelPlans),
        relplan_parts(RelPlan, _, _, _, _, ColumnTypes),
        any_interned_column(Mode, ColumnTypes)
    ->  intern_ddl(Mode, Ddl)
    ;   Ddl = []
    ).

% ═══ text constants in the id space (contract §5.3, rule two) ══════════════

% Splicing the id itself would make the emitted text a function of the
% database. EXPLAIN puts the lookup behind `Once`: one probe per STATEMENT.
text_literal_sql(Mode, identity, Literal, Sql, dict) :-
    interned_column(Mode, text),
    !,
    interned_literal_sql(Literal, Sql).
text_literal_sql(_, _, Literal, Sql, direct) :- sql_literal(Literal, Sql).

interned_literal_sql(Literal, Sql) :-
    sql_literal(Literal, Quoted),
    interned_id_sql(Quoted, Sql).

% The one id-lookup spelling, shared by the constant path and the built-string
% path so the seed reader below cannot drift from either.
interned_id_sql(ContentSql, Sql) :-
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    atomic_list_concat(['(SELECT s."__id" FROM ', QuotedDictionary,
                        ' s WHERE s."content" = ', ContentSql, ')'], Sql).

% A text COLUMN under `value` demand holds an id; the string functions need
% the characters (contract §5.3, rule one).
dictionary_content_sql(IdSql, Sql) :-
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(Sql), '(SELECT s."content" FROM ~w s WHERE s."__id" = ~w)',
           [QuotedDictionary, IdSql]).

% A literal no stored row holds resolves to no id, so the comparison matches
% nothing -- the correct answer, and the reason the read side needs no seed.
column_literal_sql(dict, Literal, Sql) :- !, interned_literal_sql(Literal, Sql).
column_literal_sql(_, Literal, Sql) :- sql_literal(Literal, Sql).

% A WRITE side needs the row to exist: an interned head column is NOT NULL, so
% an unseeded literal would resolve to NULL and lose the row.
literal_seed_ddl(Mode, Lowered, Ddl) :-
    (   interned_column(Mode, text),
        interned_literals(Lowered, Literals),
        Literals \== []
    ->  maplist(sql_literal, Literals, Quoted),
        maplist(values_row, Quoted, Rows),
        atomic_list_concat(Rows, ', ', RowsSql),
        string_dictionary_table(Dictionary),
        quote_ident(Dictionary, QuotedDictionary),
        format(atom(Sql), 'INSERT OR IGNORE INTO ~w ("content") VALUES ~w',
               [QuotedDictionary, RowsSql]),
        Ddl = [Sql]
    ;   Ddl = []
    ).

values_row(Quoted, Row) :- format(atom(Row), '(~w)', [Quoted]).

% Read back out of the SQL the lowering just wrote, never recomputed from the
% rules: one spelling produced them, so one reader cannot drift from it.
interned_literals(Lowered, Literals) :-
    findall(Literal, term_interned_literal(Lowered, Literal), Found),
    sort(Found, Literals).

term_interned_literal(Term, Literal) :-
    (   atom(Term)
    ->  atom_interned_literal(Term, Literal)
    ;   compound(Term),
        arg(_, Term, Argument),
        term_interned_literal(Argument, Literal)
    ).

% sql_literal/2 refuses a quote inside a literal, so `')` terminates the
% content at its first occurrence and no escape grammar is involved.
atom_interned_literal(Atom, Literal) :-
    interned_literal_sql('', Probe),
    sub_atom(Probe, 0, OpeningLength, 2, Opening),
    sub_atom(Atom, Start, OpeningLength, _, Opening),
    After is Start + OpeningLength,
    sub_atom(Atom, After, _, 0, Tail),
    once(sub_atom(Tail, ContentLength, 2, _, '\')')),
    sub_atom(Tail, 0, ContentLength, _, Literal).

% ═══ the decode view, returned in its table's own Ddls list ═════════════════
% One clause builds both from one Columns/ColumnTypes pair, so they cannot drift.

text_view_name(Table, ViewName) :-
    atomic_list_concat(['__txt_', Table], ViewName).

% Correlated scalar subquery, not a FROM-clause join: the same expression text
% drops into any SELECT list with no restructuring (dictionary_render_expr/3).
text_decode_expr(ColumnSql, Expr) :-
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    atomic_list_concat(['(SELECT s."content" FROM ', QuotedDictionary,
                        ' s WHERE s."__id" = ', ColumnSql, ')'], Expr).

text_view_column_expr(Mode, Column, ColumnType, Expr) :-
    quote_ident(Column, QuotedColumn),
    atomic_list_concat(['t.', QuotedColumn], ColumnSql),
    (   interned_column(Mode, ColumnType)
    ->  text_decode_expr(ColumnSql, Decoded),
        atomic_list_concat([Decoded, ' AS ', QuotedColumn], Expr)
    ;   atomic_list_concat([ColumnSql, ' AS ', QuotedColumn], Expr)
    ).

% PassThroughColumns are the table's hidden columns (__id, __refcount, _sign,
% _sequence, _phase): the view is the table's shape with text restored.
text_view_ddl(Mode, Table, Columns, ColumnTypes, PassThroughColumns, Ddl) :-
    text_view_name(Table, ViewName),
    quote_ident(ViewName, QuotedViewName),
    quote_ident(Table, QuotedTable),
    maplist(text_view_column_expr(Mode), Columns, ColumnTypes, ColumnExprs),
    findall(PassExpr,
            ( member(PassColumn, PassThroughColumns),
              quote_ident(PassColumn, QuotedPassColumn),
              atomic_list_concat(['t.', QuotedPassColumn, ' AS ',
                                  QuotedPassColumn], PassExpr) ),
            PassExprs),
    append(ColumnExprs, PassExprs, AllExprs),
    atomic_list_concat(AllExprs, ', ', SelectSql),
    atomic_list_concat(['CREATE TEMP VIEW ', QuotedViewName, ' AS SELECT ',
                        SelectSql, ' FROM ', QuotedTable, ' t'], Ddl).

% [] when nothing is interned, so a program compiled at intern(direct) emits
% no view at all rather than an identity one.
text_view_ddls(Mode, Table, Columns, ColumnTypes, PassThroughColumns, Ddls) :-
    (   any_interned_column(Mode, ColumnTypes)
    ->  text_view_ddl(Mode, Table, Columns, ColumnTypes, PassThroughColumns,
                      Ddl),
        Ddls = [Ddl]
    ;   Ddls = []
    ).

% ═══ the ingest door's intern plan (contract §6) ════════════════════════════
% Two statements, both flat in the number of arriving distinct values. Where
% StructPlane needs three, the third is a same-key/different-row preflight that
% cannot exist here: `__str`'s key IS the whole value.
text_intern_plan(Mode, RelPlans, textintern(InternSql, LookupSql, RelColumns)) :-
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(InternSql),
           'INSERT OR IGNORE INTO ~w ("content") SELECT i.value FROM json_each(?) i',
           [QuotedDictionary]),
    format(atom(LookupSql),
           'SELECT s."content" AS "__lookup", s."__id" AS "__id" FROM json_each(?) i JOIN ~w s ON s."content" = i.value',
           [QuotedDictionary]),
    findall(Name-Flags,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Name/_, _, _, _, ColumnTypes),
              any_interned_column(Mode, ColumnTypes),
              maplist(interned_column_flag(Mode), ColumnTypes, Flags) ),
            RelColumns).

interned_column_flag(Mode, ColumnType, Flag) :-
    ( interned_column(Mode, ColumnType) -> Flag = true ; Flag = false ).

% none when no column in the program is interned, so a direct-mode module
% carries no plan, no import and no statement.
program_text_intern_plan(Mode, RelPlans, Plan) :-
    (   text_intern_plan(Mode, RelPlans, textintern(InternSql, LookupSql, RelColumns)),
        RelColumns \== []
    ->  Plan = textintern(InternSql, LookupSql, RelColumns)
    ;   Plan = none
    ).

% The table a boundary read names: the decode view when one exists.
text_read_table(Mode, Table, ColumnTypes, ReadTable) :-
    (   any_interned_column(Mode, ColumnTypes)
    ->  text_view_name(Table, ReadTable)
    ;   ReadTable = Table
    ).

% ═══ DDL (round 2: no stamp columns, no __prev tables) ══════════════════════
%
% rel_ddl/6 receives the edge-headed, arrival-target, and level-headed refs.
% An edge-headed keyed rel's UPSERT targets `ON CONFLICT(<key columns>)`, and
% SQLite requires that clause to name a constraint on exactly that column
% set. A keyed arrival target needs the same key constraint because
% absorb_set_arrival/5 replaces by key. An unkeyed arrival target retains the
% all-column primary key used for exact-row Set membership.

rel_ddl(Mode, _, _, _, _, RelPlan, Ddls) :-
    relplan_parts(RelPlan, Ref, log, Columns, _, ColumnTypes),
    !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    % Plain rowid table (no PK, no WITHOUT ROWID): a Log rel's duplicate rows
    % are distinct occurrences (engine.pl q1) and must physically coexist as
    % separate rows for multisetDiff to count them correctly.
    format(atom(Ddl), 'CREATE TABLE ~w (~w)', [QuotedTable, ColumnsSql]),
    text_view_ddls(Mode, Table, Columns, ColumnTypes, [], ViewDdls),
    Ddls = [Ddl | ViewDdls].
rel_ddl(Mode, Types, EdgeHeadedRefs, ArrivalTargetRefs, LevelHeadedRefs,
        RelPlan, Ddls) :-
    relplan_parts(RelPlan, Ref, set, Columns, KeyOrNone, ColumnTypes),
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', SelectColumnsSql),
    ( set_rel_pk_sql(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargetRefs,
                     Columns, PkSql) ),
    ( memberchk(Ref, LevelHeadedRefs)
    -> RefCountColumn = ', "__refcount" INTEGER NOT NULL DEFAULT 1',
       RefCountPassThrough = ['__refcount']
    ;  RefCountColumn = '',
       RefCountPassThrough = []
    ),
    Ref = Name/_,
    % One table shape for every set rel: a surrogate __id key plus the content
    % identity as a UNIQUE constraint. The declared-type and ordinary branches
    % differ only in the companion view / index they attach.
    set_rel_table_ddl(QuotedTable, ColumnsSql, RefCountColumn, PkSql, Ddl),
    ( declared_type_name(Types, Name)
    -> dictionary_table_name(Name, ReferenceView),
       quote_ident(ReferenceView, QuotedReferenceView),
       relation_render_expr(Mode, Types, Columns, ColumnTypes, RenderExpr),
       format(atom(ViewDdl),
              'CREATE TEMP VIEW ~w AS SELECT t."__id", ~w, ~w AS "__rendered" FROM ~w t',
              [QuotedReferenceView, SelectColumnsSql, RenderExpr, QuotedTable]),
       TableDdls = [Ddl, ViewDdl],
       PassThroughColumns = ['__id' | RefCountPassThrough]
    ;  ( option_some_table(Ref, Columns, KeyOrNone, EdgeHeadedRefs,
                           ArrivalTargetRefs)
       -> option_some_index_ddl(Table, SomeIndexDdl),
          TableDdls = [Ddl, SomeIndexDdl]
       ;  TableDdls = [Ddl]
       ),
       PassThroughColumns = RefCountPassThrough
    ),
    text_view_ddls(Mode, Table, Columns, ColumnTypes, PassThroughColumns,
                   TextViewDdls),
    append(TableDdls, TextViewDdls, Ddls).

% PHASE C2 RULING 1: INTEGER storage for an int-typed column, TEXT for
% everything else (text columns and compound-term columns alike -- a
% compound value never gets an int witness, see analyze.pl:column_type_at/6,
% so it always falls through to text here, matching the ruling's flat-punt:
% compound-term columns stay inline-flat text, never their own storage
% type).
column_def(_, QuotedColumn, int, Def) :- !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL'], Def).
column_def(_, QuotedColumn, bool, Def) :- !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL CHECK (',
                        QuotedColumn, ' IN (0,1))'], Def).
column_def(_, QuotedColumn, float, Def) :- !,
    atomic_list_concat([QuotedColumn, ' REAL NOT NULL CHECK (typeof(',
                        QuotedColumn, ') = \'real\' AND ', QuotedColumn,
                        ' BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308)'],
                       Def).
column_def(_, QuotedColumn, bytes, Def) :- !,
    atomic_list_concat([QuotedColumn, ' BLOB NOT NULL CHECK (typeof(',
                        QuotedColumn, ') = \'blob\')'], Def).
% A ref column stores the dense target-row id and nothing else. No FOREIGN
% KEY clause and no ON DELETE clause: the retraction
% lab measured SQL cascade deleting a shared child out from under a live
% second parent and leaving dangling refs (types-as-rels verdict finding 6,
% plans/2026-07-28-sqlite-retraction-verdict.md fk_cascade WRONG).
column_def(_, QuotedColumn, ref(_), Def) :- !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL'], Def).
column_def(_, QuotedColumn, idref(_), Def) :- !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL'], Def).
% A relational list column stores its minted entity's id, the ref(_) shape with
% an ordered child set instead of one row.
column_def(_, QuotedColumn, list(_), Def) :- !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL'], Def).
% A list column stores the same TEXT json carrier as a json column, and adds
% the array-ness CHECK the storage kind now survives to emit. The ARRAY-ness
% predicate is verified on both SQLite builds this repo runs.
column_def(_, QuotedColumn, json_list(_), Def) :- !,
    atomic_list_concat([QuotedColumn, ' TEXT NOT NULL CHECK (json_valid(',
                        QuotedColumn, ') AND json_type(', QuotedColumn,
                        ') = \'array\')'], Def).
% A json column stores TEXT with a json_valid CHECK, never jsonb: the two
% SQLite builds this project runs disagree about whether jsonb exists at all
% (system sqlite3 3.43.2 rejects it, the @libsql driver bundles 3.45.1 and
% accepts it), and a storage decision cannot depend on a function only one of
% them has. The CHECK is what lets every json1 read below skip a validity
% guard: json_extract over a column that is not valid JSON RAISES rather than
% returning NULL, so validity has to be an invariant of the column, not a
% per-read conjunct.
column_def(_, QuotedColumn, json, Def) :- !,
    atomic_list_concat([QuotedColumn, ' TEXT NOT NULL CHECK (json_valid(',
                        QuotedColumn, '))'], Def).
% An interned column stores the dictionary id and nothing else; the value is
% restored by text_view_ddl/6 at the boundary, never by a hand-written join.
column_def(Mode, QuotedColumn, Type, Def) :-
    interned_column(Mode, Type),
    !,
    atomic_list_concat([QuotedColumn, ' INTEGER NOT NULL'], Def).
column_def(_, QuotedColumn, text, Def) :-
    atomic_list_concat([QuotedColumn, ' TEXT NOT NULL'], Def).

% ═══ relation reference projection ═════════════════════════════════════════
%
% A referenced relation remains one public table. Typed columns carry the
% entity row, the declared key or full-row fallback is UNIQUE, and hidden
% `__id INTEGER PRIMARY KEY` supplies compact parent endpoints. No semantic or
% rendered JSON column is stored.
%
% `__ref_<name>` is a TEMP view used by decode and boundary rendering. It
% exposes `__id`, typed columns, and a computed `__rendered` expression.

dictionary_table_name(TypeName, Table) :-
    ( physical_storage_name(TypeName/_, StorageName) -> true ; StorageName = TypeName ),
    atomic_list_concat(['__ref_', StorageName], Table).

relation_render_expr(Mode, Types, Columns, ColumnTypes, Expr) :-
    pairs_keys_values(Pairs, Columns, ColumnTypes),
    findall(Part,
            ( member(Column-ColumnType, Pairs),
              sql_literal(Column, ColumnLiteral),
              relation_render_column_expr(Mode, Types, Column, ColumnType, ValueExpr),
              format(atom(Part), '~w, ~w', [ColumnLiteral, ValueExpr]) ),
            Parts),
    atomic_list_concat(Parts, ', ', PartsSql),
    format(atom(Expr), 'json_object(~w)', [PartsSql]).

relation_render_column_expr(_, _, Column, ref(TypeName), Expr) :-
    !,
    quote_ident(Column, QuotedColumn),
    dictionary_table_name(TypeName, ReferenceView),
    quote_ident(ReferenceView, QuotedReferenceView),
    format(atom(Expr),
           'json((SELECT c."__rendered" FROM ~w c WHERE c."__id" = t.~w))',
           [QuotedReferenceView, QuotedColumn]).
relation_render_column_expr(_, _, Column, idref(_), Expr) :-
    !,
    quote_ident(Column, QuotedColumn),
    format(atom(Expr), 't.~w', [QuotedColumn]).
relation_render_column_expr(Mode, _, Column, ColumnType, Expr) :-
    interned_column(Mode, ColumnType),
    !,
    quote_ident(Column, QuotedColumn),
    format(atom(ColumnSql), 't.~w', [QuotedColumn]),
    text_decode_expr(ColumnSql, Expr).
relation_render_column_expr(_, _, Column, _, Expr) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Expr), 't.~w', [QuotedColumn]).

dictionary_storage_kind(Types, DeclaredType, Storage) :-
    column_storage(Types, DeclaredType, Storage).

% The boundary read of a ref column: join the dictionary, select the memoized
% rendering. Written as a correlated scalar subquery rather than a FROM-clause
% join so it drops into delta_statement/2's existing SELECT-expression list
% with no restructuring, and so the same expression text serves the snapshot
% read, the final-state read and the delta read alike.
%
% EXPLAIN receipt (v6/tsv2/tests/structPlane.test.ts): the inner query plans as
% `SEARCH d USING INTEGER PRIMARY KEY (rowid=?)`, never a SCAN.
% Bare, the outer column binds to the CHILD view whenever the two share a
% column name, and the row renders null (docs/failure-modes.md entry 52).
dictionary_render_expr(TypeName, Column, Expr) :-
    dictionary_table_name(TypeName, Table),
    quote_ident(Table, QuotedTable),
    quote_ident(Column, QuotedColumn),
    atomic_list_concat(['(SELECT d."__rendered" FROM ', QuotedTable,
                        ' d WHERE d."__id" = t.', QuotedColumn, ') AS ',
                        QuotedColumn], Expr).

% The member rel's UNIQUE (list_id, idx) carries BOTH the grouping and the
% element order. SQLite 3.43 (the system build) has no in-aggregate ORDER BY,
% so the aggregate consumes an explicitly ordered subquery below. Relying on
% the UNIQUE index's present scan order would make the public list value
% dependent on the query planner and would lose order after a restart or an
% index change.
list_view_name(EntityName, ViewName) :-
    table_name(EntityName/1, StorageName),
    atomic_list_concat(['__list_', StorageName], ViewName).

list_member_ref(EntityName, MemberName/3) :-
    atomic_list_concat([EntityName, member], '__', MemberName).

list_column_alias(Column, Alias) :-
    atomic_list_concat(['__l_', Column], Alias).

list_element_types(RelPlans, Elements) :-
    findall(Element,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, _, _, _, _, ColumnTypes),
              member(list(Element), ColumnTypes) ),
            Elements0),
    sort(Elements0, Elements).

list_view_ddls(Mode, RelPlans, Ddls) :-
    list_element_types(RelPlans, Elements),
    findall(Ddl,
            ( member(Element, Elements),
              list_view_ddl(Mode, RelPlans, Element, Ddl) ),
            Ddls).

list_view_ddl(Mode, RelPlans, Element, Ddl) :-
    canonical_type_name(list(Element), EntityName),
    list_view_name(EntityName, ViewName),
    quote_ident(ViewName, QuotedViewName),
    list_member_ref(EntityName, MemberRef),
    table_name(MemberRef, MemberTable),
    quote_ident(MemberTable, QuotedMemberTable),
    list_member_value_type(RelPlans, MemberRef, ValueType),
    list_element_render(Mode, ValueType, ValueExpr, AggregateValueExpr, JoinSql),
    format(atom(Ddl),
           'CREATE TEMP VIEW ~w AS SELECT m0."list_id" AS "list_id", coalesce((SELECT json_group_array(~w) FROM (SELECT ~w AS "value" FROM ~w m~w WHERE m."list_id" = m0."list_id" ORDER BY m."idx") ordered), ''[]'') AS "value_text" FROM ~w m0 WHERE NOT EXISTS (SELECT 1 FROM ~w m1 WHERE m1."list_id" = m0."list_id" AND m1."idx" < m0."idx")',
           [QuotedViewName, AggregateValueExpr, ValueExpr, QuotedMemberTable,
            JoinSql, QuotedMemberTable, QuotedMemberTable]).

list_member_value_type(RelPlans, MemberRef, ValueType) :-
    member(RelPlan, RelPlans),
    relplan_parts(RelPlan, MemberRef, _, Columns, _, ColumnTypes),
    nth0(Index, Columns, value),
    nth0(Index, ColumnTypes, ValueType),
    !.

% Every arm is a JOIN, never a correlated subquery: the aggregate reads one
% row per member and the planner keeps the member index as the driving scan.
list_element_render(_, ref(TypeName), 'json(r."__rendered")', 'json(ordered."value")', JoinSql) :- !,
    dictionary_table_name(TypeName, ReferenceView),
    quote_ident(ReferenceView, QuotedReferenceView),
    format(atom(JoinSql), ' LEFT JOIN ~w r ON r."__id" = m."value"',
           [QuotedReferenceView]).
list_element_render(_, list(Element), ValueExpr, 'json(ordered."value")', JoinSql) :- !,
    canonical_type_name(list(Element), NestedEntity),
    list_view_name(NestedEntity, NestedView),
    quote_ident(NestedView, QuotedNestedView),
    ValueExpr = 'json(coalesce(n."value_text", \'[]\'))',
    format(atom(JoinSql), ' LEFT JOIN ~w n ON n."list_id" = m."value"',
           [QuotedNestedView]).
list_element_render(_, json, 'json(m."value")', 'json(ordered."value")', '') :- !.
list_element_render(_, json_list(_), 'json(m."value")', 'json(ordered."value")', '') :- !.
list_element_render(Mode, ValueType, 's."content"', 'ordered."value"', JoinSql) :-
    interned_column(Mode, ValueType), !,
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(JoinSql), ' LEFT JOIN ~w s ON s."__id" = m."value"',
           [QuotedDictionary]).
list_element_render(_, _, 'm."value"', 'ordered."value"', '').

% list_id is the list's identity and the member rel is where it lives: an id
% needs no row in the entity table, which is a content dictionary that also
% allocates ids. One outer row per id, taken as the member with no smaller
% idx, keeps the view flattenable, so a read keyed on list_id stays an index
% seek instead of materializing the whole list plane (EXPLAIN in
% v6/tsv2/tests/listReadSurface.test.ts). A list with no member rows has no
% row here and reads '[]' through the boundary coalesce below.
list_column_join(Column, ColumnType, Join) :-
    ColumnType = list(Element),
    canonical_type_name(list(Element), EntityName),
    list_view_name(EntityName, ViewName),
    quote_ident(ViewName, QuotedViewName),
    list_column_alias(Column, Alias),
    quote_ident(Alias, QuotedAlias),
    quote_ident(Column, QuotedColumn),
    format(atom(Join), ' LEFT JOIN ~w ~w ON ~w."list_id" = t.~w',
           [QuotedViewName, QuotedAlias, QuotedAlias, QuotedColumn]).

list_column_joins(Columns, ColumnTypes, JoinSql) :-
    findall(Join,
            ( nth0(Index, Columns, Column),
              nth0(Index, ColumnTypes, ColumnType),
              list_column_join(Column, ColumnType, Join) ),
            Joins),
    atomic_list_concat(Joins, JoinSql).

% The per-type plan the emitter hands the runtime, in TOPOLOGICAL order:
% children before parents, so the post-order intern is one pass down the list
% and a parent's ref columns are already resolved when its own statement runs.
%
% Two statements per type per tick, both set-based over one json_each(?)
% parameter, so the statement count is FLAT in the number of arriving values
% (v6/tsv2/tests/structPlane.test.ts is the count receipt). The N+1 law is
% structural here, not a lint: there is no per-row shape to fall into.
struct_type_plans(Decls, Types, Plans) :-
    struct_type_plans(Decls, Types, [], Plans).

struct_type_plans(Decls, Types, RelPlans, Plans) :-
    (   Types == []
    ->  Plans = []
    ;   type_topological_order(Types, Ordered),
        findall(Plan, ( member(TypeName, Ordered),
                        struct_type_plan(Decls, Types, RelPlans, TypeName, Plan) ), Plans)
    ).

struct_type_plan(Decls, Types, RelPlans, TypeName,
                 structtype(TypeName, Columns, RefTypes, KeyIndices,
                            ConflictSql, InternSql, LookupSql)) :-
    type_definition(Types, TypeName, Columns, ColumnTypes),
    length(Columns, Arity),
    struct_storage_table(RelPlans, TypeName/Arity, TypeName, Table),
    ( decl_key(Decls, TypeName/Arity, KeyPositions)
    -> true
    ;  numlist(1, Arity, KeyPositions)
    ),
    maplist(one_based_to_zero_based, KeyPositions, KeyIndices),
    maplist(dictionary_ref_type(Types), ColumnTypes, RefTypes),
    quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, Width),
    incremental_json_select_exprs_from(Width, 0, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(InternSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM json_each(?)',
           [QuotedTable, ColumnsSql, SelectSql]),
    findall(JsonArg,
            ( member(QuotedColumn, QuotedColumns),
              format(atom(JsonArg), 't.~w', [QuotedColumn]) ),
            JsonArgs),
    atomic_list_concat(JsonArgs, ', ', JsonArgsSql),
    key_join_equalities(KeyPositions, QuotedColumns, 'i.value', t,
                        KeyEqualities),
    atomic_list_concat(KeyEqualities, ' AND ', KeyWhereSql),
    format(atom(ConflictSql),
           'SELECT i.value AS "__requested", json_array(~w) AS "__stored" FROM json_each(?) i JOIN ~w t ON ~w WHERE json_array(~w) <> i.value',
           [JsonArgsSql, QuotedTable, KeyWhereSql, JsonArgsSql]),
    format(atom(LookupSql),
           'SELECT i.value AS "__lookup", t."__id", json_array(~w) AS "__stored" FROM json_each(?) i JOIN ~w t ON ~w',
           [JsonArgsSql, QuotedTable, KeyWhereSql]).

struct_storage_table([], _Ref, Fallback, Fallback) :- !.
struct_storage_table(RelPlans, Ref, _Fallback, Table) :-
    relplan_storage_name(RelPlans, Ref, Table).

one_based_to_zero_based(Position, Index) :- Index is Position - 1.

key_join_equalities([], _, _, _, []).
key_join_equalities([Position | Rest], QuotedColumns, JsonExpr, TableAlias,
                    [Equality | More]) :-
    nth1(Position, QuotedColumns, QuotedColumn),
    Index is Position - 1,
    format(atom(Equality),
           '~w.~w = json_extract(~w, ''$[~w]'')',
           [TableAlias, QuotedColumn, JsonExpr, Index]),
    key_join_equalities(Rest, QuotedColumns, JsonExpr, TableAlias, More).

dictionary_ref_type(Types, DeclaredType, RefType) :-
    column_storage(Types, DeclaredType, Storage),
    ( Storage = ref(Name) -> RefType = Name ; RefType = none ).

% ═══ decode/2 as a dictionary join (SLOT-DECODE-SURFACE) ════════════════════
%
% SLOT-DECODE-SURFACE, decided: decode/2 STAYS on the surface as sugar and
% lowers to a join. Removing it was the alternative and it loses: decode/2 is
% the shipped destructuring spelling, it is what the oracle solves (body.pl
% json_decode/2), and the untyped-json arm still needs it, so removing it from
% the surface would mean two spellings for one idea rather than none.
%
% What it lowers TO is the whole point of the ruling. `decode(Where, {file:
% File})` over a column declared `place` becomes an ordinary positive body
% atom over that type's dictionary:
%
%   diag_file(File) <- diag(Where, _M), decode(Where, {file: File}).
%     becomes
%   diag_file(File) <- diag(Where, _M), '__dict_place'(Where, File, _At).
%
%   .dl6 with its rx lowering (the snippet law):
%     rel place(file: text, at: span).
%     rel diag(where: place, message: text).
%     diag_file(file) <- diag(where, message), decode(where, {file: file}).
%
%     const diagFile$ = combineLatest([diag$, placeDict$]).pipe(
%       map(([diags, places]) => diags.flatMap((diag) => {
%         const place = places.get(diag.where);      // the join: one keyed read
%         return place ? [{ file: place.file }] : [];
%       })),
%       distinctUntilChanged(sameRowSet),
%     );
%
% Doing it as a REWRITE OF THE RULE rather than a new compiler stage is what
% makes it safe: every level-statement family (recompute insert, delta arm,
% refCount arm, recursive-CTE arm, aggregate scope seed/delete/insert) reads
% the rule body, so all of them get the join from one edit and none of them
% can be the family where the destructure is silently absent -- the
% silent-filter-loss class compile_body_guards/4's own header names.
%
% The dictionary atoms are APPENDED, after every original goal, so the source
% variable is already bound when compile_pattern_arg/7 reaches the join and
% the condition comes out as `d1."__id" = b0."where"` rather than a fresh
% binding. `__id` is typed ref(Type), the same storage kind as the column that
% points at it, so join_column_types_agree/4 sees one domain and the
% cross-type join guard stays meaningful.
%
% An edge body over a NON-JSON source keeps the stop
% (check_edge_decode_sources/3, edge_body_needs_json_destructure): a compound
% ARRIVING into an untyped column is stored as canonical term text, which is
% SLOT-TERM-STRUCT's encoding question. A `json` source has no such question
% and takes the level arm's own compile_json_decodes/7.

% Compiler-minted storage tables: no col_type/3 names their columns, so every
% column is `inferred` however the mirrored type_decl/2 was written.
dictionary_relplans(Types, Plans) :-
    findall(rel(Name/DictArity, set, Cols, none),
            ( member(type_def(TypeName, Columns, ColumnTypes), Types),
              dictionary_table_name(TypeName, Name),
              length(Columns, Width), DictArity is Width + 1,
              maplist(dictionary_storage_kind(Types), ColumnTypes, StorageKinds),
              inferred_cols(['__id' | Columns], [ref(TypeName) | StorageKinds],
                            Cols) ),
            Plans).

% ═══ relation-value terms as dictionary joins ═══════════════════════════════
%
% The other dereference spelling. `decode/2` reads a value's fields by name;
% a relation-shaped TERM reads (or builds) them positionally:
%
%   rel repo(name: text).  rel fpath(name: text).
%   rel file(repo: repo, at: fpath).
%   rel span(file: file, start: int, end: int).
%
%   span(file(repo(Name), fpath(Path)), Start, End) <- raw(Name, Path, Start, End).
%   coord(Path, Start, End) <- span(file(_, fpath(Path)), Start, End).
%
% Both directions lower to the SAME thing decode/2 lowers to -- one
% `__ref_<type>` atom per level -- and that is the whole content of the fix.
% Only depth 1 worked before: bind_reference_target_identity/6 binds a whole
% body atom to its alias's `__id`, so a head argument that IS a body atom
% projects the endpoint, and nothing else did. A relation term nested one level
% further fell through compile_pattern_arg/7's generic compound branch and
% compiled to `json_extract(b1."repo", '$.fn') = 'repo'` against b1."repo",
% which is the INTEGER endpoint the level above just wrote.
% json_extract(<integer>, ...) is NULL, so the rule was permanently empty with
% no unsupported construct (plans/2026-07-30-file-span-spine-reconciled.md section 3.1).
%
% The rewrite, per rule:
%
%   span(V_file, Start, End) <-
%     raw(Name, Path, Start, End),
%     file(V_repo, V_at),                  % the target-membership atom, its
%                                          % own arguments rewritten
%     '__ref_repo'(V_repo, Name),          % children first, post-order
%     '__ref_fpath'(V_at, Path),
%     '__ref_file'(V_file, V_repo, V_at).  % the parent last
%
%   coord(Path, Start, End) <-
%     span(V_file, Start, End),
%     '__ref_fpath'(V_at, Path),
%     '__ref_file'(V_file, _, V_at).
%
% Every join is `<parent>."<column>" = <child>."__id"` or a leaf equality
% against a declared column, so every hop is an indexed SEARCH: __id is the
% INTEGER PRIMARY KEY and the value columns carry the identity UNIQUE. Receipts
% in v6/tsv2/tests/relationDepth.test.ts.
%
% Two details that are load-bearing rather than incidental:
%
%   MEMOIZED BY TERM IDENTITY within one rule, so the `repo(Name)` a head
%   builds and the `repo(Name)` its target-membership atom carries resolve to
%   ONE variable and ONE dictionary atom. Without it the two occurrences join
%   the same table twice with nothing relating them.
%
%   POST-ORDER, children before the parent, matching type_topological_order/2
%   and the intern order. SQLite reorders the FROM list itself, so this is
%   about the emitted text reading in dependency order rather than about the
%   plan.
%
% Positions the rewrite does not reach are NAMED REFUSALS, never silence:
% relation_pattern_not_lowerable/1 below runs over the rewritten rule and
% refuses anything left behind (a relation term under not/1, or inside a
% splice construct). Those are compiler capability limits -- the reference
% engine executes all of them -- so they live here and not in
% 0_program_check.pl, per that file's own division of labour.

% Edge rules do not get this rewrite. edge_statements_for_rule/4 compiles a
% trigger occurrence against RelPlans alone -- the dictionary plans are level-
% body-only by construction (Edge 2, see the comment at the call site) -- so
% there is nowhere for the per-level join to go. A relation value in an edge
% rule is therefore a named compiler unsupported construct rather than the whole-atom
% endpoint bind it used to get at depth 1, which agreed with nothing: the
% oracle stores the canonical object and prints its JSON, and the depth-1 bind
% printed prolog term text. The reference engine keeps executing all of these;
% this is a capability limit, and the honest shape for one is a name.
check_edge_rule_relation_values(Types, RelPlans, (Head <+ Body)) :-
    !,
    (   relation_pattern_residue(Types, RelPlans, Head, Body,
                                 relation_pattern_not_lowerable(Ref, Column,
                                                                TypeName, Value))
    ->  throw(unsupported_construct(
                  relation_value_in_edge_rule(Ref, Column, TypeName, Value)))
    ;   true
    ).
check_edge_rule_relation_values(_, _, _).

expand_relation_pattern_rules(Types, RelPlans, Rules0, Rules) :-
    (   Types == []
    ->  Rules = Rules0
    ;   maplist(expand_relation_pattern_rule(Types, RelPlans), Rules0, Rules)
    ).

expand_relation_pattern_rule(Types, RelPlans, (Head0 <- Body0), (Head <- Body)) :- !,
    conjunction_goals(Body0, Goals0),
    rewrite_relation_atom(Types, RelPlans, Head0, Head, st([], []), State1),
    rewrite_relation_goals(Types, RelPlans, Goals0, Goals, State1,
                           st(_, DictionaryAtoms0)),
    elide_dictionary_atoms_the_body_already_joins(Goals, DictionaryAtoms0,
                                                  DictionaryAtoms, Elided),
    append(Goals, DictionaryAtoms, AllGoals),
    goals_conjunction(AllGoals, Body),
    check_relation_patterns_lowered(Types, RelPlans, Elided, Head, Body).
expand_relation_pattern_rule(_, _, Rule, Rule).

% ── the dictionary atom the body already is ──────────────────────────────────
%
% A dictionary atom exists to name a value's identity endpoint. When the rule's
% body ALREADY reads the very row that value is -- the same relation, the same
% argument variables, position for position -- the endpoint is that atom's own
% `__id` and the dictionary join is the table joined to a view of itself:
%
%   rel user(id: int, name: text) key(1).
%   rel selected(choice: user).
%   selected(user(Id, Name)) <- user(Id, Name).
%
%   with the elision  INSERT ... SELECT b0."__id" FROM "user" b0
%   without it        INSERT ... SELECT b1."__id" FROM "user" b0, "__ref_user" b1
%                                WHERE b1."id" = b0."id" AND b1."name" = b0."name"
%
% The second form is what the depth-N rewrite emitted at 472320f4: correct, and
% a self-join through a TEMP VIEW over the same table on every value column,
% which the incremental arm pays again (a 3-way delta join for a rule with one
% body atom). bind_reference_target_identity/6 has bound a whole body atom to
% its alias's `__id` since before that commit; what the rewrite changed is that
% the head no longer HOLDS the atom, so nothing looked the binding up. Putting
% the atom back where it is redundant restores the one-table plan and keeps the
% depth-N machinery for the levels that genuinely need it.
%
% THREE CONDITIONS, all necessary:
%
%   the endpoint is unified with the body atom, so the head reads the atom
%   again and compile_pattern_arg resolves it through Bound;
%
%   the endpoint occurs NOWHERE in the body goals, or the substitution would
%   put a compound back into a ref column that is being READ (a body-side
%   relation pattern is a genuine dictionary join and stays one); and
%
%   the endpoint occurs in no OTHER dictionary atom, or a parent level would
%   receive a compound where it expects its child's endpoint variable.
%
% Everything outside those three keeps the join it had, which is why this is an
% elision and not a second lowering strategy.
elide_dictionary_atoms_the_body_already_joins(Goals, Atoms0, Atoms, Elided) :-
    elide_dictionary_atoms(Goals, Atoms0, [], Atoms, Elided).

elide_dictionary_atoms(_, [], _, [], []).
elide_dictionary_atoms(Goals, [Atom | Rest], Seen, Kept, Elided) :-
    append(Seen, Rest, OtherAtoms),
    (   dictionary_atom_is_the_body_atom(Goals, OtherAtoms, Atom, Target)
    ->  Kept = More, Elided = [Target | MoreElided], NextSeen = Seen
    ;   Kept = [Atom | More], Elided = MoreElided, NextSeen = [Atom | Seen]
    ),
    elide_dictionary_atoms(Goals, Rest, NextSeen, More, MoreElided).

dictionary_atom_is_the_body_atom(Goals, OtherAtoms, DictionaryAtom, Target) :-
    DictionaryAtom =.. [Table, Endpoint | Args],
    var(Endpoint),
    dictionary_table_for_type(TypeName, Table),
    Target =.. [TypeName | Args],
    identical_member(Goals, Target),
    \+ holds_variable(Goals, Endpoint),
    \+ holds_variable(OtherAtoms, Endpoint),
    Endpoint = Target.

dictionary_table_for_type(TypeName, Table) :-
    (   physical_storage_name(_, _)
    ->  physical_storage_name(Ref, StorageName),
        Ref = TypeName/_,
        atomic_list_concat(['__ref_', StorageName], Table)
    ;   atomic_list_concat(['__ref_', TypeName], Table)
    ).

% Membership by term IDENTITY. `memberchk/2` would UNIFY, which for a body of
% variables succeeds against the wrong atom and binds the rule's own variables
% to each other -- the same reason memoized_relation_value/3 is hand-walked.
identical_member([Goal | Rest], Target) :-
    ( Goal == Target -> true ; identical_member(Rest, Target) ).

holds_variable(Term, Variable) :-
    (   Term == Variable
    ->  true
    ;   compound(Term),
        arg(_, Term, Sub),
        holds_variable(Sub, Variable)
    ).

rewrite_relation_goals(_, _, [], [], State, State).
rewrite_relation_goals(Types, RelPlans, [Goal0 | Rest0], [Goal | Rest],
                       State0, State) :-
    (   nonvar(Goal0),
        body_surface_for_term(Goal0, _, _, _, _, _)
    ->  Goal = Goal0, State1 = State0         % registry construct: not an atom
    ;   rewrite_relation_atom(Types, RelPlans, Goal0, Goal, State0, State1)
    ),
    rewrite_relation_goals(Types, RelPlans, Rest0, Rest, State1, State).

% One atom's ARGUMENTS. The atom keeps its name and arity; only a ref-typed
% column holding a relation term changes, and it changes to the variable that
% column's dictionary atom binds.
rewrite_relation_atom(Types, RelPlans, Atom0, Atom, State0, State) :-
    (   compound(Atom0),
        functor(Atom0, Name, Arity),
        relplan_column_types(RelPlans, Name/Arity, ColumnTypes)
    ->  Atom0 =.. [_ | Args0],
        rewrite_relation_arguments(Types, ColumnTypes, Args0, Args,
                                   State0, State),
        Atom =.. [Name | Args]
    ;   Atom = Atom0, State = State0
    ).

rewrite_relation_arguments(_, [], [], [], State, State) :- !.
rewrite_relation_arguments(Types, [ColumnType | Types0], [Arg0 | Args0],
                           [Arg | Args], State0, State) :-
    !,
    (   ColumnType = ref(TypeName),
        relation_value_term(Types, TypeName, Arg0, Value)
    ->  intern_relation_value(Types, TypeName, Value, Arg, State0, State1)
    ;   Arg = Arg0, State1 = State0
    ),
    rewrite_relation_arguments(Types, Types0, Args0, Args, State1, State).
% A column-type list shorter or longer than the argument list means the ref is
% not the one the relplan describes; leave the remaining arguments alone
% rather than guessing a position.
rewrite_relation_arguments(_, _, Args, Args, State, State).

% A relation term becomes the variable its dictionary atom binds. Children are
% interned first, so the parent's atom already has its child endpoints in hand.
intern_relation_value(Types, TypeName, Value, Endpoint,
                      st(Memo0, Atoms0), st(Memo, Atoms)) :-
    (   memoized_relation_value(Memo0, Value, Found)
    ->  Endpoint = Found, Memo = Memo0, Atoms = Atoms0
    ;   type_definition(Types, TypeName, _Columns, ColumnTypes),
        maplist(dictionary_storage_kind(Types), ColumnTypes, StorageKinds),
        Value =.. [_ | Args0],
        rewrite_relation_arguments(Types, StorageKinds, Args0, Args,
                                   st(Memo0, Atoms0), st(Memo1, Atoms1)),
        dictionary_table_name(TypeName, Table),
        DictionaryAtom =.. [Table, Endpoint | Args],
        Memo = [Value-Endpoint | Memo1],
        append(Atoms1, [DictionaryAtom], Atoms)
    ).

% Variable IDENTITY, never unification: `=`/2 here would bind the rule's own
% variables to each other. Walked with recursion for the same reason
% decode_binding_type/5 is: findall/3 copies its template.
memoized_relation_value([Term-Endpoint | Rest], Value, Found) :-
    ( Term == Value -> Found = Endpoint ; memoized_relation_value(Rest, Value, Found) ).

% Nothing relation-shaped may survive in a ref column after the rewrite. What
% can survive is a position the rewrite deliberately does not enter: under
% not/1, whose lowering is a NOT EXISTS subquery with no room for the extra
% joins, or inside a splice construct, whose arguments are trigger occurrences
% rather than level atoms. Named here rather than silently compiled, which is
% exactly what the old json_extract fallthrough did.
% Elided is the list of relation terms put BACK into the rule by
% elide_dictionary_atoms_the_body_already_joins/4. Those are lowered -- through
% the body atom's own identity bind rather than through a dictionary join --
% so they are not residue. Compared by identity, and the whole list is walked
% rather than only the first residue, so a genuinely unlowerable term sitting
% behind an elided one is still the one reported.
check_relation_patterns_lowered(Types, RelPlans, Elided, Head, Body) :-
    (   relation_pattern_residue(Types, RelPlans, Head, Body, Residue),
        Residue = relation_pattern_not_lowerable(_, _, _, Value),
        \+ identical_member(Elided, Value)
    ->  throw(unsupported_construct(Residue))
    ;   true
    ).

relation_pattern_residue(Types, RelPlans, Head, Body,
                         relation_pattern_not_lowerable(Ref, Column, TypeName, Value)) :-
    % Rank B11: one wrapper family, stated once in 0_body_walk.pl and
    % projected here, in 0_program_check.pl, and in the oracle's rewriter.
    body_relation_atoms((Head, Body),
                        walk_policy(descend_not(true), splice_bare(true)),
                        _, Atom),
    compound(Atom),
    functor(Atom, Name, Arity),
    Ref = Name/Arity,
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    relplan_columns(RelPlans, Ref, Columns),
    nth1(Position, ColumnTypes, ref(TypeName)),
    nth1(Position, Columns, Column),
    arg(Position, Atom, Value),
    ( relation_value_shape(Types, TypeName, Value)
    ; contextual_relation_value_shape(Types, TypeName, Value) ).

contextual_relation_value_shape(Types, TypeName, Value) :-
    compound(Value),
    functor(Value, Functor, 1),
    memberchk(Functor, ['{}', obj]),
    \+ relation_value_term(Types, TypeName, Value, _).
% Left NONDETERMINISTIC on purpose. Both callers commit to the first solution
% through their own `->`, and check_relation_patterns_lowered/5 has to be able
% to step PAST a term the elision put back before deciding there is no residue.
% A cut here made the first witness the only one it could ever see.

% Runs even when the program declares NO type. A rule with a decode goal and
% no struct type must reach decode_source_not_struct, and skipping the pass on
% an empty type table instead left decode/2 in the body where body_ref_uses/2
% does not see it -- the head variable it should have bound then failed far
% away as unbound_head_var, a diagnostic that names neither decode nor the
% missing declaration. A rule with no decode goal keeps its body term
% UNCHANGED (identity, not a rebuild), which is what keeps every pre-existing
% emitted module byte-identical.
expand_decode_rules(Types, RelPlans, Rules, Expanded) :-
    maplist(expand_decode_rule(Types, RelPlans), Rules, Expanded).

expand_decode_rule(Types, RelPlans, (Head <- Body), (Head <- Expanded)) :- !,
    conjunction_goals(Body, Goals),
    partition(is_decode_goal, Goals, DecodeGoals, OtherGoals),
    partition(json_decode_goal(RelPlans, Goals), DecodeGoals,
              JsonDecodeGoals, StructDecodeGoals),
    (   StructDecodeGoals == []
    ->  % Nothing to rewrite. A rule with only json decodes keeps its body
        % term UNCHANGED (identity, not a rebuild), which is what keeps every
        % pre-existing emitted module byte-identical and what leaves the json
        % goals in the position compile_body_guards/5 reads them from.
        Expanded = Body
    ;   foldl(decode_goal_atoms(Types, RelPlans, OtherGoals), StructDecodeGoals,
              [], Atoms),
        append([OtherGoals, JsonDecodeGoals, Atoms], AllGoals),
        goals_conjunction(AllGoals, Expanded)
    ).

expand_decode_rule(_, _, Rule, Rule).

% THE DISPATCH, and the only place it is made: a decode whose source is bound
% by a positive body atom at a column declared `json` lowers to json1 SQL, not
% to a dictionary join. Everything else keeps the struct arm, including its
% decode_source_not_struct unsupported construct for a source with no typed binding at all.
%
% Deliberately a separate walk from decode_binding_type/5 rather than one
% widened predicate: that one commits (cut) on the first ref-typed binding it
% finds and stepping PAST a non-ref binding is behaviour the struct arm
% depends on. Sharing a cut between the two would silently change which
% binding a repeated variable resolves to.
json_decode_goal(RelPlans, BodyGoals, decode(Source, _)) :-
    var(Source),
    member(Atom, BodyGoals),
    compound(Atom),
    functor(Atom, Name, Arity),
    relplan_column_types(RelPlans, Name/Arity, ColumnTypes),
    Atom =.. [_ | Args],
    nth1(Position, Args, Argument),
    Argument == Source,
    ( nth1(Position, ColumnTypes, json)
    ; nth1(Position, ColumnTypes, json_list(_))
    ),
    !.


is_decode_goal(Goal) :- nonvar(Goal), Goal = decode(_, _).

goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, More)) :- goals_conjunction(Rest, More).

decode_goal_atoms(Types, RelPlans, BodyGoals, decode(Source, Pattern), Acc0, Acc) :-
    decode_source_type(Types, RelPlans, BodyGoals, Acc0, decode(Source, Pattern), TypeName),
    decode_pattern_atoms(Types, TypeName, Source, Pattern, Atoms),
    append(Acc0, Atoms, Acc).

% The declared type of the variable decode/2 reads, resolved from whichever
% positive body atom (or already-emitted dictionary atom) binds it. A source
% with no ref-typed binding is a NAMED unsupported construct, never a lowering that answers
% something: the untyped-json arm still needs its own encoding decision, which
% is SLOT-TERM-STRUCT's question and not this one's.
decode_source_type(Types, RelPlans, BodyGoals, DictAtoms, Goal, TypeName) :-
    Goal = decode(Source, _),
    (   var(Source),
        decode_binding_type(RelPlans, BodyGoals, DictAtoms, Source, Found)
    ->  TypeName = Found
    ;   throw(unsupported_construct(decode_source_not_struct(Goal)))
    ),
    ( declared_type_name(Types, TypeName) -> true
    ; throw(unsupported_construct(column_type_unknown(TypeName))) ).

% Walked with member/2 over the goal list, never collected with findall/3:
% findall COPIES its template, and the whole resolution here is `Argument ==
% Source`, variable IDENTITY. A findall in this position silently answers "no
% binding" for every source and every decode becomes decode_source_not_struct.
% (The prolog-org journal records the same bite twice, both times with a
% failure message far from the cause.)
decode_binding_type(RelPlans, BodyGoals, DictAtoms, Source, Found) :-
    ( member(Atom, BodyGoals) ; member(Atom, DictAtoms) ),
    compound(Atom),
    functor(Atom, Name, Arity),
    relplan_column_types(RelPlans, Name/Arity, ColumnTypes),
    Atom =.. [_ | Args],
    nth1(Position, Args, Argument),
    Argument == Source,
    nth1(Position, ColumnTypes, ref(Found)),
    !.

decode_pattern_atoms(Types, TypeName, Source, Pattern, Atoms) :-
    (   Pattern = {}(Fields)
    ->  true
    ;   throw(unsupported_construct(decode_pattern_not_object(TypeName, Pattern)))
    ),
    braces_pattern_pairs(Fields, Pairs),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    forall(member(Key-_, Pairs),
           ( memberchk(Key, Columns) -> true
           ; throw(unsupported_construct(decode_field_unknown(TypeName, Key))) )),
    length(Columns, Width),
    length(Slots, Width),
    foldl(decode_slot(Types, Pairs, Columns, ColumnTypes), Slots, 1-[]-[], _-_-NestedGroups),
    dictionary_table_name(TypeName, Table),
    Atom =.. [Table, Source | Slots],
    append(NestedGroups, Nested),
    append([Atom], Nested, Atoms).

% One dictionary column: the pattern's own argument when the pattern names it,
% otherwise a fresh anonymous variable (an object pattern is OPEN -- body.pl
% json_decode/2 ignores keys the pattern does not mention). A nested object
% pattern over a ref column becomes ANOTHER dictionary atom, keyed on the
% fresh variable this slot binds, which is how depth costs no new construct.
decode_slot(Types, Pairs, Columns, ColumnTypes, Slot,
            Position-Acc-Nested0, NextPosition-Acc-Nested) :-
    nth1(Position, Columns, Column),
    nth1(Position, ColumnTypes, ColumnType),
    NextPosition is Position + 1,
    (   memberchk(Column-SubPattern, Pairs)
    ->  (   nonvar(SubPattern), SubPattern = {}(_), declared_type_name(Types, ColumnType)
        ->  decode_pattern_atoms(Types, ColumnType, Slot, SubPattern, SubAtoms),
            append(Nested0, [SubAtoms], Nested)
        ;   Slot = SubPattern, Nested = Nested0
        )
    ;   Nested = Nested0
    ).

braces_pattern_pairs((Left, Right), Pairs) :- !,
    braces_pattern_pairs(Left, LeftPairs),
    braces_pattern_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
braces_pattern_pairs(Key: Pattern, [Key-Pattern]).

incremental_json_select_exprs_from(0, _, []) :- !.
incremental_json_select_exprs_from(N, Index, [Expr | More]) :-
    N > 0,
    atomic_list_concat(['json_extract(value, \'$[', Index, ']\')'], Expr),
    NextIndex is Index + 1,
    NextN is N - 1,
    incremental_json_select_exprs_from(NextN, NextIndex, More).

% ═══ arrival statement templates (round 2: Log rel drops tick/seq params) ═══

arrival_statement(RelPlan,
                  arrivalstmt(Ref, log, AddSql, none, IncrementalAddSql, none)) :-
    relplan_parts(RelPlan, Ref, log, Columns, _, _),
    !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    atomic_list_concat(['INSERT INTO ', QuotedTable, ' (', ColumnsSql,
                        ') VALUES (', PlaceholdersSql, ')'], AddSql),
    incremental_arrival_add_sql('INSERT INTO', '', QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql).
arrival_statement(RelPlan,
                  arrivalstmt(Ref, set, AddSql, DelSql, IncrementalAddSql, IncrementalDelSql)) :-
    relplan_parts(RelPlan, Ref, set, Columns, KeyOrNone, _),
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    set_arrival_sql_parts(KeyOrNone, QuotedColumns, Insert, ConflictSql),
    atomic_list_concat([Insert, ' ', QuotedTable, ' (', ColumnsSql,
                        ') VALUES (', PlaceholdersSql, ')', ConflictSql],
                       AddSql),
    maplist(eq_placeholder, QuotedColumns, EqParts),
    atomic_list_concat(EqParts, ' AND ', WhereSql),
    atomic_list_concat(['DELETE FROM ', QuotedTable, ' WHERE ', WhereSql],
                       DelSql),
    incremental_arrival_add_sql(Insert, ConflictSql, QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql),
    incremental_json_select_exprs(QuotedColumns, 0, DeleteSelectExprs),
    atomic_list_concat(DeleteSelectExprs, ', ', DeleteSelectSql),
    atomic_list_concat(['DELETE FROM ', QuotedTable, ' WHERE (', ColumnsSql,
                        ') IN (SELECT ', DeleteSelectSql,
                        ' FROM json_each(?)) RETURNING ', ColumnsSql],
                       IncrementalDelSql).

set_arrival_sql_parts(none, _, 'INSERT OR IGNORE INTO', '') :- !.
set_arrival_sql_parts(key(KeyPositions), QuotedColumns, 'INSERT INTO', ConflictSql) :-
    nth1_list(KeyPositions, QuotedColumns, QuotedKeyColumns),
    atomic_list_concat(QuotedKeyColumns, ', ', KeySql),
    findall(UpdateColumn,
            ( nth1(Position, QuotedColumns, UpdateColumn),
              \+ memberchk(Position, KeyPositions) ),
            UpdateColumns),
    (   UpdateColumns == []
    ->  format(atom(ConflictSql), ' ON CONFLICT (~w) DO NOTHING', [KeySql])
    ;   findall(Assignment,
                ( member(UpdateColumn, UpdateColumns),
                  format(atom(Assignment), '~w = excluded.~w',
                         [UpdateColumn, UpdateColumn]) ),
                Assignments),
        atomic_list_concat(Assignments, ', ', AssignmentSql),
        format(atom(ConflictSql),
               ' ON CONFLICT (~w) DO UPDATE SET ~w',
               [KeySql, AssignmentSql])
    ).

incremental_arrival_add_sql(Insert, ConflictSql, QuotedTable, ColumnsSql, QuotedColumns, Sql) :-
    incremental_json_select_exprs(QuotedColumns, 0, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    ( ConflictSql == '' -> SelectTail = '' ; SelectTail = ' WHERE true' ),
    format(atom(Sql),
           '~w ~w (~w) SELECT ~w FROM json_each(?)~w~w RETURNING ~w',
           [Insert, QuotedTable, ColumnsSql, SelectSql, SelectTail,
            ConflictSql, ColumnsSql]).

incremental_json_select_exprs([], _, []).
incremental_json_select_exprs([_ | Rest], Index, [Expr | More]) :-
    atomic_list_concat(['json_extract(value, \'$[', Index, ']\')'], Expr),
    NextIndex is Index + 1,
    incremental_json_select_exprs(Rest, NextIndex, More).

eq_placeholder(QuotedColumn, Text) :-
    atomic_list_concat([QuotedColumn, ' = ?'], Text).

placeholders(0, []) :- !.
placeholders(N, ['?' | Rest]) :- N > 0, N1 is N - 1, placeholders(N1, Rest).

% ═══ edge rule lowering ═══════════════════════════════════════════════════
% PHASE C2 RULING 2: a rule's body classifies via analyze.pl:edge_trigger_
% shape/2 into marked_single(TriggerAtom) (unchanged from round 2: exactly
% one trigger, no other body goal, TriggerAtom must be Log-kind),
% unmarked_conjunction(Atoms) (N >= 1 plain positive atoms, no only/1
% anywhere -- engine.pl's unmarked fallback wraps EVERY one as its own
% independent trigger, body.pl:96-110/153-155), or
% sampled_conjunction(TriggerAtoms, SampleAtoms, PreAtoms, NegAtoms,
% GuardGoals), where
% latest/1 removes the SampleAtoms from the trigger set while retaining them
% as current-state base-table reads, pre/1 reads the tick-local __pre table,
% not/1 contributes a NOT EXISTS over the
% negated rel's current table, and the comparison/bind goals become WHERE
% conditions and SELECT-expression bindings (the same three compilers a level
% body uses: compile_positive_uses/6, compile_guard_goals/4,
% compile_negative_uses/4, folded in that order so a `:=` can read a variable
% an earlier atom bound and a NOT EXISTS can read either). Lowering produces ONE
% edgestmt/6 PER CANDIDATE TRIGGER ATOM (edge_statements_for_rule/4): for
% marked_single that is the existing single edgestmt, unchanged; for
% unmarked_conjunction with N atoms it is N edgestmt entries, one per atom
% acting as the trigger with the OTHER N-1 atoms as a real SQL join against
% their CURRENT table contents (compile_positive_uses/6, reused unchanged
% from the level-rule side, seeded with the trigger atom's own numbered-
% placeholder bindings as Bound0 so a variable shared between the trigger
% and another atom becomes an equality constraint rather than a fresh,
% unconstrained alias column). N=1 (marked_single, or an unmarked body with
% exactly one atom) has zero OTHER atoms to join, so ProjectSql stays the
% same bare `SELECT <head-expr-list>` with no FROM clause -- byte-identical
% to round 2's text for every already-IDENTICAL marked_single fixture
% (verified via the plunit SQL-text snapshot tests, unchanged).

edge_statements_for_rule(Mode, EdgeHeadedRefs, RelPlans, (Head <+ Body),
                         EdgeStatements) :-
    edge_trigger_shape(Body, Shape),
    check_edge_decode_sources(RelPlans, Body, Shape),
    ( Shape = marked_single(TriggerAtom)
    -> rel_ref(TriggerAtom, TriggerRef),
       ( relplan_kind(RelPlans, TriggerRef, log) -> true
       ; throw(unsupported_construct(edge_trigger_not_log(TriggerRef))) ),
       edge_statement_single(Mode, RelPlans, Head, TriggerAtom, [], [], [], [],
                             arrival, EdgeStmt),
       EdgeStatements = [EdgeStmt]
    ; Shape = unmarked_conjunction(Atoms)
    -> findall(EdgeStmt,
               ( select(TriggerAtom, Atoms, OtherAtoms),
                 edge_statement_single(Mode, RelPlans, Head, TriggerAtom, OtherAtoms,
                                       [], [], [], arrival, EdgeStmt) ),
               EdgeStatements)
    ; Shape = sampled_conjunction(TriggerAtoms, SampleAtoms, PreAtoms,
                                  NegAtoms, GuardGoals)
    -> arrival_trigger_kind(EdgeHeadedRefs, PreAtoms, NegAtoms, ArrivalKind),
       findall(EdgeStmt,
               ( select(TriggerAtom, TriggerAtoms, OtherTriggerAtoms),
                 append(OtherTriggerAtoms, SampleAtoms, OtherAtoms),
                 edge_statement_single(Mode, RelPlans, Head, TriggerAtom, OtherAtoms,
                                       PreAtoms, NegAtoms, GuardGoals,
                                       ArrivalKind, EdgeStmt) ),
               EdgeStatements)
    % ONE arm, from the finalize'd rel's departure frontier. The other
    % positive atoms are joins, never arms: an arrival occurrence on one of
    % them leaves finalize standing in the body and body.pl's
    % `solve(finalize(_), _) :- !, fail` makes that arm derive nothing, so
    % emitting it would be emitting a statement that can only ever return
    % zero rows.
    ; Shape = departure_trigger(FinalizeAtom, OtherPositiveAtoms, SampleAtoms,
                                PreAtoms, NegAtoms, GuardGoals)
    -> append(OtherPositiveAtoms, SampleAtoms, OtherAtoms),
       ( PreAtoms == [] -> DepartureKind = departure
       ; DepartureKind = ordered_departure
       ),
       edge_statement_single(Mode, RelPlans, Head, FinalizeAtom, OtherAtoms, PreAtoms,
                             NegAtoms, GuardGoals, DepartureKind, EdgeStmt),
       EdgeStatements = [EdgeStmt]
    ).

% WHICH EMISSION ORDER THE ARMS RUN IN. `arrival` is arm-major: emit_ts.pl
% walks ORDERED_EDGE_ARMS outermost, so an arm drains its whole batch before
% the next arm sees its first row, and source line order decides who ran
% first. `ordered_arrival` is occurrence-major: one edgestmt carrying it makes
% ordered_program/1 true, and the whole module then runs the ordered
% occurrence loop, which walks arrivals outermost and offers each occurrence
% to every arm (emit_ts.pl:1490 applyOrderedOccurrence).
%
% Ruling one_pick_order (conformance/rulings.pl): the pick inside a tick reads
% the ARRIVAL index on both doors, and source arm order is not an axis of the
% clock. Arm-major is only observable when one arm's write can silence another
% arm inside the same tick, and the two body forms that let it are pre/1 (the
% fold reads the evolving store) and a negation over a rel that some edge rule
% heads (the guard-by-negation pick: whoever writes first blocks the rest).
% Both take ordered_arrival, so neither can be refereed by line order.
% A negation over a rel NO edge rule writes cannot change inside the tick, so
% it stays arm-major and its emitted text is unchanged.
arrival_trigger_kind(_EdgeHeadedRefs, PreAtoms, _NegAtoms, ordered_arrival) :-
    PreAtoms \== [], !.
arrival_trigger_kind(EdgeHeadedRefs, _PreAtoms, NegAtoms, ordered_arrival) :-
    member(NegAtom, NegAtoms),
    rel_ref(NegAtom, NegRef),
    memberchk(NegRef, EdgeHeadedRefs),
    !.
arrival_trigger_kind(_EdgeHeadedRefs, _PreAtoms, _NegAtoms, arrival).

% One arm: TriggerAtom's own args bind to numbered placeholders (unchanged
% compile_trigger_bound/2); OtherAtoms (possibly []) join against the
% CURRENT store, seeded with that same placeholder Bound so shared
% variables become equality constraints, not fresh columns.
%
% HeadRef's own kind decides the WRITE shape (engine.pl apply_edge_writes/6,
% :236-254): a Log head APPENDS unconditionally, every derived row a
% distinct occurrence, no key concept at all (keyed_log_rel already refuses
% a Log rel EVER being declared keyed); a Set head UPSERTS by key,
% last-write-wins (unchanged from before this ruling). KeyColumns is `[]`
% for a Log head -- edge_resolver_block (emit_ts.pl) branches on HeadKind
% too, since a Log head's resolver must NOT collapse multiple derived rows
% into one Map entry the way a Set head's last-write-wins fold does (every
% key would otherwise be the same empty `[]`).
edge_statement_single(Mode, RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                      NegAtoms, GuardGoals, TriggerKind,
                      edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns,
                               ProjectSql, WriteSql, DeltaProjectSql,
                               TriggerKind,
                               edgeinterns(ProjectInternSqls, DeltaInternSqls))) :-
    rel_ref(TriggerAtom, TriggerRef),
    rel_ref(Head, HeadRef),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    ( HeadKind == set
    -> ( relplan_key(RelPlans, HeadRef, key(KeyPositions)) -> true
       ; throw(unsupported_construct(edge_into_unkeyed_set(HeadRef))) )
    ; true  % log: no key concept, KeyPositions unused below
    ),
    TriggerAtom =.. [_ | TriggerArgs],
    relplan_column_types(RelPlans, TriggerRef, TriggerBoundColumnTypes),
    trigger_read_mode(TriggerKind, Mode, TriggerMode),
    compile_trigger_bound(TriggerMode, TriggerArgs, TriggerBoundColumnTypes, TriggerBound),
    reference_trigger_samples(RelPlans, TriggerKind, TriggerAtom,
                              OtherAtoms, IdentityOtherAtoms),
    % maplist, NEVER findall (analyze.pl:ref_occurrence_args/3's own
    % comment names this exact hazard): findall copies its template per
    % solution, which would sever OtherArgs from the SAME variable
    % objects Head's arguments share -- head_select_list's bound_lookup
    % would then never find them, throwing unbound_head_var even though
    % the variables genuinely ARE bound (confirmed empirically: this
    % bug shipped in an earlier draft and unmarked_edge_replays_backlog
    % is the fixture that caught it).
    maplist(other_atom_use, IdentityOtherAtoms, OtherUses),
    maplist(pre_atom_use, PreAtoms, PreUses),
    append(OtherUses, PreUses, PositiveUses),
    compile_positive_uses(Mode, RelPlans, PositiveUses, TriggerBound, PositiveBound,
                          PositiveFromParts, PositiveWhereTexts),
    compile_edge_guards(Mode, GuardGoals, PositiveBound, Bound, JsonFromParts,
                        GuardWhereTexts),
    append(PositiveFromParts, JsonFromParts, FromParts),
    maplist(negated_atom_use, NegAtoms, NegUses),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    append([PositiveWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    ( FromParts == [] -> FromSql = none ; from_parts_sql(FromParts, FromSql) ),
    ( WhereTexts == [] -> WhereSql = none ; atomic_list_concat(WhereTexts, ' AND ', WhereSql) ),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    % Aliased AS HeadColumns (not `none`, unlike a level rule's SELECT,
    % which has an explicit INSERT column list and does not need aliases):
    % the emitter reads one projected row back via named column access
    % (runtime/rows.ts's own `row[column]` idiom), and reconstructing
    % aliases by string surgery on an alias-free SELECT would be unsafe --
    % a json_object(...) expression's OWN internal commas would look
    % identical to expression-list separators to any naive re-splitter.
    head_select_list(Mode, HeadColumnTypes, Head, Bound, HeadColumns, SelectExprs, BuiltValues, ListInterns),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, ProjectInternSqls),
    % A FROM-less SELECT with a WHERE is the guard-only arm (every body goal
    % past the trigger is a comparison, a bind or a NOT EXISTS): SQLite
    % evaluates it over the one implicit row and returns zero rows when the
    % condition is false, which is exactly "this occurrence derives nothing".
    ( FromSql == none, WhereSql == none
    -> format(atom(ProjectSql), 'SELECT ~w', [SelectSql])
    ; FromSql == none
    -> format(atom(ProjectSql), 'SELECT ~w WHERE ~w', [SelectSql, WhereSql])
    ; WhereSql == none
    -> format(atom(ProjectSql), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
    ; format(atom(ProjectSql), 'SELECT ~w FROM ~w WHERE ~w', [SelectSql, FromSql, WhereSql])
    ),
    edge_delta_project_sql(Mode, RelPlans, Head, TriggerAtom, IdentityOtherAtoms,
                           PreAtoms, NegAtoms, GuardGoals, HeadColumns,
                           TriggerKind, DeltaProjectSql, DeltaInternSqls),
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    length(HeadColumns, ColumnCount), placeholders(ColumnCount, ValuePlaceholders),
    atomic_list_concat(ValuePlaceholders, ', ', ValuePlaceholdersSql),
    ( HeadKind == log
    -> KeyColumns = [],
       format(atom(WriteSql), 'INSERT INTO ~w (~w) VALUES (~w)',
              [QuotedHeadTable, HeadColumnsSql, ValuePlaceholdersSql])
    ;  % KeyColumns indexes HeadColumns (the keyed rel's OWN columns), not
       % TriggerColumns -- round 1 indexed the wrong list here (silently
       % correct only by fixture coincidence: open_scope and route_change
       % both have session_id at position 1). Fixed in this rewrite.
       nth1_list(KeyPositions, HeadColumns, KeyColumns),
       maplist(quote_ident, KeyColumns, QuotedKeyColumns),
       atomic_list_concat(QuotedKeyColumns, ', ', KeyColumnsSql),
       subtract(HeadColumns, KeyColumns, NonKeyColumns),
       ( NonKeyColumns == []
       -> format(atom(ConflictClause), 'ON CONFLICT(~w) DO NOTHING', [KeyColumnsSql])
       ;  maplist(quote_ident, NonKeyColumns, QuotedNonKeyColumns),
          maplist(excluded_assignment, QuotedNonKeyColumns, SetParts),
          atomic_list_concat(SetParts, ', ', SetSql),
          format(atom(ConflictClause), 'ON CONFLICT(~w) DO UPDATE SET ~w', [KeyColumnsSql, SetSql])
       ),
       format(atom(WriteSql), 'INSERT INTO ~w (~w) VALUES (~w) ~w',
              [QuotedHeadTable, HeadColumnsSql, ValuePlaceholdersSql, ConflictClause])
    ).

% Per-arrival edge projection starts from parameter placeholders rather than
% the target table alias. When the trigger relation is itself a referenced
% entity, sample its current public row as an ordinary positive use. That
% indexed equality join supplies the same __id binding as a non-trigger body
% atom. Departure triggers stay row occurrences after the target has gone and
% therefore cannot take this current-membership join.
reference_trigger_samples(RelPlans, arrival, TriggerAtom,
                          OtherAtoms, IdentityOtherAtoms) :-
    rel_ref(TriggerAtom, TriggerRef),
    reference_target_ref(RelPlans, TriggerRef),
    \+ member_same_term(TriggerAtom, OtherAtoms),
    !,
    IdentityOtherAtoms = [TriggerAtom | OtherAtoms].
reference_trigger_samples(RelPlans, ordered_arrival, TriggerAtom,
                          OtherAtoms, IdentityOtherAtoms) :-
    reference_trigger_samples(RelPlans, arrival, TriggerAtom, OtherAtoms,
                              IdentityOtherAtoms),
    !.
reference_trigger_samples(_, _, _, OtherAtoms, OtherAtoms).

reference_target_ref(RelPlans, Name/_Arity) :-
    relplan_reference_target(RelPlans, Name).

member_same_term(Term, [Candidate | _]) :- Term == Candidate, !.
member_same_term(Term, [_ | Rest]) :- member_same_term(Term, Rest).

edge_delta_project_sql(Mode, RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                       NegAtoms, GuardGoals, HeadColumns, TriggerKind,
                       DeltaProjectSql, InternSqls) :-
    rel_ref(TriggerAtom, TriggerRef),
    rel_ref(Head, HeadRef),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    TriggerAtom =.. [_ | TriggerArgs],
    (   memberchk(TriggerKind, [departure, ordered_departure])
    ->  departure_frontier_table_name(TriggerRef, FrontierTable)
    ;   frontier_table_name(TriggerRef, FrontierTable)
    ),
    quote_ident(FrontierTable, QuotedFrontierTable),
    DeltaAlias = d0,
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    relplan_column_types(RelPlans, TriggerRef, TriggerColumnTypes),
    trigger_read_mode(TriggerKind, Mode, TriggerMode),
    compile_atom_args(TriggerMode, TriggerArgs, TriggerColumns, TriggerColumnTypes, DeltaAlias, [],
                      TriggerBound, TriggerWhereParts),
    maplist(where_text, TriggerWhereParts, TriggerWhereTexts),
    maplist(other_atom_use, OtherAtoms, OtherUses),
    maplist(pre_atom_use, PreAtoms, PreUses),
    append(OtherUses, PreUses, PositiveUses),
    compile_positive_uses(Mode, RelPlans, PositiveUses, TriggerBound, PositiveBound,
                          OtherFromParts, OtherWhereTexts),
    compile_edge_guards(Mode, GuardGoals, PositiveBound, Bound, JsonFromParts,
                        GuardWhereTexts),
    maplist(negated_atom_use, NegAtoms, NegUses),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, HeadColumns, SelectExprs, BuiltValues, ListInterns),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w ~w', [QuotedFrontierTable, DeltaAlias]),
    append([[DeltaFrom], OtherFromParts, JsonFromParts], FromParts),
    from_parts_sql(FromParts, FromSql),
    append([['d0."_phase" >= 0' | TriggerWhereTexts], OtherWhereTexts,
            GuardWhereTexts, NegWhereTexts], WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaProjectSql),
           'SELECT ~w FROM ~w WHERE ~w ORDER BY d0."_phase", d0."_sequence"',
           [SelectSql, FromSql, WhereSql]),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, InternSqls).

% THE EDGE ARM'S DECODE DECISION IS THE LEVEL ARM'S: json_decode_goal/3 over
% the shape's own positive atoms, so a rule KIND never decides it.
check_edge_decode_sources(RelPlans, Body, Shape) :-
    conjunction_goals(Body, Goals),
    include(is_decode_goal, Goals, DecodeGoals),
    (   DecodeGoals == []
    ->  true
    ;   once(edge_shape_positive_atoms(Shape, PositiveAtoms)),
        forall(member(DecodeGoal, DecodeGoals),
               (   json_decode_goal(RelPlans, PositiveAtoms, DecodeGoal)
               ->  true
               ;   throw(unsupported_construct(
                             edge_body_needs_json_destructure(Body)))
               ))
    ).

% The atoms whose declared columns can bind a decode source. NegAtoms are
% excluded for check_edge_head_column_types/2's reason: check mode binds none.
edge_shape_positive_atoms(marked_single(TriggerAtom), [TriggerAtom]).
edge_shape_positive_atoms(unmarked_conjunction(Atoms), Atoms).
edge_shape_positive_atoms(sampled_conjunction(TriggerAtoms, SampleAtoms,
                                              PreAtoms, _, _), Atoms) :-
    maplist(pre_atom_rel, PreAtoms, PreRelAtoms),
    append([TriggerAtoms, SampleAtoms, PreRelAtoms], Atoms).
edge_shape_positive_atoms(departure_trigger(FinalizeAtom, OtherAtoms,
                                            SampleAtoms, PreAtoms, _, _),
                          Atoms) :-
    maplist(pre_atom_rel, PreAtoms, PreRelAtoms),
    append([[FinalizeAtom], OtherAtoms, SampleAtoms, PreRelAtoms], Atoms).
% A shape analyze.pl already named unsupported binds nothing, so its decode
% reaches the named stop below instead of failing this predicate silently.
edge_shape_positive_atoms(_, []).

pre_atom_rel(pre(Atom, _Seed), Atom) :- !.
pre_atom_rel(Atom, Atom).

% analyze.pl:edge_sampled_goals/6 buckets decode/2 with the guards; it splits
% back out here, ahead of them, so a comparison reads what a decode bound.
compile_edge_guards(Mode, GuardGoals, Bound0, Bound, JsonFromParts, WhereTexts) :-
    include(is_decode_goal, GuardGoals, DecodeGoals),
    exclude(is_decode_goal, GuardGoals, PlainGuardGoals),
    compile_json_decodes(DecodeGoals, 0, _, Bound0, Bound1,
                         JsonFromParts, DecodeWhereTexts),
    compile_guard_goals(Mode, PlainGuardGoals, Bound1, Bound, OtherWhereTexts),
    append(DecodeWhereTexts, OtherWhereTexts, WhereTexts).

excluded_assignment(QuotedColumn, Text) :- format(atom(Text), '~w = excluded.~w', [QuotedColumn, QuotedColumn]).

other_atom_use(Atom, use(Ref, Args, pos, unmarked)) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].
pre_atom_use(pre(Atom, Seed), use(Ref, Args, pos, seeded_pre(Seed))) :- !,
    rel_ref(Atom, Ref),
    Atom =.. [_ | Args].
pre_atom_use(Atom, use(Ref, Args, pos, pre)) :-
    rel_ref(Atom, Ref),
    Atom =.. [_ | Args].

negated_atom_use(Atom, use(Ref, Args, neg, unmarked)) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].

% Numbered placeholders (?1, ?2, ...), one per trigger argument position, in
% TRIGGER-argument order -- the emitter passes `arrival.row` (already in
% that exact order, since a rel's stored row IS its declared column order)
% as the bind args UNCHANGED, so a head expression can reference the same
% trigger argument more than once (?1 reused) without the emitter needing
% to reorder or duplicate anything.
% The door interns an arrival's text columns before the resolver binds them
% (§6), so a placeholder carries the same id a stored column does.
compile_trigger_bound(Mode, TriggerArgs, TriggerColumnTypes, Bound) :-
    compile_trigger_bound(Mode, TriggerArgs, TriggerColumnTypes, 1, Bound).
compile_trigger_bound(_, [], [], _, []).
compile_trigger_bound(Mode, [Arg | Rest], [ColumnType | RestTypes], Index,
                      [Arg-typed(Placeholder, ColumnType, Encoding) | MoreBound]) :-
    ( var(Arg) -> true ; throw(unsupported_construct(trigger_arg_not_var(Arg))) ),
    column_encoding(Mode, ColumnType, Encoding),
    format(atom(Placeholder), '?~w', [Index]),
    NextIndex is Index + 1,
    compile_trigger_bound(Mode, Rest, RestTypes, NextIndex, MoreBound).

nth1_list([], _, []).
nth1_list([Position | Rest], List, [Element | More]) :- nth1(Position, List, Element), nth1_list(Rest, List, More).

% ═══ level rule lowering ═════════════════════════════════════════════════════
% levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, RefCountSql):
% InsertSqls is a
% LIST, one entry
% per rule clause headed by HeadRef, not one levelstmt per rule -- the phase
% C sweep found real multi-clause-per-head fixtures (shell_stream.pl's
% terminal_is_terminal: `stream_status(Args, running) <- ...` and
% `stream_status(Args, done) <- ...`, two separate clauses unioning into one
% rel, standard datalog "OR of clauses" semantics engine.pl already
% implements correctly). The original one-levelstmt-per-RULE shape emitted
% an unconditional DELETE per clause, so a second clause sharing the same
% head silently wiped the first clause's just-inserted rows (a genuine
% lowering bug, not a supported-subset gap -- both clauses individually
% compile fine, the union was never taken). Grouping ADJACENT same-head
% rules (strat.pl:sql_rule_order/2 already keeps them adjacent, since a
% stratum group's topo order is per HEAD REF, and every rule sharing that
% ref is emitted together) into one DELETE-once-INSERT-per-clause unit fixes
% it without touching stratification. The common single-clause-per-head case
% (every fixture before this one) still renders byte-identically: InsertSqls
% is a singleton list, and emit_ts.pl's recompute_levels_fn_lines/2 flattens
% [Delete, Insert] the same way whether the list has one entry or several.

level_statement_groups(Mode, RelPlans, RuleOrder, LevelStatements) :-
    group_adjacent_by_head(RuleOrder, Groups),
    maplist(level_statement_group(Mode, RelPlans), Groups, LevelStatements).

group_adjacent_by_head([], []).
group_adjacent_by_head([Rule | Rest], [HeadRef-[Rule | SameHeadRest] | MoreGroups]) :-
    rule_head_ref(Rule, HeadRef),
    take_same_head(HeadRef, Rest, SameHeadRest, Remaining),
    group_adjacent_by_head(Remaining, MoreGroups).

take_same_head(HeadRef, [Rule | Rest], [Rule | SameRest], Remaining) :-
    rule_head_ref(Rule, HeadRef), !,
    take_same_head(HeadRef, Rest, SameRest, Remaining).
take_same_head(_, Rules, [], Rules).

level_statement_group(Mode, RelPlans, HeadRef-Rules,
                      levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql,
                                RefCountSql, AggregateSql, DeltaInternSqls)) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    format(atom(DeleteSql), 'DELETE FROM ~w', [QuotedHeadTable]),
    maplist(level_insert_statements(Mode, RelPlans, HeadRef), Rules, InsertGroups),
    append(InsertGroups, InsertSqls),
    partition(rule_is_aggregate, Rules, AggregateRules, PlainRules),
    ( AggregateRules == []
    -> level_delta_insert_sql(Mode, RelPlans, HeadRef, Rules, DeltaInsertSql,
                                DeltaInternSqls),
       level_ref_count_sql(Mode, RelPlans, HeadRef, Rules, RefCountSql),
       AggregateSql = none
    ;  PlainRules \== []
    -> throw(unsupported_construct(aggregate_head_mixed_with_plain_clause(HeadRef)))
    ;  % An aggregate head is maintained by GROUP-SCOPED recompute, never by
       % the monotone delta-join insert (an aggregate row CHANGES rather than
       % only arriving) and never by refCount reconciliation (a refCount is a
       % count of derivations of one row; an aggregate row has exactly one
       % derivation, its group). Both slots are `none`; the emitter renders
       % them null and 1_incremental.ts dispatches on the aggregate plan.
       DeltaInsertSql = none,
       DeltaInternSqls = [],
       RefCountSql = none,
       (   avg_aggregate_rules(AggregateRules)
       ->  level_avg_sql(Mode, RelPlans, HeadRef, AggregateRules, AggregateSql)
       ;   level_aggregate_sql(Mode, RelPlans, HeadRef, AggregateRules, AggregateSql)
       )
    ).

avg_aggregate_rules(Rules) :-
    Rules \== [],
    forall(member(Rule, Rules),
           ( Rule = (Head <- _),
             aggregate_head_template(Head, Template),
             memberchk(agg(avg, _), Template) )).

% ═══ group-scoped aggregate maintenance (the incremental family) ════════════
% Four statements per tick, all SQL-side, all scoped to the groups this
% tick's deltas can have touched:
%
%   1. ScopeClearSql   DELETE FROM __agg_scope_<head>
%   2. ScopeSeedSqls   one arm per (clause, positive body atom): the GROUP
%                      KEYS reachable from that rel's staged delta rows,
%                      BOTH SIGNS (a retraction changes a group exactly as an
%                      arrival does). DISTINCT, so the arm returns groups, not
%                      derivations.
%   3. DeleteScopedSql DELETE the head's existing rows for those groups,
%                      RETURNING them -> the -1 delta events.
%   4. InsertScopedSqls re-derive those groups from scratch and INSERT,
%                      RETURNING them -> the +1 delta events.
%
% WHY DELETE-THEN-RECOMPUTE RATHER THAN INCREMENTAL ARITHMETIC. count and sum
% are decomposable (+= on add, -= on retract) and min/max are NOT: the
% match-frontier lab's rx table records "incremental min/max over a
% retractable set" as one of its two IMPOSSIBLE rows, because removing the
% current minimum tells you nothing about the next one without re-reading the
% group. Rather than run two maintenance strategies side by side, all four
% kinds take the same shape and the SCOPE is what keeps it cheap: the
% recompute reads only the groups whose members actually moved, never the
% whole table. That is the header's "delta-compare on inserts, GROUP-SCOPED
% recompute on deletes, never whole-table" with one statement family instead
% of two.
%
% A -1/+1 pair for a group whose aggregate value did NOT change cancels at
% the boundary: 1_incremental.ts's boundaryDelta/2 groups by ROW VALUE and
% sums the signed counts, so an unchanged row nets to zero and never reaches
% the tick log. Over-approximating the scope is therefore SAFE (a wasted
% recompute, invisible); under-approximating would not be, which is why the
% seed arms carry no guard conditions at all.
level_aggregate_sql(Mode, RelPlans, HeadRef, Rules,
                    aggsql(ScopeColumns, ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                           DeleteScopedSql, InsertScopedSqls, InternSqls)) :-
    aggregate_scope_columns(RelPlans, HeadRef, Rules, ScopeColumns, ScopeTypes),
    aggregate_scope_table_name(HeadRef, ScopeTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    format(atom(ScopeClearSql), 'DELETE FROM ~w', [QuotedScopeTable]),
    findall(SeedSql,
            ( member(Rule, Rules),
              aggregate_scope_seed_sql(Mode, RelPlans, ScopeColumns, QuotedScopeTable,
                                       Rule, SeedSql) ),
            ScopeSeedSqls),
    aggregate_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns,
                                QuotedScopeTable, DeleteScopedSql),
    maplist(aggregate_insert_scoped_sql(Mode, RelPlans, HeadRef, ScopeColumns,
                                        QuotedScopeTable),
            Rules, InsertScopedPairs),
    pairs_keys_values(InsertScopedPairs, InsertScopedSqls, InternGroups),
    append(InternGroups, InternSqls).

% AVG is decomposed at the storage seam. The accumulator table holds one
% REAL sum and one INTEGER count per live group; the public head stores only
% sum/count, so a changed group reads one accumulator row and never scans the
% source relation during the delta path. The refresh statements are scoped to
% groups touched by this tick. Boot uses the same source query without the
% tick-local delta predicate.
level_avg_sql(Mode, RelPlans, HeadRef, Rules,
              avgsql(ScopeColumns, ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                     DeleteScopedSql, InsertScopedSqls, BootSqls)) :-
    aggregate_scope_columns(RelPlans, HeadRef, Rules, ScopeColumns, ScopeTypes),
    aggregate_scope_table_name(HeadRef, ScopeTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    format(atom(ScopeClearSql), 'DELETE FROM ~w', [QuotedScopeTable]),
    findall(SeedSql,
            ( member(Rule, Rules),
              aggregate_scope_seed_sql(Mode, RelPlans, ScopeColumns, QuotedScopeTable,
                                       Rule, SeedSql) ),
            ScopeMarkers),
    avg_accumulator_table_name(HeadRef, AccumulatorTable),
    quote_ident(AccumulatorTable, QuotedAccumulatorTable),
    avg_refresh_sqls(Mode, RelPlans, HeadRef, Rules, ScopeColumns,
                     QuotedScopeTable, QuotedAccumulatorTable,
                     RefreshSqls),
    avg_accumulator_seed_sql(ScopeColumns, QuotedScopeTable,
                             QuotedAccumulatorTable, AccumulatorSeedSql),
    append([ScopeMarkers, [AccumulatorSeedSql], RefreshSqls], ScopeSeedSqls),
    avg_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                          DeleteScopedSql),
    Rules = [FirstRule | _],
    FirstRule =.. [_Operator, Head, _Body],
    aggregate_head_template(Head, Template),
    avg_insert_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                          QuotedAccumulatorTable, Template, InsertScopedSql),
    InsertScopedSqls = [InsertScopedSql],
    format(atom(ClearAccumulatorSql), 'DELETE FROM ~w', [QuotedAccumulatorTable]),
    avg_boot_refresh_sqls(Mode, RelPlans, HeadRef, Rules, QuotedAccumulatorTable,
                          BootRefreshSqls),
    avg_boot_insert_sql(RelPlans, HeadRef, QuotedAccumulatorTable, Template,
                        BootInsertSql),
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    format(atom(BootDeleteSql), 'DELETE FROM ~w', [QuotedHeadTable]),
    append([[ClearAccumulatorSql], BootRefreshSqls,
            [BootDeleteSql, BootInsertSql]], BootSqls).

avg_accumulator_table_name(Ref, Table) :-
    table_name(Ref, StorageName),
    format(atom(Table), '__avg_acc_~w', [StorageName]).

avg_refresh_sqls(Mode, RelPlans, HeadRef, Rules, ScopeColumns,
                 QuotedScopeTable, QuotedAccumulatorTable, Sqls) :-
    maplist(avg_refresh_sql(Mode, RelPlans, HeadRef, ScopeColumns,
                            QuotedScopeTable, QuotedAccumulatorTable),
            Rules, Sqls).

avg_boot_refresh_sqls(Mode, RelPlans, HeadRef, Rules, QuotedAccumulatorTable, Sqls) :-
    maplist(avg_boot_refresh_sql(Mode, RelPlans, HeadRef, QuotedAccumulatorTable),
            Rules, Sqls).

avg_refresh_sql(Mode, RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                QuotedAccumulatorTable, Rule, Sql) :-
    avg_delta_rows_sql(Mode, RelPlans, HeadRef, Rule, DeltaRowsSql),
    avg_accumulator_scope_predicate(ScopeColumns, QuotedScopeTable, 'a', ScopeSql),
    avg_accumulator_update_sql(QuotedAccumulatorTable, ScopeColumns, DeltaRowsSql,
                               ScopeSql, Sql).

avg_boot_refresh_sql(Mode, RelPlans, HeadRef, QuotedAccumulatorTable, Rule, Sql) :-
    avg_body_rows_sql(Mode, RelPlans, HeadRef, Rule, BodyRowsSql),
    avg_accumulator_boot_update_sql(QuotedAccumulatorTable, BodyRowsSql, Sql).

avg_accumulator_seed_sql(['_all'], QuotedScopeTable,
                         QuotedAccumulatorTable, Sql) :- !,
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w ("__group_1", "__sum", "__count") SELECT 0, 0.0, 0 FROM ~w',
           [QuotedAccumulatorTable, QuotedScopeTable]).
avg_accumulator_seed_sql(ScopeColumns, QuotedScopeTable,
                         QuotedAccumulatorTable, Sql) :-
    avg_scope_seed_projection(ScopeColumns, 1, ProjectionColumns),
    avg_accumulator_key_columns(ScopeColumns, 1, AccumulatorKeyColumns),
    atomic_list_concat(ProjectionColumns, ', ', ProjectionSql),
    atomic_list_concat(AccumulatorKeyColumns, ', ', AccumulatorKeysSql),
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w (~w, "__sum", "__count") SELECT ~w, 0.0, 0 FROM ~w',
           [QuotedAccumulatorTable, AccumulatorKeysSql, ProjectionSql,
            QuotedScopeTable]).

avg_scope_seed_projection([], _, []).
avg_scope_seed_projection([Column | Rest], Position, [QuotedColumn | More]) :-
    quote_ident(Column, QuotedColumn),
    NextPosition is Position + 1,
    avg_scope_seed_projection(Rest, NextPosition, More).

% The body projection is deliberately a two-column relation: group key plus
% numeric contribution. Keeping the projection explicit makes the accumulator
% update independent of the public head's derived average column.
avg_body_rows_sql(Mode, RelPlans, _HeadRef, (Head <- Body), Sql) :-
    aggregate_head_template(Head, Template),
    memberchk(agg(avg, ValueExpr), Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    aggregate_group_exprs(Mode, Template, Bound, GroupExprs),
    compile_expr(Mode, identity, ValueExpr, Bound, ValueSql, ValueType, _Encoding),
    memberchk(ValueType, [int, float]),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    append(FromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    avg_body_where_sql(WhereTexts, WhereSql),
    avg_body_projection(GroupExprs, ValueSql, ProjectionSql),
    format(atom(Sql), 'SELECT ~w FROM ~w~w',
           [ProjectionSql, FromSql, WhereSql]).

% The delta path updates sum and count from the staged signed rows. This keeps
% the source relation out of the maintenance query: the source scan belongs
% only to boot, while each tick searches the delta rows for affected groups.
avg_delta_rows_sql(Mode, RelPlans, _HeadRef, (Head <- Body), Sql) :-
    aggregate_head_template(Head, Template),
    memberchk(agg(avg, ValueExpr), Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, [use(DeltaRef, DeltaArgs, pos, _)]),
    include(is_negative_use, Uses, []),
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(Mode, DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      Bound0, DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    JsonFromParts = [],
    aggregate_group_exprs(Mode, Template, Bound, GroupExprs),
    compile_expr(Mode, identity, ValueExpr, Bound, ValueSql, ValueType, _Encoding),
    memberchk(ValueType, [int, float]),
    append([DeltaWhereTexts, GuardWhereTexts], WhereTexts0),
    avg_group_projection(GroupExprs, 1, GroupProjectionParts),
    format(atom(ValueProjection), '~w AS "__value"', [ValueSql]),
    append(GroupProjectionParts,
           [ValueProjection, 'd0."_sign" AS "__sign"'],
           ProjectionParts),
    atomic_list_concat(ProjectionParts, ', ', ProjectionSql),
    append(['d0."_sign" IN (-1, 1)'], WhereTexts0, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(Sql), 'SELECT ~w FROM ~w d0 WHERE ~w',
           [ProjectionSql, QuotedDeltaTable, WhereSql]).

avg_body_projection([], ValueSql, ProjectionSql) :-
    format(atom(ProjectionSql), '~w AS "__value"', [ValueSql]),
    !.
avg_body_projection(GroupExprs, ValueSql, ProjectionSql) :-
    avg_group_projection(GroupExprs, 1, GroupParts),
    format(atom(ValueProjection), '~w AS "__value"', [ValueSql]),
    append(GroupParts, [ValueProjection], Parts),
    atomic_list_concat(Parts, ', ', ProjectionSql).

avg_group_projection([], _, []).
avg_group_projection([GroupExpr | Rest], Position, [Sql | More]) :-
    format(atom(Sql), '~w AS "__group_~w"', [GroupExpr, Position]),
    NextPosition is Position + 1,
    avg_group_projection(Rest, NextPosition, More).

avg_body_where_sql([], '').
avg_body_where_sql(WhereTexts, WhereSql) :-
    WhereTexts \== [],
    atomic_list_concat(WhereTexts, ' AND ', Joined),
    format(atom(WhereSql), ' WHERE ~w', [Joined]).

avg_accumulator_scope_predicate(['_all'], _QuotedScopeTable, _Alias,
                                '1 = 1') :- !.
avg_accumulator_scope_predicate(ScopeColumns, QuotedScopeTable, _Alias, Sql) :-
    avg_scope_key_columns(ScopeColumns, 1, ScopeKeys),
    atomic_list_concat(ScopeKeys, ', ', ScopeKeysSql),
    avg_accumulator_key_columns(ScopeColumns, 1, AccumulatorKeys),
    atomic_list_concat(AccumulatorKeys, ', ', AccumulatorKeysSql),
    format(atom(Sql), '(~w) IN (SELECT ~w FROM ~w)',
           [AccumulatorKeysSql, ScopeKeysSql, QuotedScopeTable]).

avg_scope_key_columns([], _, []).
avg_scope_key_columns([Column | Rest], Position, [QuotedColumn | More]) :-
    quote_ident(Column, QuotedColumn),
    NextPosition is Position + 1,
    avg_scope_key_columns(Rest, NextPosition, More).

avg_scope_equalities([], _, _, _, []).
avg_scope_equalities([Column | Rest], QuotedScopeTable, Alias, Position,
                     [Sql | More]) :-
    quote_ident(Column, QuotedColumn),
    format(atom(AccumulatorColumn), '"__group_~w"', [Position]),
    format(atom(Sql), 'EXISTS (SELECT 1 FROM ~w s WHERE s.~w = ~w.~w)',
           [QuotedScopeTable, QuotedColumn, Alias, AccumulatorColumn]),
    NextPosition is Position + 1,
    avg_scope_equalities(Rest, QuotedScopeTable, Alias, NextPosition, More).

avg_accumulator_update_sql(QuotedAccumulatorTable, ScopeColumns, BodyRowsSql,
                           ScopeSql, Sql) :-
    avg_body_matches_accumulator(ScopeColumns, MatchSql),
    avg_where_parts([MatchSql, ScopeSql], BodyWhereSql),
    format(atom(Sql),
           'UPDATE ~w AS a SET "__sum" = "__sum" + COALESCE((SELECT sum(contributions."__sign" * contributions."__value") FROM (~w) contributions WHERE ~w), 0.0), "__count" = "__count" + COALESCE((SELECT sum(contributions."__sign") FROM (~w) contributions WHERE ~w), 0) WHERE ~w',
           [QuotedAccumulatorTable, BodyRowsSql, BodyWhereSql,
            BodyRowsSql, BodyWhereSql, ScopeSql]).

avg_body_matches_accumulator(['_all'], '1 = 1') :- !.
avg_body_matches_accumulator(ScopeColumns, Sql) :-
    avg_body_acc_equalities(ScopeColumns, 1, Equalities),
    atomic_list_concat(Equalities, ' AND ', Sql).

avg_body_acc_equalities([], _, []).
avg_body_acc_equalities([_Column | Rest], Position, [Sql | More]) :-
    format(atom(AccumulatorColumn), '"__group_~w"', [Position]),
    format(atom(Sql), 'contributions."__group_~w" = a.~w',
           [Position, AccumulatorColumn]),
    NextPosition is Position + 1,
    avg_body_acc_equalities(Rest, NextPosition, More).

avg_where_parts(Parts, Sql) :-
    exclude(=('1 = 1'), Parts, Meaningful),
    ( Meaningful == [] -> Sql = '1 = 1' ; atomic_list_concat(Meaningful, ' AND ', Sql) ).

avg_accumulator_boot_update_sql(QuotedAccumulatorTable, BodyRowsSql, Sql) :-
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w ("__group_1", "__sum", "__count") SELECT "__group_1", sum("__value"), count(*) FROM (~w) contributions GROUP BY "__group_1"',
           [QuotedAccumulatorTable, BodyRowsSql]).

avg_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                      Sql) :-
    aggregate_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns,
                                QuotedScopeTable, Sql).

avg_insert_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                      QuotedAccumulatorTable, Template, Sql) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    avg_head_projection(Template, ProjectionColumns),
    atomic_list_concat(ProjectionColumns, ', ', ProjectionSql),
    avg_accumulator_scope_predicate(ScopeColumns, QuotedScopeTable, 'a', ScopeSql),
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM ~w a WHERE a."__count" > 0 AND ~w RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, ProjectionSql,
            QuotedAccumulatorTable, ScopeSql, HeadColumnsSql]).

avg_boot_insert_sql(RelPlans, HeadRef, QuotedAccumulatorTable, Template, Sql) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    avg_head_projection(Template, ProjectionColumns),
    atomic_list_concat(ProjectionColumns, ', ', ProjectionSql),
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM ~w a WHERE a."__count" > 0 RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, ProjectionSql,
            QuotedAccumulatorTable, HeadColumnsSql]).

avg_head_projection(Template, ProjectionColumns) :-
    avg_head_projection_(Template, 1, ProjectionColumns).

avg_head_projection_([], _, []) :- !.
avg_head_projection_([plain(_) | Rest], Position,
                     [Projection | More]) :-
    !,
    format(atom(Projection), 'a."__group_~w"', [Position]),
    NextPosition is Position + 1,
    avg_head_projection_(Rest, NextPosition, More).
avg_head_projection_([agg(avg, _) | Rest], Position,
                     ['a."__sum" / a."__count"' | More]) :-
    !,
    avg_head_projection_(Rest, Position, More).

avg_scope_from(['_all'], QuotedScopeTable, QuotedAccumulatorTable,
               FromSql) :- !,
    format(atom(FromSql), '~w s JOIN ~w a ON 1 = 1',
           [QuotedScopeTable, QuotedAccumulatorTable]).
avg_scope_from(ScopeColumns, QuotedScopeTable, QuotedAccumulatorTable,
               FromSql) :-
    avg_join_equalities(ScopeColumns, 1, Equalities),
    atomic_list_concat(Equalities, ' AND ', EqualitySql),
    format(atom(FromSql), '~w s JOIN ~w a ON ~w',
           [QuotedScopeTable, QuotedAccumulatorTable, EqualitySql]).

avg_join_equalities([], _, []).
avg_join_equalities([Column | Rest], Position, [Sql | More]) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Sql), 'a."__group_~w" = s.~w', [Position, QuotedColumn]),
    NextPosition is Position + 1,
    avg_join_equalities(Rest, NextPosition, More).

aggregate_scope_table_name(Ref, ScopeTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__agg_scope_', Table], ScopeTable).

% The scope table's columns are the head's OWN plain (grouped) columns, so a
% group key in the scope table and a group key in the head table compare
% column for column with matching storage types. A head with zero plain
% columns (star_bag(json_array(_)) shape: one row for the whole rel) gets a
% single sentinel column instead, since SQLite has no zero-column table.
aggregate_scope_columns(RelPlans, HeadRef, [Rule | _], ScopeColumns, ScopeTypes) :-
    Rule = (Head <- _),
    aggregate_head_template(Head, Template),
    aggregate_group_positions(Template, Positions),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    ( Positions == []
    -> ScopeColumns = ['_all'], ScopeTypes = [int]
    ;  nth1_list(Positions, HeadColumns, ScopeColumns),
       nth1_list(Positions, HeadColumnTypes, ScopeTypes)
    ).

% One seed arm per positive body atom of one clause. The group expressions
% are compiled against THAT atom's staged delta rows alone (alias d0 over
% __delta_<ref>); if a group expression needs a variable this atom does not
% bind, compile_expr throws unbound_head_var and the whole program is refused
% by name (aggregate_group_not_delta_local) rather than silently seeding an
% incomplete scope -- an UNDER-approximated scope would leave a stale
% aggregate row behind with no delta at all.
aggregate_scope_seed_sql(Mode, RelPlans, ScopeColumns, QuotedScopeTable, (Head <- Body),
                         SeedSql) :-
    aggregate_head_template(Head, Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    member(use(DeltaRef, DeltaArgs, pos, _), PosUses),
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(Mode, DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      DeltaBound, DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    aggregate_scope_group_exprs(Mode, Template, DeltaBound, Head, GroupExprs),
    atomic_list_concat(GroupExprs, ', ', GroupSql),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    atomic_list_concat(QuotedScopeColumns, ', ', ScopeColumnsSql),
    append(['d0."_sign" IN (-1, 1)'], DeltaWhereTexts, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(SeedSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT DISTINCT ~w FROM ~w d0 WHERE ~w',
           [QuotedScopeTable, ScopeColumnsSql, GroupSql, QuotedDeltaTable, WhereSql]).

% A NEGATED atom is a delta source too, and only seeded when its own args bind
% every group column; the rest is the open row in docs/failure-modes.md.
aggregate_scope_seed_sql(Mode, RelPlans, ScopeColumns, QuotedScopeTable, (Head <- Body),
                         SeedSql) :-
    aggregate_head_template(Head, Template),
    body_ref_uses(Body, Uses),
    member(use(DeltaRef, DeltaArgs, neg, _), Uses),
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(Mode, DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      DeltaBound, DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    catch(aggregate_scope_group_exprs(Mode, Template, DeltaBound, Head, GroupExprs),
          unsupported_construct(aggregate_group_not_delta_local(_)),
          fail),
    atomic_list_concat(GroupExprs, ', ', GroupSql),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    atomic_list_concat(QuotedScopeColumns, ', ', ScopeColumnsSql),
    append(['d0."_sign" IN (-1, 1)'], DeltaWhereTexts, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(SeedSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT DISTINCT ~w FROM ~w d0 WHERE ~w',
           [QuotedScopeTable, ScopeColumnsSql, GroupSql, QuotedDeltaTable, WhereSql]).

aggregate_scope_group_exprs(Mode, Template, DeltaBound, Head, GroupExprs) :-
    aggregate_group_positions(Template, Positions),
    ( Positions == []
    -> GroupExprs = ['0']
    ;  catch(aggregate_group_exprs(Mode, Template, DeltaBound, GroupExprs),
             unsupported_construct(unbound_head_var(_)),
             throw(unsupported_construct(aggregate_group_not_delta_local(Head))))
    ).

aggregate_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                            DeleteScopedSql) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    atomic_list_concat(QuotedScopeColumns, ', ', ScopeColumnsSql),
    ( ScopeColumns == ['_all']
    -> format(atom(DeleteScopedSql),
              'DELETE FROM ~w WHERE EXISTS (SELECT 1 FROM ~w) RETURNING ~w',
              [QuotedHeadTable, QuotedScopeTable, HeadColumnsSql])
    ;  format(atom(DeleteScopedSql),
              'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM ~w) RETURNING ~w',
              [QuotedHeadTable, ScopeColumnsSql, ScopeColumnsSql,
               QuotedScopeTable, HeadColumnsSql])
    ).

aggregate_insert_scoped_sql(Mode, RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                            (Head <- Body), InsertScopedSql-InternSqls) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    aggregate_head_template(Head, Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    aggregate_group_exprs(Mode, Template, Bound, GroupExprs),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    atomic_list_concat(QuotedScopeColumns, ', ', ScopeColumnsSql),
    ( ScopeColumns == ['_all']
    -> format(atom(ScopeWhereText), 'EXISTS (SELECT 1 FROM ~w)', [QuotedScopeTable])
    ;  atomic_list_concat(GroupExprs, ', ', GroupKeySql),
       format(atom(ScopeWhereText), '(~w) IN (SELECT ~w FROM ~w)',
              [GroupKeySql, ScopeColumnsSql, QuotedScopeTable])
    ),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts, [ScopeWhereText]],
           AllWhereTexts),
    append(FromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    aggregate_select_statement(Mode, HeadColumnTypes, Head, Template, Bound, FromSql,
                               AllWhereTexts, SelectStatement, InternSqls),
    format(atom(InsertScopedSql), 'INSERT OR IGNORE INTO ~w (~w) ~w RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, SelectStatement, HeadColumnsSql]).

% The scope columns and their storage types ride inside the aggsql/7 term
% itself, so DDL emission needs nothing but the levelstmt (lower_program/2
% no longer has the rule list in scope by then).
aggregate_scope_ddl(Mode, levelstmt(HeadRef, _, _, _, _,
                              aggsql(ScopeColumns, ScopeTypes, _, _, _, _, _), _),
                    [Ddl]) :- !,
    aggregate_scope_table_name(HeadRef, ScopeTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    maplist(column_def(Mode), QuotedScopeColumns, ScopeTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedScopeColumns, ', ', PrimaryKeySql),
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedScopeTable, ColumnsSql, PrimaryKeySql]).
aggregate_scope_ddl(Mode, levelstmt(HeadRef, _, _, _, _,
                              avgsql(ScopeColumns, ScopeTypes, _, _, _, _, _), _),
                    [ScopeDdl, AccumulatorDdl]) :- !,
    aggregate_scope_table_name(HeadRef, ScopeTable),
    avg_accumulator_table_name(HeadRef, AccumulatorTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    quote_ident(AccumulatorTable, QuotedAccumulatorTable),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    maplist(column_def(Mode), QuotedScopeColumns, ScopeTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedScopeColumns, ', ', PrimaryKeySql),
    format(atom(ScopeDdl),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedScopeTable, ColumnsSql, PrimaryKeySql]),
    avg_accumulator_columns(Mode, ScopeColumns, ScopeTypes,
                            AccumulatorColumnsSql, AccumulatorPrimaryKeySql),
    format(atom(AccumulatorDdl),
           'CREATE TEMP TABLE ~w (~w, "__sum" REAL NOT NULL, "__count" INTEGER NOT NULL, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedAccumulatorTable, AccumulatorColumnsSql,
            AccumulatorPrimaryKeySql]).
aggregate_scope_ddl(_, _, []).

avg_accumulator_columns(_, ['_all'], _,
                        '"__group_1" INTEGER NOT NULL', '"__group_1"') :- !.
avg_accumulator_columns(Mode, ScopeColumns, ScopeTypes, ColumnsSql,
                        PrimaryKeySql) :-
    avg_accumulator_group_columns(Mode, ScopeColumns, ScopeTypes, 1, GroupColumns),
    atomic_list_concat(GroupColumns, ', ', ColumnsSql),
    avg_accumulator_key_columns(ScopeColumns, 1, KeyColumns),
    atomic_list_concat(KeyColumns, ', ', PrimaryKeySql).

avg_accumulator_group_columns(_, [], [], _, []).
avg_accumulator_group_columns(Mode, [_ | RestColumns], [Type | RestTypes], Position,
                              [Column | More]) :-
    format(atom(QuotedColumn), '"__group_~w"', [Position]),
    column_def(Mode, QuotedColumn, Type, Column),
    NextPosition is Position + 1,
    avg_accumulator_group_columns(Mode, RestColumns, RestTypes, NextPosition, More).

avg_accumulator_key_columns([], _, []).
avg_accumulator_key_columns([_ | Rest], Position, [Column | More]) :-
    format(atom(Column), '"__group_~w"', [Position]),
    NextPosition is Position + 1,
    avg_accumulator_key_columns(Rest, NextPosition, More).

% The delta and both frontier copies are written by SQL that reads the same
% predicates the head mutation reads, so no derived row crosses the JS seam.
level_ref_count_sql(Mode, RelPlans, HeadRef, Rules,
                  refcountsql(ClearSql, SeedSql, UpdateSql, StageRetractSql,
                             CollectZeroSql, ClearNewSql, FillNewSql,
                             StageAddSql, StageFrontierSql,
                             StageNextFrontierSql, InsertNewSql, ExpandPlan,
                             DredPlan, FixpointIr, SupportInternSqls,
                             SupportCountPlan)) :-
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    ref_count_table_name(HeadRef, RefCountTable),
    quote_ident(RefCountTable, QuotedRefCountTable),
    delta_table_name(HeadRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    frontier_table_name(HeadRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    next_frontier_table_name(HeadRef, NextFrontierTable),
    quote_ident(NextFrontierTable, QuotedNextFrontierTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    qualified_column_list(HeadColumns, n, NewColumnsSql),
    qualified_equalities(HeadColumns, n, h, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    format(atom(ClearSql), 'DELETE FROM ~w', [QuotedRefCountTable]),
    ( rules_read_head_recursively(HeadRef, Rules)
    -> recursive_ref_count_seed_sql(Mode, RelPlans, HeadRef, Rules,
                                  QuotedRefCountTable, HeadColumns,
                                  HeadColumnsSql, SeedSql),
       SupportInternSqls = [],
       level_expand_plan(Mode, RelPlans, HeadRef, Rules, ExpandPlan),
       ( level_dred_plan(Mode, RelPlans, HeadRef, Rules, DredPlan0)
       -> DredPlan = DredPlan0
       ;  DredPlan = none
       )
    ;  counted_ref_count_seed_sql(Mode, RelPlans, Rules, QuotedRefCountTable,
                                HeadColumnsSql, SeedSql, SupportInternSqls),
       ExpandPlan = none,
       DredPlan = none
    ),
    (   DredPlan == none
    ->  FixpointIr = none
    ;   level_fixpoint_ir(Mode, RelPlans, HeadRef, Rules, FixpointIr0)
    ->  FixpointIr = FixpointIr0
    ;   FixpointIr = none
    ),
    format(atom(UpdateSql),
           'UPDATE ~w AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM ~w n WHERE ~w), 0)',
           [QuotedHeadTable, QuotedRefCountTable, EqualitySql]),
    format(atom(StageRetractSql),
           'INSERT INTO ~w ("_sign", "_sequence", ~w) SELECT -1, row_number() OVER () - 1, ~w FROM ~w WHERE "__refcount" <= 0',
           [QuotedDeltaTable, HeadColumnsSql, HeadColumnsSql, QuotedHeadTable]),
    format(atom(CollectZeroSql),
           'DELETE FROM ~w WHERE "__refcount" <= 0',
           [QuotedHeadTable]),
    arrival_scratch_table_name(HeadRef, NewTable),
    quote_ident(NewTable, QuotedNewTable),
    format(atom(ClearNewSql), 'DELETE FROM ~w', [QuotedNewTable]),
    % The antijoin runs ONCE into a rowid scratch table. Its rowid then serves
    % as `_sequence`, which no set rel orders by, replacing three window sorts.
    nth0(0, HeadColumns, FirstColumn),
    quote_ident(FirstColumn, QuotedFirstColumn),
    format(atom(FillNewSql),
           'INSERT INTO ~w (~w, "__refcount") SELECT ~w, n."__refcount" FROM ~w n LEFT JOIN ~w h ON ~w WHERE h.~w IS NULL',
           [QuotedNewTable, HeadColumnsSql, NewColumnsSql, QuotedRefCountTable,
            QuotedHeadTable, EqualitySql, QuotedFirstColumn]),
    format(atom(StageAddSql),
           'INSERT INTO ~w ("_sign", "_sequence", ~w) SELECT 1, "rowid" - 1, ~w FROM ~w',
           [QuotedDeltaTable, HeadColumnsSql, HeadColumnsSql, QuotedNewTable]),
    stage_frontier_sqls(HeadRef, QuotedFrontierTable, QuotedNextFrontierTable,
                        QuotedHeadTable, QuotedNewTable, HeadColumnsSql,
                        EqualitySql, StageFrontierSql, StageNextFrontierSql),
    % OR IGNORE lets the head's own primary key reject the rows already there,
    % so the fill reads the support table straight through with no antijoin.
    format(atom(InsertNewSql),
           'INSERT OR IGNORE INTO ~w (~w, "__refcount") SELECT ~w, n."__refcount" FROM ~w n',
           [QuotedHeadTable, HeadColumnsSql, NewColumnsSql, QuotedRefCountTable]),
    support_count_plan(Mode, RelPlans, HeadRef, Rules, QuotedRefCountTable,
                       QuotedHeadTable, HeadColumns, SupportCountPlan).

% frontier(shared): the recount verb's shared arm publishes this rel's
% per-rule support to the shared ledger, keyed by the durable row id, after
% the head insert so every resident row carries its counts. rule_id is the
% arm's 0-based ordinal among the head's lowered rules, the same ordering
% statement_rule_ids/3 numbers. A single-arm head reads the staging table it
% already filled; two or more re-read their own arm, since the staging sum
% cannot be split back apart.
support_count_plan(Mode, RelPlans, HeadRef, Rules, QuotedRefCountTable,
                   QuotedHeadTable, HeadColumns,
                   supportcount(ClearSql, WriteSqls)) :-
    frontier_mode(shared),
    !,
    shared_frontier_relation_id(HeadRef, RelationId),
    format(atom(ClearSql),
           'DELETE FROM "__support_count" WHERE "relation_id" = ~w',
           [RelationId]),
    (   Rules = [_]
    ->  qualified_equalities(HeadColumns, n, h, StagingEqualities),
        atomic_list_concat(StagingEqualities, ' AND ', StagingEqualitySql),
        format(atom(WriteSql),
               'INSERT INTO "__support_count" ("relation_id", "row_id", "rule_id", "count") SELECT ~w, h."__id", 0, n."__refcount" FROM ~w n JOIN ~w h ON ~w',
               [RelationId, QuotedRefCountTable, QuotedHeadTable,
                StagingEqualitySql]),
        WriteSqls = [WriteSql]
    ;   qualified_equalities(HeadColumns, a, h, ArmEqualities),
        atomic_list_concat(ArmEqualities, ' AND ', ArmEqualitySql),
        findall(ArmWriteSql,
                ( nth0(RuleId, Rules, Rule),
                  level_ref_count_arm(Mode, RelPlans, Rule, Arm, _),
                  format(atom(ArmWriteSql),
                         'INSERT INTO "__support_count" ("relation_id", "row_id", "rule_id", "count") SELECT ~w, h."__id", ~w, a."__refcount" FROM (~w) a JOIN ~w h ON ~w',
                         [RelationId, RuleId, Arm, QuotedHeadTable,
                          ArmEqualitySql]) ),
                WriteSqls)
    ).
support_count_plan(_, _, _, _, _, _, _, none).

arrival_scratch_table_name(Ref, NewTable) :-
    table_name(Ref, Table),
    atomic_list_concat(['__new_', Table], NewTable).

% Shared mode stages (relation_id, phase, sequence, row_id): the durable
% row's __id resolved by joining the head on the __new_ scratch columns.
stage_frontier_sqls(HeadRef, _QuotedFrontierTable, _QuotedNextFrontierTable,
                    QuotedHeadTable, QuotedNewTable, _HeadColumnsSql,
                    EqualitySql, StageFrontierSql, StageNextFrontierSql) :-
    frontier_mode(shared),
    !,
    shared_frontier_relation_id(HeadRef, RelationId),
    format(atom(StageFrontierSql),
           'INSERT INTO "__frontier" ("relation_id", "_phase", "_sequence", "row_id") SELECT ~w, ?, n."rowid" - 1, h."__id" FROM ~w n JOIN ~w h ON ~w',
           [RelationId, QuotedNewTable, QuotedHeadTable, EqualitySql]),
    format(atom(StageNextFrontierSql),
           'INSERT INTO "__next_frontier" ("relation_id", "_phase", "_sequence", "row_id") SELECT ~w, ?, n."rowid" - 1, h."__id" FROM ~w n JOIN ~w h ON ~w',
           [RelationId, QuotedNewTable, QuotedHeadTable, EqualitySql]).
stage_frontier_sqls(_HeadRef, QuotedFrontierTable, QuotedNextFrontierTable,
                    _QuotedHeadTable, QuotedNewTable, HeadColumnsSql,
                    _EqualitySql, StageFrontierSql, StageNextFrontierSql) :-
    format(atom(StageFrontierSql),
           'INSERT INTO ~w ("_phase", "_sequence", ~w) SELECT ?, "rowid" - 1, ~w FROM ~w',
           [QuotedFrontierTable, HeadColumnsSql, HeadColumnsSql, QuotedNewTable]),
    format(atom(StageNextFrontierSql),
           'INSERT INTO ~w ("_phase", "_sequence", ~w) SELECT ?, "rowid" - 1, ~w FROM ~w',
           [QuotedNextFrontierTable, HeadColumnsSql, HeadColumnsSql, QuotedNewTable]).

qualified_column_list(Columns, Alias, Sql) :-
    findall(Part,
            ( member(Column, Columns),
              quote_ident(Column, QuotedColumn),
              format(atom(Part), '~w.~w', [Alias, QuotedColumn]) ),
            Parts),
    atomic_list_concat(Parts, ', ', Sql).

counted_ref_count_seed_sql(Mode, RelPlans, Rules, QuotedRefCountTable,
                         HeadColumnsSql, SeedSql, InternSqls) :-
    maplist(level_ref_count_arm(Mode, RelPlans), Rules, RefCountArms, InternGroups),
    append(InternGroups, InternSqls),
    atomic_list_concat(RefCountArms, ' UNION ALL ', RefCountUnionSql),
    format(atom(SeedSql),
           'INSERT INTO ~w (~w, "__refcount") SELECT ~w, sum("__refcount") FROM (~w) GROUP BY ~w',
           [QuotedRefCountTable, HeadColumnsSql, HeadColumnsSql,
            RefCountUnionSql, HeadColumnsSql]).

recursive_ref_count_seed_sql(Mode, RelPlans, HeadRef, Rules, QuotedRefCountTable,
                           HeadColumns, HeadColumnsSql, SeedSql) :-
    partition(rule_reads_head(HeadRef), Rules, RecursiveRules, BaseRules),
    maplist(check_single_recursive_read(HeadRef), RecursiveRules),
    maplist(level_recursive_arm(Mode, RelPlans), BaseRules, BaseArms0),
    maplist(level_recursive_arm(Mode, RelPlans), RecursiveRules, RecursiveArms),
    ( BaseArms0 == []
    -> empty_recursive_anchor(HeadColumns, EmptyAnchor),
       BaseArms = [EmptyAnchor]
    ;  BaseArms = BaseArms0
    ),
    append(BaseArms, RecursiveArms, AllArms),
    atomic_list_concat(AllArms, ' UNION ', RecursiveUnionSql),
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    format(atom(SeedSql),
           'INSERT INTO ~w (~w, "__refcount") WITH RECURSIVE ~w (~w) AS (~w) SELECT ~w, 1 FROM ~w',
           [QuotedRefCountTable, HeadColumnsSql, QuotedHeadTable,
            HeadColumnsSql, RecursiveUnionSql, HeadColumnsSql,
            QuotedHeadTable]).

% Bounds HOPS, not rows: a closure is bounded by graph depth, a growing
% measure passes any depth. Both doors read this number out of the plan.
fixpoint_round_cap(1000).

% rx `expand` spelling of the recursive seed: same fixpoint as the CTE, and
% the refCount table's WITHOUT ROWID key keeps downstream scan order identical.
level_expand_plan(Mode, RelPlans, HeadRef, Rules,
                  expandplan(ClearASql, ClearBSql, SeedSqls,
                             HopABSql, HopBASql, AbsorbASql, AbsorbBSql,
                             RoundCap)) :-
    fixpoint_round_cap(RoundCap),
    partition(rule_reads_head(HeadRef), Rules, RecursiveRules, BaseRules),
    expand_table_name(HeadRef, a, TableA),
    expand_table_name(HeadRef, b, TableB),
    quote_ident(TableA, QuotedTableA),
    quote_ident(TableB, QuotedTableB),
    ref_count_table_name(HeadRef, RefCountTable),
    quote_ident(RefCountTable, QuotedRefCountTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(ClearASql), 'DELETE FROM ~w', [QuotedTableA]),
    format(atom(ClearBSql), 'DELETE FROM ~w', [QuotedTableB]),
    maplist(expand_seed_sql(Mode, RelPlans, QuotedTableA, HeadColumnsSql),
            BaseRules, SeedSqls),
    expand_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedTableA,
                   QuotedTableB, QuotedRefCountTable, HeadColumns,
                   HeadColumnsSql, HopABSql),
    expand_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedTableB,
                   QuotedTableA, QuotedRefCountTable, HeadColumns,
                   HeadColumnsSql, HopBASql),
    expand_absorb_sql(QuotedRefCountTable, QuotedTableA, HeadColumnsSql, AbsorbASql),
    expand_absorb_sql(QuotedRefCountTable, QuotedTableB, HeadColumnsSql, AbsorbBSql).

expand_table_name(Ref, a, TableName) :-
    table_name(Ref, Table),
    format(atom(TableName), '__expand_a_~w', [Table]).
expand_table_name(Ref, b, TableName) :-
    table_name(Ref, Table),
    format(atom(TableName), '__expand_b_~w', [Table]).

expand_seed_sql(Mode, RelPlans, QuotedWaveTable, HeadColumnsSql, Rule, SeedSql) :-
    level_recursive_arm(Mode, RelPlans, Rule, BaseArm),
    format(atom(SeedSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM (~w)',
           [QuotedWaveTable, HeadColumnsSql, HeadColumnsSql, BaseArm]).

% The WITH clause shadows the head's table name with the source wavefront, so
% the recursive arm text is reused verbatim; only frontier rows feed the hop.
expand_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedFromTable,
               QuotedIntoTable, QuotedRefCountTable, HeadColumns,
               HeadColumnsSql, HopSql) :-
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    maplist(level_recursive_arm(Mode, RelPlans), RecursiveRules, RecursiveArms),
    atomic_list_concat(RecursiveArms, ' UNION ALL ', ArmsSql),
    qualified_equalities(HeadColumns, x, n, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    format(atom(HopSql),
           'WITH ~w (~w) AS (SELECT ~w FROM ~w) INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM (~w) x WHERE NOT EXISTS (SELECT 1 FROM ~w n WHERE ~w)',
           [QuotedHeadTable, HeadColumnsSql, HeadColumnsSql, QuotedFromTable,
            QuotedIntoTable, HeadColumnsSql, HeadColumnsSql, ArmsSql,
            QuotedRefCountTable, EqualitySql]).

expand_absorb_sql(QuotedRefCountTable, QuotedWaveTable, HeadColumnsSql, AbsorbSql) :-
    format(atom(AbsorbSql),
           'INSERT OR IGNORE INTO ~w (~w, "__refcount") SELECT ~w, 1 FROM ~w',
           [QuotedRefCountTable, HeadColumnsSql, HeadColumnsSql, QuotedWaveTable]).

% ═══ in-place recursive-head maintenance (engine.rs:407, :454) ════════════
% Rows left in __cone_<rel> once the rederive walk stops are the retractions.

dred_ping_table_name(Ref, TableName) :-
    table_name(Ref, Table),
    format(atom(TableName), '__ping_~w', [Table]).

dred_pong_table_name(Ref, TableName) :-
    table_name(Ref, Table),
    format(atom(TableName), '__pong_~w', [Table]).

dred_cone_table_name(Ref, TableName) :-
    table_name(Ref, Table),
    format(atom(TableName), '__cone_~w', [Table]).

% A negated atom retracts on an ARRIVAL, a `pre` atom reads a snapshot with no
% delta, a `__ref_*` atom has neither: all three hide retractions from a seed.
dred_plan_admissible(Rules) :-
    forall(member((_ <- Body), Rules), dred_rule_admissible(Body)).

dred_rule_admissible(Body) :-
    body_ref_uses(Body, Uses),
    \+ ( member(Use, Uses), is_negative_use(Use) ),
    \+ ( member(use(_, _, pos, pre), Uses) ),
    \+ ( member(Use, Uses), dictionary_use(Use) ),
    include(is_positive_use, Uses, PosUses),
    PosUses \== [].

level_dred_plan(Mode, RelPlans, HeadRef, Rules,
                dredplan(ClearPingSql, ClearPongSql, ClearConeSql,
                         AssertSeedSqls, AssertHopABSql, AssertHopBASql,
                         CommitASql, CommitBSql, ArrivalASql, ArrivalBSql,
                         DredSeedSqls, DredHopABSql, DredHopBASql,
                         ConeAbsorbASql, ConeAbsorbBSql,
                         ConeTrimSql, HeadDeleteSql, RederiveSeedSqls,
                         ReviveHopABSql, ReviveHopBASql,
                         ConeDropASql, ConeDropBSql,
                         StageRetractSql, HeadCountSql)) :-
    partition(rule_reads_head(HeadRef), Rules, RecursiveRules, _BaseRules),
    RecursiveRules \== [],
    dred_plan_admissible(Rules),
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    dred_ping_table_name(HeadRef, PingTable), quote_ident(PingTable, QuotedPing),
    dred_pong_table_name(HeadRef, PongTable), quote_ident(PongTable, QuotedPong),
    dred_cone_table_name(HeadRef, ConeTable), quote_ident(ConeTable, QuotedCone),
    arrival_scratch_table_name(HeadRef, NewTable), quote_ident(NewTable, QuotedNew),
    delta_table_name(HeadRef, DeltaTable), quote_ident(DeltaTable, QuotedDelta),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(ClearPingSql), 'DELETE FROM ~w', [QuotedPing]),
    format(atom(ClearPongSql), 'DELETE FROM ~w', [QuotedPong]),
    format(atom(ClearConeSql), 'DELETE FROM ~w', [QuotedCone]),
    dred_absent_probe(HeadColumns, QuotedHeadTable, HeadAbsentSql),
    dred_present_probe(HeadColumns, QuotedHeadTable, HeadPresentSql),
    dred_absent_probe(HeadColumns, QuotedCone, ConeAbsentSql),
    dred_present_probe(HeadColumns, QuotedCone, ConePresentSql),
    findall(SeedSql,
            ( member(Rule, Rules),
              dred_seed_sql(Mode, RelPlans, Rule, QuotedPing, HeadColumnsSql,
                            HeadAbsentSql, 1, SeedSql) ),
            AssertSeedSqls),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPing, QuotedPong,
                 HeadColumnsSql, HeadAbsentSql, AssertHopABSql),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPong, QuotedPing,
                 HeadColumnsSql, HeadAbsentSql, AssertHopBASql),
    dred_commit_sql(QuotedHeadTable, QuotedPing, HeadColumnsSql, CommitASql),
    dred_commit_sql(QuotedHeadTable, QuotedPong, HeadColumnsSql, CommitBSql),
    dred_arrival_sql(QuotedNew, QuotedPing, HeadColumnsSql, ArrivalASql),
    dred_arrival_sql(QuotedNew, QuotedPong, HeadColumnsSql, ArrivalBSql),
    findall(RetractSeedSql,
            ( member(Rule, Rules),
              dred_seed_sql(Mode, RelPlans, Rule, QuotedPing, HeadColumnsSql,
                            HeadPresentSql, -1, RetractSeedSql) ),
            DredSeedSqls),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPing, QuotedPong,
                 HeadColumnsSql, ConeAbsentSql, DredHopABSql),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPong, QuotedPing,
                 HeadColumnsSql, ConeAbsentSql, DredHopBASql),
    dred_commit_sql(QuotedCone, QuotedPing, HeadColumnsSql, ConeAbsorbASql),
    dred_commit_sql(QuotedCone, QuotedPong, HeadColumnsSql, ConeAbsorbBSql),
    qualified_equalities(HeadColumns, h, QuotedCone, TrimEqualityParts),
    atomic_list_concat(TrimEqualityParts, ' AND ', TrimEqualitySql),
    format(atom(ConeTrimSql),
           'DELETE FROM ~w WHERE NOT EXISTS (SELECT 1 FROM ~w h WHERE ~w)',
           [QuotedCone, QuotedHeadTable, TrimEqualitySql]),
    format(atom(HeadDeleteSql),
           'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM ~w)',
           [QuotedHeadTable, HeadColumnsSql, HeadColumnsSql, QuotedCone]),
    findall(RederiveSql,
            ( member(Rule, Rules),
              dred_rederive_seed_sql(Mode, RelPlans, Rule, QuotedPing, QuotedCone,
                                     HeadColumns, HeadColumnsSql, RederiveSql) ),
            RederiveSeedSqls),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPing, QuotedPong,
                 HeadColumnsSql, ConePresentSql, ReviveHopABSql),
    dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedPong, QuotedPing,
                 HeadColumnsSql, ConePresentSql, ReviveHopBASql),
    dred_cone_drop_sql(QuotedCone, QuotedPing, HeadColumnsSql, ConeDropASql),
    dred_cone_drop_sql(QuotedCone, QuotedPong, HeadColumnsSql, ConeDropBSql),
    format(atom(StageRetractSql),
           'INSERT INTO ~w ("_sign", "_sequence", ~w) SELECT -1, row_number() OVER () - 1, ~w FROM ~w',
           [QuotedDelta, HeadColumnsSql, HeadColumnsSql, QuotedCone]),
    format(atom(HeadCountSql), 'SELECT count(*) AS "n" FROM ~w',
           [QuotedHeadTable]).

dred_absent_probe(HeadColumns, QuotedTable, ProbeSql) :-
    qualified_equalities(HeadColumns, x, p, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    format(atom(ProbeSql), 'NOT EXISTS (SELECT 1 FROM ~w p WHERE ~w)',
           [QuotedTable, EqualitySql]).

dred_present_probe(HeadColumns, QuotedTable, ProbeSql) :-
    qualified_equalities(HeadColumns, x, p, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    format(atom(ProbeSql), 'EXISTS (SELECT 1 FROM ~w p WHERE ~w)',
           [QuotedTable, EqualitySql]).

dred_commit_sql(QuotedTarget, QuotedWaveTable, HeadColumnsSql, CommitSql) :-
    format(atom(CommitSql), 'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM ~w',
           [QuotedTarget, HeadColumnsSql, HeadColumnsSql, QuotedWaveTable]).

% __new_<rel> keeps its rowid, so the walk appends in derivation order and the
% shipped stageAdd/stageFrontier statements read it unchanged.
dred_arrival_sql(QuotedNewTable, QuotedWaveTable, HeadColumnsSql, ArrivalSql) :-
    format(atom(ArrivalSql),
           'INSERT INTO ~w (~w, "__refcount") SELECT ~w, 1 FROM ~w',
           [QuotedNewTable, HeadColumnsSql, HeadColumnsSql, QuotedWaveTable]).

dred_cone_drop_sql(QuotedCone, QuotedWaveTable, HeadColumnsSql, DropSql) :-
    format(atom(DropSql), 'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM ~w)',
           [QuotedCone, HeadColumnsSql, HeadColumnsSql, QuotedWaveTable]).

% One seed per (rule, changed atom position). At sign -1 the OTHER atoms read
% live-or-retracted: a derivation using two retracted atoms hides otherwise.
dred_seed_sql(Mode, RelPlans, Rule, QuotedPing, HeadColumnsSql, ProbeSql, Sign,
              SeedSql) :-
    level_recursive_arm_parts(Mode, RelPlans, Rule, PosUses, PosFromParts,
                              JsonFromParts, WhereTexts, SelectSql, _RawExprs),
    rule_head_ref(Rule, HeadRef),
    nth0(Index, PosUses, use(Ref, _, pos, _)),
    Ref \== HeadRef,
    dred_seed_from_parts(RelPlans, HeadRef, PosUses, PosFromParts, Index, Sign,
                         SeedFromParts),
    append(SeedFromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    ( WhereTexts == []
    -> format(atom(ArmSql), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
    ;  atomic_list_concat(WhereTexts, ' AND ', WhereSql),
       format(atom(ArmSql), 'SELECT ~w FROM ~w WHERE ~w',
              [SelectSql, FromSql, WhereSql])
    ),
    format(atom(SeedSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM (~w) x WHERE ~w',
           [QuotedPing, HeadColumnsSql, HeadColumnsSql, ArmSql, ProbeSql]).

dred_seed_from_parts(RelPlans, HeadRef, PosUses, PosFromParts, Index, Sign,
                     SeedFromParts) :-
    findall(Part,
            ( nth0(Position, PosFromParts, LiveFrom),
              nth0(Position, PosUses, use(Ref, _, pos, _)),
              format(atom(Alias), 'b~w', [Position]),
              dred_seed_from_part(RelPlans, HeadRef, Ref, Alias, Position,
                                  Index, Sign, LiveFrom, Part) ),
            SeedFromParts).

dred_seed_from_part(RelPlans, _HeadRef, Ref, Alias, Index, Index, Sign,
                    _LiveFrom, Part) :- !,
    dred_delta_select(RelPlans, Ref, Sign, DeltaSelectSql),
    format(atom(Part), '(~w) ~w', [DeltaSelectSql, Alias]).
% The recursive atom stays a plain head read: the head is still whole here,
% and widening it would over-delete rows this tick never suspected.
dred_seed_from_part(_, HeadRef, HeadRef, _Alias, _Position, _Index, _Sign,
                    LiveFrom, LiveFrom) :- !.
dred_seed_from_part(RelPlans, _HeadRef, Ref, Alias, _Position, _Index, -1,
                    _LiveFrom, Part) :- !,
    dred_column_list(RelPlans, Ref, ColumnsSql),
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    dred_delta_select(RelPlans, Ref, -1, DeltaSelectSql),
    format(atom(Part), '(SELECT ~w FROM ~w UNION ALL ~w) ~w',
           [ColumnsSql, QuotedTable, DeltaSelectSql, Alias]).
dred_seed_from_part(_, _, _, _, _, _, _, LiveFrom, LiveFrom).

% A delta table is CUMULATIVE over the tick, so a row added and retracted in
% the same tick sits there under both signs; the liveness probe is what keeps
% a +1 seed off a fact that is already gone and a -1 seed off one still there.
dred_delta_select(RelPlans, Ref, Sign, SelectSql) :-
    relplan_columns(RelPlans, Ref, Columns),
    findall(Projection,
            ( member(Column, Columns), quote_ident(Column, QuotedColumn),
              format(atom(Projection), 'd.~w', [QuotedColumn]) ),
            Projections),
    atomic_list_concat(Projections, ', ', ProjectionsSql),
    qualified_equalities(Columns, t, d, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    delta_table_name(Ref, DeltaTable), quote_ident(DeltaTable, QuotedDelta),
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    ( Sign =:= 1 -> Liveness = 'EXISTS' ; Liveness = 'NOT EXISTS' ),
    format(atom(SelectSql),
           'SELECT ~w FROM ~w d WHERE d."_sign" = ~w AND ~w (SELECT 1 FROM ~w t WHERE ~w)',
           [ProjectionsSql, QuotedDelta, Sign, Liveness, QuotedTable,
            EqualitySql]).

dred_column_list(RelPlans, Ref, ColumnsSql) :-
    relplan_columns(RelPlans, Ref, Columns),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql).

% ProbeSql is what stops the walk: head-absent for assert, cone-absent for
% over-delete, cone-present for revive.
dred_hop_sql(Mode, RelPlans, HeadRef, RecursiveRules, QuotedFrontier, QuotedInto,
             HeadColumnsSql, ProbeSql, HopSql) :-
    findall(ArmSql,
            ( member(Rule, RecursiveRules),
              dred_hop_arm(Mode, RelPlans, HeadRef, Rule, QuotedFrontier, ArmSql) ),
            ArmSqls),
    atomic_list_concat(ArmSqls, ' UNION ALL ', ArmsSql),
    format(atom(HopSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM (~w) x WHERE ~w',
           [QuotedInto, HeadColumnsSql, HeadColumnsSql, ArmsSql, ProbeSql]).

dred_hop_arm(Mode, RelPlans, HeadRef, Rule, QuotedFrontier, ArmSql) :-
    level_recursive_arm_parts(Mode, RelPlans, Rule, PosUses, PosFromParts,
                              JsonFromParts, WhereTexts, SelectSql, _RawExprs),
    nth0(SelfIndex, PosUses, use(HeadRef, _, pos, _)),
    !,
    format(atom(SelfAlias), 'b~w', [SelfIndex]),
    format(atom(SelfFrom), '~w ~w', [QuotedFrontier, SelfAlias]),
    dred_replace_nth0(SelfIndex, PosFromParts, SelfFrom, HopFromParts),
    append(HopFromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    ( WhereTexts == []
    -> format(atom(ArmSql), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
    ;  atomic_list_concat(WhereTexts, ' AND ', WhereSql),
       format(atom(ArmSql), 'SELECT ~w FROM ~w WHERE ~w',
              [SelectSql, FromSql, WhereSql])
    ).

dred_replace_nth0(Index, List, Replacement, Replaced) :-
    findall(Item,
            ( nth0(Position, List, Original),
              ( Position =:= Index -> Item = Replacement ; Item = Original ) ),
            Replaced).

% Cone-driven, and written as a flat join rather than a wrapped subquery so
% sqlite can start from the cone instead of materializing the whole arm.
dred_rederive_seed_sql(Mode, RelPlans, Rule, QuotedPing, QuotedCone, HeadColumns,
                       HeadColumnsSql, RederiveSql) :-
    level_recursive_arm_parts(Mode, RelPlans, Rule, _PosUses, PosFromParts,
                              JsonFromParts, WhereTexts, SelectSql, RawExprs),
    append(PosFromParts, JsonFromParts, ArmFromParts),
    format(atom(ConeFrom), '~w c', [QuotedCone]),
    from_parts_sql([ConeFrom | ArmFromParts], FromSql),
    findall(Equality,
            ( nth1(Position, HeadColumns, Column),
              nth1(Position, RawExprs, RawExpr),
              quote_ident(Column, QuotedColumn),
              format(atom(Equality), '~w = c.~w', [RawExpr, QuotedColumn]) ),
            ConeEqualities),
    append(WhereTexts, ConeEqualities, AllWhereTexts),
    atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
    format(atom(RederiveSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT ~w FROM ~w WHERE ~w',
           [QuotedPing, HeadColumnsSql, SelectSql, FromSql, WhereSql]).

% ═══ backend-neutral fixpoint IR ═════════════════════════════════════════════

% Term grammar: plans/2026-08-07-plan-ir-offload-contract.md §2.4. The fence is
% dred_plan_admissible/1, called rather than restated.
level_fixpoint_ir(Mode, RelPlans, HeadRef, Rules,
                  fixpointir(Storage, Assert, Dred, Revive, Expand)) :-
    dred_plan_admissible(Rules),
    fixpoint_ir_columns(RelPlans, HeadRef, Columns, ColumnTypes),
    maplist(ir_rule_arm(Mode, RelPlans), Rules, AllParts),
    include(ir_parts_read_head(HeadRef), AllParts, RecursiveParts),
    RecursiveParts \== [],
    exclude(ir_parts_read_head(HeadRef), AllParts, BaseParts),
    ir_rel_ref(HeadRef, Ref),
    maplist(ir_hop_arm(HeadRef), RecursiveParts, Hops),
    ir_delta_seeds(HeadRef, AllParts, 1, AssertSeeds),
    ir_delta_seeds(HeadRef, AllParts, -1, DredSeeds),
    maplist(ir_cone_seed(Columns), AllParts, ReviveSeeds),
    maplist(ir_base_seed, BaseParts, ExpandSeeds),
    Assert = fixplan(Ref, Columns, ColumnTypes, AssertSeeds, Hops,
                     stop(probe(absent, head), probe(absent, head)),
                     order(round_major)),
    Dred = fixplan(Ref, Columns, ColumnTypes, DredSeeds, Hops,
                   stop(probe(present, head), probe(absent, cone)), none),
    Revive = fixplan(Ref, Columns, ColumnTypes, ReviveSeeds, Hops,
                     stop(none, probe(present, cone)), none),
    Expand = fixplan(Ref, Columns, ColumnTypes, ExpandSeeds, Hops,
                     stop(none, probe(absent, ref_count)), order(key_major)),
    interned_literals_absent(Mode, [Assert, Dred, Revive, Expand]),
    ir_storage(Mode, RelPlans, HeadRef, [Assert, Dred, Revive, Expand],
               Storage).

% Phase 1 has no IR spelling for `<interned column> = 'literal'`: the SQL
% resolves the literal through __str, and eq_lit/2 carries the bare text.
% TODO(rust-executor): lift by interning lit(text(V)) through the column's dict(R) colclass encoding before comparing; offload contract carries the sentence.
interned_literals_absent(Mode, Walks) :-
    \+ ( interned_column(Mode, text),
         member(fixplan(_, _, _, Seeds, Hops, _, _), Walks),
         ( member(Arm, Seeds) ; member(Arm, Hops) ),
         Arm = arm(_, _, Filters, _, _),
         member(eq_lit(_, lit(text(_))), Filters) ).

ir_rel_ref(Name/Arity, ref(Name, Arity)).

% Every rel any src reads, plus the head, which is what wave/1 and cone/0 carry.
% col(Index, Ordinal) resolves through the arm's src to a row here.
ir_storage(Mode, RelPlans, HeadRef, Walks, Storage) :-
    findall(Ref,
            ( member(fixplan(_, _, _, Seeds, Hops, _, _), Walks),
              ( member(Arm, Seeds) ; member(Arm, Hops) ),
              Arm = arm(Sources, _, _, _, _),
              member(src(_, Source), Sources),
              ir_source_ref(Source, Ref) ),
            SourceRefs),
    sort([HeadRef | SourceRefs], Refs),
    maplist(ir_rel_storage(Mode, RelPlans), Refs, Storage).

ir_source_ref(rel(ref(Name, Arity)), Name/Arity).
ir_source_ref(rel_or_retracted(ref(Name, Arity)), Name/Arity).
ir_source_ref(delta(ref(Name, Arity), _, _), Name/Arity).

ir_rel_storage(Mode, RelPlans, Ref, relstorage(IrRef, ColumnClasses)) :-
    ir_rel_ref(Ref, IrRef),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    maplist(ir_column_class(Mode), Columns, ColumnTypes, ColumnClasses),
    uniform_text_encoding(ColumnClasses).

% INVARIANT, not a unsupported construct: two encodings on one program's text columns would
% put the two sides of a text join in different id spaces, silently empty.
% Unreachable while interned_column/2 is one clause; it exists so that the day
% a per-column waiver returns, it fires at compile time instead.
uniform_text_encoding(ColumnClasses) :-
    findall(Encoding,
            member(colclass(_, text, _, _, Encoding), ColumnClasses),
            Encodings),
    sort(Encodings, Distinct),
    (   ( Distinct == [] ; Distinct = [_] )
    ->  true
    ;   throw(unsupported_construct(mixed_text_encoding(Distinct)))
    ).

% The comparator, which the declared type does not give: bool and ref(_) both
% store INTEGER, json stores TEXT (column_def/3:939), and no COLLATE is emitted.
ir_column_class(Mode, Column, Type, colclass(Column, TypeName, StorageClass,
                                             Collation, Encoding)) :-
    ir_column_storage(Mode, Type, TypeName, StorageClass, Encoding),
    ( StorageClass == text -> Collation = binary ; Collation = none ).

% Encoding is the interning slot: ref(Target) already stores a dense id into
% Target's table rather than the value, which is dictionary encoding.
ir_column_storage(_, ref(Target), ref, integer, dict(Target)) :- !.
ir_column_storage(_, idref(_), id, integer, direct) :- !.
ir_column_storage(_, bool, bool, integer, direct) :- !.
ir_column_storage(_, int, int, integer, direct) :- !.
ir_column_storage(_, float, float, real, direct) :- !.
ir_column_storage(_, json, json, text, direct) :- !.
ir_column_storage(_, json_list(_), list, text, direct) :- !.
ir_column_storage(_, bytes, bytes, blob, direct) :- !.
% The comparator over an entity id is the integer one, so the IR type name is
% the storage's, never the spelling's.
ir_column_storage(_, list(_), int, integer, direct) :- !.
% An interned text column reports storage `integer`; without the encoding slot
% the pair {type: text, storage: integer} is uninterpretable to an executor.
ir_column_storage(Mode, text, text, integer, dict(Dictionary)) :-
    interned_column(Mode, text),
    !,
    string_dictionary_table(Dictionary).
ir_column_storage(_, text, text, text, direct).

% The executor's comparator is defined by these types, so a head column whose
% storage class is a dictionary id or json has no phase-1 spelling.
fixpoint_ir_columns(RelPlans, HeadRef, Columns, ColumnTypes) :-
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    forall(member(ColumnType, ColumnTypes),
           memberchk(ColumnType, [int, text, float, bool, bytes])).

ir_parts_read_head(HeadRef, armparts(PosUses, _, _, _)) :-
    memberchk(use(HeadRef, _, pos, _), PosUses).

% One seed per (rule, non-self positive atom), matching dred_seed_sql/7's
% nth0 enumeration; the enumerated atom reads its delta at Sign.
ir_delta_seeds(HeadRef, AllParts, Sign, Seeds) :-
    findall(arm(Sources, Equalities, Filters, Project, none),
            ( member(armparts(PosUses, Equalities, Filters, Project), AllParts),
              nth0(Index, PosUses, use(Ref, _, pos, _)),
              Ref \== HeadRef,
              ir_seed_sources(HeadRef, PosUses, Index, Sign, Sources) ),
            Seeds).

ir_seed_sources(HeadRef, PosUses, DeltaIndex, Sign, Sources) :-
    findall(src(Position, Source),
            ( nth0(Position, PosUses, use(Ref, _, pos, _)),
              ir_seed_source(HeadRef, Ref, Position, DeltaIndex, Sign, Source) ),
            Sources).

ir_seed_source(_, Ref, Index, Index, Sign, delta(IrRef, Sign, liveness(Live))) :-
    !,
    ir_rel_ref(Ref, IrRef),
    ( Sign =:= 1 -> Live = present ; Live = absent ).
ir_seed_source(HeadRef, HeadRef, _, _, _, rel(IrRef)) :- !,
    ir_rel_ref(HeadRef, IrRef).
ir_seed_source(_, Ref, _, _, -1, rel_or_retracted(IrRef)) :- !,
    ir_rel_ref(Ref, IrRef).
ir_seed_source(_, Ref, _, _, _, rel(IrRef)) :-
    ir_rel_ref(Ref, IrRef).

ir_hop_arm(HeadRef, armparts(PosUses, Equalities, Filters, Project),
           arm(Sources, Equalities, Filters, Project, SelfIndex)) :-
    nth0(SelfIndex, PosUses, use(HeadRef, _, pos, _)),
    !,
    findall(src(Position, Source),
            ( nth0(Position, PosUses, use(Ref, _, pos, _)),
              (   Position =:= SelfIndex
              ->  Source = wave(frontier)
              ;   ir_rel_ref(Ref, IrRef), Source = rel(IrRef)
              ) ),
            Sources).

% The revive seed is cone-driven: the cone joins in as its own source and one
% equality per head column pins the projection to the cone row.
ir_cone_seed(Columns, armparts(PosUses, Equalities0, Filters, Project),
             arm(Sources, Equalities, Filters, Project, none)) :-
    length(PosUses, ConeIndex),
    ir_rel_sources(PosUses, RelSources),
    append(RelSources, [src(ConeIndex, cone)], Sources),
    findall(eq(ProjectExpr, col(ConeIndex, Ordinal)),
            ( nth0(Ordinal, Columns, _),
              nth0(Ordinal, Project, ProjectExpr) ),
            ConeEqualities),
    append(Equalities0, ConeEqualities, Equalities).

ir_base_seed(armparts(PosUses, Equalities, Filters, Project),
             arm(Sources, Equalities, Filters, Project, none)) :-
    ir_rel_sources(PosUses, Sources).

ir_rel_sources(PosUses, Sources) :-
    findall(src(Position, rel(IrRef)),
            ( nth0(Position, PosUses, use(Ref, _, pos, _)),
              ir_rel_ref(Ref, IrRef) ),
            Sources).

% Reads the SAME Bound and where-parts compile_positive_uses/7 hands
% level_recursive_arm_parts/8; anything outside the grammar fails the head.
ir_rule_arm(Mode, RelPlans, Rule, armparts(PosUses, Equalities, Filters, Project)) :-
    Rule = (Head <- Body),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    conjunction_goals(Body, AllGoals),
    \+ ( member(Goal, AllGoals), is_decode_goal(Goal) ),
    compile_positive_uses(Mode, RelPlans, PosUses, 0, [], Bound, _FromParts,
                          WhereParts),
    ir_column_dict(RelPlans, PosUses, 0, Dict),
    ir_bound(Bound, Dict, IrBound0),
    ir_atom_conditions(WhereParts, Dict, AtomEqualities, AtomFilters),
    body_guard_goals(Body, GuardGoals),
    foldl(ir_guard_goal, GuardGoals, ir(IrBound0, [], []),
          ir(IrBound, ReversedEqualities, ReversedFilters)),
    reverse(ReversedEqualities, GuardEqualities),
    reverse(ReversedFilters, GuardFilters),
    append(AtomEqualities, GuardEqualities, Equalities),
    append(AtomFilters, GuardFilters, Filters),
    Head =.. [_ | HeadArgs],
    maplist(ir_untyped_expr(IrBound), HeadArgs, Project).

ir_column_dict(_, [], _, []).
ir_column_dict(RelPlans, [use(Ref, _, pos, _) | Rest], Index, Dict) :-
    relplan_columns(RelPlans, Ref, Columns),
    format(atom(Alias), 'b~w', [Index]),
    findall(ircol(ColumnExpr, col(Index, Ordinal)),
            ( nth0(Ordinal, Columns, Column),
              format(atom(ColumnExpr), '~w."~w"', [Alias, Column]) ),
            Here),
    NextIndex is Index + 1,
    ir_column_dict(RelPlans, Rest, NextIndex, More),
    append(Here, More, Dict).

ir_dict_lookup([ircol(Key, Ir) | Rest], ColumnExpr, Found) :-
    ( Key == ColumnExpr -> Found = Ir ; ir_dict_lookup(Rest, ColumnExpr, Found) ).

% A VARIABLE outside the expression grammar refuses the arm; a compound key is
% the reference-identity slot, and an arm that reads one refuses at ir_expr/4.
ir_bound([], _, []).
ir_bound([Key-typed(Sql, Type, _) | Rest], Dict, IrBound) :-
    (   ir_dict_lookup(Dict, Sql, Ir)
    ->  IrBound = [Key-irtyped(Ir, Type) | More]
    ;   nonvar(Key),
        IrBound = More
    ),
    ir_bound(Rest, Dict, More).

ir_atom_conditions([], _, [], []).
ir_atom_conditions([pair(Left, Right) | Rest], Dict,
                   [eq(IrLeft, IrRight) | Equalities], Filters) :-
    ir_dict_lookup(Dict, Left, IrLeft),
    ir_dict_lookup(Dict, Right, IrRight),
    ir_atom_conditions(Rest, Dict, Equalities, Filters).
ir_atom_conditions([lit(Left, Value, _) | Rest], Dict, Equalities,
                   [eq_lit(IrLeft, Literal) | Filters]) :-
    ir_dict_lookup(Dict, Left, IrLeft),
    ir_literal(Value, Literal),
    ir_atom_conditions(Rest, Dict, Equalities, Filters).

ir_literal(bool_lit(Boolean), lit(bool(Boolean))) :- !.
ir_literal(Value, lit(int(Value))) :- integer(Value), !.
ir_literal(Value, lit(float(Value))) :- float(Value), !.
ir_literal(Value, lit(text(Value))) :- atomic(Value).

ir_literal_type(lit(Literal), Type) :- functor(Literal, Type, 1).

ir_untyped_expr(IrBound, Expr, Ir) :- ir_expr(IrBound, Expr, Ir, _Type).

% Clause order and the result type both mirror compile_expr/4's, so a bound
% compound resolves first and `/` carries the int-vs-real answer SQLite gives.
ir_expr(IrBound, Expr, Ir, Type) :-
    (   var(Expr)
    ->  bound_lookup(IrBound, Expr, irtyped(Ir, Type))
    ;   Expr = bool_lit(_)
    ->  ir_literal(Expr, Ir), Type = bool
    ;   compound(Expr), bound_lookup(IrBound, Expr, irtyped(BoundIr, BoundType))
    ->  Ir = BoundIr, Type = BoundType
    ;   atomic(Expr)
    ->  ir_literal(Expr, Ir), ir_literal_type(Ir, Type)
    ;   Expr = concat(Parts)
    ->  is_list(Parts),
        maplist(ir_concat_part(IrBound), Parts, PartIrs),
        Ir = concat(PartIrs), Type = text
    ;   arithmetic_expr(Expr, Operator, Left, Right)
    ->  ir_expr(IrBound, Left, IrLeft, LeftType),
        ir_expr(IrBound, Right, IrRight, RightType),
        ir_arith_operand_type(Operator, LeftType),
        ir_arith_operand_type(Operator, RightType),
        arithmetic_result_type(Operator, LeftType, RightType, Type),
        Ir = arith(Operator, IrLeft, IrRight, Type)
    ;   fail
    ).

% compile_numeric_operand/6 and compile_concat_part/4: the same operand
% admissions, so the IR never spells an expression the SQL side refuses.
ir_arith_operand_type(mod, int) :- !.
ir_arith_operand_type(Operator, Type) :-
    Operator \== mod, memberchk(Type, [int, float]).

ir_concat_part(IrBound, Part, Ir) :-
    ir_expr(IrBound, Part, Ir, Type),
    memberchk(Type, [int, text]).

% The same left-to-right fold compile_guard_goal/3 runs, over the same goal
% list: a bind introduces a binding once and is an equality every time after.
ir_guard_goal(Goal, ir(IrBound0, Equalities0, Filters0),
              ir(IrBound, Equalities, Filters)) :-
    (   regexp_goal(Goal)
    ->  fail
    ;   tick_goal(Goal, _)
    ->  fail
    ;   bind_goal(Goal, Variable, Expr)
    ->  ir_expr(IrBound0, Expr, Ir, Type),
        (   var(Variable), \+ bound_lookup(IrBound0, Variable, _)
        ->  IrBound = [Variable-irtyped(Ir, Type) | IrBound0],
            Equalities = Equalities0, Filters = Filters0
        ;   ir_expr(IrBound0, Variable, VariableIr, _),
            IrBound = IrBound0, Filters = Filters0,
            Equalities = [eq(VariableIr, Ir) | Equalities0]
        )
    ;   guard_goal(Goal)
    ->  Goal =.. [Operator, Left, Right],
        expression(Operator/2, Family, _, infix(_), _),
        memberchk(Family, [ordered_comparison, identity_comparison]),
        ir_expr(IrBound0, Left, IrLeft, _),
        ir_expr(IrBound0, Right, IrRight, _),
        IrBound = IrBound0, Equalities = Equalities0,
        Filters = [cmp(Operator, IrLeft, IrRight) | Filters0]
    ;   fail
    ).

rules_read_head_recursively(HeadRef, Rules) :-
    member(Rule, Rules),
    rule_reads_head(HeadRef, Rule),
    !.

rule_reads_head(HeadRef, (_ <- Body)) :-
    body_ref_uses(Body, Uses),
    member(use(HeadRef, _, pos, _), Uses).

check_single_recursive_read(HeadRef, (_ <- Body)) :-
    body_ref_uses(Body, Uses),
    include(use_reads_ref(HeadRef), Uses, SelfUses),
    length(SelfUses, Count),
    ( Count =:= 1
    -> true
    ;  throw(unsupported_construct(
           recursive_cte_multiple_self_reads(HeadRef, Count)))
    ).

use_reads_ref(Ref, use(Ref, _, pos, _)).

empty_recursive_anchor(HeadColumns, Anchor) :-
    maplist(null_column_expr, HeadColumns, NullColumns),
    atomic_list_concat(NullColumns, ', ', NullColumnsSql),
    format(atom(Anchor), 'SELECT ~w WHERE 0', [NullColumnsSql]).

null_column_expr(Column, Expr) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Expr), 'NULL AS ~w', [QuotedColumn]).

level_recursive_arm(Mode, RelPlans, Rule, RecursiveArm) :-
    level_recursive_arm_parts(Mode, RelPlans, Rule, _PosUses, PosFromParts,
                              JsonFromParts, AllWhereTexts, SelectSql,
                              _RawExprs),
    append(PosFromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    ( AllWhereTexts == []
    -> format(atom(RecursiveArm), 'SELECT ~w FROM ~w',
              [SelectSql, FromSql])
    ;  atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
       format(atom(RecursiveArm), 'SELECT ~w FROM ~w WHERE ~w',
              [SelectSql, FromSql, WhereSql])
    ).

% PosFromParts stays index-aligned with PosUses: the plans below swap ONE entry
% for a delta or wavefront read. RawExprs is the head value list, alias-free.
level_recursive_arm_parts(Mode, RelPlans, Rule, PosUses, PosFromParts, JsonFromParts,
                          AllWhereTexts, SelectSql, RawExprs) :-
    Rule = (Head <- Body),
    rule_head_ref(Rule, HeadRef),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, PosFromParts,
                          PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], AllWhereTexts),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, HeadColumns, SelectExprs, BuiltValues, ListInterns),
    recursive_arm_builds_no_string(HeadRef, BuiltValues),
    recursive_arm_builds_no_list(HeadRef, ListInterns),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, none, RawExprs, _, _).

% A recursive arm lives inside one WITH RECURSIVE statement, so there is no
% place to put the intern write (§5.7.3, the per-round case).
recursive_arm_builds_no_string(_, []) :- !.
recursive_arm_builds_no_string(HeadRef, _) :-
    throw(unsupported_construct(built_text_in_recursive_head(HeadRef))).

recursive_arm_builds_no_list(_, []) :- !.
recursive_arm_builds_no_list(HeadRef, _) :-
    throw(unsupported_construct(built_list_in_recursive_head(HeadRef))).

level_ref_count_arm(Mode, RelPlans, Rule, RefCountArm, InternSqls) :-
    Rule = (Head <- Body),
    rule_head_ref(Rule, HeadRef),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    compile_coalesce_recount_markers(RelPlans, NegUses, RecountWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts, RecountWhereTexts],
           AllWhereTexts),
    append(FromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, HeadColumns, AliasedSelectExprs, BuiltValues, ListInterns),
    ref_count_group_exprs(Mode, Head, Bound, GroupExprs),
    atomic_list_concat(AliasedSelectExprs, ', ', SelectSql),
    atomic_list_concat(GroupExprs, ', ', GroupSql),
    ( AllWhereTexts == []
    -> WhereSql = none,
       format(atom(RefCountArm),
              'SELECT ~w, count(*) AS "__refcount" FROM ~w GROUP BY ~w',
              [SelectSql, FromSql, GroupSql])
    ;  atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
       format(atom(RefCountArm),
              'SELECT ~w, count(*) AS "__refcount" FROM ~w WHERE ~w GROUP BY ~w',
              [SelectSql, FromSql, WhereSql, GroupSql])
    ),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, InternSqls).

% SQLite treats a bare integer in GROUP BY as a one-based SELECT-list
% position, including when the integer is parenthesized. Adding zero makes it
% an expression while preserving its value, so only literal integer head
% columns need a different spelling from head_select_list/4. Every variable,
% atom, and compound head expression retains its existing emitted SQL bytes.
% Shared by the ref-count arm and both aggregate grouping arms
% (scoped-delta insert + recompute) so the SQLite grammar fact lives once.
ref_count_group_exprs(Mode, Head, Bound, GroupExprs) :-
    Head =.. [_ | Args],
    maplist(group_expr(Mode, Bound), Args, GroupExprs).

group_expr(Mode, Bound, Arg, GroupExpr) :-
    compile_expr(Mode, identity, Arg, Bound, Sql, _Type, _Encoding),
    ( sql_bare_integer(Sql)
    -> format(atom(GroupExpr), '(~w + 0)', [Sql])
    ;  GroupExpr = Sql
    ).

% The guard must read the COMPILED SQL, not the head term: a variable bound
% by `:= 0` compiles to the bare literal and dodged the integer(Arg) check
% (first hit by a coalesce default clause -- GROUP BY ..., 0 read as the
% positional 4th-column reference and SQLITE_ERROR'd out of range).
sql_bare_integer(Sql) :-
    atom(Sql),
    atom_number(Sql, Number),
    integer(Number).

qualified_equalities([], _, _, []).
qualified_equalities([Column | Rest], LeftAlias, RightAlias,
                     [Equality | More]) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Equality), '~w.~w = ~w.~w',
           [LeftAlias, QuotedColumn, RightAlias, QuotedColumn]),
    qualified_equalities(Rest, LeftAlias, RightAlias, More).

% Statements, not one statement: an arm that builds a string owes the dictionary
% an INSERT before the row insert reads an id back out of it (§5.7.1).
level_insert_statements(Mode, RelPlans, HeadRef, Rule, Statements) :-
    level_insert_sql(Mode, RelPlans, HeadRef, Rule, InsertSql, InternSqls),
    append(InternSqls, [InsertSql], Statements).

level_insert_sql(Mode, RelPlans, HeadRef, (Head <- Body), InsertSql, InternSqls) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    ( PosUses == [] -> throw(unsupported_construct(level_rule_no_positive_body(HeadRef))) ; true ),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], AllWhereTexts),
    append(FromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    ( aggregate_head_template(Head, Template)
    -> aggregate_select_statement(Mode, HeadColumnTypes, Head, Template, Bound, FromSql,
                                  AllWhereTexts, SelectStatement, InternSqls)
    ;  head_select_list(Mode, HeadColumnTypes, Head, Bound, none, SelectExprs, BuiltValues, ListInterns),
       atomic_list_concat(SelectExprs, ', ', SelectSql),
       ( AllWhereTexts == []
       -> WhereSql = none,
          format(atom(SelectStatement), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
       ; atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
         format(atom(SelectStatement), 'SELECT ~w FROM ~w WHERE ~w', [SelectSql, FromSql, WhereSql])
       ),
       intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
       list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
       append(TextInternSqls, ListInternSqls, InternSqls)
    ),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(InsertSql), 'INSERT OR IGNORE INTO ~w (~w) ~w', [QuotedHeadTable, HeadColumnsSql, SelectStatement]).

% The one place the body's guard/bind goals turn into SQL, shared by every
% level-rule statement family (recompute insert, delta arm, refCount arm,
% recursive-CTE arm) so a guard can never be present in one family and
% silently absent from another -- the phase-C silent-filter-loss class.
% Decode goals are collected from the WHOLE conjunction, not from
% body_guard_goals/2: that predicate selects on the registry's `infix(_)`
% lowering shape, and decode/2 is `wrapper(expr_pair, lower)`, so it was never
% in the guard fold. Every decode still standing in a body at this point is a
% json decode -- expand_decode_rules/4 has already rewritten the struct arm
% into dictionary atoms and refused a source that is neither.
%
% They compile BEFORE the ordinary guards because a decode binds variables a
% later comparison may read, which is the same left-to-right obligation
% engine.pl's solve/2 has and the same order compile_positive_uses/6 ->
% guards already establishes one level up.
compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts) :-
    conjunction_goals(Body, AllGoals),
    include(is_decode_goal, AllGoals, DecodeGoals),
    body_guard_goals(Body, GuardGoals),
    compile_json_decodes(DecodeGoals, 0, _, Bound0, Bound1,
                         JsonFromParts, DecodeWhereTexts),
    compile_guard_goals(Mode, GuardGoals, Bound1, Bound, OtherWhereTexts),
    append(DecodeWhereTexts, OtherWhereTexts, GuardWhereTexts).

% ═══ decode/2 over a `json` column : json1 SQL ══════════════════════════════
%
% The coexistence rule, and it is the whole design: THE BRACE PATTERN'S
% LOWERING IS A FUNCTION OF THE SOURCE COLUMN'S DECLARED TYPE, NEVER OF THE
% PATTERN.
%
%   rel diag(where: place, message: text).   -- a declared struct
%     decode(Where, {file: File})  ==>  '__dict_place'(Where, File, _)
%
%   rel resp(ep: text, body: json).          -- the dynamic escape
%     decode(Body, {number: N})    ==>  json_extract(b0."body", '$."number"')
%
% One surface, two lowerings, picked by the decl. A declared struct has no
% unknown keys, which is why the key axis (`$name`, `**`) is only ever
% meaningful over a json column: `decode_field_unknown` exists precisely to
% say the struct side cannot have one.
%
% Anything a decode goal reaches here has ALREADY been classified: the struct
% arm is rewritten into dictionary atoms by expand_decode_rules/4 before
% lowering, so every decode still standing in a body at this point is over a
% json column.
%
% COST, in joins (json_syntax lab §2, receipts executed against real sqlite3):
%
%   exact key, any depth   0   one json_extract path, accumulated
%   array spread           1   json_each
%   key capture $name      1   json_each -- its (key, value) columns ARE the
%                              construct, zero new SQL machinery
%   ** descent             1   json_tree
%
% Statement counts stay flat per rule: no per-arrival loop and no per-element
% statement, because every production above is a JOIN rather than a fan of
% statements.
%
% THE TYPE GUARD IS NOT COSMETIC, and this is the one thing an implementation
% of this design can get wrong silently. json_each/json_tree hand back SQL
% values, so `value` is JSON text for containers and a bare scalar for leaves,
% and descending into a leaf is NOT a silent non-match in SQLite -- it RAISES
% and kills the whole statement. Measured here, system sqlite3 3.43.2, over
% '[{"number":1},"scalar",{"number":3},7]':
%
%   WHERE e0.type = 'object' AND json_extract(e0.value,'$."number"') IS NOT NULL
%     -> 1,3
%   WHERE json_extract(e0.value,'$."number"') IS NOT NULL
%     -> Runtime error: malformed JSON
%
% The guard is emitted FIRST in the WHERE list for the same reason: SQL states
% no evaluation order for AND, and left-to-right is what makes the guard
% actually protect the extract beside it.
%
% Off an alias the guard reads the alias's own `type` column; off a path it
% reads the TWO-ARGUMENT json_type(Base, Path), which answers NULL for a
% missing key and for a path through a scalar instead of raising.

compile_json_decodes([], Index, Index, Bound, Bound, [], []).
compile_json_decodes([decode(Source, Pattern) | Rest], Index0, Index,
                     Bound0, Bound, FromParts, WhereTexts) :-
    (   bound_lookup(Bound0, Source, typed(SourceSql, SourceType, _))
    ->  true
    ;   throw(unsupported_construct(decode_source_not_bound(Source)))
    ),
    (   ( SourceType == json ; SourceType = json_list(_) )
    ->  true
    ;   throw(unsupported_construct(decode_source_not_struct(decode(Source, Pattern))))
    ),
    json_pattern_sql(Pattern, jsonpos(SourceSql, ['$'], none), Index0, Index1,
                     Bound0, Bound1, HereFrom, HereWhere),
    compile_json_decodes(Rest, Index1, Index, Bound1, Bound,
                         MoreFrom, MoreWhere),
    append(HereFrom, MoreFrom, FromParts),
    append(HereWhere, MoreWhere, WhereTexts).

% jsonpos(BaseSql, ReversedPathSegments, RootTypeSql)
%
% RootTypeSql is the alias `type` column when this position IS a json_each /
% json_tree row (the only place a cheaper and safer type answer exists than
% re-reading the value), `none` when the position is a plain column.

json_value_sql(jsonpos(BaseSql, ['$'], _), BaseSql) :- !.
json_value_sql(jsonpos(BaseSql, Segments, _), Sql) :-
    json_path_text(Segments, Path),
    format(atom(Sql), 'json_extract(~w, ''~w'')', [BaseSql, Path]).

json_type_sql(jsonpos(_, ['$'], RootTypeSql), RootTypeSql) :-
    RootTypeSql \== none, !.
json_type_sql(jsonpos(BaseSql, Segments, _), Sql) :-
    json_path_text(Segments, Path),
    format(atom(Sql), 'json_type(~w, ''~w'')', [BaseSql, Path]).

json_path_text(Reversed, Path) :-
    reverse(Reversed, Segments),
    atomic_list_concat(Segments, Path).

% A path segment is always double-quoted, so a key that is not a bare
% identifier (`/users`, `$ref`) needs no separate spelling. A key carrying a
% double quote has no unambiguous path text and is refused by name rather
% than concatenated into a broken path string.
json_path_segment(Key, Segment) :-
    (   sub_atom(Key, _, _, _, '"')
    ->  throw(unsupported_construct(json_key_contains_quote(Key)))
    ;   format(atom(Segment), '."~w"', [Key])
    ).

% ── the pattern compiler ─────────────────────────────────────────────────────

% A hole binds this position. Already bound means this is a JOIN on the value,
% the same reading compile_pattern_arg/7 gives a repeated variable.
json_pattern_sql(Pattern, Position, Index, Index, Bound0, Bound, [], WhereTexts) :-
    var(Pattern), !,
    json_value_sql(Position, ValueSql),
    (   bound_lookup(Bound0, Pattern, typed(Existing, _, ExistingEncoding))
    ->  Bound = Bound0,
        aligned_pair(direct, ValueSql, ExistingEncoding, Existing, AlignedValue, AlignedExisting),
        format(atom(Equality), '~w = ~w', [AlignedValue, AlignedExisting]),
        WhereTexts = [Equality]
    ;   % text, not json: this is the same reading compile_sub_args/7 already
        % gives a destructured value -- json_extract's result carries no
        % declared column type, so calling it text is what lets it flow into
        % an ordinary text head column without a cross-type join unsupported construct.
        Bound = [Pattern-typed(ValueSql, text, direct) | Bound0],
        format(atom(NotNull), '~w IS NOT NULL', [ValueSql]),
        WhereTexts = [NotNull]
    ).
% A TYPED CAPTURE `{stars: Stars: int}` is the same hole with its column type
% stated, and it exists because the clause above cannot state one. json1 hands
% back a real SQL INTEGER for a json number, so the extract expression was
% always right; what was missing was a way to tell the TYPE PASS so, and
% without it `star_event(Repo, Stars) <- event(Payload), decode(Payload,
% {repo: Repo, stars: Stars})` typed Stars `text` and an `int` head column
% refused the rule by name (edge_head_column_type_mismatch(total/2,2,text,int)
% -- the fail-first receipt for this clause).
%
% The declared type is ENFORCED here, never assumed: the guard is
% `json_type(<path>) = 'integer'`, the exact twin of body.pl's
% json_capture_type/2. Without it a document whose `stars` is the STRING
% "many" would extract as TEXT into an INTEGER column and SQLite's affinity
% rules would store the text -- the TEXT-collapse class this project has
% already paid for twice. The guard is emitted BEFORE the extract for the
% same reason every other guard here is: SQL states no evaluation order for
% AND, and the left-to-right text is what makes the guard protect its
% neighbour.
json_pattern_sql(Typed, Position, Index, Index, Bound0, Bound, [], WhereTexts) :-
    nonvar(Typed), Typed = (Hole : Type), var(Hole), atom(Type), !,
    json_capture_json_type(Type, JsonTypeName),
    json_value_sql(Position, ValueSql),
    json_type_sql(Position, TypeSql),
    json_capture_type_guard(JsonTypeName, TypeSql, TypeGuard),
    (   bound_lookup(Bound0, Hole, typed(Existing, _, ExistingEncoding))
    ->  Bound = Bound0,
        aligned_pair(direct, ValueSql, ExistingEncoding, Existing, AlignedValue, AlignedExisting),
        format(atom(Equality), '~w = ~w', [AlignedValue, AlignedExisting]),
        WhereTexts = [TypeGuard, Equality]
    ;   Bound = [Hole-typed(ValueSql, Type, direct) | Bound0],
        WhereTexts = [TypeGuard]
    ).
% The empty object: open with no members, so it asserts object-ness and
% nothing else.
json_pattern_sql('{}', Position, Index, Index, Bound, Bound, [], [Text]) :- !,
    json_object_guard(Position, Text).
% `[... Sub]` : one row per array element. json_each over the ARRAY, then the
% sub-pattern against each element's value.
json_pattern_sql(spread(Sub), Position, Index0, Index, Bound0, Bound,
                 FromParts, WhereTexts) :- !,
    json_value_sql(Position, ValueSql),
    json_type_sql(Position, TypeSql),
    format(atom(Alias), 'j~w', [Index0]),
    format(atom(From), 'json_each(~w) ~w', [ValueSql, Alias]),
    format(atom(ArrayGuard), '~w = ''array''', [TypeSql]),
    format(atom(ElementBase), '~w.value', [Alias]),
    format(atom(ElementType), '~w.type', [Alias]),
    Index1 is Index0 + 1,
    json_pattern_sql(Sub, jsonpos(ElementBase, ['$'], ElementType),
                     Index1, Index, Bound0, Bound, SubFrom, SubWhere),
    append([From], SubFrom, FromParts),
    append([ArrayGuard], SubWhere, WhereTexts).
json_pattern_sql('{}'(Fields), Position, Index0, Index, Bound0, Bound,
                 FromParts, WhereTexts) :- !,
    json_object_guard(Position, ObjectGuard),
    brace_pattern_pairs(Fields, Pairs),
    json_members_sql(Pairs, Position, Index0, Index, Bound0, Bound,
                     FromParts, MemberWhere),
    WhereTexts = [ObjectGuard | MemberWhere].
% A scalar in pattern position is an equality filter.
json_pattern_sql(Literal, Position, Index, Index, Bound, Bound, [], [Text]) :-
    atomic(Literal), !,
    json_value_sql(Position, ValueSql),
    sql_literal(Literal, Quoted),
    format(atom(Text), '~w = ~w', [ValueSql, Quoted]).
json_pattern_sql(Pattern, _, _, _, _, _, _, _) :-
    throw(unsupported_construct(json_pattern_shape(Pattern))).

json_object_guard(Position, Text) :-
    json_type_sql(Position, TypeSql),
    format(atom(Text), '~w = ''object''', [TypeSql]).

% The capture types, clause-for-clause with body.pl:json_capture_type/2 (the
% agreement is pinned by the json_typed_capture plunit unit and, ultimately,
% by the byte-identical tick-log grade). int/float/text map to ONE json1
% `json_type` answer each; bool maps to the pair true/false.
json_capture_json_type(int,   integer) :- !.
json_capture_json_type(float, real) :- !.
json_capture_json_type(text,  text) :- !.
json_capture_json_type(bool,  boolean) :- !.
json_capture_json_type(Type,  _) :-
    throw(unsupported_construct(json_capture_type_unknown(Type))).

% json1 answers a boolean as TWO json_type names, so that guard is a set test;
% every other type is one equality.
json_capture_type_guard(boolean, TypeSql, Guard) :- !,
    format(atom(Guard), '~w IN (''true'', ''false'')', [TypeSql]).
json_capture_type_guard(JsonTypeName, TypeSql, Guard) :-
    format(atom(Guard), '~w = ''~w''', [TypeSql, JsonTypeName]).

json_members_sql([], _, Index, Index, Bound, Bound, [], []).
json_members_sql([Key-Sub | Rest], Position, Index0, Index, Bound0, Bound,
                 FromParts, WhereTexts) :-
    json_member_sql(Key, Sub, Position, Index0, Index1, Bound0, Bound1,
                    HereFrom, HereWhere),
    json_members_sql(Rest, Position, Index1, Index, Bound1, Bound,
                     MoreFrom, MoreWhere),
    append(HereFrom, MoreFrom, FromParts),
    append(HereWhere, MoreWhere, WhereTexts).

% An EXACT key costs no join: the segment is appended to this position's path
% and the sub-pattern compiles against the extended path, so `{user: {login:
% $a}}` comes out as ONE json_extract(b0."body", '$."user"."login"').
json_member_sql(Key, Sub, jsonpos(BaseSql, Segments, RootTypeSql),
                Index0, Index, Bound0, Bound, FromParts, WhereTexts) :-
    atom(Key), Key \== '**', !,
    json_path_segment(Key, Segment),
    json_pattern_sql(Sub, jsonpos(BaseSql, [Segment | Segments], RootTypeSql),
                     Index0, Index, Bound0, Bound, FromParts, WhereTexts).
% KEY CAPTURE. json_each already yields (key, value); the construct the
% recovery doc graded "genuinely needs new surface syntax" needs zero new SQL
% (lab receipt L3). The key hole binds `key`, the sub-pattern reads `value`.
json_member_sql($(KeyHole), Sub, Position, Index0, Index, Bound0, Bound,
                FromParts, WhereTexts) :- !,
    json_value_sql(Position, ValueSql),
    format(atom(Alias), 'j~w', [Index0]),
    format(atom(From), 'json_each(~w) ~w', [ValueSql, Alias]),
    format(atom(KeySql), '~w.key', [Alias]),
    format(atom(MemberBase), '~w.value', [Alias]),
    format(atom(MemberType), '~w.type', [Alias]),
    Index1 is Index0 + 1,
    (   bound_lookup(Bound0, KeyHole, typed(ExistingKey, _, ExistingKeyEncoding))
    ->  Bound1 = Bound0,
        aligned_pair(direct, KeySql, ExistingKeyEncoding, ExistingKey, AlignedKey, AlignedExistingKey),
        format(atom(KeyText), '~w = ~w', [AlignedKey, AlignedExistingKey]),
        KeyWhere = [KeyText]
    ;   Bound1 = [KeyHole-typed(KeySql, text, direct) | Bound0],
        KeyWhere = []
    ),
    json_pattern_sql(Sub, jsonpos(MemberBase, ['$'], MemberType),
                     Index1, Index, Bound1, Bound, SubFrom, SubWhere),
    append([From], SubFrom, FromParts),
    append(KeyWhere, SubWhere, WhereTexts).
% `**` DESCENT (ruling descent_depth_cap = uncapped, "css aint got it").
% json_tree walks the whole value, root first, so the sub-pattern is tried at
% every depth including this object itself -- the same set descendant_object/2
% enumerates on the oracle side. `fullkey` rides the same join, which is what
% would make v4's dropped path bind free if a spelling is ever ruled for it.
json_member_sql('**', Sub, Position, Index0, Index, Bound0, Bound,
                FromParts, WhereTexts) :- !,
    json_value_sql(Position, ValueSql),
    format(atom(Alias), 'j~w', [Index0]),
    format(atom(From), 'json_tree(~w) ~w', [ValueSql, Alias]),
    format(atom(NodeBase), '~w.value', [Alias]),
    format(atom(NodeType), '~w.type', [Alias]),
    Index1 is Index0 + 1,
    json_pattern_sql(Sub, jsonpos(NodeBase, ['$'], NodeType),
                     Index1, Index, Bound0, Bound, SubFrom, SubWhere),
    append([From], SubFrom, FromParts),
    WhereTexts = SubWhere.
json_member_sql(Key, _, _, _, _, _, _, _, _) :-
    throw(unsupported_construct(json_key_shape(Key))).

% `{a: 1, b: 2}` is `{}`/1 over a comma conjunction of `:`/2 pairs on both
% doors. Deliberately NOT braces_pattern_pairs/2 (the struct arm's): that one
% may not see a `$`/1 or `'**'` key and a shared predicate would have to admit
% keys the struct plane refuses by design.
brace_pattern_pairs((Left, Right), Pairs) :- !,
    brace_pattern_pairs(Left, LeftPairs),
    brace_pattern_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
brace_pattern_pairs(Key: Sub, [Key-Sub]).

% ═══ aggregate heads ════════════════════════════════════════════════════════
% engine.pl's aggregate contract, clause by clause (level_eval.pl
% agg_rule_rows/4 + agg_compute/3), and how each half maps:
%
%   GROUPING  "grouping is by the evaluated non-aggregate head columns"
%             (engine.pl header, q7/q9). group_key/3 collects exactly the
%             plain(_) template positions' VALUES, so GROUP BY takes the same
%             plain head expressions, evaluated, in head order.
%   BAG       the aggregated multiset is `findall(Contribution, solve(Body))`
%             -- one contribution PER BODY DERIVATION, duplicates kept (q7,
%             the fail-pre-fix count_is_bag_of_derivations receipt: two hits
%             on one line count 2, not 1). The FROM/WHERE join produces
%             exactly those derivations as rows, so `count(*)` is the bag
%             count. NOT count(<expr>), which skips NULLs, and emphatically
%             not count(DISTINCT ...), the REJECTED READING q7 names.
%   EMPTY     `Bag \== []` -- a body with no solutions yields NO row at all.
%             With GROUP BY that falls out (no input rows, no groups); with
%             ZERO plain columns SQLite would still return one row carrying
%             count 0 over the empty set, so HAVING count(*) > 0 is the
%             guard. Emitted unconditionally: it is a tautology whenever a
%             GROUP BY is present, and the guard exactly when it is not.
%   sum       sum_list/2 over integers; SQLite sum() over INTEGER operands
%             returns INTEGER (measured through the real driver).
%   min/max   min_list/2 and max_list/2 accept NUMBERS ONLY -- they fail
%             outright on an atom -- so a non-int aggregated expression is a
%             named unsupported construct here rather than a silent lexicographic min.
aggregate_select_statement(Mode, HeadColumnTypes, Head, Template, Bound, FromSql,
                           AllWhereTexts, SelectStatement, InternSqls) :-
    Head =.. [_ | Args],
    aggregate_select_exprs(Mode, HeadColumnTypes, Template, Args, Bound, ColumnSqls,
                           Kinds),
    aggregate_group_exprs(Mode, Template, Bound, GroupExprs),
    ( AllWhereTexts == []
    -> WhereClause = ''
    ;  atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
       format(atom(WhereClause), ' WHERE ~w', [WhereSql])
    ),
    ( GroupExprs == []
    -> GroupClause = ''
    ;  atomic_list_concat(GroupExprs, ', ', GroupSql),
       format(atom(GroupClause), ' GROUP BY ~w', [GroupSql])
    ),
    aggregate_built_values(Kinds, ColumnSqls, BuiltValues),
    (   BuiltValues == []
    ->  atomic_list_concat(ColumnSqls, ', ', SelectSql),
        aggregate_grouped_sql(SelectSql, FromSql, WhereClause, GroupClause,
                              SelectStatement),
        InternSqls = []
    ;   aggregate_encoded_statement(Kinds, ColumnSqls, FromSql, WhereClause,
                                    GroupClause, SelectStatement),
        aggregate_intern_statements(BuiltValues, FromSql, WhereClause, GroupClause,
                                    InternSqls)
    ).

aggregate_grouped_sql(SelectSql, FromSql, WhereClause, GroupClause, Sql) :-
    format(atom(Sql), 'SELECT ~w FROM ~w~w~w HAVING count(*) > 0',
           [SelectSql, FromSql, WhereClause, GroupClause]).

% SQLite refuses an aggregate function inside a scalar subquery belonging to
% the same query, so the id lookup runs one level out over the grouped rows.
aggregate_encoded_statement(Kinds, ColumnSqls, FromSql, WhereClause, GroupClause,
                            Sql) :-
    aggregate_alias_names(Kinds, 1, Aliases),
    maplist(alias_select_expr, ColumnSqls, Aliases, InnerExprs),
    atomic_list_concat(InnerExprs, ', ', InnerSelectSql),
    aggregate_grouped_sql(InnerSelectSql, FromSql, WhereClause, GroupClause, InnerSql),
    maplist(aggregate_outer_expr, Kinds, Aliases, OuterExprs),
    atomic_list_concat(OuterExprs, ', ', OuterSelectSql),
    format(atom(Sql), 'SELECT ~w FROM (~w)', [OuterSelectSql, InnerSql]).

aggregate_alias_names([], _, []).
aggregate_alias_names([_ | Rest], Position, [Alias | More]) :-
    format(atom(Alias), '__agg_~w', [Position]),
    NextPosition is Position + 1,
    aggregate_alias_names(Rest, NextPosition, More).

aggregate_outer_expr(built, Alias, Expr) :- !,
    quote_ident(Alias, Quoted),
    interned_id_sql(Quoted, Expr).
aggregate_outer_expr(stored, Alias, Expr) :- quote_ident(Alias, Expr).

% An aggregated string exists once per GROUP, so its dictionary write repeats
% the arm's grouping; intern_write_sql/4's row-wise DISTINCT would not.
aggregate_intern_statements(BuiltValues, FromSql, WhereClause, GroupClause,
                            [InternSql]) :-
    maplist(aggregate_intern_arm(FromSql, WhereClause, GroupClause), BuiltValues, Arms),
    atomic_list_concat(Arms, ' UNION ', ArmsSql),
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(InternSql), 'INSERT OR IGNORE INTO ~w ("content") ~w',
           [QuotedDictionary, ArmsSql]).

aggregate_intern_arm(FromSql, WhereClause, GroupClause, ValueSql, Arm) :-
    aggregate_grouped_sql(ValueSql, FromSql, WhereClause, GroupClause, Arm).

aggregate_built_values([], [], []).
aggregate_built_values([built | Kinds], [Sql | Sqls], [Sql | Rest]) :- !,
    aggregate_built_values(Kinds, Sqls, Rest).
aggregate_built_values([stored | Kinds], [_ | Sqls], Rest) :-
    aggregate_built_values(Kinds, Sqls, Rest).

aggregate_select_exprs(_, [], [], [], _, [], []).
aggregate_select_exprs(Mode, [ColumnType | RestTypes], [TemplateArg | RestTemplate],
                       [_Arg | RestArgs], Bound, [Sql | RestSqls], [Kind | RestKinds]) :-
    aggregate_select_expr(Mode, TemplateArg, Bound, Sql, Encoding),
    (   column_encoding(Mode, ColumnType, dict), Encoding == direct
    ->  Kind = built
    ;   Kind = stored
    ),
    aggregate_select_exprs(Mode, RestTypes, RestTemplate, RestArgs, Bound, RestSqls,
                           RestKinds).

% Every aggregate function answers with the characters it computed, never with
% a dictionary id, so `direct` is the encoding of all of them.
aggregate_select_expr(Mode, plain(Expr), Bound, Sql, Encoding) :- !,
    compile_expr(Mode, identity, Expr, Bound, Sql, _Type, Encoding).
aggregate_select_expr(_, agg(count, _Expr), _Bound, 'count(*)', direct) :- !.
aggregate_select_expr(Mode, agg(sum, Expr), Bound, Sql, direct) :- !,
    compile_aggregate_number_operand(Mode, sum, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'sum(~w)', [InnerSql]).
aggregate_select_expr(Mode, agg(avg, Expr), Bound, Sql, direct) :- !,
    compile_aggregate_number_operand(Mode, avg, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'avg(~w)', [InnerSql]).
aggregate_select_expr(Mode, agg(min, Expr), Bound, Sql, direct) :- !,
    compile_aggregate_number_operand(Mode, min, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'min(~w)', [InnerSql]).
aggregate_select_expr(Mode, agg(max, Expr), Bound, Sql, direct) :- !,
    compile_aggregate_number_operand(Mode, max, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'max(~w)', [InnerSql]).
% The ELSE arm RAISES: `json_object_dup_key` is not valid JSON text, so json/1
% fails the statement with "malformed JSON", matching the oracle's throw.
aggregate_select_expr(Mode, agg(json_object, KeyExpr-ValueExpr), Bound, Sql, direct) :- !,
    compile_expr(Mode, value, KeyExpr, Bound, KeySql, _KeyType, _KeyEncoding),
    compile_expr(Mode, value, ValueExpr, Bound, ValueSql, ValueType, _ValueEncoding),
    json_group_array_value_sql(ValueType, ValueSql, AggregateValueSql),
    format(atom(Sql),
           'CASE WHEN count(DISTINCT json_array(~w, ~w)) = count(DISTINCT ~w) THEN json_group_object(~w, ~w ORDER BY ~w) ELSE json(\'json_object_dup_key\') END',
           [KeySql, AggregateValueSql, KeySql, KeySql, AggregateValueSql, KeySql]).
aggregate_select_expr(Mode, agg(json_group_array, Expr), Bound, Sql, direct) :- !,
    compile_expr(Mode, value, Expr, Bound, ValueSql, ValueType, _Encoding),
    json_group_array_value_sql(ValueType, ValueSql, AggregateValueSql),
    format(atom(Sql), 'json_group_array(~w ORDER BY ~w)',
           [AggregateValueSql, ValueSql]).
aggregate_select_expr(Mode, agg(json_group_array_ordered, ValueExpr-OrdinalExpr),
                      Bound, Sql, direct) :- !,
    compile_expr(Mode, value, ValueExpr, Bound, ValueSql, ValueType, _Encoding),
    compile_aggregate_ordinal_operand(Mode, OrdinalExpr, Bound, OrdinalSql),
    json_group_array_value_sql(ValueType, ValueSql, AggregateValueSql),
    format(atom(Sql), 'json_group_array(~w ORDER BY ~w)',
           [AggregateValueSql, OrdinalSql]).
aggregate_select_expr(Mode, agg(group_concat(Sep), Expr), Bound, Sql, direct) :- !,
    compile_expr(Mode, value, Expr, Bound, ValueSql, _, _Encoding),
    compile_aggregate_text_separator(Sep, SeparatorSql),
    format(atom(Sql), 'group_concat(~w, ~w ORDER BY ~w)',
           [ValueSql, SeparatorSql, ValueSql]).
aggregate_select_expr(Mode, agg(group_concat_ordered(Sep), ValueExpr-OrdinalExpr),
                      Bound, Sql, direct) :- !,
    compile_expr(Mode, value, ValueExpr, Bound, ValueSql, _, _Encoding),
    compile_aggregate_text_separator(Sep, SeparatorSql),
    compile_aggregate_ordinal_operand(Mode, OrdinalExpr, Bound, OrdinalSql),
    format(atom(Sql), 'group_concat(~w, ~w ORDER BY ~w)',
           [ValueSql, SeparatorSql, OrdinalSql]).
aggregate_select_expr(_, agg(Kind, _), _, _, _) :-
    throw(unsupported_construct(aggregate_kind_not_lowered(Kind))).

json_group_array_value_sql(json, ValueSql, AggregateValueSql) :- !,
    format(atom(AggregateValueSql), 'json(~w)', [ValueSql]).
json_group_array_value_sql(json_list(_), ValueSql, AggregateValueSql) :- !,
    format(atom(AggregateValueSql), 'json(~w)', [ValueSql]).
json_group_array_value_sql(_, ValueSql, ValueSql).

compile_aggregate_ordinal_operand(Mode, Expr, Bound, Sql) :-
    compile_expr(Mode, identity, Expr, Bound, Sql, Type, _Encoding),
    ( Type == int
    -> true
    ;  throw(unsupported_construct(aggregate_ordinal_not_int(Expr, Type)))
    ).

compile_aggregate_text_separator(Sep, Sql) :-
    ( nonvar(Sep), atomic(Sep), \+ number(Sep)
    -> sql_literal(Sep, Sql)
    ;  throw(unsupported_construct(aggregate_separator_not_constant(Sep)))
    ).

compile_aggregate_number_operand(Mode, Kind, Expr, Bound, Sql, Type) :-
    compile_expr(Mode, identity, Expr, Bound, Sql, Type, _Encoding),
    ( memberchk(Type, [int, float])
    -> true
    ;  throw(unsupported_construct(aggregate_operand_not_number(Kind, Expr, Type)))
    ).

aggregate_group_exprs(Mode, Template, Bound, GroupExprs) :-
    findall(GroupExpr,
            ( member(plain(Expr), Template), group_expr(Mode, Bound, Expr, GroupExpr) ),
            GroupExprs).

% The head columns an aggregate rule GROUPS BY, as SQL text, reused by the
% group-scoped incremental path below.
aggregate_group_positions(Template, Positions) :-
    findall(Position,
            ( nth1(Position, Template, TemplateArg), TemplateArg = plain(_) ),
            Positions).

level_delta_insert_sql(Mode, RelPlans, HeadRef, Rules, DeltaInsertSql, InternSqls) :-
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    level_rules_delta_arms(Mode, RelPlans, Rules, 0, DeltaArms, CteSqls,
                           InternSqls),
    atomic_list_concat(DeltaArms, ' UNION ALL ', DeltaSelectSql),
    (   CteSqls == []
    ->  WithSql = ''
    ;   atomic_list_concat(CteSqls, ', ', CtesSql),
        format(atom(WithSql), 'WITH ~w ', [CtesSql])
    ),
    format(atom(DeltaInsertSql),
           '~wINSERT OR IGNORE INTO ~w (~w) ~w RETURNING ~w',
           [WithSql, QuotedHeadTable, HeadColumnsSql, DeltaSelectSql,
            HeadColumnsSql]).

level_rules_delta_arms(_, _, [], _, [], [], []).
level_rules_delta_arms(Mode, RelPlans, [Rule | Rest], RuleIndex, DeltaArms,
                       CteSqls, InternGroups) :-
    level_rule_delta_arms(Mode, RelPlans, Rule, RuleIndex, RuleArms, RuleCtes,
                          RuleInterns),
    NextRuleIndex is RuleIndex + 1,
    level_rules_delta_arms(Mode, RelPlans, Rest, NextRuleIndex, RestArms,
                           RestCtes, RestInterns),
    append(RuleArms, RestArms, DeltaArms),
    append(RuleCtes, RestCtes, CteSqls),
    append(RuleInterns, RestInterns, InternGroups).

level_rule_delta_arms(Mode, RelPlans, (Head <- Body), RuleIndex, DeltaArms,
                      CteSqls, InternGroups) :-
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    level_coalesce_cte(Mode, RelPlans, Head, Body, PosUses, NegUses, RuleIndex,
                       CteName, CteSqls, CteInterns),
    level_positive_delta_arms(Mode, RelPlans, Head, Body, PosUses, NegUses,
                              PosUses, CteName, PositiveArms,
                              PositiveInterns),
    exclude(coalesce_recount_use, NegUses, DeltaNegUses),
    level_negative_delta_arms(Mode, RelPlans, Head, Body, PosUses,
                              DeltaNegUses, NegUses, NegativeArms,
                              NegativeInterns),
    append(PositiveArms, NegativeArms, DeltaArms),
    append([CteInterns, PositiveInterns, NegativeInterns], InternGroups).

coalesce_recount_use(use(_, _, neg, coalesce_recount)).

level_coalesce_cte(Mode, RelPlans, Head, Body, PosUses, NegUses, RuleIndex,
                   CteName, [CteSql], InternSqls) :-
    member(use(_, _, pos, coalesce(_, _)), PosUses),
    !,
    format(atom(CteName), '__coalesce_rule_~w', [RuleIndex]),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0, FromParts,
                          PosWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts,
                        GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    append(FromParts, JsonFromParts, AllFromParts),
    from_parts_sql(AllFromParts, FromSql),
    rel_ref(Head, HeadRef),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, HeadColumns,
                     HeadSelectExprs, BuiltValues, ListInterns),
    coalesce_cte_key_exprs(Mode, RelPlans, PosUses, Bound, 0, KeySelectExprs),
    append(HeadSelectExprs, KeySelectExprs, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    (   WhereTexts == []
    ->  WhereSql = none,
        format(atom(Projection), 'SELECT DISTINCT ~w FROM ~w',
               [SelectSql, FromSql])
    ;   atomic_list_concat(WhereTexts, ' AND ', WhereText),
        WhereSql = WhereText,
        format(atom(Projection), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
               [SelectSql, FromSql, WhereText])
    ),
    format(atom(CteSql), '~w AS (~w)', [CteName, Projection]),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, InternSqls).
level_coalesce_cte(_, _, _, _, _, _, _, none, [], []).

coalesce_cte_key_exprs(_, _, [], _, _, []).
coalesce_cte_key_exprs(Mode, RelPlans,
                       [use(Ref, Args, pos, coalesce(Output, _)) | Rest],
                       Bound, Position, Exprs) :-
    !,
    relplan_columns(RelPlans, Ref, Columns),
    coalesce_use_key_exprs(Mode, Args, Columns, Output, Bound, Position, 0,
                           Here),
    NextPosition is Position + 1,
    coalesce_cte_key_exprs(Mode, RelPlans, Rest, Bound, NextPosition, More),
    append(Here, More, Exprs).
coalesce_cte_key_exprs(Mode, RelPlans, [_ | Rest], Bound, Position, Exprs) :-
    NextPosition is Position + 1,
    coalesce_cte_key_exprs(Mode, RelPlans, Rest, Bound, NextPosition, Exprs).

coalesce_use_key_exprs(_, [], [], _, _, _, _, []).
coalesce_use_key_exprs(Mode, [Arg | RestArgs], [_ | RestColumns], Output,
                       Bound, Position, ColumnPosition, Exprs) :-
    NextColumnPosition is ColumnPosition + 1,
    (   Arg == Output
    ->  Exprs = More
    ;   compile_expr(Mode, identity, Arg, Bound, Sql, _, _),
        coalesce_key_alias(Position, ColumnPosition, Alias),
        quote_ident(Alias, QuotedAlias),
        format(atom(Expr), '~w AS ~w', [Sql, QuotedAlias]),
        Exprs = [Expr | More]
    ),
    coalesce_use_key_exprs(Mode, RestArgs, RestColumns, Output, Bound,
                           Position, NextColumnPosition, More).

coalesce_key_alias(Position, ColumnPosition, Alias) :-
    format(atom(Alias), 'c~w_~w', [Position, ColumnPosition]).

level_positive_delta_arms(_, _, _, _, [], _, _, _, [], []).
% STRUCT-AS-ROWS: a dictionary atom gets NO delta arm, and needs none. A
% dictionary row is created only by interning a value some ARRIVING row
% carries, and interning runs before that tick's arrival statements, so every
% new dictionary row already has its parent row in the parent's own frontier
% -- the parent's arm covers exactly the same derivations. An arm on this side
% would additionally read `__frontier___dict_<type>`, a table the DDL does not
% create (a dictionary is storage plane: no delta table, no frontier, no
% boundary), so this is the same fact stated twice: dictionaries do not move
% on their own.
level_positive_delta_arms(Mode, RelPlans, Head, Body, [_ | RestPositions],
                          NegUses, PosUses, CteName, Arms, InternGroups) :-
    length(RestPositions, RemainingCount),
    length(PosUses, PositiveCount),
    Position is PositiveCount - RemainingCount - 1,
    nth0_split(Position, PosUses, NewBeforeUses, DeltaUse, AfterUses),
    maplist(old_state_use, AfterUses, OldAfterUses),
    append(NewBeforeUses, OldAfterUses, OtherPosUses),
    level_positive_delta_arms(Mode, RelPlans, Head, Body, RestPositions,
                              NegUses, PosUses, CteName, RestArms, RestInterns),
    (   dictionary_use(DeltaUse)
    ->  Arms = RestArms, InternGroups = RestInterns
    ;   DeltaUse = use(_, _, pos, coalesce(_, _))
    ->  level_coalesce_delta_arm(RelPlans, Head, DeltaUse, Position, CteName,
                                 CoalesceArm),
        Arms = [CoalesceArm | RestArms],
        InternGroups = RestInterns
    ;   level_delta_select_arm(Mode, RelPlans, Head, Body, DeltaUse, OtherPosUses, NegUses,
                               DeltaArm, ArmInterns),
        Arms = [DeltaArm | RestArms],
        append(ArmInterns, RestInterns, InternGroups)
    ).

dictionary_use(use(Name/_Arity, _, _, _)) :- sub_atom(Name, 0, _, _, '__ref_').

old_state_use(Use, Use) :-
    dictionary_use(Use),
    !.
old_state_use(Use, Use) :-
    Use = use(_, _, pos, coalesce(_, _)),
    !.
old_state_use(use(Ref, Args, pos, Source),
              use(Ref, Args, pos, old_state(Source))).

nth0_split(0, [Selected | Rest], [], Selected, Rest) :- !.
nth0_split(Index, [Item | Rest], [Item | Before], Selected, After) :-
    Index > 0,
    NextIndex is Index - 1,
    nth0_split(NextIndex, Rest, Before, Selected, After).

% The guard walk runs HERE too, not only in the recompute insert. Omitting it
% was a real miscompile caught by the sweep, not a theoretical one: with the
% guard present in level_insert_sql/4 but absent from the delta arm,
% spine_semantics.pl's dirty_retracts_on_matching_commit correctly retracted
% dirty("src/lib.rs") at tick 2 and then the tick-3 drain re-inserted it off
% the frontier with `WorktreeDigest \== TreeDigest` simply not applied --
% oracle tick 3 is empty, actual added the row back. Every statement family
% that reproduces a rule body has to reproduce its guards; compile_body_guards/4
% is the single place that happens.
level_delta_select_arm(Mode, RelPlans, Head, Body, use(DeltaRef, DeltaArgs, pos, _),
                       OtherPosUses, NegUses, DeltaArm, InternSqls) :-
    frontier_table_name(DeltaRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    rel_ref(Head, HeadRef),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(Mode, DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      DeltaFieldBound, DeltaWhereParts),
    delta_reference_identity(RelPlans, DeltaRef, DeltaArgs, DeltaColumns,
                             DeltaFieldBound, DeltaBound,
                             IdentityFromParts, IdentityWhereTexts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    compile_positive_uses(Mode, RelPlans, OtherPosUses, DeltaBound, Bound0,
                          OtherFromParts, OtherWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts, GuardWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, none, SelectExprs, BuiltValues, ListInterns),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w d0', [QuotedFrontierTable]),
    append([[DeltaFrom], IdentityFromParts, OtherFromParts, JsonFromParts],
           FromParts),
    from_parts_sql(FromParts, FromSql),
    append([['d0."_phase" >= 0' | DeltaWhereTexts], IdentityWhereTexts,
            OtherWhereTexts], PositiveWhereTexts),
    append([PositiveWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaArm), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectSql, FromSql, WhereSql]),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, InternSqls).

level_negative_delta_arms(_, _, _, _, _, [], _, [], []).
level_negative_delta_arms(Mode, RelPlans, Head, Body, PosUses,
                          [NegUse | Rest], NegUses,
                          [Arm | MoreArms], InternGroups) :-
    level_negative_delta_arm(Mode, RelPlans, Head, Body, PosUses, NegUse,
                             NegUses, Arm, ArmInterns),
    level_negative_delta_arms(Mode, RelPlans, Head, Body, PosUses, Rest,
                              NegUses, MoreArms, RestInterns),
    append(ArmInterns, RestInterns, InternGroups).

level_negative_delta_arm(Mode, RelPlans, Head, Body, PosUses,
                         use(DeltaRef, DeltaArgs, neg, _), NegUses,
                         DeltaArm, InternSqls) :-
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    rel_ref(Head, HeadRef),
    relplan_column_types(RelPlans, HeadRef, HeadColumnTypes),
    compile_positive_uses(Mode, RelPlans, PosUses, [], Bound0,
                          PositiveFromParts, PositiveWhereTexts),
    compile_body_guards(Mode, Body, Bound0, Bound, JsonFromParts,
                        GuardWhereTexts),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_negative_atom_args(Mode, DeltaArgs, DeltaColumns,
                               DeltaColumnTypes, d0, Bound,
                               DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    compile_negative_uses(Mode, RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Mode, HeadColumnTypes, Head, Bound, none, SelectExprs,
                     BuiltValues, ListInterns),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w d0', [QuotedDeltaTable]),
    append([[DeltaFrom], PositiveFromParts, JsonFromParts], FromParts),
    from_parts_sql(FromParts, FromSql),
    append([['d0."_sign" < 0' | DeltaWhereTexts], PositiveWhereTexts,
            GuardWhereTexts, NegWhereTexts], WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaArm), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectSql, FromSql, WhereSql]),
    intern_write_statements(BuiltValues, FromSql, WhereSql, TextInternSqls),
    list_intern_statements(ListInterns, FromSql, WhereSql, ListInternSqls),
    append(TextInternSqls, ListInternSqls, InternSqls).

level_coalesce_delta_arm(RelPlans, Head,
                         use(DeltaRef, DeltaArgs, pos,
                             coalesce(Output, _)),
                         Position, CteName, Arm) :-
    frontier_table_name(DeltaRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    rel_ref(Head, HeadRef),
    relplan_storage_name(RelPlans, HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(qualified_quoted_column(q), HeadColumns, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    coalesce_event_key_equalities(DeltaArgs, DeltaColumns, Output, Position,
                                  gain, 0, GainEqualities),
    coalesce_event_key_equalities(DeltaArgs, DeltaColumns, Output, Position,
                                  loss, 0, LossEqualities),
    atomic_list_concat(['gain."_phase" >= 0' | GainEqualities], ' AND ',
                       GainWhere),
    atomic_list_concat(['loss."_sign" < 0' | LossEqualities], ' AND ',
                       LossWhere),
    format(atom(Arm),
           'SELECT * FROM (SELECT DISTINCT ~w FROM ~w q WHERE (EXISTS (SELECT 1 FROM ~w gain WHERE ~w) OR EXISTS (SELECT 1 FROM ~w loss WHERE ~w)) EXCEPT SELECT ~w FROM ~w)',
           [SelectSql, CteName, QuotedFrontierTable, GainWhere,
            QuotedDeltaTable, LossWhere,
            HeadColumnsSql, QuotedHeadTable]).

qualified_quoted_column(Alias, Column, Expr) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Expr), '~w.~w', [Alias, QuotedColumn]).

coalesce_event_key_equalities([], [], _, _, _, _, []).
coalesce_event_key_equalities([Arg | RestArgs], [Column | RestColumns], Output,
                               Position, EventAlias, ColumnPosition,
                               Equalities) :-
    NextColumnPosition is ColumnPosition + 1,
    (   Arg == Output
    ->  Equalities = More
    ;   coalesce_key_alias(Position, ColumnPosition, KeyAlias),
        quote_ident(KeyAlias, QuotedKeyAlias),
        quote_ident(Column, QuotedColumn),
        format(atom(Equality), 'q.~w = ~w.~w',
               [QuotedKeyAlias, EventAlias, QuotedColumn]),
        Equalities = [Equality | More]
    ),
    coalesce_event_key_equalities(RestArgs, RestColumns, Output, Position,
                                   EventAlias, NextColumnPosition, More).

delta_reference_identity(RelPlans, Name/Arity, Args, Columns,
                         Bound0, Bound, [From], Equalities) :-
    reference_target_ref(RelPlans, Name/Arity),
    !,
    relplan_storage_name(RelPlans, Name/Arity, Table),
    quote_ident(Table, QuotedTable),
    format(atom(From), '~w r0', [QuotedTable]),
    findall(Equality,
            ( member(Column, Columns),
              format(atom(Equality), 'r0."~w" = d0."~w"',
                     [Column, Column]) ),
            Equalities),
    length(Args, Arity),
    Atom =.. [Name | Args],
    Bound = [Atom-typed('r0."__id"', ref(Name), direct) | Bound0].
delta_reference_identity(_, _, _, _, Bound, Bound, [], []).

is_positive_use(use(_, _, pos, _)).
is_negative_use(use(_, _, neg, _)).

% ═══ delta statements (round 2: one plain "read every row" query per rel;
% the runtime diffs before/after via multisetDiff, reused not reinvented) ══
%
% ROUND 3 (reconciliation): the tick-log envelope pins compound-term
% serialization to CANONICAL PROLOG TEXT ("route_data(settings)"), not this
% compiler's storage encoding. Storage stays json1 (this compiler's own
% business, chosen over phase A's LIKE/substr for handling arbitrary arity
% with no baked-in functor-length constant) -- only the delta-snapshot READ
% renders canonical text, via canonical_column_expr/2 below. This is a SQL-
% TEXT change inside deltastmt/2, not a plan-term SHAPE change (still
% deltastmt(Ref, SelectSql, DeltaTable, BoundarySql)); it lives here rather
% than in emit_ts.pl
% because it is a SQL-generation decision a future emit_rust.pl would want
% identically, not a TypeScript-specific rendering choice.

delta_statement(Mode, RelPlan,
                deltastmt(Ref, SelectSql, DeltaTable, BoundarySql, StoredSelectSql)) :-
    relplan_parts(RelPlan, Ref, _Kind, Columns, _, ColumnTypes),
    table_name(Ref, Table),
    text_read_table(Mode, Table, ColumnTypes, ReadTable),
    quote_ident(ReadTable, QuotedTable),
    maplist(canonical_column_expr, Columns, ColumnTypes, ColumnExprs),
    atomic_list_concat(ColumnExprs, ', ', ColumnsSql),
    % Alias `t`: canonical_column_expr/3's render subqueries qualify the outer
    % row with it, so both reads below must supply it.
    list_column_joins(Columns, ColumnTypes, ListJoinSql),
    format(atom(SelectSql), 'SELECT ~w FROM ~w t~w',
           [ColumnsSql, QuotedTable, ListJoinSql]),
    delta_table_name(Ref, DeltaTable),
    text_read_table(Mode, DeltaTable, ColumnTypes, DeltaReadTable),
    quote_ident(DeltaReadTable, QuotedDeltaTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', GroupColumnsSql),
    quote_ident(Table, QuotedStoredTable),
    format(atom(StoredSelectSql), 'SELECT ~w FROM ~w',
           [GroupColumnsSql, QuotedStoredTable]),
    % The joined view carries its own "list_id", so the grouping columns and
    % the sign name the outer row explicitly.
    maplist(qualified_outer_column, Columns, QualifiedColumns),
    atomic_list_concat(QualifiedColumns, ', ', QualifiedGroupSql),
    format(atom(BoundarySql),
           'SELECT ~w, t."_sign" AS "__sign", count(*) AS "__count" FROM ~w t~w WHERE t."_sign" IN (-1, 1) GROUP BY ~w, t."_sign"',
           [ColumnsSql, QuotedDeltaTable, ListJoinSql, QualifiedGroupSql]).

qualified_outer_column(Column, Qualified) :-
    quote_ident(Column, QuotedColumn),
    atomic_list_concat(['t.', QuotedColumn], Qualified).

% ═══ `?` order tails (the final cursor, and nowhere else) ══════════════════

% deltastmt's SelectSql is ALSO the tick path's snapshot read, so each emitter
% appends this onto final_select alone and SelectSql stays byte-identical.
query_order_by_map(Decls, RelPlans, Pairs) :-
    findall(Name-Sql,
            ( member(QueryDecl, Decls),
              query_decl(QueryDecl, Atom, OrderCols),
              OrderCols \== [],
              functor(Atom, Name, Arity),
              relplan_shape(RelPlans, Name/Arity, _, Columns, _, _),
              order_by_sql(OrderCols, Columns, Sql) ),
            Pairs0),
    sort(Pairs0, Pairs).

order_by_sql(OrderCols, Columns, Sql) :-
    order_terms_sql(OrderCols, Columns, TermsSql),
    atomic_list_concat([' ORDER BY ', TermsSql], Sql).

order_terms_sql(OrderCols, Columns, TermsSql) :-
    maplist(order_term_sql(Columns), OrderCols, Terms),
    atomic_list_concat(Terms, ', ', TermsSql).

order_term_sql(Columns, order_col(Position, Direction), Term) :-
    nth1(Position, Columns, Column),
    quote_ident(Column, QuotedColumn),
    order_direction_sql(Direction, DirectionSql),
    atomic_list_concat([QuotedColumn, ' ', DirectionSql], Term).

order_direction_sql(asc, 'ASC').
order_direction_sql(desc, 'DESC').

% An index is a full copy of its key, so one is minted only where the ordered
% read reaches it: the BASE table, no UNIQUE already standing in that order.
query_order_index_ddls(Mode, Decls, RelPlans, EdgeHeadedRefs, ArrivalTargets,
                       Ddls) :-
    findall(Ref-OrderCols,
            ( member(QueryDecl, Decls),
              query_decl(QueryDecl, Atom, OrderCols),
              OrderCols \== [],
              functor(Atom, Name, Arity),
              Ref = Name/Arity ),
            Wanted0),
    sort(Wanted0, Wanted),
    group_pairs_by_key(Wanted, Grouped),
    findall(Ddl,
            ( member(Ref-OrderColsList, Grouped),
              relplan_shape(RelPlans, Ref, Kind, Columns, KeyOrNone,
                            ColumnTypes),
              nth1(Ordinal, OrderColsList, OrderCols),
              order_index_earns_its_write(Mode, Ref, Kind, Columns, KeyOrNone,
                                          ColumnTypes, EdgeHeadedRefs,
                                          ArrivalTargets, OrderCols),
              order_index_ddl(Ref, Ordinal, Columns, OrderCols, Ddl) ),
            Ddls).

% A rel with an interned or list column reads through a VIEW that decodes the
% value, so no index on the base table can order the characters it hands back.
order_index_earns_its_write(Mode, Ref, Kind, Columns, KeyOrNone, ColumnTypes,
                            EdgeHeadedRefs, ArrivalTargets, OrderCols) :-
    \+ any_interned_column(Mode, ColumnTypes),
    \+ ( member(ColumnType, ColumnTypes), ColumnType = list(_) ),
    \+ existing_unique_orders(Ref, Kind, Columns, KeyOrNone, EdgeHeadedRefs,
                              ArrivalTargets, OrderCols).

% A set rel's UNIQUE is a usable index; SQLite reads it forwards for an all-ASC
% prefix and backwards for an all-DESC one. A log rel is a plain rowid table.
existing_unique_orders(Ref, set, Columns, KeyOrNone, EdgeHeadedRefs,
                       ArrivalTargets, OrderCols) :-
    set_rel_key_positions(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargets,
                          Columns, KeyPositions),
    findall(Position, member(order_col(Position, _), OrderCols),
            OrderPositions),
    append(OrderPositions, _, KeyPositions),
    findall(Direction, member(order_col(_, Direction), OrderCols),
            Directions),
    sort(Directions, [_]).

order_index_ddl(Ref, Ordinal, Columns, OrderCols, Ddl) :-
    table_name(Ref, Table),
    quote_ident(Table, QuotedTable),
    format(atom(IndexName), '~w__order_~w', [Table, Ordinal]),
    quote_ident(IndexName, QuotedIndexName),
    order_terms_sql(OrderCols, Columns, TermsSql),
    format(atom(Ddl), 'CREATE INDEX ~w ON ~w (~w)',
           [QuotedIndexName, QuotedTable, TermsSql]).

% A stored rel's non-leading-key column some rule body compares by identity
% (==) against a literal or bound var: the composite UNIQUE key can't seek it.
audit_scan_index_pairs(RelPlans, Rules, EdgeHeadedRefs, ArrivalTargets, Pairs) :-
    findall(Ref-Column,
            audit_scan_index_pair(RelPlans, Rules, EdgeHeadedRefs,
                                  ArrivalTargets, Ref, Column),
            Pairs0),
    sort(Pairs0, Pairs).

audit_scan_index_pair(RelPlans, Rules, EdgeHeadedRefs, ArrivalTargets, Ref,
                      Column) :-
    member(Rule, Rules),
    rule_body_conjunction(Rule, Body),
    body_ref_uses(Body, Uses),
    member(use(Ref, Args, pos, _), Uses),
    relplan_shape(RelPlans, Ref, set, Columns, KeyOrNone, _),
    set_rel_key_positions(Ref, KeyOrNone, EdgeHeadedRefs, ArrivalTargets,
                          Columns, [Leading | _]),
    nth1(Position, Args, Arg),
    Position \== Leading,
    audit_scan_index_filtered(Arg, Body),
    nth1(Position, Columns, Column).

rule_body_conjunction((_ <- Body), Body).
rule_body_conjunction((_ <+ Body), Body).

% An inline literal argument compiles to the same WHERE equality as a
% `== Literal` guard (both feed compile_atom_args' bound-arg path).
audit_scan_index_filtered(Arg, _Body) :- atomic(Arg), !.
audit_scan_index_filtered(Arg, Body) :-
    var(Arg),
    body_guard_goals(Body, Goals),
    member(Left == Right, Goals),
    ( Arg == Left ; Arg == Right ).

audit_scan_index_ddls(RelPlans, Rules, EdgeHeadedRefs, ArrivalTargets, Ddls) :-
    audit_scan_index_pairs(RelPlans, Rules, EdgeHeadedRefs, ArrivalTargets,
                           Pairs),
    findall(Ddl,
            ( member(Ref-Column, Pairs), audit_scan_index_ddl(Ref, Column, Ddl) ),
            Ddls).

audit_scan_index_ddl(Ref, Column, Ddl) :-
    table_name(Ref, Table),
    quote_ident(Table, QuotedTable),
    format(atom(IndexName), '~w__scan_~w', [Table, Column]),
    quote_ident(IndexName, QuotedIndexName),
    quote_ident(Column, QuotedColumn),
    format(atom(Ddl), 'CREATE INDEX ~w ON ~w (~w)',
           [QuotedIndexName, QuotedTable, QuotedColumn]).

retention_statement(RelPlans, keep(Ref, count(Limit)),
                    retentionstmt(Ref, Limit, DeleteSql)) :-
    integer(Limit),
    Limit >= 0,
    relplan_shape(RelPlans, Ref, log, Columns, _, _),
    table_name(Ref, Table),
    quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(DeleteSql),
           'DELETE FROM ~w WHERE rowid NOT IN (SELECT rowid FROM ~w ORDER BY rowid DESC LIMIT ~w) RETURNING ~w',
           [QuotedTable, QuotedTable, Limit, ColumnsSql]).

retention_statements(Decls, RelPlans, RetentionStatements) :-
    findall(RetentionStatement,
            ( member(KeepDecl, Decls),
              KeepDecl = keep(_, count(_)),
              retention_statement(RelPlans, KeepDecl, RetentionStatement)
            ),
            RetentionStatements).

delta_ddl(Mode, DepartureRefs, RelPlan, Ddl) :-
    relplan_parts(RelPlan, Ref, _, Columns, _, ColumnTypes),
    delta_ddl(Mode, RelPlan, BaseDdl),
    (   memberchk(Ref, DepartureRefs)
    ->  departure_frontier_ddl(Ref, Columns, ColumnTypes, DepartureDdl),
        append(BaseDdl, DepartureDdl, Ddl)
    ;   Ddl = BaseDdl
    ).

% Columns match what the runtime stages, not what the rel stores: the staged
% rows are the boundary delta's, so trigger_read_mode/3 decides the encoding.
departure_frontier_ddl(Ref, Columns, ColumnTypes, [TableDdl]) :-
    departure_frontier_table_name(Ref, DepartureTable),
    quote_ident(DepartureTable, QuotedDepartureTable),
    trigger_read_mode(departure, dict, FrontierMode),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(FrontierMode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    format(atom(TableDdl),
           'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedDepartureTable, ColumnsSql]).

pre_ddl(Mode, RelPlans, Ref, Ddl) :-
    relplan_shape(RelPlans, Ref, _, Columns, KeyOrNone, ColumnTypes),
    pre_table_name(Ref, PreTable),
    quote_ident(PreTable, QuotedPreTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    ( KeyOrNone = key(KeyPositions)
    -> nth1_list(KeyPositions, Columns, KeyColumns),
       maplist(quote_ident, KeyColumns, QuotedKeyColumns),
       atomic_list_concat(QuotedKeyColumns, ', ', KeyColumnsSql),
       format(atom(Ddl),
              'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
              [QuotedPreTable, ColumnsSql, KeyColumnsSql])
    ;  format(atom(Ddl), 'CREATE TEMP TABLE ~w (~w)',
              [QuotedPreTable, ColumnsSql])
    ).

% No _phase index on the next-frontier: nothing filters that table by phase
% (0 of 747 emitted modules had a query plan choose one).
delta_ddl(Mode, RelPlan, Ddls) :-
    relplan_parts(RelPlan, Ref, _Kind, Columns, _, ColumnTypes),
    delta_table_name(Ref, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(['CREATE TEMP TABLE ', QuotedDeltaTable,
                        ' ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ',
                        ColumnsSql, ')'], TableDdl),
    atomic_list_concat([DeltaTable, '_sign'], IndexName),
    quote_ident(IndexName, QuotedIndexName),
    atomic_list_concat(['CREATE INDEX ', QuotedIndexName, ' ON ',
                        QuotedDeltaTable, ' ("_sign")'], IndexDdl),
    atomic_list_concat([DeltaTable, '_group'], GroupIndexName),
    quote_ident(GroupIndexName, QuotedGroupIndexName),
    atomic_list_concat(QuotedColumns, ', ', GroupColumnsSql),
    atomic_list_concat(['CREATE INDEX ', QuotedGroupIndexName, ' ON ',
                        QuotedDeltaTable, ' (', GroupColumnsSql, ')'],
                       GroupIndexDdl),
    frontier_family_ddl(Ref, Columns, ColumnsSql, FrontierFamilyDdl),
    text_view_ddls(Mode, DeltaTable, Columns, ColumnTypes,
                   ['_sign', '_sequence'], DeltaViewDdls),
    append([[TableDdl, IndexDdl, GroupIndexDdl], FrontierFamilyDdl,
            DeltaViewDdls], Ddls).

frontier_family_ddl(Ref, Columns, _ColumnsSql, ViewDdls) :-
    frontier_mode(shared),
    !,
    shared_frontier_view_ddl(Ref, Columns, ViewDdls).
frontier_family_ddl(Ref, _Columns, ColumnsSql,
                    [FrontierDdl, FrontierIndexDdl, NextFrontierDdl]) :-
    frontier_table_name(Ref, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    atomic_list_concat(['CREATE TEMP TABLE ', QuotedFrontierTable,
                        ' ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ',
                        ColumnsSql, ')'], FrontierDdl),
    atomic_list_concat([FrontierTable, '_phase'], FrontierIndexName),
    quote_ident(FrontierIndexName, QuotedFrontierIndexName),
    atomic_list_concat(['CREATE INDEX ', QuotedFrontierIndexName, ' ON ',
                        QuotedFrontierTable, ' ("_phase")'], FrontierIndexDdl),
    next_frontier_table_name(Ref, NextFrontierTable),
    quote_ident(NextFrontierTable, QuotedNextFrontierTable),
    atomic_list_concat(['CREATE TEMP TABLE ', QuotedNextFrontierTable,
                        ' ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ',
                        ColumnsSql, ')'], NextFrontierDdl).

% An aggregate head has no refCount table (aggsql/7 replaces the refCount
% family entirely -- level_statement_group/3's own comment), so it gets no
% refCount DDL either.
ref_count_ddl(_, _, levelstmt(_, _, _, _, none, _, _), []) :- !.
ref_count_ddl(Mode, RelPlans, levelstmt(HeadRef, _, _, _, RefCountSql, _, _), DdlList) :-
    ref_count_head_ddl(Mode, RelPlans, HeadRef, [Ddl, NewDdl, ZeroIndexDdl]),
    ( RefCountSql = refcountsql(_, _, _, _, _, _, _, _, _, _, _, ExpandPlan, DredPlan, _, _, _),
      ExpandPlan = expandplan(_, _, _, _, _, _, _, _)
    -> expand_wave_ddl(Mode, RelPlans, HeadRef, WaveDdl),
       dred_wave_ddl(Mode, RelPlans, HeadRef, DredPlan, DredDdl),
       append([[Ddl, NewDdl, ZeroIndexDdl], WaveDdl, DredDdl], DdlList)
    ;  DdlList = [Ddl, NewDdl, ZeroIndexDdl]
    ).

dred_wave_ddl(_, _, _, none, []) :- !.
dred_wave_ddl(Mode, RelPlans, HeadRef, _, [PingDdl, PongDdl, ConeDdl]) :-
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
    dred_ping_table_name(HeadRef, PingTable),
    dred_pong_table_name(HeadRef, PongTable),
    dred_cone_table_name(HeadRef, ConeTable),
    maplist(dred_wave_table_ddl(ColumnsSql, PrimaryKeySql),
            [PingTable, PongTable, ConeTable], [PingDdl, PongDdl, ConeDdl]).

dred_wave_table_ddl(ColumnsSql, PrimaryKeySql, TableName, Ddl) :-
    quote_ident(TableName, QuotedTable),
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedTable, ColumnsSql, PrimaryKeySql]).

expand_wave_ddl(Mode, RelPlans, HeadRef, [DdlA, DdlB]) :-
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def(Mode), QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
    expand_table_name(HeadRef, a, TableA),
    expand_table_name(HeadRef, b, TableB),
    quote_ident(TableA, QuotedTableA),
    quote_ident(TableB, QuotedTableB),
    format(atom(DdlA),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedTableA, ColumnsSql, PrimaryKeySql]),
    format(atom(DdlB),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedTableB, ColumnsSql, PrimaryKeySql]).

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
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w (~w, "__refcount" INTEGER NOT NULL, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedRefCountTable, ColumnsSql, PrimaryKeySql]),
    % Keeps its rowid: three staging reads use it as `_sequence`, and the set
    % is already distinct because the refCount table it drains has the key.
    arrival_scratch_table_name(HeadRef, NewTable),
    quote_ident(NewTable, QuotedNewTable),
    format(atom(NewDdl),
           'CREATE TEMP TABLE ~w (~w, "__refcount" INTEGER NOT NULL)',
           [QuotedNewTable, ColumnsSql]),
    % Partial, so it holds only rows the retraction pass is about to take and
    % costs nothing on an additive tick where no row falls to zero.
    format(atom(ZeroIndexName), '~w_zero', [HeadTable]),
    quote_ident(ZeroIndexName, QuotedZeroIndexName),
    format(atom(ZeroIndexDdl),
           'CREATE INDEX ~w ON ~w ("__refcount") WHERE "__refcount" <= 0',
           [QuotedZeroIndexName, QuotedHeadTable]).

% INTEGER columns cannot hold a json1 compound under the inferred storage
% contract, so their delta reads use the quoted column directly. TEXT columns
% retain the canonical Prolog term rendering: a json1-encoded compound
% (json_object('fn', F, 'args', json_array(A1, A2, ...))) becomes
% "F(A1,A2,...)"; anything else passes through unchanged. json_valid/1 plus
% json_type/1 = 'object' gates the compound branch because a bare
% numeric-looking atom like '123' is itself valid JSON. group_concat over
% json_each's '$.args' array renders any number of arguments in original order.
canonical_column_expr(Column, int, Expr) :-
    !,
    outer_column_expr(Column, Expr).
canonical_column_expr(Column, bool, Expr) :-
    !,
    outer_column_expr(Column, Expr).
canonical_column_expr(Column, float, Expr) :-
    !,
    outer_column_expr(Column, Expr).
canonical_column_expr(Column, bytes, Expr) :-
    !,
    outer_column_expr(Column, Expr).
% STRUCT-AS-ROWS, arc header Edge 1: a ref column reads its VALUE, never its
% id. The rendering was computed once at intern time, so this is one indexed
% probe per row and no recursion regardless of nesting depth.
canonical_column_expr(Column, ref(TypeName), Expr) :-
    !,
    dictionary_render_expr(TypeName, Column, Expr).
canonical_column_expr(Column, idref(_), Expr) :-
    !,
    outer_column_expr(Column, Expr).
% A json column's STORED TEXT is already the rendering. The cross-target log
% contract is canonical JSON (sorted keys, no whitespace), and json1 will not
% canonicalize for us at any point in the pipeline -- json() minifies but
% PRESERVES key order and json_group_object follows row order -- so
% canonicalization has to happen once, on the way in, where
% canonical_json_text/2 already does it for the oracle and the TS arrival seam
% does it for the emitter. Reading the column back through json() here would
% be a second, weaker canonicalizer that disagrees with the first.
canonical_column_expr(Column, json, Expr) :-
    !,
    outer_column_expr(Column, Expr).
% A list column's stored text is its own array text, rendered as-is like a
% json column's.
canonical_column_expr(Column, json_list(_), Expr) :-
    !,
    outer_column_expr(Column, Expr).
% The ELEMENTS are the boundary value; the entity id is storage, exactly as a
% ref column's `__id` is. The join delta_statement/3 adds supplies the alias.
canonical_column_expr(Column, list(_), Expr) :-
    !,
    quote_ident(Column, QuotedColumn),
    list_column_alias(Column, Alias),
    quote_ident(Alias, QuotedAlias),
    format(atom(Expr), 'coalesce(~w."value_text", \'[]\') AS ~w',
           [QuotedAlias, QuotedColumn]).
% THE GUARD TESTS FOR THE TAGGED TERM, not merely for an object. `json_valid`
% plus `json_type = 'object'` is true of EVERY json object, including one a
% program legitimately stores in a text column, and for those the THEN branch
% computes `NULL || '(' || ... || ')'` -- which is NULL, in a column the
% runtime's own IRowValue contract says is never null. Receipt (json_flex lab,
% 2026-07-30): a text column holding `{"a":1}` reached ticklog.ts as `null` and
% the run died with `Cannot read properties of null (reading '0')`.
%
% `$.fn` must be a text member and `$.args` an array member, which is exactly
% what the writer at :459 emits and nothing else has to be. The `coalesce`
% is the zero-argument case: `json_each` over `[]` returns no rows, so
% `group_concat` answers NULL and the whole concatenation collapses the same
% way -- the same defect one arity down, and the writer emits `json_array()`
% for a nullary functor, so it was reachable.
%
% What stays ambiguous, and is a named card rather than a fix: a text value
% that genuinely IS `{"fn":"x","args":[]}` still renders as `x()`. The tagged
% encoding has no reserved marker, so shape is all this expression can read.
canonical_column_expr(Column, text, Expr) :-
    outer_column_expr(Column, Outer),
    quote_ident(Column, QuotedColumn),
    format(atom(Expr),
           'CASE WHEN json_valid(~w) AND json_type(~w) = \'object\' AND json_type(~w, \'$.fn\') = \'text\' AND json_type(~w, \'$.args\') = \'array\' THEN json_extract(~w, \'$.fn\') || \'(\' || coalesce((SELECT group_concat(value, \',\') FROM json_each(~w, \'$.args\')), \'\') || \')\' ELSE ~w END AS ~w',
           [Outer, Outer, Outer, Outer, Outer, Outer, Outer, QuotedColumn]).

canonical_column_expr(Column, Expr) :-
    canonical_column_expr(Column, text, Expr).

% Every boundary read is a JOIN once a list column is present, so an outer
% column names its row; bare, `list_id` is ambiguous against the joined view.
outer_column_expr(Column, Expr) :-
    quote_ident(Column, QuotedColumn),
    atomic_list_concat(['t.', QuotedColumn], Expr).

% ═══ boot (initial seed only; round 2 drops __prev priming entirely, since
% there is no __prev table anymore) ══════════════════════════════════════
% engine.pl:run_program seeds Initial rows and computes the t=0 level closure
% BEFORE tick 1's state(...) even exists -- a non-tick step with no slot in
% IGenProgram (tick/2 only takes an arrivals batch, no "this is boot" flag).
% This compiler emits it as an EXTRA `boot: IBootStatement[]` field beyond
% the five IGenProgram names ("extend by adding fields, never renaming");
% who calls it and when is real seam friction -- v6/tsv2/scripts/
% run-emitted.ts (the reconciliation runner) now answers this concretely:
% it runs `boot` after DDL and before the tick fold, confirmed by that
% script's own header comment.

boot_seed_statement(Mode, Decls, Types, RelPlan, Initial, Statements) :-
    relplan_parts(RelPlan, Ref, log, Columns, _, ColumnTypes),
    !,
    boot_rows_statements(Mode, Decls, Types, 'INSERT INTO', Ref, Columns, ColumnTypes, Initial, Statements).
boot_seed_statement(Mode, Decls, Types, RelPlan, Initial, Statements) :-
    relplan_parts(RelPlan, Ref, set, Columns, _, ColumnTypes),
    boot_rows_statements(Mode, Decls, Types, 'INSERT OR IGNORE INTO', Ref, Columns, ColumnTypes, Initial, Statements).

boot_rows_statements(Mode, Decls, Types, Insert, Ref, Columns, ColumnTypes, Initial, Statements) :-
    findall(Group,
            ( member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Values],
              boot_row_statements(Mode, Decls, Types, Insert, Ref, Columns, ColumnTypes, Values, Group) ),
            Groups),
    append(Groups, Statements).

% STRUCT-AS-ROWS on the boot path. A seed row whose column is a ref carries a
% VALUE, and the dense id it must store does not exist until the dictionary
% row does. So one seed row becomes: the intern statements for every value in
% its ref columns (children before parents), then the row insert itself, whose
% ref parameter is a `SELECT "__id" ... WHERE <declared key>` subquery rather
% than a bind. Arrivals take the same shape at runtime; this is the same plan
% with the values known at compile time.
boot_row_statements(Mode, Decls, Types, Insert, Ref, Columns, ColumnTypes, Values, Statements) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    boot_column_slots(Mode, Decls, Types, ColumnTypes, Values, Descs, InternStatements),
    maplist(slot_desc_slot, Descs, Slots),
    slot_desc_params(Descs, Params),
    atomic_list_concat(Slots, ', ', SlotsSql),
    format(atom(Sql), '~w ~w (~w) VALUES (~w)', [Insert, QuotedTable, ColumnsSql, SlotsSql]),
    append(InternStatements, [bootstmt(Sql, Params)], Statements).

boot_column_slots(_, _, _, [], [], [], []).
boot_column_slots(Mode, Decls, Types, [ColumnType | ColumnTypes], [Value | Values],
                  [Desc | Descs], Statements) :-
    boot_column_slot(Mode, Decls, Types, ColumnType, Value, Desc, Group),
    boot_column_slots(Mode, Decls, Types, ColumnTypes, Values, Descs, More),
    append(Group, More, Statements).

boot_column_slot(Mode, Decls, Types, ColumnType, Value, slot_desc(Slot, Params), Statements) :-
    (   ColumnType = ref(TypeName)
    ->  struct_intern_statements(Mode, Decls, Types, TypeName, Value, Slot, Params, Statements)
    ;   interned_column(Mode, ColumnType)
    ->  text_intern_boot_statements(Value, Slot, Params, Statements)
    ;   % A json column stores canonical JSON TEXT, so an Initial seed row
        % binds the rendered text rather than the raw braces term. Same
        % canonicalizer, same reason as the arrival seam. A list column is the
        % same carrier, so it binds the rendered array text the same way.
        ( ColumnType == json ; ColumnType = json_list(_) )
    ->  canonical_json_text(Value, Text),
        Slot = '?', Params = [Text], Statements = []
    ;   Slot = '?', Params = [Value], Statements = []
    ).

% An Initial row bypasses the arrival door entirely, so under dict its text
% values reach an INTEGER column with nothing having interned them.
text_intern_boot_statements(Value, Slot, [Value], [bootstmt(InternSql, [Value])]) :-
    string_dictionary_table(Dictionary),
    quote_ident(Dictionary, QuotedDictionary),
    format(atom(InternSql),
           'INSERT OR IGNORE INTO ~w ("content") VALUES (?)', [QuotedDictionary]),
    format(atom(Slot),
           '(SELECT "__id" FROM ~w WHERE "content" = ?)', [QuotedDictionary]).

slot_desc_slot(slot_desc(Slot, _), Slot).

slot_desc_params([], []).
slot_desc_params([slot_desc(_, Params) | Rest], All) :-
    slot_desc_params(Rest, More),
    append(Params, More, All).

% Post-order: every referenced target insert precedes its parent's insert, so
% a parent's ref column can resolve by declared key against a row that already
% exists. The current type-cycle unsupported construct is what makes this terminate.
struct_intern_statements(Mode, Decls, Types, TypeName, Value, LookupSlot, LookupParams, Statements) :-
    type_definition(Types, TypeName, Columns, ColumnTypes),
    type_field_values(Types, TypeName, Value, FieldValues),
    boot_column_slots(Mode, Decls, Types, ColumnTypes, FieldValues, Descs, ChildStatements),
    maplist(slot_desc_slot, Descs, Slots),
    slot_desc_params(Descs, Params),
    length(Columns, Arity),
    table_name(TypeName/Arity, Table),
    quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    atomic_list_concat(Slots, ', ', SlotsSql),
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)',
           [QuotedTable, ColumnsSql, SlotsSql]),
    ( decl_key(Decls, TypeName/Arity, KeyPositions)
    -> true
    ;  numlist(1, Arity, KeyPositions)
    ),
    findall(KeyColumn, (member(Position, KeyPositions), nth1(Position, QuotedColumns, KeyColumn)), KeyColumns),
    findall(KeyDesc, (member(Position, KeyPositions), nth1(Position, Descs, KeyDesc)), KeyDescs),
    maplist(slot_desc_slot, KeyDescs, KeySlots),
    slot_desc_params(KeyDescs, LookupParams),
    findall(Equality,
            ( nth1(Index, KeyColumns, KeyColumn),
              nth1(Index, KeySlots, KeySlot),
              format(atom(Equality), '~w = ~w', [KeyColumn, KeySlot]) ),
            Equalities),
    atomic_list_concat(Equalities, ' AND ', WhereSql),
    format(atom(LookupSlot),
           '(SELECT "__id" FROM ~w WHERE ~w)',
           [QuotedTable, WhereSql]),
    append(ChildStatements, [bootstmt(Sql, Params)], Statements).

% ═══ top level ═══════════════════════════════════════════════════════════════

lower_program(Plan, Lowered) :-
    Plan = plan(_, _, _, RelPlans, _, _, _, _, _),
    with_storage_context(RelPlans,
        with_shared_frontier_ids(RelPlans,
            lower_program_in_context(Plan, Lowered))),
    shared_frontier_guard(Plan, Lowered).

lower_program_in_context(plan(Name, prog(Decls, Rules), LoweringTypes, RelPlans, ArrivalTargets, RuleOrder, EdgeRules, _SubscribedRels, Mode),
              lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)) :-
    findall(EdgeHeadedRef, ( member(EdgeRule, EdgeRules), rule_head_ref(EdgeRule, EdgeHeadedRef) ), EdgeHeadedRefs),
    findall(LevelHeadedRef,
            ( member(LevelRule, RuleOrder), rule_head_ref(LevelRule, LevelHeadedRef) ),
            LevelHeadedRefs),
    run_compile_step(lower, rel_ddl,
        maplist(rel_ddl(Mode, LoweringTypes, EdgeHeadedRefs, ArrivalTargets,
                        LevelHeadedRefs),
                RelPlans, RelationDdlGroups), _),
    run_compile_step(lower, listened_departure_refs,
        listened_departure_refs(Rules, DepartureRefs), _),
    run_compile_step(lower, delta_ddl,
        maplist(delta_ddl(Mode, DepartureRefs), RelPlans, DeltaDdlGroups), _),
    append(RelationDdlGroups, RelationDdl0),
    run_compile_step(lower, list_view_ddls,
        list_view_ddls(Mode, RelPlans, ListViewDdl), _),
    append(RelationDdl0, ListViewDdl, RelationDdl),
    append(DeltaDdlGroups, DeltaDdl),
    include(arrival_target_relplan(ArrivalTargets), RelPlans, ArrivalRelPlans),
    run_compile_step(lower, arrival_statement,
        maplist(arrival_statement, ArrivalRelPlans, ArrivalStatements), _),
    % One rule may lower to MULTIPLE edgestmt entries now (an unmarked or
    % sampled conjunction with N trigger atoms produces N arms), so this
    % maplist collects a GROUP per rule and flattens, rather than assuming
    % one-to-one.
    run_compile_step(lower, check_edge_rule_relation_values,
        maplist(check_edge_rule_relation_values(LoweringTypes, RelPlans),
                EdgeRules), _),
    run_compile_step(lower, edge_statements_for_rule,
        maplist(edge_statements_for_rule(Mode, EdgeHeadedRefs, RelPlans),
                EdgeRules, EdgeStatementGroups), _),
    append(EdgeStatementGroups, EdgeStatements),
    % STRUCT-AS-ROWS: level bodies are compiled against RelPlans PLUS the
    % dictionary plans, and with decode/2 already rewritten into dictionary
    % atoms. The dictionary plans reach the BODY compiler only -- never
    % rel_ddl/5, delta_statement/2, arrival_statement/2 or relColumns -- which
    % is Edge 2 enforced by construction rather than by a filter someone can
    % forget: a dictionary has no delta table to report and no name the tick
    % log can reach.
    run_compile_step(lower, dictionary_relplans,
        dictionary_relplans(LoweringTypes, DictionaryRelPlans), _),
    append(DictionaryRelPlans, RelPlans, BodyRelPlans),
    % Relation TERMS become dictionary atoms before decode/2 does, so a
    % variable this pass introduces is already a legal decode source when
    % decode_binding_type/5 goes looking for one. The reverse order would make
    % the two spellings non-composable for no reason.
    run_compile_step(lower, expand_relation_pattern_rules,
        expand_relation_pattern_rules(LoweringTypes, BodyRelPlans, RuleOrder,
                                      PatternedRuleOrder), _),
    run_compile_step(lower, expand_decode_rules,
        expand_decode_rules(LoweringTypes, BodyRelPlans, PatternedRuleOrder,
                            DecodedRuleOrder), _),
    run_compile_step(lower, level_statement_groups,
        level_statement_groups(Mode, BodyRelPlans, DecodedRuleOrder,
                               RuleLevelStatements), _),
    run_compile_step(lower, retention_statements,
        retention_statements(Decls, RelPlans, RetentionStatements), _),
    append(RuleLevelStatements, RetentionStatements, LevelStatements),
    run_compile_step(lower, ref_count_ddl,
        maplist(ref_count_ddl(Mode, RelPlans), RuleLevelStatements,
                RefCountDdlGroups), _),
    run_compile_step(lower, aggregate_scope_ddl,
        maplist(aggregate_scope_ddl(Mode), RuleLevelStatements,
                AggregateScopeDdlGroups), _),
    append(RefCountDdlGroups, RefCountDdl),
    append(AggregateScopeDdlGroups, AggregateScopeDdl),
    findall(PreRef,
            ( member((_ <+ EdgeBody), Rules),
              level_body_pre_ref(EdgeBody, PreRef) ),
            PreRefs0),
    sort(PreRefs0, PreRefs),
    run_compile_step(lower, pre_ddl,
        maplist(pre_ddl(Mode, RelPlans), PreRefs, PreDdl), _),
    program_uses_tick(prog(Decls, Rules), UsesTick),
    ( UsesTick == true -> tick_table_ddl(TickDdl) ; TickDdl = [] ),
    program_uses_catalog(prog(Decls, Rules), UsesCatalog),
    ( UsesCatalog == true
    -> catalog_table_ddl(CatalogTableDdl),
       % Ordering: must run after level_statement_groups (RuleLevelStatements is its input).
       catalog_row_ddl(Mode, Name, Rules, RelPlans, DepartureRefs, PreRefs,
                       LoweringTypes, RuleLevelStatements, Decls,
                       CatalogRowDdl)
    ;  CatalogTableDdl = [], CatalogRowDdl = [] ),
    % STRUCT-AS-ROWS: the dictionaries come FIRST in the DDL list, in
    % topological order, so a program's storage plane exists before any table
    % whose columns point into it.
    run_compile_step(lower, program_intern_ddl,
        program_intern_ddl(Mode, RelPlans, InternDdl), _),
    run_compile_step(lower, delta_statement,
        maplist(delta_statement(Mode), RelPlans, DeltaStatements), _),
    run_compile_step(lower, acyclic_guard_ddl,
        acyclic_guard_ddl(Decls, RelPlans, AcyclicDdl), _),
    run_compile_step(lower, query_order_index_ddls,
        query_order_index_ddls(Mode, Decls, RelPlans, EdgeHeadedRefs,
                               ArrivalTargets, OrderIndexDdl), _),
    run_compile_step(lower, audit_scan_index_ddls,
        audit_scan_index_ddls(RelPlans, Rules, EdgeHeadedRefs, ArrivalTargets,
                              AuditScanIndexDdl), _),
    append([RelationDdl, OrderIndexDdl, AuditScanIndexDdl, AcyclicDdl,
            DeltaDdl, RefCountDdl, AggregateScopeDdl, PreDdl, TickDdl,
            CatalogTableDdl, CatalogRowDdl],
           BodyDdl),
    run_compile_step(lower, literal_seed_ddl,
        literal_seed_ddl(Mode,
                         seeded(BodyDdl, ArrivalStatements, EdgeStatements,
                                LevelStatements, DeltaStatements),
                         SeedDdl), _),
    ( frontier_mode(shared) -> shared_frontier_ddl(SharedDdl) ; SharedDdl = [] ),
    append([InternDdl, SeedDdl, SharedDdl, BodyDdl], Ddl).

arrival_target_relplan(ArrivalTargets, RelPlan) :-
    relplan_parts(RelPlan, Ref, _, _, _, _),
    memberchk(Ref, ArrivalTargets).

% Constructs outside plan steps 1-4 refuse loudly under frontier(shared);
% each reason is a TODO site, never a language limit.
shared_frontier_guard(Plan, Lowered) :-
    (   frontier_mode(shared),
        shared_frontier_todo(Plan, Lowered, Reason)
    ->  throw(unsupported_construct(frontier_shared_todo(Reason)))
    ;   true
    ).

shared_frontier_todo(_, lowered(_, _, _, EdgeStatements, _, _, _, _),
                     edge_rules) :-
    EdgeStatements \== [].
shared_frontier_todo(_, lowered(_, _, _, _, LevelStatements, _, _, _),
                     retention) :-
    member(retentionstmt(_, _, _), LevelStatements).
shared_frontier_todo(_, lowered(_, _, _, _, LevelStatements, _, _, _),
                     aggregate_head) :-
    member(levelstmt(_, _, _, _, none, _, _), LevelStatements).
shared_frontier_todo(_, lowered(_, _, _, _, LevelStatements, _, _, _),
                     recursion) :-
    member(levelstmt(_, _, _, _, RefCountSql, _, _), LevelStatements),
    RefCountSql = refcountsql(_, _, _, _, _, _, _, _, _, _, _, ExpandPlan, _, _, _, _),
    ExpandPlan \== none.
shared_frontier_todo(plan(_, prog(_, Rules), _, _, _, _, _, _, _), _,
                     departure) :-
    listened_departure_refs(Rules, DepartureRefs),
    DepartureRefs \== [].
shared_frontier_todo(plan(_, _, _, RelPlans, _, _, _, _, _), _,
                     non_set_rel(Ref)) :-
    member(RelPlan, RelPlans),
    relplan_parts(RelPlan, Ref, Kind, _, _, _),
    Kind \== set.
shared_frontier_todo(plan(_, _, _, RelPlans, _, _, _, _, _), _,
                     bytes_column(Ref)) :-
    member(RelPlan, RelPlans),
    relplan_parts(RelPlan, Ref, _, _, _, ColumnTypes),
    memberchk(bytes, ColumnTypes).
shared_frontier_todo(plan(_, Prog, _, _, _, _, _, _, _), _, tick) :-
    program_uses_tick(Prog, true).
shared_frontier_todo(plan(_, prog(Decls, _), _, _, _, _, _, _, _), _, host) :-
    member(Decl, Decls),
    functor(Decl, sh_decl, _).

% ═══ the six write verbs ════════════════════════════════════════════════
% Every transient write a tick makes is one of six verbs. A relation row
% carries five of them, a rule row carries recount. arrive and publish name
% the durable and boundary SQL the compiler already specializes; stage and
% read_staged name the frontier a strategy owns, which is the ONE place
% per_rel and shared differ in text; clear names the tables a tick boundary
% empties. The rule join stays compiler-produced
% (plans/2026-08-19-shared-sqlite-frontier.md, Decisions), so recount carries
% its seed SQL rather than a rebuild recipe.
write_verb(arrive).
write_verb(stage).
write_verb(read_staged).
write_verb(recount).
write_verb(publish).
write_verb(clear).

lowered_program_data(Plan, Data) :-
    lower_program(Plan, Lowered),
    lowered_program_data(Plan, Lowered, Data).

lowered_program_data(Plan, Lowered,
                     program_data(Relations, Rules, [], [], [])) :-
    Plan = plan(_, _, _, RelPlans, _, RuleOrder, _, _, _),
    Lowered = lowered(_, _, ArrivalStatements, _, LevelStatements,
                      DeltaStatements, _, _),
    findall(relation_data(Id, Ref, Table, Columns, KeyOrNone, materialized,
                          Verbs),
            ( nth0(Id, RelPlans, RelPlan),
              relplan_parts(RelPlan, Ref, _, Columns, KeyOrNone, _),
              relplan_storage_name(RelPlan, Table),
              relation_write_verbs(RelPlans, Ref, Columns, ArrivalStatements,
                                   DeltaStatements, Verbs) ),
            Relations),
    findall(rule_data(RuleId, HeadId, InputIds, Verbs),
            ( nth0(RuleId, RuleOrder, Rule),
              rule_head_ref(Rule, HeadRef),
              relation_ref_index(RelPlans, HeadRef, HeadId),
              findall(InputId,
                      ( rule_body_ref(Rule, BodyRef),
                        relation_ref_index(RelPlans, BodyRef, InputId) ),
                      InputIds),
              rule_write_verbs(HeadRef, LevelStatements, Verbs) ),
            Rules).

relation_write_verbs(RelPlans, Ref, Columns, ArrivalStatements,
                     DeltaStatements,
                     [ verb(arrive, ArriveText),
                       verb(stage, StageTarget),
                       verb(read_staged, sql(ReadStagedSql)),
                       verb(publish, PublishText),
                       verb(clear, tables(ClearTables)) ]) :-
    (   memberchk(arrivalstmt(Ref, _, AddSql, _, _, _), ArrivalStatements)
    ->  ArriveText = sql(AddSql)
    ;   ArriveText = derived
    ),
    (   memberchk(deltastmt(Ref, _, _, BoundarySql, _), DeltaStatements)
    ->  PublishText = sql(BoundarySql)
    ;   PublishText = unobserved
    ),
    frontier_table_name(Ref, FrontierName),
    next_frontier_table_name(Ref, NextFrontierName),
    delta_table_name(Ref, DeltaTable),
    (   frontier_mode(shared)
    ->  shared_frontier_relation_id(RelPlans, Ref, RelationId),
        StageTarget = shared_frontier(RelationId),
        shared_frontier_table(SharedFrontier),
        shared_next_frontier_table(SharedNextFrontier),
        shared_support_table(SharedSupport),
        ClearTables = [DeltaTable, SharedNextFrontier, SharedFrontier,
                       SharedSupport]
    ;   StageTarget = frontier(FrontierName, NextFrontierName),
        ClearTables = [DeltaTable, NextFrontierName, FrontierName]
    ),
    % Identical text in both modes: under shared the name resolves to a TEMP
    % view over the shared table, which is what keeps every compiled read
    % byte-identical.
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    quote_ident(FrontierName, QuotedFrontierName),
    format(atom(ReadStagedSql),
           'SELECT "_phase", "_sequence", ~w FROM ~w',
           [ColumnsSql, QuotedFrontierName]).

rule_write_verbs(HeadRef, LevelStatements,
                 [verb(recount, recount(SeedSql, SupportCount))]) :-
    memberchk(levelstmt(HeadRef, _, _, _, RefCountSql, _, _), LevelStatements),
    RefCountSql = refcountsql(_, SeedSql, _, _, _, _, _, _, _, _, _, _, _, _,
                              _, SupportCountPlan),
    !,
    (   SupportCountPlan = supportcount(ClearSql, WriteSqls)
    ->  SupportCount = support_count(ClearSql, WriteSqls)
    ;   SupportCount = none
    ).
rule_write_verbs(_, _, [verb(recount, recount(none, none))]).

relation_ref_index(RelPlans, Ref, Index) :-
    nth0(Index, RelPlans, RelPlan),
    relplan_parts(RelPlan, Ref, _, _, _, _),
    !.

rule_body_ref(Rule, Ref) :-
    rule_body_of(Rule, Body),
    conjunction_goals(Body, Goals),
    member(Goal, Goals),
    Goal \= (\+ _),
    callable(Goal),
    functor(Goal, Name, Arity),
    Arity > 0,
    Ref = Name/Arity.

% Boot statements, computed on demand (needs Initial, which plan/6 does not
% carry -- compile.pl calls this directly with the fixture's Initial list).
% LevelStatements (from THIS SAME lower_program/2 call, Lowered's own field)
% seeds the t=0 level closure -- see boot_level_recompute_statements/2 below,
% surfaced as a real gap by PHASE C2 RULING 2's widening: the first fixture
% with both non-empty Initial data AND a level rule reading it
% (head_move_flips_current_tree_in_one_tick) only reached compilation once
% unmarked edge triggers were accepted, and its "before" snapshot was empty
% at tick 1 without this.
boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements,
                BootStatements) :-
    with_storage_context(RelPlans,
                         boot_statements_in_context(Mode, Decls, Types, RelPlans, Initial,
                                                    LevelStatements, BootStatements)).

boot_statements_in_context(Mode, Decls, Types, RelPlans, Initial, LevelStatements,
                BootStatements) :-
    run_compile_step(boot, boot_seed_statement_for,
        maplist(boot_seed_statement_for(Mode, Decls, Types, Initial),
                RelPlans, SeedGroups), _),
    append(SeedGroups, SeedStatements),
    run_compile_step(boot, boot_level_recompute_statements,
        boot_level_recompute_statements(LevelStatements,
                                        LevelBootStatements), _),
    append(SeedStatements, LevelBootStatements, BootStatements).

boot_seed_statement_for(Mode, Decls, Types, Initial, RelPlan, Statements) :-
    relplan_parts(RelPlan, Name/_, _, _, _, _),
    boot_seed_statement(Mode, Decls, Types, RelPlan, Initial, Statements0),
    tag_boot_statements(Name, Statements0, Statements).

% Every boot statement names the rel it exists for, which is what the emitted
% module's subscribe-cone filter reads. A seed row's struct-intern statements
% carry the PARENT rel: they exist only to make that row insertable, so
% dropping the parent must drop them with it. Most rels seed nothing, and the
% [] clause is what keeps the compile-speed ratchet from paying for them.
tag_boot_statements(_, [], []) :- !.
tag_boot_statements(Name, [bootstmt(Sql, Params) | Rest],
                    [bootstmt(Name, Sql, Params) | Tagged]) :-
    tag_boot_statements(Name, Rest, Tagged).

% engine.pl:run_program computes level_closure(Decls, PlainLevel, AggRules,
% BaseRows, 0, Level0) ONCE, immediately after seeding Initial rows and before tick 1's
% state(...) exists -- the SAME DELETE/INSERT-SELECT SQL recomputeLevels runs
% inside a tick (lower.pl:level_statement_group/3), run once more here with
% no bind params (a literal statement, not a template) so a level view over
% Initial-seeded data starts at its real t=0 rows rather than empty.
boot_level_recompute_statements(LevelStatements, BootStatements) :-
    findall(bootstmt(HeadName, Sql, []),
            ( member(LevelStatement, LevelStatements),
              LevelStatement = levelstmt(HeadName/_, _, _, _, _, _, _),
              boot_level_statement_sql(LevelStatement, Sql) ),
            BootStatements).

boot_level_statement_sql(levelstmt(_, _DeleteSql, _InsertSqls, _, _,
                                   avgsql(_, _, _, _, _, _, BootSqls), _), Sql) :- !,
    member(Sql, BootSqls).
boot_level_statement_sql(levelstmt(_, DeleteSql, InsertSqls, _, _, _, _), Sql) :-
    ( Sql = DeleteSql ; member(Sql, InsertSqls) ).
