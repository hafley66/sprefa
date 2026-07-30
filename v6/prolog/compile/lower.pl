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
%                        DeltaInsertSql, SupportSql) and
%                        retentionstmt(Ref, Limit, DeleteSql),
%                        already in execution order (strat.pl:sql_rule_order/2).
%     DeltaStatements  : list of deltastmt(Ref, SelectAllSql, DeltaTable,
%                        BoundarySql). SelectAllSql preserves the recompute
%                        referee. DeltaTable and BoundarySql carry P1's
%                        tick-local change stream.
%
% plus boot_statements/5, a SEPARATE list of bootstmt(Sql, Params) (needs
% Initial, which plan/6 does not carry, plus LevelStatements for the t=0
% level closure -- PHASE C2 RULING 2, boot_level_recompute_statements/2).
%
% TARGET-NEUTRAL BY CONSTRUCTION (user directive, mid-arc: a future Rust
% backend must consume this unchanged): every field above is SQL text plus
% plain Prolog structure -- no TypeScript syntax, no rxjs, no host-language
% idiom anywhere in this file. `emit_ts.pl` is the ONE backend that renders
% this term. A future emit_rust.pl reads the identical lowered/8 +
% boot_statements/5 + RelPlans and renders sqlx/rusqlite calls around the
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
          [ lower_program/2, boot_statements/5, relplan_kind/3,
            % The expression-lowering seam, exported for the
            % expression_inventory unit (rank R5 of
            % plans/2026-07-29-prolog-org-review.md). That unit walks every
            % registry.pl expression/5 row and checks it lowers to the SQL the
            % row declares, which has to reach these two by name.
            compile_expr/4, compile_comparison/3,
            % The column-expression and refCount-SQL seams (rank R8). The
            % sql_text_snapshots and incremental_mode units pin the exact text
            % these produce; both were private qualified calls before.
            canonical_column_expr/2, level_support_sql/4,
            % The departure frontier's table name (TICK PHASE ALIGNMENT target
            % 2). emit_ts.pl renders both the relation-plan field and the
            % departure arm's SELECT, and the name has exactly one definition.
            departure_frontier_table_name/2, departure_read_sql/3,
            % STRUCT-AS-ROWS: the storage plane's own names, exported so the
            % emitter can render the intern plan and the plunit units can pin
            % the exact SQL text.
            dictionary_table_name/2, dictionary_render_expr/3,
            struct_type_plans/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(analyze).
:- use_module(registry, [expression/5, body_surface_for_term/6]).
:- use_module('../0_type_plane',
              [ type_definitions/2, type_definition/4, column_storage/3,
                type_topological_order/2, type_canonical_json/4,
                type_field_values/4, declared_type_name/2,
                relation_value_shape/3 ]).
:- use_module('../0_body_walk', [walk_body/3]).
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

pre_table_name(Name/_Arity, PreTable) :-
    format(atom(PreTable), '__pre_~w', [Name]).

% Last tick's net -delta rows of a rel some rule binds with finalize/1
% (engine.pl tick/7's DepartureCarry). Emitted ONLY for those rels
% (analyze:listened_departure_refs/2), which is what keeps a program with no
% finalize in it byte-identical to what the previous emitter wrote. Same
% column shape as the arrival frontier on purpose: the arm's delta SELECT is
% then the SAME text with one table name swapped, so no new SQL shape enters
% the emitter and the existing EXPLAIN receipts still describe it.
departure_frontier_table_name(Name/_Arity, DepartureTable) :-
    format(atom(DepartureTable), '__departure_frontier_~w', [Name]).

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

support_table_name(Name/_Arity, SupportTable) :-
    format(atom(SupportTable), '__support_next_~w', [Name]).

quote_ident(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

sql_literal(Atom, Literal) :-
    atomic(Atom),
    ( number(Atom)
    -> ( float(Atom)
       -> float_class(Atom, Class),
          ( memberchk(Class, [normal, subnormal, zero])
          -> format(atom(Literal), '~h', [Atom])
          ; throw(unsupported_construct(non_finite_float_literal(Atom))) )
       ; format(atom(Literal), '~w', [Atom]) )
    ;  ( sub_atom(Atom, _, _, _, '\'')
       -> throw(unsupported_construct(quote_in_literal(Atom)))
       ; format(atom(Literal), '\'~w\'', [Atom]) )
    ).
sql_literal(bool_lit(true), '1') :- !.
sql_literal(bool_lit(false), '0') :- !.

% ═══ relplan lookup ══════════════════════════════════════════════════════════

relplan_columns(RelPlans, Ref, Columns) :- memberchk(relplan(Ref, _, Columns, _, _), RelPlans).
relplan_kind(RelPlans, Ref, Kind) :- memberchk(relplan(Ref, Kind, _, _, _), RelPlans).
relplan_column_types(RelPlans, Ref, ColumnTypes) :- memberchk(relplan(Ref, _, _, _, ColumnTypes), RelPlans).

% ═══ pattern-argument compiler (level-rule bodies; unchanged from round 1) ══
% compile_pattern_arg(Arg, ColumnExpr, Bound0, Bound, WhereParts, Mode)
% Mode = bind (level positive atom, may introduce new bindings) | check
% (negated atom, read-only: never introduces a binding, an unbound var there
% imposes no condition -- negation-as-failure over an unconstrained position).

% EXPRESSION LIFT: a Bound entry is now typed(Sql, int|text), not bare Sql.
% lower.pl has to know a bound variable's SQL TYPE, not only its text, for
% three reasons the phase-C sweep documented as miscompiles: `/` is integer
% division only when both operands are INTEGER storage class, `mod` needs the
% floored correction only over integers, and a comparison between an
% INTEGER-affinity column and a TEXT one silently applies affinity conversion
% where the oracle's ==/2 is term identity. Carrying the type beside the text
% is what lets each of those be a NAMED refusal instead of a silent answer.
compile_pattern_arg(Arg, ColumnExpr, ColumnType, Bound0, Bound, WhereParts, Mode) :-
    ( var(Arg)
    -> ( bound_lookup(Bound0, Arg, typed(Existing, ExistingType))
       -> join_column_types_agree(ColumnExpr, ColumnType, Existing, ExistingType),
          WhereParts = [pair(ColumnExpr, Existing)], Bound = Bound0
       ; Mode == bind
       -> WhereParts = [], Bound = [Arg-typed(ColumnExpr, ColumnType) | Bound0]
       ; WhereParts = [], Bound = Bound0
       )
    ; Arg = bool_lit(_)
    -> WhereParts = [lit(ColumnExpr, Arg)], Bound = Bound0
    ; compound(Arg)
    -> Arg =.. [Functor | SubArgs],
       FnCheck = pair_lit(ColumnExpr, Functor),
       compile_sub_args(SubArgs, ColumnExpr, 0, Bound0, Bound, MoreWhere, Mode),
       WhereParts = [FnCheck | MoreWhere]
    ; atomic(Arg)
    -> WhereParts = [lit(ColumnExpr, Arg)], Bound = Bound0
    ; throw(unsupported_construct(pattern_arg(Arg)))
    ).

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
join_column_types_agree(ColumnExpr, ColumnType, Existing, ExistingType) :-
    throw(unsupported_construct(
        join_column_type_mismatch(ColumnExpr, ColumnType, Existing, ExistingType))).

% A destructured sub-argument comes back through json_extract, whose result
% carries no declared column type at all -- typed text, matching the
% inline-flat compound punt (PHASE C2 RULING 1).
compile_sub_args([], _, _, Bound, Bound, [], _).
compile_sub_args([SubArg | Rest], ParentExpr, Index, Bound0, Bound, WhereParts, Mode) :-
    format(atom(SubExpr), 'json_extract(~w, \'$.args[~w]\')', [ParentExpr, Index]),
    compile_pattern_arg(SubArg, SubExpr, text, Bound0, Bound1, HereWhere, Mode),
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
compile_positive_uses(RelPlans, [use(Ref, Args, pos, Source) | Rest], Index, Bound0, Bound, [From | MoreFrom], WhereParts) :-
    positive_use_table(Source, Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'b~w', [Index]),
    format(atom(From), '~w ~w', [QuotedTable, Alias]),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    compile_atom_args(Args, Columns, ColumnTypes, Alias, Bound0, FieldBound, HereWhere),
    bind_reference_target_identity(RelPlans, Ref, Args, Alias,
                                   FieldBound, Bound1),
    NextIndex is Index + 1,
    compile_positive_uses(RelPlans, Rest, NextIndex, Bound1, Bound, MoreFrom, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

positive_use_table(pre, Ref, Table) :- !, pre_table_name(Ref, Table).
positive_use_table(_, Ref, Table) :- table_name(Ref, Table).

% A public relation that appears as another relation's column domain has a
% hidden dense __id. Bind the complete body atom to that endpoint while its
% ordinary fields remain bound independently. A head value with the same
% relation-shaped term can then project the already-joined row identity
% instead of manufacturing JSON or performing a hidden target write.
bind_reference_target_identity(RelPlans, Name/Arity, Args, Alias,
                               Bound0, Bound) :-
    member(relplan(_, _, _, _, ColumnTypes), RelPlans),
    memberchk(ref(Name), ColumnTypes),
    !,
    length(Args, Arity),
    Atom =.. [Name | Args],
    format(atom(IdExpr), '~w."__id"', [Alias]),
    Bound = [Atom-typed(IdExpr, ref(Name)) | Bound0].
bind_reference_target_identity(_, _, _, _, Bound, Bound).

compile_atom_args([], [], [], _, Bound, Bound, []).
compile_atom_args([Arg | RestArgs], [Column | RestColumns], [ColumnType | RestTypes],
                  Alias, Bound0, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Arg, ColumnExpr, ColumnType, Bound0, Bound1, HereWhere, bind),
    compile_atom_args(RestArgs, RestColumns, RestTypes, Alias, Bound1, Bound, MoreWhere),
    append(HereWhere, MoreWhere, WhereParts).

% ═══ negative body-atom compilation (NOT EXISTS; unchanged from round 1) ════

compile_negative_uses(RelPlans, Uses, Bound, NegTexts) :-
    compile_negative_uses(RelPlans, Uses, 0, Bound, NegTexts).

compile_negative_uses(_, [], _, _, []).
compile_negative_uses(RelPlans, [use(Ref, Args, neg, _) | Rest], Index, Bound, [Text | More]) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    format(atom(Alias), 'n~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    compile_negative_atom_args(Args, Columns, ColumnTypes, Alias, Bound, WhereParts),
    maplist(where_text, WhereParts, WhereTexts),
    ( WhereTexts == []
    -> format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w)', [QuotedTable, Alias])
    ; atomic_list_concat(WhereTexts, ' AND ', Joined),
      format(atom(Text), 'NOT EXISTS (SELECT 1 FROM ~w ~w WHERE ~w)', [QuotedTable, Alias, Joined])
    ),
    NextIndex is Index + 1,
    compile_negative_uses(RelPlans, Rest, NextIndex, Bound, More).

compile_negative_atom_args([], [], [], _, _, []).
compile_negative_atom_args([Arg | RestArgs], [Column | RestColumns], [ColumnType | RestTypes],
                           Alias, Bound, WhereParts) :-
    format(atom(ColumnExpr), '~w."~w"', [Alias, Column]),
    compile_pattern_arg(Arg, ColumnExpr, ColumnType, Bound, _BoundUnused, HereWhere, check),
    compile_negative_atom_args(RestArgs, RestColumns, RestTypes, Alias, Bound, MoreWhere),
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
%        is why a non-int operand is a named refusal below rather than a
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
compile_expr(Expr, Bound, Sql, Type) :-
    ( var(Expr)
    -> ( bound_lookup(Bound, Expr, typed(Sql, Type))
       -> true
       ;  throw(unsupported_construct(unbound_head_var(Expr))) )
    ; Expr = bool_lit(_)
    -> sql_literal(Expr, Sql), Type = bool
    ; compound(Expr),
      bound_lookup(Bound, Expr, typed(Sql, Type))
    -> true
    ; integer(Expr)
    -> sql_literal(Expr, Sql), Type = int
    ; float(Expr)
    -> sql_literal(Expr, Sql), Type = float
    ; atomic(Expr)
    -> sql_literal(Expr, Sql), Type = text
    ; Expr = concat(Parts)
    -> compile_concat_parts(Parts, Bound, Expr, PartSqls),
       atomic_list_concat(PartSqls, ' || ', Joined),
       format(atom(Sql), '(~w)', [Joined]),
       Type = text
    ; text_scalar_expr(Expr, Function, Argument)
    -> compile_text_operand(Argument, Bound, Expr, ArgumentSql),
       text_scalar_sql(Function, ArgumentSql, Sql),
       Type = text
    ; arithmetic_expr(Expr, Operator, Left, Right)
    -> compile_numeric_operand(Operator, Left, Bound, Expr, LeftSql, LeftType),
       compile_numeric_operand(Operator, Right, Bound, Expr, RightSql, RightType),
       arithmetic_sql(Operator, LeftSql, RightSql, LeftType, RightType, Sql),
       arithmetic_result_type(Operator, LeftType, RightType, Type)
    ; json_value_expr(Expr)
    -> throw(unsupported_construct(json_value_expression(Expr)))
    ; compound(Expr)
    -> Expr =.. [Functor | SubArgs],
       maplist(compile_term_sub_expr(Bound), SubArgs, SubSqls),
       ( SubSqls == []
       -> format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array())', [Functor])
       ; atomic_list_concat(SubSqls, ', ', Joined),
         format(atom(Sql), 'json_object(\'fn\', \'~w\', \'args\', json_array(~w))', [Functor, Joined])
       ),
       Type = text
    ; throw(unsupported_construct(head_expr(Expr)))
    ).

compile_term_sub_expr(Bound, Arg, Sql) :- compile_expr(Arg, Bound, Sql, _Type).

compile_expr_bound(Bound, Arg, Sql) :- compile_expr(Arg, Bound, Sql, _Type).

% The operator inventory is registry.pl's expression/5 (rank R5 of
% plans/2026-07-29-prolog-org-review.md), not a local list.
arithmetic_expr(Expr, Operator, Left, Right) :-
    compound(Expr), Expr =.. [Operator, Left, Right],
    expression(Operator/2, arithmetic, _, _, _).

text_scalar_expr(Expr, Function, Argument) :-
    compound(Expr), Expr =.. [Function, Argument],
    expression(Function/1, text_scalar, _, _, _).

compile_text_operand(Operand, Bound, Whole, Sql) :-
    compile_expr(Operand, Bound, Sql, Type),
    ( Type == text
    -> true
    ;  throw(unsupported_construct(text_operand_not_text(Whole, Operand, Type)))
    ).

text_scalar_sql(Function, ArgumentSql, Sql) :-
    expression(Function/1, text_scalar, _, Rendering, _),
    text_scalar_rendering(Rendering, ArgumentSql, Sql).

% SQLite's @libsql seam has lower()/unicode(), but no scalar-function
% registration. The recursive scalar expression preserves V5 `normalize`.
text_scalar_rendering(ascii_alnum_lower, ArgumentSql, Sql) :-
    format(atom(Sql),
           '(WITH RECURSIVE "__norm_chars"("i", "c") AS (SELECT 1, substr(~w, 1, 1) UNION ALL SELECT "i" + 1, substr(~w, "i" + 1, 1) FROM "__norm_chars" WHERE "i" < length(~w)) SELECT coalesce(group_concat(lower("c"), \'\'), \'\') FROM "__norm_chars" WHERE (unicode("c") BETWEEN 48 AND 57) OR (unicode("c") BETWEEN 65 AND 90) OR (unicode("c") BETWEEN 97 AND 122))',
           [ArgumentSql, ArgumentSql, ArgumentSql]).

% The json arm's own VALUE grammar: a braces literal ({}/1) and a list. Both
% are ordinary compound terms structurally, and the generic compound branch
% below would happily wrap either in this compiler's json1 tagged-term
% encoding -- which is NOT what the oracle stores. engine.pl:json_canon/2
% canonicalizes a braces literal to obj(SortedPairs) and a list to a list,
% and the shared tick-log encoder then renders those as
% obj([|](-(name,cli),[|](-(stars,4),[]))) -- right-nested cons text, not a
% json1 object.
%
% MEASURED, not predicted: with only the bind lift in place and no refusal
% here, json_arm.pl's braces_literal_canonicalizes compiled clean and stored
% the text "null" where the oracle holds
% obj([|](-(name,cli),[|](-(stars,4),[]))), and braces_in_head_position (which
% the sweep has been calling IDENTICAL-but-vacuous since phase C, because its
% Schedule is empty) stored {}({"fn":":","args":["repo","cli"]}). The
% final-state leg is what surfaced both. Refused by name until the json arm
% is lowered as its own class; that is the SAME cons-text encoding gap
% registry.pl records against json_array/json_object.
json_value_expr(Expr) :- compound(Expr), Expr = {}(_), !.
json_value_expr(Expr) :- is_list(Expr), Expr \== [], !.
json_value_expr(Expr) :- compound(Expr), Expr = [_ | _].

compile_int_operand(Operand, Bound, Whole, Sql) :-
    compile_expr(Operand, Bound, Sql, Type),
    ( Type == int
    -> true
    ;  throw(unsupported_construct(arith_operand_not_int(Whole, Operand, Type)))
    ).

compile_numeric_operand(mod, Operand, Bound, Whole, Sql, int) :-
    !,
    compile_int_operand(Operand, Bound, Whole, Sql).
compile_numeric_operand(_, Operand, Bound, Whole, Sql, Type) :-
    compile_expr(Operand, Bound, Sql, Type),
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
compile_concat_parts(Parts, _, Whole, _) :-
    \+ is_list(Parts),
    throw(unsupported_construct(concat_not_a_list(Whole))).
compile_concat_parts(Parts, Bound, Whole, PartSqls) :-
    is_list(Parts),
    maplist(compile_concat_part(Bound, Whole), Parts, PartSqls).

compile_concat_part(Bound, Whole, Part, Sql) :-
    compile_expr(Part, Bound, Sql, Type),
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

compile_guard_goals(Goals, Bound0, Bound, WhereTexts) :-
    foldl(compile_guard_goal, Goals, Bound0-[], Bound-ReversedTexts),
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

compile_guard_goal(Goal, Bound0-Texts0, Bound-Texts) :-
    ( tick_goal(Goal, Variable)
    -> tick_column_sql(TickSql),
       ( \+ bound_lookup(Bound0, Variable, _)
       -> Bound = [Variable-typed(TickSql, int) | Bound0], Texts = Texts0
       ;  compile_expr(Variable, Bound0, VariableSql, _),
          format(atom(Text), '~w = ~w', [VariableSql, TickSql]),
          Bound = Bound0, Texts = [Text | Texts0]
       )
    ;  bind_goal(Goal, Variable, Expr)
    -> compile_expr(Expr, Bound0, Sql, Type),
       ( var(Variable), \+ bound_lookup(Bound0, Variable, _)
       -> Bound = [Variable-typed(Sql, Type) | Bound0], Texts = Texts0
       ;  compile_expr(Variable, Bound0, VariableSql, _VariableType),
          format(atom(Text), '~w = ~w', [VariableSql, Sql]),
          Bound = Bound0, Texts = [Text | Texts0]
       )
    ;  guard_goal(Goal)
    -> compile_comparison(Goal, Bound0, Text),
       Bound = Bound0, Texts = [Text | Texts0]
    ;  throw(unsupported_construct(guard_goal_shape(Goal)))
    ).

% engine.pl solve_comparison/1: `< =< > >=` run through eval_int2/4, so BOTH
% operands must be integers or the reference engine throws arith_on_non_int
% -- SQLite would instead compare a TEXT-affinity value against an INTEGER
% one under its own affinity rules and answer something. `==`/`\==` are
% eval_expr then Prolog ==/2, term identity, so `1 == '1'` is FALSE there;
% SQLite's `=` between an INTEGER column and a TEXT one applies affinity and
% can answer TRUE. Both cases are refused by name rather than lowered to an
% operator that means something else.
compile_comparison(Goal, Bound, Text) :-
    Goal =.. [Operator, Left, Right],
    compile_expr(Left, Bound, LeftSql, LeftType),
    compile_expr(Right, Bound, RightSql, RightType),
    comparison_operator_sql(Operator, Goal, LeftType, RightType, OperatorSql),
    format(atom(Text), '(~w ~w ~w)', [LeftSql, OperatorSql, RightSql]).

% Family, SQL text and type rule all come from registry.pl's expression/5
% (rank R5). The two type rules are named there: both_int for the ordered
% comparisons (the Int-only law), same_type for the identity ones. The
% refusal terms are unchanged, and an operator with no row still refuses by
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

head_select_list(Head, Bound, ColumnAliases, SelectExprs) :-
    Head =.. [_ | Args],
    maplist(compile_expr_bound(Bound), Args, SelectExprs0),
    ( is_list(ColumnAliases)
    -> maplist(alias_select_expr, SelectExprs0, ColumnAliases, SelectExprs)
    ; SelectExprs = SelectExprs0
    ).

alias_select_expr(Expr, Alias, AliasedExpr) :- format(atom(AliasedExpr), '~w AS "~w"', [Expr, Alias]).

% ═══ DDL (round 2: no stamp columns, no __prev tables) ══════════════════════
%
% rel_ddl/5 receives the edge-headed, arrival-target, and level-headed refs.
% An edge-headed keyed rel's UPSERT targets `ON CONFLICT(<key columns>)`, and
% SQLite requires that clause to name a constraint on exactly that column
% set. A keyed arrival target needs the same key constraint because
% absorb_set_arrival/5 replaces by key. An unkeyed arrival target retains the
% all-column primary key used for exact-row Set membership.

rel_ddl(_, _, _, _, relplan(Ref, log, Columns, _, ColumnTypes), [Ddl]) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    % Plain rowid table (no PK, no WITHOUT ROWID): a Log rel's duplicate rows
    % are distinct occurrences (engine.pl q1) and must physically coexist as
    % separate rows for multisetDiff to count them correctly.
    format(atom(Ddl), 'CREATE TABLE ~w (~w)', [QuotedTable, ColumnsSql]).
rel_ddl(Types, EdgeHeadedRefs, ArrivalTargetRefs, LevelHeadedRefs,
        relplan(Ref, set, Columns, KeyOrNone, ColumnTypes), Ddls) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedColumns, ', ', SelectColumnsSql),
    ( ( memberchk(Ref, EdgeHeadedRefs) ; memberchk(Ref, ArrivalTargetRefs) ),
      KeyOrNone = key(KeyPositions)
    -> nth1_list(KeyPositions, Columns, KeyColumns),
       maplist(quote_ident, KeyColumns, QuotedKeyColumns),
       atomic_list_concat(QuotedKeyColumns, ', ', PkSql)
    ;  atomic_list_concat(QuotedColumns, ', ', PkSql)
    ),
    ( memberchk(Ref, LevelHeadedRefs)
    -> SupportColumn = ', "__support_count" INTEGER NOT NULL DEFAULT 1'
    ;  SupportColumn = ''
    ),
    Ref = Name/_,
    ( declared_type_name(Types, Name)
    -> format(atom(Ddl),
              'CREATE TABLE ~w ("__id" INTEGER PRIMARY KEY, ~w~w, UNIQUE (~w))',
              [QuotedTable, ColumnsSql, SupportColumn, PkSql]),
       dictionary_table_name(Name, ReferenceView),
       quote_ident(ReferenceView, QuotedReferenceView),
       relation_render_expr(Types, Columns, ColumnTypes, RenderExpr),
       format(atom(ViewDdl),
              'CREATE TEMP VIEW ~w AS SELECT t."__id", ~w, ~w AS "__rendered" FROM ~w t',
              [QuotedReferenceView, SelectColumnsSql, RenderExpr, QuotedTable]),
       Ddls = [Ddl, ViewDdl]
    ;  format(atom(Ddl),
              'CREATE TABLE ~w (~w~w, PRIMARY KEY (~w)) WITHOUT ROWID',
              [QuotedTable, ColumnsSql, SupportColumn, PkSql]),
       Ddls = [Ddl]
    ).

% PHASE C2 RULING 1: INTEGER storage for an int-typed column, TEXT for
% everything else (text columns and compound-term columns alike -- a
% compound value never gets an int witness, see analyze.pl:column_type_at/6,
% so it always falls through to text here, matching the ruling's flat-punt:
% compound-term columns stay inline-flat text, never their own storage
% type).
column_def(QuotedColumn, int, Def) :- !, format(atom(Def), '~w INTEGER NOT NULL', [QuotedColumn]).
column_def(QuotedColumn, bool, Def) :- !,
    format(atom(Def), '~w INTEGER NOT NULL CHECK (~w IN (0,1))',
           [QuotedColumn, QuotedColumn]).
column_def(QuotedColumn, float, Def) :- !,
    format(atom(Def),
           '~w REAL NOT NULL CHECK (typeof(~w) = \'real\' AND ~w BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308)',
           [QuotedColumn, QuotedColumn, QuotedColumn]).
% A ref column stores the dense target-row id and nothing else. No FOREIGN
% KEY clause and no ON DELETE clause: the retraction
% lab measured SQL cascade deleting a shared child out from under a live
% second parent and leaving dangling refs (types-as-rels verdict finding 6,
% plans/2026-07-28-sqlite-retraction-verdict.md fk_cascade WRONG).
column_def(QuotedColumn, ref(_), Def) :- !, format(atom(Def), '~w INTEGER NOT NULL', [QuotedColumn]).
column_def(QuotedColumn, text, Def) :- format(atom(Def), '~w TEXT NOT NULL', [QuotedColumn]).

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
    atomic_list_concat(['__ref_', TypeName], Table).

relation_render_expr(Types, Columns, ColumnTypes, Expr) :-
    pairs_keys_values(Pairs, Columns, ColumnTypes),
    findall(Part,
            ( member(Column-ColumnType, Pairs),
              sql_literal(Column, ColumnLiteral),
              relation_render_column_expr(Types, Column, ColumnType, ValueExpr),
              format(atom(Part), '~w, ~w', [ColumnLiteral, ValueExpr]) ),
            Parts),
    atomic_list_concat(Parts, ', ', PartsSql),
    format(atom(Expr), 'json_object(~w)', [PartsSql]).

relation_render_column_expr(_, Column, ref(TypeName), Expr) :-
    !,
    quote_ident(Column, QuotedColumn),
    dictionary_table_name(TypeName, ReferenceView),
    quote_ident(ReferenceView, QuotedReferenceView),
    format(atom(Expr),
           'json((SELECT c."__rendered" FROM ~w c WHERE c."__id" = t.~w))',
           [QuotedReferenceView, QuotedColumn]).
relation_render_column_expr(_, Column, _, Expr) :-
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
dictionary_render_expr(TypeName, Column, Expr) :-
    dictionary_table_name(TypeName, Table),
    quote_ident(Table, QuotedTable),
    quote_ident(Column, QuotedColumn),
    format(atom(Expr),
           '(SELECT d."__rendered" FROM ~w d WHERE d."__id" = ~w) AS ~w',
           [QuotedTable, QuotedColumn, QuotedColumn]).

% The per-type plan the emitter hands the runtime, in TOPOLOGICAL order:
% children before parents, so the post-order intern is one pass down the list
% and a parent's ref columns are already resolved when its own statement runs.
%
% Two statements per type per tick, both set-based over one json_each(?)
% parameter, so the statement count is FLAT in the number of arriving values
% (v6/tsv2/tests/structPlane.test.ts is the count receipt). The N+1 law is
% structural here, not a lint: there is no per-row shape to fall into.
struct_type_plans(Decls, Plans) :-
    type_definitions(Decls, Types),
    (   Types == []
    ->  Plans = []
    ;   type_topological_order(Types, Ordered),
        findall(Plan, ( member(TypeName, Ordered),
                        struct_type_plan(Decls, Types, TypeName, Plan) ), Plans)
    ).

struct_type_plan(Decls, Types, TypeName,
                 structtype(TypeName, Columns, RefTypes, KeyIndices,
                            ConflictSql, InternSql, LookupSql)) :-
    type_definition(Types, TypeName, Columns, ColumnTypes),
    length(Columns, Arity),
    ( decl_key(Decls, TypeName/Arity, KeyPositions)
    -> true
    ;  numlist(1, Arity, KeyPositions)
    ),
    maplist(one_based_to_zero_based, KeyPositions, KeyIndices),
    maplist(dictionary_ref_type(Types), ColumnTypes, RefTypes),
    quote_ident(TypeName, QuotedTable),
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
% Edge bodies keep the refusal (analyze.pl edge_body_needs_json_destructure):
% a compound value that ARRIVES into an untyped column is stored as canonical
% term text, and that encoding question is SLOT-TERM-STRUCT's, not this one's.

dictionary_relplans(Types, Plans) :-
    findall(relplan(Name/DictArity, set, ['__id' | Columns], none,
                    [ref(TypeName) | StorageKinds]),
            ( member(type_def(TypeName, Columns, ColumnTypes), Types),
              dictionary_table_name(TypeName, Name),
              length(Columns, Width), DictArity is Width + 1,
              maplist(dictionary_storage_kind(Types), ColumnTypes, StorageKinds) ),
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
% no refusal (plans/2026-07-30-file-span-spine-reconciled.md section 3.1).
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

% Edge rules do not get this rewrite. edge_statements_for_rule/3 compiles a
% trigger occurrence against RelPlans alone -- the dictionary plans are level-
% body-only by construction (Edge 2, see the comment at the call site) -- so
% there is nowhere for the per-level join to go. A relation value in an edge
% rule is therefore a named compiler refusal rather than the whole-atom
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
                           st(_, DictionaryAtoms)),
    append(Goals, DictionaryAtoms, AllGoals),
    goals_conjunction(AllGoals, Body),
    check_relation_patterns_lowered(Types, RelPlans, Head, Body).
expand_relation_pattern_rule(_, _, Rule, Rule).

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
        relation_value_shape(Types, TypeName, Arg0)
    ->  intern_relation_value(Types, TypeName, Arg0, Arg, State0, State1)
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
check_relation_patterns_lowered(Types, RelPlans, Head, Body) :-
    (   relation_pattern_residue(Types, RelPlans, Head, Body, Residue)
    ->  throw(unsupported_construct(Residue))
    ;   true
    ).

relation_pattern_residue(Types, RelPlans, Head, Body,
                         relation_pattern_not_lowerable(Ref, Column, TypeName, Value)) :-
    walk_body((Head, Body),
              walk_policy(descend_not(true), splice_bare(true)), Events),
    member(event(_, _, Surface, Term), Events),
    (   Surface == plain_atom
    ->  Atom = Term
    ;   nonvar(Term), functor(Term, Wrapper, 1),
        memberchk(Wrapper, [latest, pre, finalize]),
        arg(1, Term, Atom)
    ),
    compound(Atom),
    functor(Atom, Name, Arity),
    Ref = Name/Arity,
    relplan_column_types(RelPlans, Ref, ColumnTypes),
    relplan_columns(RelPlans, Ref, Columns),
    nth1(Position, ColumnTypes, ref(TypeName)),
    nth1(Position, Columns, Column),
    arg(Position, Atom, Value),
    relation_value_shape(Types, TypeName, Value),
    !.

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
    (   DecodeGoals == []
    ->  Expanded = Body
    ;   foldl(decode_goal_atoms(Types, RelPlans, OtherGoals), DecodeGoals, [], Atoms),
        append(OtherGoals, Atoms, AllGoals),
        goals_conjunction(AllGoals, Expanded)
    ).
expand_decode_rule(_, _, Rule, Rule).

is_decode_goal(Goal) :- nonvar(Goal), Goal = decode(_, _).

goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, More)) :- goals_conjunction(Rest, More).

decode_goal_atoms(Types, RelPlans, BodyGoals, decode(Source, Pattern), Acc0, Acc) :-
    decode_source_type(Types, RelPlans, BodyGoals, Acc0, decode(Source, Pattern), TypeName),
    decode_pattern_atoms(Types, TypeName, Source, Pattern, Atoms),
    append(Acc0, Atoms, Acc).

% The declared type of the variable decode/2 reads, resolved from whichever
% positive body atom (or already-emitted dictionary atom) binds it. A source
% with no ref-typed binding is a NAMED refusal, never a lowering that answers
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
    format(atom(Expr), 'json_extract(value, \'$[~w]\')', [Index]),
    NextIndex is Index + 1,
    NextN is N - 1,
    incremental_json_select_exprs_from(NextN, NextIndex, More).

% ═══ arrival statement templates (round 2: Log rel drops tick/seq params) ═══

arrival_statement(relplan(Ref, log, Columns, _, _),
                  arrivalstmt(Ref, log, AddSql, none, IncrementalAddSql, none)) :- !,
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    format(atom(AddSql), 'INSERT INTO ~w (~w) VALUES (~w)', [QuotedTable, ColumnsSql, PlaceholdersSql]),
    incremental_arrival_add_sql('INSERT INTO', '', QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql).
arrival_statement(relplan(Ref, set, Columns, KeyOrNone, _),
                  arrivalstmt(Ref, set, AddSql, DelSql, IncrementalAddSql, IncrementalDelSql)) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    length(Columns, N), placeholders(N, Placeholders),
    atomic_list_concat(Placeholders, ', ', PlaceholdersSql),
    set_arrival_sql_parts(KeyOrNone, QuotedColumns, Insert, ConflictSql),
    format(atom(AddSql), '~w ~w (~w) VALUES (~w)~w',
           [Insert, QuotedTable, ColumnsSql, PlaceholdersSql, ConflictSql]),
    maplist(eq_placeholder, QuotedColumns, EqParts),
    atomic_list_concat(EqParts, ' AND ', WhereSql),
    format(atom(DelSql), 'DELETE FROM ~w WHERE ~w', [QuotedTable, WhereSql]),
    incremental_arrival_add_sql(Insert, ConflictSql, QuotedTable, ColumnsSql, QuotedColumns,
                                IncrementalAddSql),
    incremental_json_select_exprs(QuotedColumns, 0, DeleteSelectExprs),
    atomic_list_concat(DeleteSelectExprs, ', ', DeleteSelectSql),
    format(atom(IncrementalDelSql),
           'DELETE FROM ~w WHERE (~w) IN (SELECT ~w FROM json_each(?)) RETURNING ~w',
           [QuotedTable, ColumnsSql, DeleteSelectSql, ColumnsSql]).

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
    format(atom(Expr), 'json_extract(value, \'$[~w]\')', [Index]),
    NextIndex is Index + 1,
    incremental_json_select_exprs(Rest, NextIndex, More).

eq_placeholder(QuotedColumn, Text) :- format(atom(Text), '~w = ?', [QuotedColumn]).

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
       edge_statement_single(RelPlans, Head, TriggerAtom, [], [], [], [],
                             EdgeStmt),
       EdgeStatements = [EdgeStmt]
    ; Shape = unmarked_conjunction(Atoms)
    -> findall(EdgeStmt,
               ( select(TriggerAtom, Atoms, OtherAtoms),
                 edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms,
                                       [], [], [], EdgeStmt) ),
               EdgeStatements)
    ; Shape = sampled_conjunction(TriggerAtoms, SampleAtoms, PreAtoms,
                                  NegAtoms, GuardGoals)
    -> findall(EdgeStmt,
               ( select(TriggerAtom, TriggerAtoms, OtherTriggerAtoms),
                 append(OtherTriggerAtoms, SampleAtoms, OtherAtoms),
                 edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms,
                                       PreAtoms, NegAtoms, GuardGoals,
                                       EdgeStmt) ),
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
       edge_statement_single(RelPlans, Head, FinalizeAtom, OtherAtoms, PreAtoms,
                             NegAtoms, GuardGoals, DepartureKind, EdgeStmt),
       EdgeStatements = [EdgeStmt]
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
edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                      NegAtoms, GuardGoals, EdgeStmt) :-
    ( PreAtoms == [] -> TriggerKind = arrival
    ; TriggerKind = ordered_arrival
    ),
    edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                          NegAtoms, GuardGoals, TriggerKind, EdgeStmt).

edge_statement_single(RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                      NegAtoms, GuardGoals, TriggerKind,
                      edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns,
                               ProjectSql, WriteSql, DeltaProjectSql,
                               TriggerKind)) :-
    rel_ref(TriggerAtom, TriggerRef),
    rel_ref(Head, HeadRef),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    ( HeadKind == set
    -> ( memberchk(relplan(HeadRef, set, _, key(KeyPositions), _), RelPlans) -> true
       ; throw(unsupported_construct(edge_into_unkeyed_set(HeadRef))) )
    ; true  % log: no key concept, KeyPositions unused below
    ),
    TriggerAtom =.. [_ | TriggerArgs],
    relplan_column_types(RelPlans, TriggerRef, TriggerBoundColumnTypes),
    compile_trigger_bound(TriggerArgs, TriggerBoundColumnTypes, TriggerBound),
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
    compile_positive_uses(RelPlans, PositiveUses, TriggerBound, PositiveBound,
                          FromParts, PositiveWhereTexts),
    compile_guard_goals(GuardGoals, PositiveBound, Bound, GuardWhereTexts),
    maplist(negated_atom_use, NegAtoms, NegUses),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append([PositiveWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    ( FromParts == [] -> FromSql = none ; atomic_list_concat(FromParts, ', ', FromSql) ),
    ( WhereTexts == [] -> WhereSql = none ; atomic_list_concat(WhereTexts, ' AND ', WhereSql) ),
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
    edge_delta_project_sql(RelPlans, Head, TriggerAtom, IdentityOtherAtoms,
                           PreAtoms, NegAtoms, GuardGoals, HeadColumns,
                           TriggerKind, DeltaProjectSql),
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
    member(relplan(_, _, _, _, ColumnTypes), RelPlans),
    memberchk(ref(Name), ColumnTypes).

member_same_term(Term, [Candidate | _]) :- Term == Candidate, !.
member_same_term(Term, [_ | Rest]) :- member_same_term(Term, Rest).

edge_delta_project_sql(RelPlans, Head, TriggerAtom, OtherAtoms, PreAtoms,
                       NegAtoms, GuardGoals, HeadColumns, TriggerKind,
                       DeltaProjectSql) :-
    rel_ref(TriggerAtom, TriggerRef),
    TriggerAtom =.. [_ | TriggerArgs],
    (   memberchk(TriggerKind, [departure, ordered_departure])
    ->  departure_frontier_table_name(TriggerRef, FrontierTable)
    ;   frontier_table_name(TriggerRef, FrontierTable)
    ),
    quote_ident(FrontierTable, QuotedFrontierTable),
    DeltaAlias = d0,
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    relplan_column_types(RelPlans, TriggerRef, TriggerColumnTypes),
    compile_atom_args(TriggerArgs, TriggerColumns, TriggerColumnTypes, DeltaAlias, [],
                      TriggerBound, TriggerWhereParts),
    maplist(where_text, TriggerWhereParts, TriggerWhereTexts),
    maplist(other_atom_use, OtherAtoms, OtherUses),
    maplist(pre_atom_use, PreAtoms, PreUses),
    append(OtherUses, PreUses, PositiveUses),
    compile_positive_uses(RelPlans, PositiveUses, TriggerBound, PositiveBound,
                          OtherFromParts, OtherWhereTexts),
    compile_guard_goals(GuardGoals, PositiveBound, Bound, GuardWhereTexts),
    maplist(negated_atom_use, NegAtoms, NegUses),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Head, Bound, HeadColumns, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w ~w', [QuotedFrontierTable, DeltaAlias]),
    append([DeltaFrom], OtherFromParts, FromParts),
    atomic_list_concat(FromParts, ', ', FromSql),
    append([['d0."_phase" >= 0' | TriggerWhereTexts], OtherWhereTexts,
            GuardWhereTexts, NegWhereTexts], WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaProjectSql),
           'SELECT ~w FROM ~w WHERE ~w ORDER BY d0."_phase", d0."_sequence"',
           [SelectSql, FromSql, WhereSql]).

excluded_assignment(QuotedColumn, Text) :- format(atom(Text), '~w = excluded.~w', [QuotedColumn, QuotedColumn]).

other_atom_use(Atom, use(Ref, Args, pos, unmarked)) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].
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
compile_trigger_bound(TriggerArgs, TriggerColumnTypes, Bound) :-
    compile_trigger_bound(TriggerArgs, TriggerColumnTypes, 1, Bound).
compile_trigger_bound([], [], _, []).
compile_trigger_bound([Arg | Rest], [ColumnType | RestTypes], Index,
                      [Arg-typed(Placeholder, ColumnType) | MoreBound]) :-
    ( var(Arg) -> true ; throw(unsupported_construct(trigger_arg_not_var(Arg))) ),
    format(atom(Placeholder), '?~w', [Index]),
    NextIndex is Index + 1,
    compile_trigger_bound(Rest, RestTypes, NextIndex, MoreBound).

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
                                SupportSql, AggregateSql)) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    format(atom(DeleteSql), 'DELETE FROM ~w', [QuotedHeadTable]),
    maplist(level_insert_sql(RelPlans, HeadRef), Rules, InsertSqls),
    partition(rule_is_aggregate, Rules, AggregateRules, PlainRules),
    ( AggregateRules == []
    -> level_delta_insert_sql(RelPlans, HeadRef, Rules, DeltaInsertSql),
       level_support_sql(RelPlans, HeadRef, Rules, SupportSql),
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
       SupportSql = none,
       level_aggregate_sql(RelPlans, HeadRef, AggregateRules, AggregateSql)
    ).

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
level_aggregate_sql(RelPlans, HeadRef, Rules,
                    aggsql(ScopeColumns, ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                           DeleteScopedSql, InsertScopedSqls)) :-
    aggregate_scope_columns(RelPlans, HeadRef, Rules, ScopeColumns, ScopeTypes),
    aggregate_scope_table_name(HeadRef, ScopeTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    format(atom(ScopeClearSql), 'DELETE FROM ~w', [QuotedScopeTable]),
    findall(SeedSql,
            ( member(Rule, Rules),
              aggregate_scope_seed_sql(RelPlans, ScopeColumns, QuotedScopeTable,
                                       Rule, SeedSql) ),
            ScopeSeedSqls),
    aggregate_delete_scoped_sql(RelPlans, HeadRef, ScopeColumns,
                                QuotedScopeTable, DeleteScopedSql),
    maplist(aggregate_insert_scoped_sql(RelPlans, HeadRef, ScopeColumns,
                                        QuotedScopeTable),
            Rules, InsertScopedSqls).

aggregate_scope_table_name(Name/_Arity, ScopeTable) :-
    format(atom(ScopeTable), '__agg_scope_~w', [Name]).

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
aggregate_scope_seed_sql(RelPlans, ScopeColumns, QuotedScopeTable, (Head <- Body),
                         SeedSql) :-
    aggregate_head_template(Head, Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    member(use(DeltaRef, DeltaArgs, pos, _), PosUses),
    delta_table_name(DeltaRef, DeltaTable),
    quote_ident(DeltaTable, QuotedDeltaTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      DeltaBound, DeltaWhereParts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    aggregate_scope_group_exprs(Template, DeltaBound, Head, GroupExprs),
    atomic_list_concat(GroupExprs, ', ', GroupSql),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    atomic_list_concat(QuotedScopeColumns, ', ', ScopeColumnsSql),
    append(['d0."_sign" IN (-1, 1)'], DeltaWhereTexts, WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(SeedSql),
           'INSERT OR IGNORE INTO ~w (~w) SELECT DISTINCT ~w FROM ~w d0 WHERE ~w',
           [QuotedScopeTable, ScopeColumnsSql, GroupSql, QuotedDeltaTable, WhereSql]).

aggregate_scope_group_exprs(Template, DeltaBound, Head, GroupExprs) :-
    aggregate_group_positions(Template, Positions),
    ( Positions == []
    -> GroupExprs = ['0']
    ;  catch(aggregate_group_exprs(Template, DeltaBound, GroupExprs),
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

aggregate_insert_scoped_sql(RelPlans, HeadRef, ScopeColumns, QuotedScopeTable,
                            (Head <- Body), InsertScopedSql) :-
    table_name(HeadRef, HeadTable), quote_ident(HeadTable, QuotedHeadTable),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    aggregate_head_template(Head, Template),
    body_ref_uses(Body, Uses),
    include(is_positive_use, Uses, PosUses),
    include(is_negative_use, Uses, NegUses),
    compile_positive_uses(RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Body, Bound0, Bound, GuardWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    aggregate_group_exprs(Template, Bound, GroupExprs),
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
    atomic_list_concat(FromParts, ', ', FromSql),
    aggregate_select_statement(Head, Template, Bound, FromSql, AllWhereTexts,
                               SelectStatement),
    format(atom(InsertScopedSql), 'INSERT OR IGNORE INTO ~w (~w) ~w RETURNING ~w',
           [QuotedHeadTable, HeadColumnsSql, SelectStatement, HeadColumnsSql]).

% The scope columns and their storage types ride inside the aggsql/6 term
% itself, so DDL emission needs nothing but the levelstmt (lower_program/2
% no longer has the rule list in scope by then).
aggregate_scope_ddl(levelstmt(HeadRef, _, _, _, _,
                              aggsql(ScopeColumns, ScopeTypes, _, _, _, _)),
                    [Ddl]) :- !,
    aggregate_scope_table_name(HeadRef, ScopeTable),
    quote_ident(ScopeTable, QuotedScopeTable),
    maplist(quote_ident, ScopeColumns, QuotedScopeColumns),
    maplist(column_def, QuotedScopeColumns, ScopeTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    atomic_list_concat(QuotedScopeColumns, ', ', PrimaryKeySql),
    format(atom(Ddl),
           'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID',
           [QuotedScopeTable, ColumnsSql, PrimaryKeySql]).
aggregate_scope_ddl(_, []).

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
    compile_positive_uses(RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Body, Bound0, Bound, GuardWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], AllWhereTexts),
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
    compile_positive_uses(RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Body, Bound0, Bound, GuardWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], AllWhereTexts),
    atomic_list_concat(FromParts, ', ', FromSql),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    head_select_list(Head, Bound, HeadColumns, AliasedSelectExprs),
    support_group_exprs(Head, Bound, GroupExprs),
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

% SQLite treats a bare integer in GROUP BY as a one-based SELECT-list
% position, including when the integer is parenthesized. Adding zero makes it
% an expression while preserving its value, so only literal integer head
% columns need a different spelling from head_select_list/4. Every variable,
% atom, and compound head expression retains its existing emitted SQL bytes.
% Shared by the support-count arm and both aggregate grouping arms
% (scoped-delta insert + recompute) so the SQLite grammar fact lives once.
support_group_exprs(Head, Bound, GroupExprs) :-
    Head =.. [_ | Args],
    maplist(group_expr(Bound), Args, GroupExprs).

group_expr(Bound, Arg, GroupExpr) :-
    compile_expr(Arg, Bound, Sql, _Type),
    ( integer(Arg)
    -> format(atom(GroupExpr), '(~w + 0)', [Sql])
    ;  GroupExpr = Sql
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
    compile_positive_uses(RelPlans, PosUses, [], Bound0, FromParts, PosWhereTexts),
    compile_body_guards(Body, Bound0, Bound, GuardWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    append([PosWhereTexts, GuardWhereTexts, NegWhereTexts], AllWhereTexts),
    atomic_list_concat(FromParts, ', ', FromSql),
    ( aggregate_head_template(Head, Template)
    -> aggregate_select_statement(Head, Template, Bound, FromSql, AllWhereTexts, SelectStatement)
    ;  head_select_list(Head, Bound, none, SelectExprs),
       atomic_list_concat(SelectExprs, ', ', SelectSql),
       ( AllWhereTexts == []
       -> format(atom(SelectStatement), 'SELECT ~w FROM ~w', [SelectSql, FromSql])
       ; atomic_list_concat(AllWhereTexts, ' AND ', WhereSql),
         format(atom(SelectStatement), 'SELECT ~w FROM ~w WHERE ~w', [SelectSql, FromSql, WhereSql])
       )
    ),
    maplist(quote_ident, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(InsertSql), 'INSERT OR IGNORE INTO ~w (~w) ~w', [QuotedHeadTable, HeadColumnsSql, SelectStatement]).

% The one place the body's guard/bind goals turn into SQL, shared by every
% level-rule statement family (recompute insert, delta arm, refCount arm,
% recursive-CTE arm) so a guard can never be present in one family and
% silently absent from another -- the phase-C silent-filter-loss class.
compile_body_guards(Body, Bound0, Bound, GuardWhereTexts) :-
    body_guard_goals(Body, GuardGoals),
    compile_guard_goals(GuardGoals, Bound0, Bound, GuardWhereTexts).

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
%             named refusal here rather than a silent lexicographic min.
aggregate_select_statement(Head, Template, Bound, FromSql, AllWhereTexts, SelectStatement) :-
    Head =.. [_ | Args],
    aggregate_select_exprs(Template, Args, Bound, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    aggregate_group_exprs(Template, Bound, GroupExprs),
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
    format(atom(SelectStatement),
           'SELECT ~w FROM ~w~w~w HAVING count(*) > 0',
           [SelectSql, FromSql, WhereClause, GroupClause]).

aggregate_select_exprs([], [], _, []).
aggregate_select_exprs([TemplateArg | RestTemplate], [_Arg | RestArgs], Bound,
                       [Sql | RestSqls]) :-
    aggregate_select_expr(TemplateArg, Bound, Sql),
    aggregate_select_exprs(RestTemplate, RestArgs, Bound, RestSqls).

aggregate_select_expr(plain(Expr), Bound, Sql) :- !,
    compile_expr(Expr, Bound, Sql, _Type).
aggregate_select_expr(agg(count, _Expr), _Bound, 'count(*)') :- !.
aggregate_select_expr(agg(sum, Expr), Bound, Sql) :- !,
    compile_aggregate_number_operand(sum, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'sum(~w)', [InnerSql]).
aggregate_select_expr(agg(avg, Expr), Bound, Sql) :- !,
    compile_aggregate_number_operand(avg, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'avg(~w)', [InnerSql]).
aggregate_select_expr(agg(min, Expr), Bound, Sql) :- !,
    compile_aggregate_number_operand(min, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'min(~w)', [InnerSql]).
aggregate_select_expr(agg(max, Expr), Bound, Sql) :- !,
    compile_aggregate_number_operand(max, Expr, Bound, InnerSql, _),
    format(atom(Sql), 'max(~w)', [InnerSql]).
aggregate_select_expr(agg(Kind, _), _, _) :-
    throw(unsupported_construct(aggregate_kind_not_lowered(Kind))).

compile_aggregate_number_operand(Kind, Expr, Bound, Sql, Type) :-
    compile_expr(Expr, Bound, Sql, Type),
    ( memberchk(Type, [int, float])
    -> true
    ;  throw(unsupported_construct(aggregate_operand_not_number(Kind, Expr, Type)))
    ).

aggregate_group_exprs(Template, Bound, GroupExprs) :-
    findall(GroupExpr,
            ( member(plain(Expr), Template), group_expr(Bound, Expr, GroupExpr) ),
            GroupExprs).

% The head columns an aggregate rule GROUPS BY, as SQL text, reused by the
% group-scoped incremental path below.
aggregate_group_positions(Template, Positions) :-
    findall(Position,
            ( nth1(Position, Template, TemplateArg), TemplateArg = plain(_) ),
            Positions).

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
    level_positive_delta_arms(RelPlans, Head, Body, PosUses, NegUses, PosUses, DeltaArms).

level_positive_delta_arms(_, _, _, [], _, _, []).
% STRUCT-AS-ROWS: a dictionary atom gets NO delta arm, and needs none. A
% dictionary row is created only by interning a value some ARRIVING row
% carries, and interning runs before that tick's arrival statements, so every
% new dictionary row already has its parent row in the parent's own frontier
% -- the parent's arm covers exactly the same derivations. An arm on this side
% would additionally read `__frontier___dict_<type>`, a table the DDL does not
% create (a dictionary is storage plane: no delta table, no frontier, no
% boundary), so this is the same fact stated twice: dictionaries do not move
% on their own.
level_positive_delta_arms(RelPlans, Head, Body, [_ | RestPositions], NegUses, PosUses,
                          Arms) :-
    length(RestPositions, RemainingCount),
    length(PosUses, PositiveCount),
    Position is PositiveCount - RemainingCount - 1,
    nth0_select(Position, PosUses, DeltaUse, OtherPosUses),
    level_positive_delta_arms(RelPlans, Head, Body, RestPositions, NegUses, PosUses, RestArms),
    (   dictionary_use(DeltaUse)
    ->  Arms = RestArms
    ;   level_delta_select_arm(RelPlans, Head, Body, DeltaUse, OtherPosUses, NegUses, DeltaArm),
        Arms = [DeltaArm | RestArms]
    ).

dictionary_use(use(Name/_Arity, _, _, _)) :- sub_atom(Name, 0, _, _, '__ref_').

nth0_select(0, [Selected | Rest], Selected, Rest) :- !.
nth0_select(Index, [Item | Rest], Selected, [Item | More]) :-
    Index > 0,
    NextIndex is Index - 1,
    nth0_select(NextIndex, Rest, Selected, More).

% The guard walk runs HERE too, not only in the recompute insert. Omitting it
% was a real miscompile caught by the sweep, not a theoretical one: with the
% guard present in level_insert_sql/4 but absent from the delta arm,
% spine_semantics.pl's dirty_retracts_on_matching_commit correctly retracted
% dirty("src/lib.rs") at tick 2 and then the tick-3 drain re-inserted it off
% the frontier with `WorktreeDigest \== TreeDigest` simply not applied --
% oracle tick 3 is empty, actual added the row back. Every statement family
% that reproduces a rule body has to reproduce its guards; compile_body_guards/4
% is the single place that happens.
level_delta_select_arm(RelPlans, Head, Body, use(DeltaRef, DeltaArgs, pos, _),
                       OtherPosUses, NegUses, DeltaArm) :-
    frontier_table_name(DeltaRef, FrontierTable),
    quote_ident(FrontierTable, QuotedFrontierTable),
    relplan_columns(RelPlans, DeltaRef, DeltaColumns),
    relplan_column_types(RelPlans, DeltaRef, DeltaColumnTypes),
    compile_atom_args(DeltaArgs, DeltaColumns, DeltaColumnTypes, d0, [],
                      DeltaFieldBound, DeltaWhereParts),
    delta_reference_identity(RelPlans, DeltaRef, DeltaArgs, DeltaColumns,
                             DeltaFieldBound, DeltaBound,
                             IdentityFromParts, IdentityWhereTexts),
    maplist(where_text, DeltaWhereParts, DeltaWhereTexts),
    compile_positive_uses(RelPlans, OtherPosUses, DeltaBound, Bound0,
                          OtherFromParts, OtherWhereTexts),
    compile_body_guards(Body, Bound0, Bound, GuardWhereTexts),
    compile_negative_uses(RelPlans, NegUses, Bound, NegWhereTexts),
    head_select_list(Head, Bound, none, SelectExprs),
    atomic_list_concat(SelectExprs, ', ', SelectSql),
    format(atom(DeltaFrom), '~w d0', [QuotedFrontierTable]),
    append([[DeltaFrom], IdentityFromParts, OtherFromParts], FromParts),
    atomic_list_concat(FromParts, ', ', FromSql),
    append([['d0."_phase" >= 0' | DeltaWhereTexts], IdentityWhereTexts,
            OtherWhereTexts], PositiveWhereTexts),
    append([PositiveWhereTexts, GuardWhereTexts, NegWhereTexts], WhereTexts),
    atomic_list_concat(WhereTexts, ' AND ', WhereSql),
    format(atom(DeltaArm), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectSql, FromSql, WhereSql]).

delta_reference_identity(RelPlans, Name/Arity, Args, Columns,
                         Bound0, Bound, [From], Equalities) :-
    reference_target_ref(RelPlans, Name/Arity),
    !,
    quote_ident(Name, QuotedTable),
    format(atom(From), '~w r0', [QuotedTable]),
    findall(Equality,
            ( member(Column, Columns),
              format(atom(Equality), 'r0."~w" = d0."~w"',
                     [Column, Column]) ),
            Equalities),
    length(Args, Arity),
    Atom =.. [Name | Args],
    Bound = [Atom-typed('r0."__id"', ref(Name)) | Bound0].
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

retention_statement(RelPlans, keep(Ref, count(Limit)),
                    retentionstmt(Ref, Limit, DeleteSql)) :-
    integer(Limit),
    Limit >= 0,
    memberchk(relplan(Ref, log, Columns, _, _), RelPlans),
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

delta_ddl(DepartureRefs, RelPlan, Ddl) :-
    RelPlan = relplan(Ref, _, Columns, _, ColumnTypes),
    delta_ddl(RelPlan, BaseDdl),
    (   memberchk(Ref, DepartureRefs)
    ->  departure_frontier_ddl(Ref, Columns, ColumnTypes, DepartureDdl),
        append(BaseDdl, DepartureDdl, Ddl)
    ;   Ddl = BaseDdl
    ).

departure_frontier_ddl(Ref, Columns, ColumnTypes, [TableDdl, IndexDdl]) :-
    departure_frontier_table_name(Ref, DepartureTable),
    quote_ident(DepartureTable, QuotedDepartureTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
    atomic_list_concat(ColumnDefs, ', ', ColumnsSql),
    format(atom(TableDdl),
           'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)',
           [QuotedDepartureTable, ColumnsSql]),
    format(atom(IndexName), '~w_phase', [DepartureTable]),
    quote_ident(IndexName, QuotedIndexName),
    format(atom(IndexDdl), 'CREATE INDEX ~w ON ~w ("_phase")',
           [QuotedIndexName, QuotedDepartureTable]).

pre_ddl(RelPlans, Ref, Ddl) :-
    memberchk(relplan(Ref, _, Columns, KeyOrNone, ColumnTypes), RelPlans),
    pre_table_name(Ref, PreTable),
    quote_ident(PreTable, QuotedPreTable),
    maplist(quote_ident, Columns, QuotedColumns),
    maplist(column_def, QuotedColumns, ColumnTypes, ColumnDefs),
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

% An aggregate head has no refCount table (aggsql/6 replaces the refCount
% family entirely -- level_statement_group/3's own comment), so it gets no
% refCount DDL either.
support_ddl(_, levelstmt(_, _, _, _, none, _), []) :- !.
support_ddl(RelPlans, levelstmt(HeadRef, _, _, _, _, _), [Ddl]) :-
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
canonical_column_expr(Column, bool, QuotedColumn) :-
    !,
    quote_ident(Column, QuotedColumn).
canonical_column_expr(Column, float, QuotedColumn) :-
    !,
    quote_ident(Column, QuotedColumn).
% STRUCT-AS-ROWS, arc header Edge 1: a ref column reads its VALUE, never its
% id. The rendering was computed once at intern time, so this is one indexed
% probe per row and no recursion regardless of nesting depth.
canonical_column_expr(Column, ref(TypeName), Expr) :-
    !,
    dictionary_render_expr(TypeName, Column, Expr).
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

boot_seed_statement(Decls, Types, relplan(Ref, log, Columns, _, ColumnTypes), Initial, Statements) :- !,
    boot_rows_statements(Decls, Types, 'INSERT INTO', Ref, Columns, ColumnTypes, Initial, Statements).
boot_seed_statement(Decls, Types, relplan(Ref, set, Columns, _, ColumnTypes), Initial, Statements) :-
    boot_rows_statements(Decls, Types, 'INSERT OR IGNORE INTO', Ref, Columns, ColumnTypes, Initial, Statements).

boot_rows_statements(Decls, Types, Insert, Ref, Columns, ColumnTypes, Initial, Statements) :-
    findall(Group,
            ( member(Row, Initial), rel_ref(Row, Ref), Row =.. [_ | Values],
              boot_row_statements(Decls, Types, Insert, Ref, Columns, ColumnTypes, Values, Group) ),
            Groups),
    append(Groups, Statements).

% STRUCT-AS-ROWS on the boot path. A seed row whose column is a ref carries a
% VALUE, and the dense id it must store does not exist until the dictionary
% row does. So one seed row becomes: the intern statements for every value in
% its ref columns (children before parents), then the row insert itself, whose
% ref parameter is a `SELECT "__id" ... WHERE <declared key>` subquery rather
% than a bind. Arrivals take the same shape at runtime; this is the same plan
% with the values known at compile time.
boot_row_statements(Decls, Types, Insert, Ref, Columns, ColumnTypes, Values, Statements) :-
    table_name(Ref, Table), quote_ident(Table, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    boot_column_slots(Decls, Types, ColumnTypes, Values, Descs, InternStatements),
    maplist(slot_desc_slot, Descs, Slots),
    slot_desc_params(Descs, Params),
    atomic_list_concat(Slots, ', ', SlotsSql),
    format(atom(Sql), '~w ~w (~w) VALUES (~w)', [Insert, QuotedTable, ColumnsSql, SlotsSql]),
    append(InternStatements, [bootstmt(Sql, Params)], Statements).

boot_column_slots(_, _, [], [], [], []).
boot_column_slots(Decls, Types, [ColumnType | ColumnTypes], [Value | Values],
                  [Desc | Descs], Statements) :-
    boot_column_slot(Decls, Types, ColumnType, Value, Desc, Group),
    boot_column_slots(Decls, Types, ColumnTypes, Values, Descs, More),
    append(Group, More, Statements).

boot_column_slot(Decls, Types, ColumnType, Value, slot_desc(Slot, Params), Statements) :-
    (   ColumnType = ref(TypeName)
    ->  struct_intern_statements(Decls, Types, TypeName, Value, Slot, Params, Statements)
    ;   Slot = '?', Params = [Value], Statements = []
    ).

slot_desc_slot(slot_desc(Slot, _), Slot).

slot_desc_params([], []).
slot_desc_params([slot_desc(_, Params) | Rest], All) :-
    slot_desc_params(Rest, More),
    append(Params, More, All).

% Post-order: every referenced target insert precedes its parent's insert, so
% a parent's ref column can resolve by declared key against a row that already
% exists. The current type-cycle refusal is what makes this terminate.
struct_intern_statements(Decls, Types, TypeName, Value, LookupSlot, LookupParams, Statements) :-
    type_definition(Types, TypeName, Columns, ColumnTypes),
    type_field_values(Types, TypeName, Value, FieldValues),
    boot_column_slots(Decls, Types, ColumnTypes, FieldValues, Descs, ChildStatements),
    maplist(slot_desc_slot, Descs, Slots),
    slot_desc_params(Descs, Params),
    quote_ident(TypeName, QuotedTable),
    maplist(quote_ident, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    atomic_list_concat(Slots, ', ', SlotsSql),
    format(atom(Sql),
           'INSERT OR IGNORE INTO ~w (~w) VALUES (~w)',
           [QuotedTable, ColumnsSql, SlotsSql]),
    length(Columns, Arity),
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

lower_program(plan(Name, prog(Decls, Rules), RelPlans, ArrivalTargets, RuleOrder, EdgeRules),
              lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)) :-
    type_definitions(Decls, LoweringTypes),
    findall(EdgeHeadedRef, ( member(EdgeRule, EdgeRules), rule_head_ref(EdgeRule, EdgeHeadedRef) ), EdgeHeadedRefs),
    findall(LevelHeadedRef,
            ( member(LevelRule, RuleOrder), rule_head_ref(LevelRule, LevelHeadedRef) ),
            LevelHeadedRefs),
    maplist(rel_ddl(LoweringTypes, EdgeHeadedRefs, ArrivalTargets, LevelHeadedRefs),
            RelPlans, RelationDdlGroups),
    listened_departure_refs(Rules, DepartureRefs),
    maplist(delta_ddl(DepartureRefs), RelPlans, DeltaDdlGroups),
    append(RelationDdlGroups, RelationDdl),
    append(DeltaDdlGroups, DeltaDdl),
    include(arrival_target_relplan(ArrivalTargets), RelPlans, ArrivalRelPlans),
    maplist(arrival_statement, ArrivalRelPlans, ArrivalStatements),
    % One rule may lower to MULTIPLE edgestmt entries now (an unmarked or
    % sampled conjunction with N trigger atoms produces N arms), so this
    % maplist collects a GROUP per rule and flattens, rather than assuming
    % one-to-one.
    maplist(check_edge_rule_relation_values(LoweringTypes, RelPlans), EdgeRules),
    maplist(edge_statements_for_rule(RelPlans), EdgeRules, EdgeStatementGroups),
    append(EdgeStatementGroups, EdgeStatements),
    % STRUCT-AS-ROWS: level bodies are compiled against RelPlans PLUS the
    % dictionary plans, and with decode/2 already rewritten into dictionary
    % atoms. The dictionary plans reach the BODY compiler only -- never
    % rel_ddl/5, delta_statement/2, arrival_statement/2 or relColumns -- which
    % is Edge 2 enforced by construction rather than by a filter someone can
    % forget: a dictionary has no delta table to report and no name the tick
    % log can reach.
    dictionary_relplans(LoweringTypes, DictionaryRelPlans),
    append(DictionaryRelPlans, RelPlans, BodyRelPlans),
    % Relation TERMS become dictionary atoms before decode/2 does, so a
    % variable this pass introduces is already a legal decode source when
    % decode_binding_type/5 goes looking for one. The reverse order would make
    % the two spellings non-composable for no reason.
    expand_relation_pattern_rules(LoweringTypes, BodyRelPlans, RuleOrder,
                                  PatternedRuleOrder),
    expand_decode_rules(LoweringTypes, BodyRelPlans, PatternedRuleOrder,
                        DecodedRuleOrder),
    level_statement_groups(BodyRelPlans, DecodedRuleOrder, RuleLevelStatements),
    retention_statements(Decls, RelPlans, RetentionStatements),
    append(RuleLevelStatements, RetentionStatements, LevelStatements),
    maplist(support_ddl(RelPlans), RuleLevelStatements, SupportDdlGroups),
    maplist(aggregate_scope_ddl, RuleLevelStatements, AggregateScopeDdlGroups),
    append(SupportDdlGroups, SupportDdl),
    append(AggregateScopeDdlGroups, AggregateScopeDdl),
    findall(PreRef,
            ( member((_ <+ EdgeBody), Rules),
              level_body_pre_ref(EdgeBody, PreRef) ),
            PreRefs0),
    sort(PreRefs0, PreRefs),
    maplist(pre_ddl(RelPlans), PreRefs, PreDdl),
    program_uses_tick(prog(Decls, Rules), UsesTick),
    ( UsesTick == true -> tick_table_ddl(TickDdl) ; TickDdl = [] ),
    % STRUCT-AS-ROWS: the dictionaries come FIRST in the DDL list, in
    % topological order, so a program's storage plane exists before any table
    % whose columns point into it.
    append([RelationDdl, DeltaDdl, SupportDdl, AggregateScopeDdl, PreDdl,
            TickDdl], Ddl),
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
boot_statements(Decls, RelPlans, Initial, LevelStatements, BootStatements) :-
    type_definitions(Decls, Types),
    maplist(boot_seed_statement_for(Decls, Types, Initial), RelPlans, SeedGroups),
    append(SeedGroups, SeedStatements),
    boot_level_recompute_statements(LevelStatements, LevelBootStatements),
    append(SeedStatements, LevelBootStatements, BootStatements).

boot_seed_statement_for(Decls, Types, Initial, RelPlan, Statements) :-
    boot_seed_statement(Decls, Types, RelPlan, Initial, Statements).

% engine.pl:run_program computes level_closure(PlainLevel, AggRules, BaseRows,
% 0, Level0) ONCE, immediately after seeding Initial rows and before tick 1's
% state(...) exists -- the SAME DELETE/INSERT-SELECT SQL recomputeLevels runs
% inside a tick (lower.pl:level_statement_group/3), run once more here with
% no bind params (a literal statement, not a template) so a level view over
% Initial-seeded data starts at its real t=0 rows rather than empty.
boot_level_recompute_statements(LevelStatements, BootStatements) :-
    findall(bootstmt(Sql, []),
            ( member(levelstmt(_, DeleteSql, InsertSqls, _, _, _), LevelStatements),
              ( Sql = DeleteSql ; member(Sql, InsertSqls) ) ),
            BootStatements).
