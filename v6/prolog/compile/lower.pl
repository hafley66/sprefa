% lower.pl : rule -> SQL text. Compiles the plan/6 term compile.pl builds
% into a lowered/8 term:
%
%   lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements,
%           DeltaStatements, RelPlans, ArrivalTargets)
%     Ddl              : list of CREATE TABLE SQL strings.
%     RelPlans         : list of relplan(Ref, log|set, Columns, key(Ps)|none,
%                        ColumnTypes) -- ColumnTypes (PHASE C2 RULING 1) is a
%                        int|text list parallel to Columns, chosen upstream
%                        by analyze.pl:rel_column_types/7 from declaration
%                        authority when present, otherwise from the fixture's
%                        literal witnesses; column_def/3 below is the one
%                        place that reads it.
%     ArrivalStatements: list of arrivalstmt(Ref, Kind, AddSql, DelSqlOrNone,
%                        IncrementalAddSql, IncrementalDelSqlOrNone).
%     EdgeStatements   : list of edgestmt(HeadRef, TriggerRef, HeadColumns,
%                        KeyColumns, ProjectSql, WriteSql, DeltaProjectSql).
%     LevelStatements  : list of levelstmt(HeadRef, DeleteSql, InsertSqls,
%                        DeltaInsertSql, SupportSql),
%                        already in execution order (strat.pl:sql_rule_order/2).
%     DeltaStatements  : list of deltastmt(Ref, SelectAllSql, DeltaTable,
%                        BoundarySql). SelectAllSql preserves the recompute
%                        referee. DeltaTable and BoundarySql carry P1's
%                        tick-local change stream.
%
% plus boot_statements/4, a SEPARATE list of bootstmt(Sql, Params) (needs
% Initial, which plan/6 does not carry, plus LevelStatements for the t=0
% level closure -- PHASE C2 RULING 2, boot_level_recompute_statements/2).
%
% TARGET-NEUTRAL BY CONSTRUCTION (user directive, mid-arc: a future Rust
% backend must consume this unchanged): every field above is SQL text plus
% plain Prolog structure -- no TypeScript syntax, no rxjs, no host-language
% idiom anywhere in this file. `emit_ts.pl` is the ONE backend that renders
% this term. A future emit_rust.pl reads the identical lowered/8 +
% boot_statements/4 + RelPlans and renders sqlx/rusqlite calls around the
% SAME SQL strings -- SQLite is the shared middle language both backends
% speak; nothing here decides how a HOST assembles statements into a program.
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
% Log-rel DDL/arrivals dropped their stamp columns entirely. Reported as a
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
% ── keyed replace only binds edge writes, never arrivals ────────────────────
% engine.pl absorb_arrivals/8 never consults decl_key/1: an OUTSIDE arrival
% into a keyed Set rel is plain exact-row add/remove, same as an unkeyed Set
% rel. Only apply_edge_writes/6 does delete-old-then-insert-new BY KEY. So a
% table backing a keyed rel gets PRIMARY KEY over ALL declared columns
% (WITHOUT ROWID, exact-row identity, matching srow(Row) membership) --
% never PRIMARY KEY(key columns), which would conflate the two write paths
% onto one schema-level constraint. Key uniqueness for an edge-headed rel is
% enforced procedurally by the UPSERT's ON CONFLICT clause, never by the
% table schema.
%
% ── acyclic-by-construction level recompute (UNCHANGED from round 1) ───────
% engine.pl re-derives a whole stratum GROUP to a joint fixpoint because
% relax_strata's Gap=0 rule lets two positively-dependent rules land in the
% SAME stratum number. strat.pl verified both target fixtures collapse to
% exactly one such group; sql_rule_order/2 topo-sorts within it. A single
% DELETE-then-INSERT-SELECT pass per rule in that order computes the SAME
% rows a joint fixpoint would for an ACYCLIC chain; a genuine positive cycle
% inside one group is refused at strat.pl:topo_order_group/2.

:- module(lower, [ lower_program/2, boot_statements/4, relplan_kind/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(analyze).
:- use_module('../conformance/body', [rel_ref/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ identifiers ═════════════════════════════════════════════════════════════

table_name(Name/_Arity, Name).

delta_table_name(Name/_Arity, DeltaTable) :-
    format(atom(DeltaTable), '__delta_~w', [Name]).

frontier_table_name(Name/_Arity, FrontierTable) :-
    format(atom(FrontierTable), '__frontier_~w', [Name]).

next_frontier_table_name(Name/_Arity, NextFrontierTable) :-
    format(atom(NextFrontierTable), '__next_frontier_~w', [Name]).

support_table_name(Name/_Arity, SupportTable) :-
    format(atom(SupportTable), '__support_next_~w', [Name]).

quote_ident(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

sql_literal(Atom, Literal) :-
    atomic(Atom),
    ( number(Atom)
    -> format(atom(Literal), '~w', [Atom])
    ;  ( sub_atom(Atom, _, _, _, '\'')
       -> throw(unsupported_construct(quote_in_literal(Atom)))
       ; format(atom(Literal), '\'~w\'', [Atom]) )
    ).

% ═══ relplan lookup ══════════════════════════════════════════════════════════

relplan_columns(RelPlans, Ref, Columns) :- memberchk(relplan(Ref, _, Columns, _, _), RelPlans).
relplan_kind(RelPlans, Ref, Kind) :- memberchk(relplan(Ref, Kind, _, _, _), RelPlans).
relplan_column_types(RelPlans, Ref, ColumnTypes) :- memberchk(relplan(Ref, _, _, _, ColumnTypes), RelPlans).

% ═══ pattern-argument compiler (level-rule bodies; unchanged from round 1) ══
% compile_pattern_arg(Arg, ColumnExpr, Bound0, Bound, WhereParts, Mode)
% Mode = bind (level positive atom, may introduce new bindings) | check
% (negated atom, read-only: never introduces a binding, an unbound var there
% imposes no condition -- negation-as-failure over an unconstrained position).

compile_pattern_arg(Arg, ColumnExpr, Bound0, Bound, WhereParts, Mode) :-
    ( var(Arg)
    -> ( bound_lookup(Bound0, Arg, Existing)
       -> WhereParts = [pair(ColumnExpr, Existing)], Bound = Bound0
       ; Mode == bind
       -> WhereParts = [], Bound = [Arg-ColumnExpr | Bound0]
       ; WhereParts = [], Bound = Bound0
       )
    ; compound(Arg)
    -> Arg =.. [Functor | SubArgs],
       FnCheck = pair_lit(ColumnExpr, Functor),
       compile_sub_args(SubArgs, ColumnExpr, 0, Bound0, Bound, MoreWhere, Mode),
       WhereParts = [FnCheck | MoreWhere]
    ; atomic(Arg)
    -> WhereParts = [lit(ColumnExpr, Arg)], Bound = Bound0
    ; throw(unsupported_construct(pattern_arg(Arg)))
    ).

compile_sub_args([], _, _, Bound, Bound, [], _).
compile_sub_args([SubArg | Rest], ParentExpr, Index, Bound0, Bound, WhereParts, Mode) :-
    format(atom(SubExpr), 'json_extract(~w, \'$.args[~w]\')', [ParentExpr, Index]),
    compile_pattern_arg(SubArg, SubExpr, Bound0, Bound1, HereWhere, Mode),
    NextIndex is Index + 1,
    compile_sub_args(Rest, ParentExpr, NextIndex, Bound1, Bound, MoreWhere, Mode),
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
where_text(lit(Left, Value), Text) :- sql_literal(Value, Quoted), format(atom(Text), '~w = ~w', [Left, Quoted]).

% ═══ positive body-atom compilation (level rules only, round 2: edge rules
% no longer use this -- see compile_trigger_bound/2 below) ═══════════════

compile_positive_uses(RelPlans, Uses, Bound0, Bound, FromParts, WhereTexts) :-
    compile_positive_uses(RelPlans, Uses, 0, Bound0, Bound, FromParts, WhereParts),
    maplist(where_text, WhereParts, WhereTexts).

compile_positive_uses(_, [], _, Bound, Bound, [], []).
compile_positive_uses(RelPlans, [use(Ref, Args, pos, _) | Rest], Index, Bound0, Bound, [From | MoreFrom], WhereParts) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'b~w', [Index]),
    format(atom(From), '~w ~w', [QuotedTable, Alias]),
    relplan_columns(RelPlans, Ref, Columns),
    compile_atom_args(Args, Columns, Alias, Bound0, Bound1, HereWhere),
    NextIndex is Index + 1,
    compile_positive_uses(RelPlans, Rest, NextIndex, Bound1, Bound, MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

compile_atom_args([], [], _, Bound, Bound, []).
compile_atom_args([Arg | RestArgs], [Column | RestColumns], Alias, Bound0, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Arg, ColumnExpr, Bound0, Bound1, HereWhere, bind),
    compile_atom_args(RestArgs, RestColumns, Alias, Bound1, Bound, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

% ═══ negative body-atom compilation (NOT EXISTS; unchanged from round 1) ════

compile_negative_uses(RelPlans, Uses, Bound, NegTexts) :-
    compile_negative_uses(RelPlans, Uses, 0, Bound, NegTexts).

compile_negative_uses(_, [], _, _, []).
compile_negative_uses(RelPlans, [use(Ref, Args, neg, _) | Rest], Index, Bound, [Text | More]) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'n~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    compile_negative_atom_args(Args, Columns, Alias, Bound, WhereParts),
    maplist(where_text, WhereParts, WhereTexts),
    ( WhereTexts == []
    -> format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w)', [QuotedTable, Alias])
    ; atomic_list_concat(WhereTexts, ' AND ', Joined),
      format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w WHERE ~w)', [QuotedTable, Alias, Joined])
    ),
    NextIndex is Index + 1,
    compile_negative_uses(RelPlans, Rest, NextIndex, Bound, More).

compile_negative_atom_args([], [], _, _, []).
compile_negative_atom_args([Arg | RestArgs], [Column | RestColumns], Alias, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Arg, ColumnExpr, Bound, _BoundUnused, HereWhere, check),
    compile_negative_atom_args(RestArgs, RestColumns, Alias, Bound, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

% ═══ head expression compilation (unchanged from round 1; reused for BOTH
% level rules, via table-alias Bound, and edge rules, via numbered-
% placeholder Bound -- compile_head_expr/3 does not care where a bound
% variable's SQL text came from) ═════════════════════════════════════════

compile_head_expr(Arg, Bound, Sql) :-
    ( var(Arg)
    -> ( bound_lookup(Bound, Arg, Sql) -> true ; throw(unsupported_construct(unbound_head_var(Arg))) )
    ; compound(Arg)
    -> Arg =.. [Functor | SubArgs],
       maplist(compile_head_expr_bound(Bound), SubArgs, SubSqls),
       ( SubSqls == []
       -> format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array())', [Functor])
       ; atomic_list_concat(SubSqls, ', ', Joined),
         format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array(~w))', [Functor, Joined])
       )
    ; atomic(Arg)
    -> sql_literal(Arg, Sql)
    ; throw(unsupported_construct(head_expr(Arg)))
    ).

compile_head_expr_bound(Bound, Arg, Sql) :- compile_head_expr(Arg, Bound, Sql).

head_select_list(Head, Bound, ColumnAliases, SelectExprs) :-
    Head =.. [_ | Args],
    maplist(compile_head_expr_bound(Bound), Args, SelectExprs0),
    ( is_list(ColumnAliases)
    -> maplist(alias_select_expr, SelectExprs0, ColumnAliases, SelectExprs)
    ; SelectExprs = SelectExprs0
    ).

alias_select_expr(Expr, Alias, AliasedExpr) :- format(atom(AliasedExpr), '~w AS "~w"', [Expr, Alias]).

% ═══ DDL (round 2: no stamp columns, no __prev tables) ══════════════════════
%
% rel_ddl/3's third argument is the set of edge-headed refs. An edge-headed
% keyed rel's UPSERT (edge_statement/3's UpsertSql) targets `ON
% CONFLICT(<key columns>)`, and SQLite requires that clause to name a REAL
% constraint on EXACTLY that column set -- "ON CONFLICT clause does not
% match any PRIMARY KEY or UNIQUE constraint" is a genuine runtime error,
% caught running the emitted program against the real seam (reconciliation
% round 2), not a static analysis finding. A non-edge-headed Set rel (an
% arrival-target only, keyed or not) still gets PK = ALL columns: outside
% arrivals never consult decl_key/1 (absorb_arrivals/8 treats every Set rel
% as exact-row membership, matching srow(Row)), so an all-column PK is the
% right invariant there, and no ON CONFLICT clause ever targets it.

rel_ddl(_, _, relplan(Ref, log, Columns, _, ColumnTypes), [Ddl]) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    % Plain rowid table (no PK, no WITHOUT ROWID): a Log rel's duplicate rows
    % are distinct occurrences (engine.pl q1) and must physically coexist as
    % separate rows for multisetDiff to count them correctly.
    format(atom(Ddl), 'CREATE TABLE ~w (~w)', [QuotedTable, ColumnsSql]).
rel_ddl(EdgeHeadedRefs, LevelHeadedRefs,
        relplan(Ref, set, Columns, KeyOrNone, ColumnTypes), [Ddl]) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    ( memberchk(Ref, EdgeHeadedRefs), KeyOrNone = key(KeyPositions)
    -> nth1_list(KeyPositions, Columns, KeyColumns),
       maplist(quote_ident, KeyColumns, QuotedKeyColumns),
       atomic_list_concat(QuotedKeyColumns, ', ', PkSql)
    ;  atomic_list_concat(QuotedColumns, ', ', PkSql)
    ),
    ( memberchk(Ref, LevelHeadedRefs)
    -> SupportColumn = ', "__support_count" INTEGER NOT NULL DEFAULT 1'
    ;  SupportColumn = ''
    ),
    format(atom(Ddl), 'CREATE TABLE ~w (~w~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedTable, ColumnsSql, SupportColumn, PkSql]).

% PHASE C2 RULING 1: INTEGER storage for an int-typed column, TEXT for
% everything else (text columns and compound-term columns alike -- a
% compound value never gets an int witness, see analyze.pl:column_type_at/6,
% so it always falls through to text here, matching the ruling's flat-punt:
% compound-term columns stay inline-flat text, never their own storage
% type).
column_def(QuotedColumn, int, Def) :- !, format(atom(Def), '~w INTEGER NOT NULL', [QuotedColumn]).
column_def(QuotedColumn, text, Def) :- format(atom(Def), '~w TEXT NOT NULL', [QuotedColumn]).

% ═══ arrival statement templates (round 2: Log rel drops tick/seq params) ═══

arrival_statement(relplan(Ref, log, Columns, _, _),
                  arrivalstmt(Ref, log, AddSql, none, IncrementalAddSql, none)) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
    incremental_arrival_add_sql(log, QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql).
arrival_statement(relplan(Ref, set, Columns, _, _),
                  arrivalstmt(Ref, set, AddSql, DelSql, IncrementalAddSql, IncrementalDelSql)) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
    maplist(eq_placeholder, QuotedColumns, EqParts),
    atomic_list_concat(EqParts, ' AND ', WhereSql),
    format(atom(DelSql), 'DELETE FROM ~w WHERE ~w', [QuotedTable, WhereSql]),
    incremental_arrival_add_sql(set, QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql),
    incremental_json_select_exprs(QuotedColumns, 0, DeleteSelectExprs),
    atomic_list_concat(DeleteSelectExprs, ', ', DeleteSelectSql),
    format(atom(IncrementalDelSql),
           'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM json_each(?)) RETURNING ~w',
           [QuotedTable, ColumnsSql, DeleteSelectSql, ColumnsSql]).

incremental_arrival_add_sql(Kind, QuotedTable, ColumnsSql, QuotedColumns, Sql) :-
    incremental_json_select_exprs(QuotedColumns, 0, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    ( Kind == log -> Insert = 'INSERT INTO' ; Insert = 'INSERT OR IGNORE INTO' ),
    format(atom(Sql),
           '~w ~w (~w) SELECT ~w FROM json_each(?) RETURNING ~w',
           [Insert, QuotedTable, ColumnsSql, SelectSql, ColumnsSql]).

incremental_json_select_exprs([], _, []).
incremental_json_select_exprs([_ | Rest], Index, [Expr | More]) :-
    format(atom(Expr), 'json_extract(value, \'$[~w]\')', [Index]),
    NextIndex is Index + 1,
    incremental_json_select_exprs(Rest, NextIndex, More).

eq_placeholder(QuotedColumn, Text) :- format(atom(Text), '~w = ?', [QuotedColumn]).

placeholders(0, []) :- !.
placeholders(N, ['?' | Rest]) :- N > 0, N1 is N - 1, placeholders(N1, Rest).

% ═══ edge rule lowering ═══════════════════════════════════════════════════
% PHASE C2 RULING 2: a rule's body classifies via analyze.pl:edge_trigger_
% shape/2 into marked_single(TriggerAtom) (unchanged from round 2: exactly
% one trigger, no other body goal, TriggerAtom must be Log-kind) or
% unmarked_conjunction(Atoms) (N >= 1 plain positive atoms, no only/1
% anywhere -- engine.pl's unmarked fallback wraps EVERY one as its own
% independent trigger, body.pl:96-110/153-155). Lowering produces ONE
% edgestmt/6 PER CANDIDATE TRIGGER ATOM (edge_statements_for_rule/3): for
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

edge_statements_for_rule(RelPlans, (Head <+ Body), EdgeStatements) :-
    edge_trigger_shape(Body, Shape),
    ( Shape = marked_single(TriggerAtom)
    -> rel_ref(TriggerAtom, TriggerRef),
       ( relplan_kind(RelPlans, TriggerRef, log) -> true
       ; throw(unsupported_construct(edge_trigger_not_log(TriggerRef))) ),
       edge_statement_single(RelPlans, Head, TriggerAtom, [], EdgeStmt),
       EdgeStatements = [EdgeStmt]
    ; Shape = unmarked_conjunction(Atoms)
    -> findall(EdgeStmt,
               ( select(TriggerAtom, Atoms, OtherAtoms),
                 edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms, EdgeStmt) ),
               EdgeStatements)
    ).

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
edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms,
                      edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns,
                               ProjectSql, WriteSql, DeltaProjectSql)) :-
    rel_ref(TriggerAtom, TriggerRef),
    rel_ref(Head, HeadRef),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    ( HeadKind == set
    -> ( memberchk(relplan(HeadRef, set, _, key(KeyPositions), _), RelPlans) -> true
       ; throw(unsupported_construct(edge_into_unkeyed_set(HeadRef))) )
    ; true  % log: no key concept, KeyPositions unused below
    ),
    TriggerAtom =.. [_ | TriggerArgs],
    compile_trigger_bound(TriggerArgs, TriggerBound),
    ( OtherAtoms == []
    -> Bound = TriggerBound, FromSql = none, WhereSql = none
    ;  % maplist, NEVER findall (analyze.pl:ref_occurrence_args/3's own
       % comment names this exact hazard): findall copies its template per
       % solution, which would sever OtherArgs from the SAME variable
       % objects Head's arguments share -- head_select_list's bound_lookup
       % would then never find them, throwing unbound_head_var even though
       % the variables genuinely ARE bound (confirmed empirically: this
       % bug shipped in an earlier draft and unmarked_edge_replays_backlog
       % is the fixture that caught it).
       maplist(other_atom_use, OtherAtoms, OtherUses),
       compile_positive_uses(RelPlans, OtherUses, TriggerBound, Bound, FromParts, WhereTexts),
       atomic_list_concat(FromParts, ', ', FromSql),
       ( WhereTexts == [] -> WhereSql = none ; atomic_list_concat(WhereTexts, ' AND ', WhereSql) )
    ),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    % Aliased AS HeadColumns (not `none`, unlike a level rule's SELECT,
    % which has an explicit INSERT column list and does not need aliases):
    % the emitter reads one projected row back via named column access
    % (runtime/rows.ts's own `row[column]` idiom), and reconstructing
    % aliases by string surgery on an alias-free SELECT would be unsafe --
    % a json_object(...) expression's OWN internal commas would look
    % identical to expression-list separators to any naive re-splitter.
    head_select_list(Head, Bound, HeadColumns, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    ( FromSql == none
    -> format(atom(ProjectSql), 'SELECT ~w', [SelectSql])
    ; WhereSql == none
    -> format(atom(ProjectSql), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
    ; format(atom(ProjectSql), 'SELECT ~w FROM ~w WHERE ~w', [SelectSql, FromSql, WhereSql])
    ),
    edge_delta_project_sql(RelPlans, Head, TriggerAtom, OtherAtoms, HeadColumns, DeltaProjectSql),
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

edge_delta_project_sql(RelPlans, Head, TriggerAtom, OtherAtoms, HeadColumns, DeltaProjectSql) :-
    rel_ref(TriggerAtom, TriggerRef),
    TriggerAtom =.. [_ | TriggerArgs],
    frontier_table_name(TriggerRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    DeltaAlias = d0,
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    compile_atom_args(TriggerArgs, TriggerColumns, DeltaAlias, [], TriggerBound, TriggerWhereParts),
    maplist(where_text, TriggerWhereParts, TriggerWhereTexts),
    maplist(other_atom_use, OtherAtoms, OtherUses),
    compile_positive_uses(RelPlans, OtherUses, TriggerBound, Bound, OtherFromParts, OtherWhereTexts),
    head_select_list(Head, Bound, HeadColumns, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w ~w', [QuotedFrontierTable, DeltaAlias]),
    append([DeltaFrom], OtherFromParts, FromParts),
    atomic_list_concat(FromParts, ', ', FromSql),
    append(['d0."_phase" >= 0' | TriggerWhereTexts], OtherWhereTexts, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaProjectSql),
           'SELECT ~w FROM ~w WHERE ~w ORDER BY d0."_phase", d0."_sequence"',
           [SelectSql, FromSql, WhereSql]).

excluded_assignment(QuotedColumn, Text) :- format(atom(Text), '~w = excluded.~w', [QuotedColumn, QuotedColumn]).

other_atom_use(Atom, use(Ref, Args, pos, unmarked)) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].

% Numbered placeholders (?1, ?2, ...), one per trigger argument position, in
% TRIGGER-argument order -- the emitter passes `arrival.row` (already in
% that exact order, since a rel's stored row IS its declared column order)
% as the bind args UNCHANGED, so a head expression can reference the same
% trigger argument more than once (?1 reused) without the emitter needing
% to reorder or duplicate anything.
compile_trigger_bound(TriggerArgs, Bound) :- compile_trigger_bound(TriggerArgs, 1, Bound).
compile_trigger_bound([], _, []).
compile_trigger_bound([Arg | Rest], Index, [Arg-Placeholder | MoreBound]) :-
    ( var(Arg) -> true ; throw(unsupported_construct(trigger_arg_not_var(Arg))) ),
    format(atom(Placeholder), '?~w', [Index]),
    NextIndex is Index + 1,
    compile_trigger_bound(Rest, NextIndex, MoreBound).

nth1_list([], _, []).
nth1_list([Position | Rest], List, [Element | More]) :- nth1(Position, List, Element), nth1_list(Rest, List, More).

% ═══ level rule lowering ═════════════════════════════════════════════════════
% levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, SupportSql):
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

level_statement_groups(RelPlans, RuleOrder, LevelStatements) :-
    group_adjacent_by_head(RuleOrder, Groups),
    maplist(level_statement_group(RelPlans), Groups, LevelStatements).

group_adjacent_by_head([], []).
group_adjacent_by_head([Rule | Rest], [HeadRef-[Rule | SameHeadRest] | MoreGroups]) :-
    rule_head_ref(Rule, HeadRef),
    take_same_head(HeadRef, Rest, SameHeadRest, Remaining),
    group_adjacent_by_head(Remaining, MoreGroups).

take_same_head(HeadRef, [Rule | Rest], [Rule | SameRest], Remaining) :-
    rule_head_ref(Rule, HeadRef), !,
    take_same_head(HeadRef, Rest, SameRest, Remaining).
take_same_head(_, Rules, [], Rules).

level_statement_group(RelPlans, HeadRef-Rules,
                      levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql,
                                SupportSql)) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    format(atom(DeleteSql), 'DELETE FROM ~w', [QuotedHeadTable]),
    maplist(level_insert_sql(RelPlans, HeadRef), Rules, InsertSqls),
    level_delta_insert_sql(RelPlans, HeadRef, Rules, DeltaInsertSql),
    level_support_sql(RelPlans, HeadRef, Rules, SupportSql).

level_support_sql(RelPlans, HeadRef, Rules,
                  supportsql(ClearSql, SeedSql, UpdateSql, CollectZeroSql,
                             InsertNewSql)) :-
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    support_table_name(HeadRef, SupportTable),
    quote_ident(SupportTable, QuotedSupportTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    qualified_equalities(HeadColumns, n, h, EqualityParts),
    atomic_list_concat(EqualityParts, ' AND ', EqualitySql),
    format(atom(ClearSql), 'DELETE FROM ~w', [QuotedSupportTable]),
    ( rules_read_head_recursively(HeadRef, Rules)
    -> recursive_support_seed_sql(RelPlans, HeadRef, Rules,
                                  QuotedSupportTable, HeadColumns,
                                  HeadColumnsSql, SeedSql)
    ;  counted_support_seed_sql(RelPlans, Rules, QuotedSupportTable,
                                HeadColumnsSql, SeedSql)
    ),
    format(atom(UpdateSql),
           'UPDATE ~w AS h SET "__support_count" = "__support_count" - ("__support_count" - COALESCE((SELECT n."__support_count" FROM ~w n WHERE ~w), 0))',
           [QuotedHeadTable, QuotedSupportTable, EqualitySql]),
    format(atom(CollectZeroSql),
           'DELETE FROM ~w WHERE "__support_count" <= 0 RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql]),
    format(atom(InsertNewSql),
           'INSERT INTO ~w (~w, "__support_count") SELECT ~w, n."__support_count" FROM ~w n WHERE NOT EXISTS (SELECT 1 FROM ~w h WHERE ~w) RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, HeadColumnsSql,
            QuotedSupportTable, QuotedHeadTable, EqualitySql, HeadColumnsSql]).

counted_support_seed_sql(RelPlans, Rules, QuotedSupportTable,
                         HeadColumnsSql, SeedSql) :-
    maplist(level_support_arm(RelPlans), Rules, SupportArms),
    atomic_list_concat(SupportArms, ' UNION ALL ', SupportUnionSql),
    format(atom(SeedSql),
           'INSERT INTO ~w (~w, "__support_count") SELECT ~w, sum("__support_count") FROM (~w) GROUP BY ~w',
           [QuotedSupportTable, HeadColumnsSql, HeadColumnsSql,
            SupportUnionSql, HeadColumnsSql]).

recursive_support_seed_sql(RelPlans, HeadRef, Rules, QuotedSupportTable,
                           HeadColumns, HeadColumnsSql, SeedSql) :-
    partition(rule_reads_head(HeadRef), Rules, RecursiveRules, BaseRules),
    maplist(check_single_recursive_read(HeadRef), RecursiveRules),
    maplist(level_recursive_arm(RelPlans), BaseRules, BaseArms0),
    maplist(level_recursive_arm(RelPlans), RecursiveRules, RecursiveArms),
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
           'INSERT INTO ~w (~w, "__support_count") WITH RECURSIVE ~w (~w) AS (~w) SELECT ~w, 1 FROM ~w',
           [QuotedSupportTable, HeadColumnsSql, QuotedHeadTable,
            HeadColumnsSql, RecursiveUnionSql, HeadColumnsSql,
            QuotedHeadTable]).

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

level_recursive_arm(RelPlans, Rule, RecursiveArm) :-
    Rule = (Head <- Body),
    rule_head_ref(Rule, HeadRef),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(RelPlans, PosUses, [], Bound, FromParts, PosWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append(PosWhereTexts, NegWhereTexts, AllWhereTexts),
    atomic_list_concat(FromParts, ', ', FromSql),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    head_select_list(Head, Bound, HeadColumns, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    ( AllWhereTexts == []
    -> format(atom(RecursiveArm), 'SELECT ~w FROM ~w',
              [SelectSql, FromSql])
    ;  atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
       format(atom(RecursiveArm), 'SELECT ~w FROM ~w WHERE ~w',
              [SelectSql, FromSql, WhereSql])
    ).

level_support_arm(RelPlans, Rule, SupportArm) :-
    Rule = (Head <- Body),
    rule_head_ref(Rule, HeadRef),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(RelPlans, PosUses, [], Bound, FromParts, PosWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append(PosWhereTexts, NegWhereTexts, AllWhereTexts),
    atomic_list_concat(FromParts, ', ', FromSql),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    head_select_list(Head, Bound, HeadColumns, AliasedSelectExprs),
    head_select_list(Head, Bound, none, GroupExprs),
    atomic_list_concat(AliasedSelectExprs, ', ', SelectSql),
    atomic_list_concat(GroupExprs, ', ', GroupSql),
    ( AllWhereTexts == []
    -> format(atom(SupportArm),
              'SELECT ~w, count(*) AS "__support_count" FROM ~w GROUP BY ~w',
              [SelectSql, FromSql, GroupSql])
    ;  atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
       format(atom(SupportArm),
              'SELECT ~w, count(*) AS "__support_count" FROM ~w WHERE ~w GROUP BY ~w',
              [SelectSql, FromSql, WhereSql, GroupSql])
    ).

qualified_equalities([], _, _, []).
qualified_equalities([Column | Rest], LeftAlias, RightAlias,
                     [Equality | More]) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Equality), '~w.~w = ~w.~w',
           [LeftAlias, QuotedColumn, RightAlias, QuotedColumn]),
    qualified_equalities(Rest, LeftAlias, RightAlias, More).

level_insert_sql(RelPlans, HeadRef, (Head <- Body), InsertSql) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    ( PosUses == [] -> throw(unsupported_construct(level_rule_no_positive_body(HeadRef))) ; true ),
    compile_positive_uses(RelPlans, PosUses, [], Bound, FromParts, PosWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append(PosWhereTexts, NegWhereTexts, AllWhereTexts),
    atomic_list_concat(FromParts, ', ', FromSql),
    head_select_list(Head, Bound, none, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    ( AllWhereTexts == []
    -> format(atom(SelectStatement), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
    ; atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
      format(atom(SelectStatement), 'SELECT ~w FROM ~w WHERE ~w', [SelectSql, FromSql, WhereSql])
    ),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(InsertSql), 'INSERT OR IGNORE INTO ~w (~w) ~w', [QuotedHeadTable, HeadColumnsSql, SelectStatement]).

level_delta_insert_sql(RelPlans, HeadRef, Rules, DeltaInsertSql) :-
    table_name(HeadRef, HeadTable),
    quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    level_rules_delta_arms(RelPlans, Rules, DeltaArms),
    atomic_list_concat(DeltaArms, ' UNION ALL ', DeltaSelectSql),
    format(atom(DeltaInsertSql),
           'INSERT OR IGNORE INTO ~w (~w) ~w RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, DeltaSelectSql, HeadColumnsSql]).

level_rules_delta_arms(_, [], []).
level_rules_delta_arms(RelPlans, [Rule | Rest], DeltaArms) :-
    level_rule_delta_arms(RelPlans, Rule, RuleArms),
    level_rules_delta_arms(RelPlans, Rest, RestArms),
    append(RuleArms, RestArms, DeltaArms).

level_rule_delta_arms(RelPlans, (Head <- Body), DeltaArms) :-
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    level_positive_delta_arms(RelPlans, Head, PosUses, NegUses, PosUses, DeltaArms).

level_positive_delta_arms(_, _, [], _, _, []).
level_positive_delta_arms(RelPlans, Head, [_ | RestPositions], NegUses, PosUses,
                          [DeltaArm | RestArms]) :-
    length(RestPositions, RemainingCount),
    length(PosUses, PositiveCount),
    Position is PositiveCount - RemainingCount - 1,
    nth0_select(Position, PosUses, DeltaUse, OtherPosUses),
    level_delta_select_arm(RelPlans, Head, DeltaUse, OtherPosUses, NegUses, DeltaArm),
    level_positive_delta_arms(RelPlans, Head, RestPositions, NegUses, PosUses, RestArms).

nth0_select(0, [Selected | Rest], Selected, Rest) :- !.
nth0_select(Index, [Item | Rest], Selected, [Item | More]) :-
    Index > 0,
    NextIndex is Index - 1,
    nth0_select(NextIndex, Rest, Selected, More).

level_delta_select_arm(RelPlans, Head, use(DeltaRef, DeltaArgs, pos, _),
                       OtherPosUses, NegUses, DeltaArm) :-
    frontier_table_name(DeltaRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    compile_atom_args(DeltaArgs, DeltaColumns, d0, [], DeltaBound, DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    compile_positive_uses(RelPlans, OtherPosUses, DeltaBound, Bound,
                          OtherFromParts, OtherWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Head, Bound, none, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w d0', [QuotedFrontierTable]),
    append([DeltaFrom], OtherFromParts, FromParts),
    atomic_list_concat(FromParts, ', ', FromSql),
    append(['d0."_phase" >= 0' | DeltaWhereTexts], OtherWhereTexts, PositiveWhereTexts),
    append(PositiveWhereTexts, NegWhereTexts, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaArm), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectSql, FromSql, WhereSql]).

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

delta_statement(relplan(Ref, _Kind, Columns, _, ColumnTypes),
                deltastmt(Ref, SelectSql, DeltaTable, BoundarySql)) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(canonical_column_expr, Columns, ColumnTypes, ColumnExprs),
    atomic_list_concat(ColumnExprs, ', ', ColumnsSql),
    format(atom(SelectSql), 'SELECT ~w FROM ~w', [ColumnsSql, QuotedTable]),
    delta_table_name(Ref, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', GroupColumnsSql),
    format(atom(BoundarySql),
           'SELECT ~w, "_sign" AS "__sign", count(*) AS "__count" FROM ~w WHERE "_sign" IN (-1, 1) GROUP BY ~w, "_sign"',
           [ColumnsSql, QuotedDeltaTable, GroupColumnsSql]).

delta_ddl(relplan(Ref, _Kind, Columns, _, ColumnTypes),
          [TableDdl, IndexDdl, FrontierDdl, FrontierIndexDdl,
           NextFrontierDdl, NextFrontierIndexDdl]) :-
    delta_table_name(Ref, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    format(atom(TableDdl),
           'CREATE TEMP TABLE ~w ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedDeltaTable, ColumnsSql]),
    format(atom(IndexName), '~w_sign', [DeltaTable]),
    quote_ident(IndexName, QuotedIndexName),
    format(atom(IndexDdl),
           'CREATE INDEX ~w ON ~w ("_sign")',
           [QuotedIndexName, QuotedDeltaTable]),
    frontier_table_name(Ref, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    format(atom(FrontierDdl),
           'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedFrontierTable, ColumnsSql]),
    format(atom(FrontierIndexName), '~w_phase', [FrontierTable]),
    quote_ident(FrontierIndexName, QuotedFrontierIndexName),
    format(atom(FrontierIndexDdl),
           'CREATE INDEX ~w ON ~w ("_phase")',
           [QuotedFrontierIndexName, QuotedFrontierTable]),
    next_frontier_table_name(Ref, NextFrontierTable),
    quote_ident(NextFrontierTable, QuotedNextFrontierTable),
    format(atom(NextFrontierDdl),
           'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedNextFrontierTable, ColumnsSql]),
    format(atom(NextFrontierIndexName), '~w_phase', [NextFrontierTable]),
    quote_ident(NextFrontierIndexName, QuotedNextFrontierIndexName),
    format(atom(NextFrontierIndexDdl),
           'CREATE INDEX ~w ON ~w ("_phase")',
           [QuotedNextFrontierIndexName, QuotedNextFrontierTable]).

support_ddl(RelPlans, levelstmt(HeadRef, _, _, _, _), Ddl) :-
    support_table_name(HeadRef, SupportTable),
    quote_ident(SupportTable, QuotedSupportTable),
    relplan_columns(RelPlans, HeadRef, Columns),
    relplan_column_types(RelPlans, HeadRef, ColumnTypes),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PrimaryKeySql),
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w (~w, "__support_count" INTEGER NOT NULL, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedSupportTable, ColumnsSql, PrimaryKeySql]).

% INTEGER columns cannot hold a json1 compound under the inferred storage
% contract, so their delta reads use the quoted column directly. TEXT columns
% retain the canonical Prolog term rendering: a json1-encoded compound
% (json_object('fn', F, 'args', json_array(A1, A2, ...))) becomes
% "F(A1,A2,...)"; anything else passes through unchanged. json_valid/1 plus
% json_type/1 = 'object' gates the compound branch because a bare
% numeric-looking atom like '123' is itself valid JSON. group_concat over
% json_each's '$.args' array renders any number of arguments in original order.
canonical_column_expr(Column, int, QuotedColumn) :-
    !,
    quote_ident(Column, QuotedColumn).
canonical_column_expr(Column, text, Expr) :-
    quote_ident(Column, QuotedColumn),
    format(atom(Expr),
           'CASE WHEN json_valid(~w) AND json_type(~w) = \'object\' THEN json_extract(~w, \'$.fn\') || \'(\' || (SELECT group_concat(value, \',\') FROM json_each(~w, \'$.args\')) || \')\' ELSE ~w END AS ~w',
           [QuotedColumn, QuotedColumn, QuotedColumn, QuotedColumn, QuotedColumn, QuotedColumn]).

canonical_column_expr(Column, Expr) :-
    canonical_column_expr(Column, text, Expr).

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

boot_seed_statement(relplan(Ref, log, Columns, _, _), Initial, Statements) :- !,
    findall(bootstmt(Sql, Values),
            ( member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Values],
              table_name(Ref, Table), quote_ident(Table, QuotedTable),
              maplist(quote_ident, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              length(Columns, N), placeholders(N, Placeholders),
              atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
              format(atom(Sql), 'INSERT INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]) ),
            Statements).
boot_seed_statement(relplan(Ref, set, Columns, _, _), Initial, Statements) :-
    findall(bootstmt(Sql, Values),
            ( member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Values],
              table_name(Ref, Table), quote_ident(Table, QuotedTable),
              maplist(quote_ident, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              length(Columns, N), placeholders(N, Placeholders),
              atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
              format(atom(Sql), 'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]) ),
            Statements).

% ═══ top level ═══════════════════════════════════════════════════════════════

lower_program(plan(Name, prog(_Decls, _Rules), RelPlans, ArrivalTargets, RuleOrder, EdgeRules),
              lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)) :-
    findall(EdgeHeadedRef, ( member(EdgeRule, EdgeRules), rule_head_ref(EdgeRule, EdgeHeadedRef) ), EdgeHeadedRefs),
    findall(LevelHeadedRef,
            ( member(LevelRule, RuleOrder), rule_head_ref(LevelRule, LevelHeadedRef) ),
            LevelHeadedRefs),
    maplist(rel_ddl(EdgeHeadedRefs, LevelHeadedRefs), RelPlans, RelationDdlGroups),
    maplist(delta_ddl, RelPlans, DeltaDdlGroups),
    append(RelationDdlGroups, RelationDdl),
    append(DeltaDdlGroups, DeltaDdl),
    include(arrival_target_relplan(ArrivalTargets), RelPlans, ArrivalRelPlans),
    maplist(arrival_statement, ArrivalRelPlans, ArrivalStatements),
    % PHASE C2 RULING 2: one rule may lower to MULTIPLE edgestmt entries now
    % (an unmarked_conjunction body with N atoms produces N arms), so this
    % maplist collects a GROUP per rule and flattens, rather than assuming
    % one-to-one.
    maplist(edge_statements_for_rule(RelPlans), EdgeRules, EdgeStatementGroups),
    append(EdgeStatementGroups, EdgeStatements),
    level_statement_groups(RelPlans, RuleOrder, LevelStatements),
    maplist(support_ddl(RelPlans), LevelStatements, SupportDdl),
    append([RelationDdl, DeltaDdl, SupportDdl], Ddl),
    maplist(delta_statement, RelPlans, DeltaStatements).

arrival_target_relplan(ArrivalTargets, relplan(Ref, _, _, _, _)) :- memberchk(Ref, ArrivalTargets).

% Boot statements, computed on demand (needs Initial, which plan/6 does not
% carry -- compile.pl calls this directly with the fixture's Initial list).
% LevelStatements (from THIS SAME lower_program/2 call, Lowered's own field)
% seeds the t=0 level closure -- see boot_level_recompute_statements/2 below,
% surfaced as a real gap by PHASE C2 RULING 2's widening: the first fixture
% with both non-empty Initial data AND a level rule reading it
% (head_move_flips_current_tree_in_one_tick) only reached compilation once
% unmarked edge triggers were accepted, and its "before" snapshot was empty
% at tick 1 without this.
boot_statements(RelPlans, Initial, LevelStatements, BootStatements) :-
    maplist(boot_seed_statement_for(Initial), RelPlans, SeedGroups), append(SeedGroups, SeedStatements),
    boot_level_recompute_statements(LevelStatements, LevelBootStatements),
    append(SeedStatements, LevelBootStatements, BootStatements).

boot_seed_statement_for(Initial, RelPlan, Statements) :- boot_seed_statement(RelPlan, Initial, Statements).

% engine.pl:run_program computes level_closure(PlainLevel, AggRules, BaseRows,
% 0, Level0) ONCE, immediately after seeding Initial rows and before tick 1's
% state(...) exists -- the SAME DELETE/INSERT-SELECT SQL recomputeLevels runs
% inside a tick (lower.pl:level_statement_group/3), run once more here with
% no bind params (a literal statement, not a template) so a level view over
% Initial-seeded data starts at its real t=0 rows rather than empty.
boot_level_recompute_statements(LevelStatements, BootStatements) :-
    findall(bootstmt(Sql, []),
            ( member(levelstmt(_, DeleteSql, InsertSqls, _, _), LevelStatements),
              ( Sql = DeleteSql ; member(Sql, InsertSqls) ) ),
            BootStatements).
