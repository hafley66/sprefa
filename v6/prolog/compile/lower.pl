% lower.pl : rule -> SQL text. Compiles the plan/6 term compile.pl builds
% into a lowered/8 term:
%
%   lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements,
%           DeltaStatements, RelPlans, ArrivalTargets)
%     Ddl              : list of CREATE TABLE SQL strings.
%     RelPlans         : list of relplan(Ref, log|set, Columns, key(Ps)|none).
%     ArrivalStatements: list of arrivalstmt(Ref, Kind, AddSql, DelSqlOrNone).
%     EdgeStatements   : list of edgestmt(HeadRef, DeleteSql, InsertSql).
%     LevelStatements  : list of levelstmt(HeadRef, DeleteSql, InsertSql),
%                        already in execution order (strat.pl:sql_rule_order/2).
%     DeltaStatements  : list of deltastmt(Ref, Kind, AddsSql, DelsSqlOrNone,
%                        RefreshSqlList).
%
% plus boot_statements/3, a SEPARATE list of bootstmt(Sql, Params) (needs
% Initial, which plan/6 does not carry).
%
% TARGET-NEUTRAL BY CONSTRUCTION (user directive, mid-arc: a future Rust
% backend must consume this unchanged): every field above is SQL text plus
% plain Prolog structure -- no TypeScript syntax, no rxjs, no host-language
% idiom anywhere in this file. `emit_ts.pl` is the ONE backend that renders
% this term; it is the only module in v6/prolog/compile/ that imports or
% mentions rxjs/TypeScript syntax (verified: grep -niE "rxjs|Observable|
% interface |import \{" over analyze.pl/strat.pl/lower.pl/compile.pl finds
% zero code hits, only doc-comment mentions of the TARGET name). A future
% emit_rust.pl reads the identical lowered/8 + boot_statements/3 + RelPlans
% and renders sqlx/rusqlite calls (or whatever the Rust seam turns out to
% be) around the SAME SQL strings -- SQLite is the shared middle language
% both backends speak; nothing here decides how a HOST assembles statements
% into a program.
%
% ── representation of a structured column value ─────────────────────────────
% A term column (route_data(RouteId), obj(...), any compound) has no scalar
% SQL type, so it is stored as the SQLite json1 encoding
%   json_object('fn', <functor atom>, 'args', json_array(<arg exprs>))
% A body pattern that DESTRUCTURES a compound (route_view's
% `demanded(route_data(RouteId), _)`) compiles to a functor-tag equality
% (json_extract(col,'$.fn') = 'route_data') plus one json_extract(col,
% '$.args[N]') expression PER sub-argument position, which then binds like
% any other column expression. This is a compiler decision engine.pl does not
% make (it just unifies Prolog terms); SQLite's json1 extension is the
% concrete choice, verified working with sqlite3 3.43.2 (json_extract, exact
% json_object() text equality across two calls with identical arguments) --
% see the compile arc's SQL-check harness for the receipt.
%
% ── keyed replace only binds edge writes, never arrivals ────────────────────
% engine.pl absorb_arrivals/8 (line 182) never consults decl_key/1: an
% OUTSIDE arrival into a keyed Set rel is plain exact-row add/remove, same
% as an unkeyed Set rel. Only apply_edge_writes/6 (line 237) does
% delete-old-then-insert-new BY KEY. So a table backing a keyed rel gets
% PRIMARY KEY over ALL declared columns (WITHOUT ROWID, exact-row identity,
% matching srow(Row) membership) -- never PRIMARY KEY(key columns), which
% would conflate the two write paths onto one schema-level constraint. Key
% uniqueness for an edge-headed rel is enforced procedurally by the edge
% statement's DELETE-by-key that runs before its INSERT, never by the table
% schema. This is a compiler decision the header's IGenProgram sketch does
% not anticipate.
%
% ── acyclic-by-construction level recompute ──────────────────────────────────
% engine.pl re-derives a whole stratum GROUP to a joint fixpoint
% (level_eval.pl:plain_fixpoint) because relax_strata's Gap=0 rule lets two
% positively-dependent rules land in the SAME stratum number. strat.pl
% verified (against level_eval.pl itself, not reimplemented blind) that both
% target fixtures collapse to exactly one such group; sql_rule_order/2 then
% topo-sorts within it. A single DELETE-then-INSERT-SELECT pass per rule in
% that order computes the SAME rows a joint fixpoint would for an ACYCLIC
% chain (a second pass would add nothing new); a genuine positive cycle
% inside one group is refused at strat.pl:topo_order_group/2, not silently
% single-passed.

:- module(lower, [ lower_program/2, boot_statements/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(analyze).
:- use_module('../conformance/body', [rel_ref/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ identifiers ═════════════════════════════════════════════════════════════

table_name(Name/_Arity, Name).
prev_table_name(Name/_Arity, PrevName) :- format(atom(PrevName), '~w__prev', [Name]).

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

relplan_columns(RelPlans, Ref, Columns) :- memberchk(relplan(Ref, _, Columns, _), RelPlans).
relplan_kind(RelPlans, Ref, Kind) :- memberchk(relplan(Ref, Kind, _, _), RelPlans).

% ═══ pattern-argument compiler ═══════════════════════════════════════════════
% compile_pattern_arg(Arg, ColumnExpr, Bound0, Bound, WhereParts, Mode)
% Mode = bind (level/edge positive atom, may introduce new bindings) | check
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
% (an earlier version of this predicate did) unifies the output with the
% FIRST pair's value at head-unification time regardless of which branch
% fires, so every lookup past the first list element fails outright. This is
% exactly the bug class the descriptive-names style law exists to prevent.
bound_lookup([Var-PairExpr | Rest], Target, Expr) :-
    ( Var == Target -> Expr = PairExpr ; bound_lookup(Rest, Target, Expr) ).

where_text(pair(Left, Right), Text) :- format(atom(Text), '~w = ~w', [Left, Right]).
where_text(pair_lit(Left, Functor), Text) :-
    sql_literal(Functor, Quoted),
    format(atom(Text), 'json_extract(~w, \'$.fn\') = ~w', [Left, Quoted]).
where_text(lit(Left, Value), Text) :- sql_literal(Value, Quoted), format(atom(Text), '~w = ~w', [Left, Quoted]).

% ═══ positive body-atom compilation (level rules + edge trigger) ═══════════

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

% ═══ negative body-atom compilation (NOT EXISTS) ════════════════════════════

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

% ═══ head expression compilation ════════════════════════════════════════════

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

% ═══ DDL ═════════════════════════════════════════════════════════════════════

rel_ddl(relplan(Ref, log, Columns, _), [Ddl]) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    format(atom(Ddl),
           'CREATE TABLE ~w (tick INTEGER NOT NULL, seq INTEGER NOT NULL, ~w, PRIMARY KEY (tick, seq)) WITHOUT ROWID',
           [QuotedTable, ColumnsSql]).
rel_ddl(relplan(Ref, set, Columns, _), [MainDdl, PrevDdl]) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    prev_table_name(Ref, PrevTable), quote_ident(PrevTable, QuotedPrevTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', PkSql),
    format(atom(MainDdl), 'CREATE TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID', [QuotedTable, ColumnsSql, PkSql]),
    format(atom(PrevDdl), 'CREATE TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID', [QuotedPrevTable, ColumnsSql, PkSql]).

column_def(QuotedColumn, Def) :- format(atom(Def), '~w TEXT NOT NULL', [QuotedColumn]).

% ═══ arrival statement templates ═════════════════════════════════════════════

arrival_statement(relplan(Ref, log, Columns, _), arrivalstmt(Ref, log, AddSql, none)) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT INTO ~w (tick, seq, ~w) VALUES (?, ?, ~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]).
arrival_statement(relplan(Ref, set, Columns, _), arrivalstmt(Ref, set, AddSql, DelSql)) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
    maplist(eq_placeholder, QuotedColumns, EqParts),
    atomic_list_concat(EqParts, ' AND ', WhereSql),
    format(atom(DelSql), 'DELETE FROM ~w WHERE ~w', [QuotedTable, WhereSql]).

eq_placeholder(QuotedColumn, Text) :- format(atom(Text), '~w = ?', [QuotedColumn]).

placeholders(0, []) :- !.
placeholders(N, ['?' | Rest]) :- N > 0, N1 is N - 1, placeholders(N1, Rest).

% ═══ edge rule lowering ══════════════════════════════════════════════════════
% Supported shape only (analyze.pl:check_edge_rule_shape already refused
% anything wider): `Head <+ only(TriggerAtom)`, TriggerAtom a Log-kind EDB
% ref, Head a keyed Set ref (edge_into_unkeyed_set / edge_write_log_head are
% both refused here -- neither target fixture writes an edge head that is a
% Log rel or an unkeyed Set rel, so this compiler does not lower them yet).

edge_statement(RelPlans, (Head <+ only(TriggerAtom)), edgestmt(HeadRef, DeleteSql, InsertSql)) :-
    rel_ref(TriggerAtom, TriggerRef),
    ( relplan_kind(RelPlans, TriggerRef, log) -> true
    ; throw(unsupported_construct(edge_trigger_not_log(TriggerRef))) ),
    rel_ref(Head, HeadRef),
    ( relplan_kind(RelPlans, HeadRef, set) -> true
    ; throw(unsupported_construct(edge_write_log_head(HeadRef))) ),
    ( memberchk(relplan(HeadRef, set, _, key(KeyPositions)), RelPlans) -> true
    ; throw(unsupported_construct(edge_into_unkeyed_set(HeadRef))) ),
    table_name(TriggerRef, TriggerTable), quote_ident(TriggerTable, QuotedTriggerTable),
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    TriggerAtom =.. [_ | TriggerArgs],
    compile_positive_uses(RelPlans, [use(TriggerRef, TriggerArgs, pos, marked)], [], Bound, [_From], _WhereTexts),
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    nth1_list(KeyPositions, TriggerColumns, KeyColumns),
    maplist(quote_ident, KeyColumns, QuotedKeyColumns),
    % Outer DELETE reads the HEAD table's own key columns (unaliased); its
    % subquery is independent of Bound, so it is free to pick any alias ("t1")
    % for the trigger table it scans. The INSERT's SELECT list, in contrast,
    % is built from Bound, which compile_positive_uses/6 already aliased
    % "b0" for the (only) positive body atom -- the INSERT's own FROM/subquery
    % aliases MUST match "b0", or the SELECT list references a table alias
    % the FROM clause never introduces (a real bug an earlier draft of this
    % predicate had: it hardcoded "t1" here while Bound said "b0").
    atomic_list_concat(QuotedKeyColumns, ', ', HeadKeySql),
    key_in_subquery(QuotedKeyColumns, 't1', TriggerKeySql),
    format(atom(DeleteSql),
           'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM ~w t1 WHERE t1.tick = ?)',
           [QuotedHeadTable, HeadKeySql, TriggerKeySql, QuotedTriggerTable]),
    head_select_list(Head, Bound, none, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    key_equal_conditions('b0', 'b0_dup', KeyColumns, KeyJoinConditions),
    atomic_list_concat(KeyJoinConditions, ' AND ', KeyJoinSql),
    format(atom(LatestSeqSubquery),
           '(SELECT MAX(b0_dup.seq) FROM ~w b0_dup WHERE b0_dup.tick = b0.tick AND ~w)',
           [QuotedTriggerTable, KeyJoinSql]),
    format(atom(InsertSql),
           'INSERT INTO ~w (~w) SELECT ~w FROM ~w b0 WHERE b0.tick = ? AND b0.seq = ~w',
           [QuotedHeadTable, HeadColumnsSql, SelectSql, QuotedTriggerTable, LatestSeqSubquery]).

nth1_list([], _, []).
nth1_list([Position | Rest], List, [Element | More]) :- nth1(Position, List, Element), nth1_list(Rest, List, More).

% Column names here are RAW (unquoted) -- the format template supplies the
% quotes itself. Passing already-quote_ident'd atoms here double-quotes them
% (`t1.""session_id""`, invalid SQL) -- an earlier draft had exactly that bug.
key_equal_conditions(_, _, [], []).
key_equal_conditions(AliasLeft, AliasRight, [Column | Rest], [Condition | More]) :-
    format(atom(Condition), '~w."~w" = ~w."~w"', [AliasLeft, Column, AliasRight, Column]),
    key_equal_conditions(AliasLeft, AliasRight, Rest, More).

key_in_subquery(QuotedKeyColumns, Alias, Sql) :-
    maplist(alias_column(Alias), QuotedKeyColumns, Refs),
    atomic_list_concat(Refs, ', ', Sql).

alias_column(Alias, QuotedColumn, Ref) :- format(atom(Ref), '~w.~w', [Alias, QuotedColumn]).

% ═══ level rule lowering ═════════════════════════════════════════════════════

level_statement(RelPlans, (Head <- Body), levelstmt(HeadRef, DeleteSql, InsertSql)) :-
    rel_ref(Head, HeadRef),
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
    format(atom(DeleteSql), 'DELETE FROM ~w', [QuotedHeadTable]),
    format(atom(InsertSql), 'INSERT OR IGNORE INTO ~w (~w) ~w', [QuotedHeadTable, HeadColumnsSql, SelectStatement]).

is_positive_use(use(_, _, pos, _)).
is_negative_use(use(_, _, neg, _)).

% ═══ delta / snapshot statements ═════════════════════════════════════════════

delta_statement(relplan(Ref, log, Columns, _), deltastmt(Ref, log, AddsSql, none, [])) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(AddsSql), 'SELECT ~w FROM ~w WHERE tick = ? ORDER BY seq', [ColumnsSql, QuotedTable]).
delta_statement(relplan(Ref, set, Columns, _), deltastmt(Ref, set, AddsSql, DelsSql, [RefreshDeleteSql, RefreshInsertSql])) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    prev_table_name(Ref, PrevTable), quote_ident(PrevTable, QuotedPrevTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(AddsSql),
           'SELECT ~w FROM ~w EXCEPT SELECT ~w FROM ~w ORDER BY ~w',
           [ColumnsSql, QuotedTable, ColumnsSql, QuotedPrevTable, ColumnsSql]),
    format(atom(DelsSql),
           'SELECT ~w FROM ~w EXCEPT SELECT ~w FROM ~w ORDER BY ~w',
           [ColumnsSql, QuotedPrevTable, ColumnsSql, QuotedTable, ColumnsSql]),
    format(atom(RefreshDeleteSql), 'DELETE FROM ~w', [QuotedPrevTable]),
    format(atom(RefreshInsertSql), 'INSERT INTO ~w SELECT ~w FROM ~w', [QuotedPrevTable, ColumnsSql, QuotedTable]).

% ═══ boot (initial seed + baseline snapshot priming) ════════════════════════
% engine.pl:run_program seeds Initial rows and computes the t=0 level closure
% BEFORE tick 1's state(...) even exists (run_ticks starts at state(1, ...)
% with PrevAll already the closed, seeded baseline) -- a non-tick step with no
% slot in IGenProgram (tick/2 only takes an arrivals batch, no "this is boot"
% flag). This compiler emits it as an EXTRA `boot: IBootStatement[]` field
% beyond the five IGenProgram names, which is allowed ("extend by adding
% fields, never renaming") but is real seam friction: nothing in the header
% says who runs it or when relative to the first tick() call. Flagged in the
% arc report, not resolved unilaterally here.

boot_seed_statement(relplan(Ref, log, Columns, _), Initial, Statements) :- !,
    findall(bootstmt(Sql, Values),
            ( nth0(Index, Initial, Row), rel_ref(Row, Ref), Row =.. [_ | Values],
              table_name(Ref, Table), quote_ident(Table, QuotedTable),
              maplist(quote_ident, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              length(Columns, N), placeholders(N, Placeholders),
              atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
              format(atom(Sql), 'INSERT INTO ~w (tick, seq, ~w) VALUES (0, ~w, ~w)',
                     [QuotedTable, ColumnsSql, Index, PlaceholdersSql]) ),
            Statements).
boot_seed_statement(relplan(Ref, set, Columns, _), Initial, Statements) :-
    findall(bootstmt(Sql, Values),
            ( member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Values],
              table_name(Ref, Table), quote_ident(Table, QuotedTable),
              maplist(quote_ident, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              length(Columns, N), placeholders(N, Placeholders),
              atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
              format(atom(Sql), 'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]) ),
            Statements).

prime_snapshot_statement(relplan(Ref, set, Columns, _), bootstmt(Sql, [])) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    prev_table_name(Ref, PrevTable), quote_ident(PrevTable, QuotedPrevTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Sql), 'INSERT INTO ~w SELECT ~w FROM ~w', [QuotedPrevTable, ColumnsSql, QuotedTable]).

% ═══ top level ═══════════════════════════════════════════════════════════════

lower_program(plan(Name, prog(_Decls, _Rules), RelPlans, ArrivalTargets, RuleOrder, EdgeRules),
              lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)) :-
    maplist(rel_ddl, RelPlans, DdlGroups), append(DdlGroups, Ddl),
    include(arrival_target_relplan(ArrivalTargets), RelPlans, ArrivalRelPlans),
    maplist(arrival_statement, ArrivalRelPlans, ArrivalStatements),
    maplist(edge_statement(RelPlans), EdgeRules, EdgeStatements),
    maplist(level_statement(RelPlans), RuleOrder, LevelStatements),
    maplist(delta_statement, RelPlans, DeltaStatements).

arrival_target_relplan(ArrivalTargets, relplan(Ref, _, _, _)) :- memberchk(Ref, ArrivalTargets).

% Boot statements, computed on demand (needs Initial, which plan/6 does not
% carry -- compile.pl calls this directly with the fixture's Initial list).
boot_statements(RelPlans, Initial, BootStatements) :-
    maplist(boot_seed_statement_for(Initial), RelPlans, SeedGroups), append(SeedGroups, SeedStatements),
    include(is_set_relplan, RelPlans, SetRelPlans),
    maplist(prime_snapshot_statement, SetRelPlans, PrimeStatements),
    append(SeedStatements, PrimeStatements, BootStatements).

boot_seed_statement_for(Initial, RelPlan, Statements) :- boot_seed_statement(RelPlan, Initial, Statements).
is_set_relplan(relplan(_, set, _, _)).
