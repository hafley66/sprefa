% plunit_tests.pl : self-grading item 1 (plunit over analyze/strat/lower).
% Stratum order for both target fixtures, and per-rule SQL text snapshots
% for every edge/level statement lower.pl emits. These are UNIT tests over
% the Prolog compiler stages themselves (analyze -> strat -> lower), never
% touching sqlite3 -- test/run_sql_check.pl is the separate execution-level
% harness (self-grading item 2).
%
% Run: swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module(library(plunit)).
:- use_module(library(apply)).
:- use_module('../compile', [ read_fixture_term/4, program_plan/2 ]).
:- use_module('../strat', [ stratum_groups/2 ]).
:- use_module('../lower', [ lower_program/2 ]).
:- use_module('../analyze', [ check_supported_subset/1 ]).
:- use_module('../../0_enum_expand', [ expand_enum_program/2 ]).
:- use_module('../parse_dl', [ parse_dl/4 ]).

% Resolved relative to this file's own load-time directory (mirrors
% sweep.pl's compile_dir/1 pattern -- prolog_load_context/2 only answers
% inside a directive running WHILE this file loads, so the directory is
% captured once, here, into a fact) rather than a hardcoded absolute path --
% a hardcoded path to a worktree that no longer exists is a portability bug,
% not a style nit (a prior version of this line named a stale worktree that
% happened to still exist on this machine by coincidence).
:- dynamic(test_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(test_dir_fact(Here)).

fixture_file(File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/scopes.pl'], File).

% once/1 around both: plunit warns ("Test succeeded with choicepoint") on a
% test whose body leaves one open, and neither read_fixture_term/4 nor
% program_plan/2 promises single-solution determinism on its own (nothing
% downstream needs a second solution, so committing here is correct, not a
% workaround).
load_plan(Name, Plan) :-
    once(( fixture_file(File),
           read_fixture_term(File, Name, Term, Bindings),
           program_plan(Term-Bindings, Plan) )).

lowered_for(Name, Lowered) :-
    once(( load_plan(Name, Plan), lower_program(Plan, Lowered) )).

:- begin_tests(stratum_order).

% Ground truth taken directly from probing level_eval.pl:stratify_level_rules/2
% itself (not reimplemented blind) before strat.pl was written: BOTH target
% fixtures' level rules collapse into exactly ONE stratum group, since a
% positive dependency (Gap=0) never forces separation -- only a negated read
% does, and neither fixture negates anything. strat.pl:stratum_groups/2 must
% reproduce that grouping exactly.

test(switch_as_keyed_replace_one_group) :-
    load_plan(switch_as_keyed_replace, plan(_, prog(_, Rules), _, _, _, _)),
    stratum_groups(Rules, Groups),
    length(Groups, 1),
    Groups = [Group],
    length(Group, 2).

test(demand_laziness_one_group) :-
    load_plan(demand_laziness_effect_rows, plan(_, prog(_, Rules), _, _, _, _)),
    stratum_groups(Rules, Groups),
    length(Groups, 1),
    Groups = [Group],
    length(Group, 2).

% sql_rule_order/2 (via program_plan/2's RuleOrder) topo-sorts WITHIN that
% one group: demanded must precede route_view (route_view's body reads
% demanded); demanded must precede effect_call likewise.

test(switch_as_keyed_replace_rule_order) :-
    load_plan(switch_as_keyed_replace, plan(_, _, _, _, RuleOrder, _)),
    RuleOrder = [(DemandedHead <- _), (RouteViewHead <- _)],
    functor(DemandedHead, demanded, 2),
    functor(RouteViewHead, route_view, 2).

test(demand_laziness_rule_order) :-
    load_plan(demand_laziness_effect_rows, plan(_, _, _, _, RuleOrder, _)),
    RuleOrder = [(DemandedHead <- _), (EffectCallHead <- _)],
    functor(DemandedHead, demanded, 2),
    functor(EffectCallHead, effect_call, 1).

test(self_recursive_level_rule_remains_in_p2_order) :-
    Rules = [(path(X, Y) <- path(X, Z), edge(Z, Y))],
    once(strat:sql_rule_order(Rules, Rules)).

:- end_tests(stratum_order).

:- begin_tests(column_naming).

% analyze.pl:rel_columns/4 mines column names from the fixture's OWN surface
% variable names (via read_fixture_term/4's variable_names preservation),
% not from any hardcoded per-fixture table. relplan(Ref, Kind, Columns, Key,
% ColumnTypes) -- ColumnTypes (PHASE C2 RULING 1) all TEXT here: neither
% fixture's own literal values (Schedule/Initial/rule literals) ever put an
% integer at any of these positions, so analyze.pl:rel_column_types/5's
% "zero int witnesses -> text" default is exactly what fires, including for
% `target` (a compound route_data(...) column, which never gets an atomic
% witness at all and stays text per the ruling's flat-punt).

test(switch_as_keyed_replace_columns) :-
    load_plan(switch_as_keyed_replace, plan(_, _, RelPlans, _, _, _)),
    memberchk(relplan(open_scope/2, set, [session_id, target], key([1]), [text, text]), RelPlans),
    memberchk(relplan(demanded/2, set, [target, session_id], none, [text, text]), RelPlans),
    memberchk(relplan(route_view/2, set, [route_id, body], none, [text, text]), RelPlans),
    memberchk(relplan(route_change/2, log, [session_id, route_id], none, [text, text]), RelPlans),
    memberchk(relplan(route_row/2, set, [route_id, body], none, [text, text]), RelPlans).

test(demand_laziness_columns) :-
    load_plan(demand_laziness_effect_rows, plan(_, _, RelPlans, _, _, _)),
    memberchk(relplan(open_feed/2, set, [session_id, target], key([1]), [text, text]), RelPlans),
    memberchk(relplan(demanded/2, set, [target, session_id], none, [text, text]), RelPlans),
    memberchk(relplan(effect_call/1, set, [target], none, [text]), RelPlans).

:- end_tests(column_naming).

:- begin_tests(sql_text_snapshots).

% Per-rule SQL text, pinned exactly. A change here is either a deliberate
% respell (update the snapshot in the same commit as the reason) or a
% regression (the test is the reason it got caught).

% Round 2: no tick number reaches tick(), so edge writes lower to a
% parameterless-FROM projection (numbered placeholders ?1/?2 bound directly
% to the trigger arrival row's own values) plus a static UPSERT, not a
% self-join filtered by a stamp column.
test(switch_as_keyed_replace_edge_sql) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, _, _, [edgestmt(open_scope/2, route_change/2, HeadColumns, KeyColumns, ProjectSql, UpsertSql, DeltaProjectSql)], _, _, _, _),
    HeadColumns == [session_id, target],
    KeyColumns == [session_id],
    ProjectSql ==
      'SELECT ?1 AS "session_id", json_object(\'fn\', \'route_data\', \'args\', json_array(?2)) AS "target"',
    UpsertSql ==
      'INSERT INTO "open_scope" ("session_id", "target") VALUES (?, ?) ON CONFLICT("session_id") DO UPDATE SET "target" = excluded."target"',
    DeltaProjectSql ==
      'SELECT d0."session_id" AS "session_id", json_object(\'fn\', \'route_data\', \'args\', json_array(d0."route_id")) AS "target" FROM "__frontier_route_change" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"'.

% An edge-headed keyed rel's table must carry PRIMARY KEY on the KEY
% COLUMNS ALONE, matching the UPSERT's ON CONFLICT target -- SQLite
% requires an EXACT constraint match ("ON CONFLICT clause does not match
% any PRIMARY KEY or UNIQUE constraint" otherwise), a real error only the
% real sqlite3 CLI / real seam surfaced, never a Prolog-level check. A
% non-edge-headed Set rel (route_row, arrival-target only) still gets PK on
% ALL columns (exact-row dedup, matching absorb_arrivals/8).
test(switch_as_keyed_replace_ddl_pk_shape) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, Ddl, _, _, _, _, _, _),
    include(ddl_for_table(open_scope), Ddl, [OpenScopeDdl]),
    once(sub_atom(OpenScopeDdl, _, _, _, 'PRIMARY KEY ("session_id")')),
    \+ sub_atom(OpenScopeDdl, _, _, _, 'PRIMARY KEY ("session_id", "target")'),
    include(ddl_for_table(route_row), Ddl, [RouteRowDdl]),
    once(sub_atom(RouteRowDdl, _, _, _, 'PRIMARY KEY ("route_id", "body")')).

test(switch_as_keyed_replace_frontier_ddl) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, Ddl, _, _, _, _, _, _),
    memberchk('CREATE TEMP TABLE "__frontier_route_change" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "session_id" TEXT NOT NULL, "route_id" TEXT NOT NULL)', Ddl),
    memberchk('CREATE INDEX "__frontier_route_change_phase" ON "__frontier_route_change" ("_phase")', Ddl),
    memberchk('CREATE TEMP TABLE "__next_frontier_open_scope" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "session_id" TEXT NOT NULL, "target" TEXT NOT NULL)', Ddl).

ddl_for_table(Table, Ddl) :-
    format(atom(Needle), 'CREATE TABLE "~w" (', [Table]),
    sub_atom(Ddl, 0, _, _, Needle).

% InsertSqls is a LIST (one entry per rule clause sharing the head ref --
% lower.pl:level_statement_group/3, the phase C multi-clause-per-head fix);
% both fixtures here have exactly one clause per head, so each list is a
% singleton.
test(switch_as_keyed_replace_level_sql) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    LevelStatements = [levelstmt(demanded/2, DemandedDelete, [DemandedInsert], _, _), levelstmt(route_view/2, RouteViewDelete, [RouteViewInsert], _, _)],
    DemandedDelete == 'DELETE FROM "demanded"',
    DemandedInsert == 'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT b0."target", b0."session_id" FROM "open_scope" b0',
    RouteViewDelete == 'DELETE FROM "route_view"',
    RouteViewInsert ==
      'INSERT OR IGNORE INTO "route_view" ("route_id", "body") SELECT json_extract(b0."target", \'$.args[0]\'), b1."body" FROM "demanded" b0, "route_row" b1 WHERE json_extract(b0."target", \'$.fn\') = \'route_data\' AND b1."route_id" = json_extract(b0."target", \'$.args[0]\')'.

test(demand_laziness_no_edge_rules) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, _, [], _, _, _, _).

test(demand_laziness_incremental_arrival_is_one_batch_statement) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, ArrivalStatements, _, _, _, _, _),
    memberchk(arrivalstmt(open_feed/2, set, _, _, IncrementalAddSql, _),
              ArrivalStatements),
    IncrementalAddSql ==
      'INSERT OR IGNORE INTO "open_feed" ("session_id", "target") SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?) RETURNING "session_id", "target"'.

test(demand_laziness_level_sql) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    LevelStatements = [levelstmt(demanded/2, _, [DemandedInsert], DemandedDeltaInsert, _), levelstmt(effect_call/1, _, [EffectCallInsert], EffectCallDeltaInsert, _)],
    DemandedInsert == 'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT b0."target", b0."session_id" FROM "open_feed" b0',
    EffectCallInsert == 'INSERT OR IGNORE INTO "effect_call" ("target") SELECT b0."target" FROM "demanded" b0',
    DemandedDeltaInsert ==
      'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT DISTINCT d0."target", d0."session_id" FROM "__frontier_open_feed" d0 WHERE d0."_phase" >= 0 RETURNING "target", "session_id"',
    EffectCallDeltaInsert ==
      'INSERT OR IGNORE INTO "effect_call" ("target") SELECT DISTINCT d0."target" FROM "__frontier_demanded" d0 WHERE d0."_phase" >= 0 RETURNING "target"'.

% Round 2: one plain "read every row" query per rel (log and set alike) --
% no __prev shadow table, no EXCEPT, no tick filter. The runtime (or, for
% this harness, test/run_sql_check.pl's own Prolog-side multiset_diff/4)
% diffs the before/after row lists.
% Round 3 (reconciliation): the delta-snapshot SELECT renders canonical
% Prolog term text for the tick-log envelope (json1 stays the storage
% encoding; only the log-facing read converts), via
% lower:canonical_column_expr/2, applied per column. Pinned exactly here
% (a small, stable unit) since the full per-rel SelectSql built from it is
% long enough that pinning it verbatim would be a brittle, unreadable test
% -- those two tests below check the STRUCTURAL pieces (FROM table, the
% json_valid/json_type guard, the AS alias) instead.

test(canonical_column_expr_shape) :-
    lower:canonical_column_expr(target, Expr),
    Expr ==
      'CASE WHEN json_valid("target") AND json_type("target") = \'object\' THEN json_extract("target", \'$.fn\') || \'(\' || (SELECT group_concat(value, \',\') FROM json_each("target", \'$.args\')) || \')\' ELSE "target" END AS "target"'.

test(switch_as_keyed_replace_delta_sql_open_scope) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, _, _, _, _, DeltaStatements, _, _),
    memberchk(deltastmt(open_scope/2, SelectSql, __delta_open_scope, BoundarySql), DeltaStatements),
    once(sub_atom(SelectSql, _, _, _, 'FROM "open_scope"')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("target")')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("session_id")')),
    once(sub_atom(SelectSql, _, _, _, 'AS "session_id"')),
    once(sub_atom(SelectSql, _, _, _, 'AS "target"')),
    once(sub_atom(BoundarySql, _, _, _, 'FROM "__delta_open_scope"')),
    once(sub_atom(BoundarySql, _, _, _, '"_sign" IN (-1, 1)')).

test(switch_as_keyed_replace_delta_sql_route_change_log) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, _, _, _, _, DeltaStatements, _, _),
    memberchk(deltastmt(route_change/2, SelectSql, __delta_route_change, _), DeltaStatements),
    once(sub_atom(SelectSql, _, _, _, 'FROM "route_change"')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("route_id")')),
    once(sub_atom(SelectSql, _, _, _, 'AS "route_id"')).

:- end_tests(sql_text_snapshots).

:- begin_tests(incremental_mode).

test(positive_edge_level_program_is_incremental) :-
    load_plan(switch_as_keyed_replace, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, LevelStatements, _, _, _),
    emit_ts:incremental_program_safe(Plan, EdgeStatements, LevelStatements, true).

test(negative_level_body_uses_incremental_reconcile) :-
    load_plan(merge_policy, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, LevelStatements, _, _, _),
    emit_ts:incremental_program_safe(Plan, EdgeStatements, LevelStatements, true),
    emit_ts:reconcile_every_tick(Plan, true).

test(acyclic_support_count_statements_are_emitted) :-
    lowered_for(shared_demand_refcount, Lowered),
    Lowered = lowered(_, Ddl, _, _, LevelStatements, _, _, _),
    memberchk('CREATE TEMP TABLE "__support_next_effect_call" ("target" TEXT NOT NULL, "__support_count" INTEGER NOT NULL, PRIMARY KEY ("target")) WITHOUT ROWID', Ddl),
    memberchk(levelstmt(effect_call/1, _, _, _,
                        supportsql(ClearSql, SeedSql, UpdateSql,
                                   CollectZeroSql, InsertNewSql)),
              LevelStatements),
    ClearSql == 'DELETE FROM "__support_next_effect_call"',
    once(sub_atom(SeedSql, _, _, _, 'count(*) AS "__support_count"')),
    once(sub_atom(UpdateSql, _, _, _, 'SET "__support_count" = "__support_count" -')),
    CollectZeroSql == 'DELETE FROM "effect_call" WHERE "__support_count" <= 0 RETURNING "target"',
    once(sub_atom(InsertNewSql, _, _, _, 'WHERE NOT EXISTS')).

test(self_recursive_support_uses_recursive_cte_reseed) :-
    RelPlans = [
        relplan(root/1, set, [node], none, [int]),
        relplan(edge/2, set, [parent, child], none, [int, int]),
        relplan(path/1, set, [node], none, [int])
    ],
    Rules = [
        (path(Node) <- root(Node)),
        (path(Child) <- path(Parent), edge(Parent, Child))
    ],
    lower:level_support_sql(
        RelPlans, path/1, Rules,
        supportsql(_, SeedSql, _, _, _)),
    once(sub_atom(
        SeedSql, _, _, _,
        'WITH RECURSIVE "path" ("node") AS')),
    once(sub_atom(SeedSql, _, _, _, 'FROM "path" b0')),
    Plan = plan(test, prog([], Rules), RelPlans, [], Rules, []),
    emit_ts:retraction_guard(Plan, 'recursive-cte-reseed').

test(set_delete_arrival_is_one_json_batch_statement) :-
    lowered_for(shared_demand_refcount, Lowered),
    Lowered = lowered(_, _, ArrivalStatements, _, _, _, _, _),
    memberchk(arrivalstmt(open_feed/2, set, _, _, _, IncrementalDelSql),
              ArrivalStatements),
    IncrementalDelSql ==
      'DELETE FROM "open_feed" WHERE ("session_id", "target") IN (SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?)) RETURNING "session_id", "target"'.

:- end_tests(incremental_mode).

:- begin_tests(supported_subset_gate).

% analyze.pl:check_supported_subset/1 refuses constructs lower.pl cannot
% lower yet, with a specific term rather than a generic failure -- verify the
% guard itself fires rather than silently passing through.

test(rejects_aggregate_head, [throws(unsupported_construct(aggregate_head(_)))]) :-
    Prog = prog([], [ (total(count(X)) <- item(X)) ]),
    check_supported_subset(Prog).

% PHASE C2 RULING 2 renamed this refusal from the blanket edge_body_shape to
% the precise edge_body_needs_negation (analyze.pl:edge_trigger_shape/2):
% a marked-single trigger with any extra body goal is the OUT-OF-SCOPE
% "marked + extra guard" bucket (SCOREBOARD.md's 9-fixture tally), distinct
% from the unmarked-conjunction shape this ruling widened.
test(rejects_edge_body_with_extra_goal, [throws(unsupported_construct(edge_body_needs_negation(_)))]) :-
    Prog = prog([keyed(scope/1, [1])], [ (scope(X) <+ (open(X), not(closed(X)))) ]),
    check_supported_subset(Prog).

test(rejects_pre_in_level_body, [throws(unsupported_construct(level_body_goal(_, pre(_))))]) :-
    Prog = prog([], [ (snapshot(X) <- pre(item(X))) ]),
    check_supported_subset(Prog).

:- end_tests(supported_subset_gate).

:- begin_tests(expression_miscompile_guards).

% ═══ FAIL-FIRST CHECK (a): TEXT-collapse "1" vs 1 ═══════════════════════════
% Written BEFORE the expression lift, per the arc contract, and red at three
% distinct stages on the way in:
%
%   RED 1 (pre-lift)  : program_plan/2 throws
%                       unsupported_construct(head_arithmetic(...)) -- the
%                       phase-C guard that turned this exact miscompile into
%                       a refusal.
%   RED 2 (naive lift): the guard is gone and the arithmetic fuses into SQL,
%                       but union_size/3's third column has NO literal
%                       witness of its own (its only occurrences are the
%                       head's `LeftSize + RightSize - Shared` compound and
%                       jaccard's body variable `Union`), so PHASE C2 RULING
%                       1's "zero witnesses -> text" default stores the
%                       computed 12 in a TEXT column. The tick-log/final-state
%                       encoder then prints "12" where the oracle prints 12,
%                       AND `Union > 0` compares a TEXT-affinity column
%                       against an integer literal.
%   GREEN             : the level-head expression type reaches the column
%                       (analyze.pl program_column_types/7), union_size col3
%                       is INTEGER, and 12 crosses the boundary as 12.
%
% The type list, not the DDL text, is the assertion: lower.pl:column_def/3 is
% the single reader of it, so a wrong type here is a wrong CREATE TABLE by
% construction.

test(head_arithmetic_column_is_int_not_text_collapse) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, head_expression_evaluates_derived_column, Term, Bindings),
           program_plan(Term-Bindings, plan(_, _, RelPlans, _, _, _)) )),
    memberchk(relplan(union_size/3, _, _, _, UnionTypes), RelPlans),
    assertion(UnionTypes == [text, text, int]),
    memberchk(relplan(callee_set_size/2, _, _, _, CalleeTypes), RelPlans),
    assertion(CalleeTypes == [text, int]).

% Same collapse one hop further out: `Sum := Base + Extra` binds a variable
% the head then reads. The bind's own type has to reach over_budget/2's second
% column or the comparison `Sum > 10` runs against TEXT affinity.
test(bind_result_column_is_int_not_text_collapse) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, bind_computes_derived_value_then_comparison_filters,
                             Term, Bindings),
           program_plan(Term-Bindings, plan(_, _, RelPlans, _, _, _)) )),
    memberchk(relplan(over_budget/2, _, _, _, Types), RelPlans),
    assertion(Types == [text, int]).

% concat/1 is the other direction of the same boundary: an Int piece
% auto-converts to text inside the interpolation lowering target
% (engine.pl:eval_expr concat -> atomic_list_concat), so the head column that
% receives it must stay TEXT even though one of its inputs is an integer
% column. A naive "any arithmetic-ish expression is int" rule would collapse
% it the other way.
test(concat_result_column_stays_text) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, interpolation_desugars_to_concat, Term, Bindings),
           program_plan(Term-Bindings, plan(_, _, RelPlans, _, _, _)) )),
    memberchk(relplan(message/3, _, _, _, Types), RelPlans),
    assertion(Types == [text, int, text]).

expressions_fixture_file(File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/expressions.pl'], File).

:- end_tests(expression_miscompile_guards).

:- begin_tests(enum_decl_expansion).

test(parser_retains_semicolon_enum_decl) :-
    string_codes("rel body(page(view: view) ; redirect(to: text)).", Codes),
    parse_dl(Codes, Prog, Bindings, Findings),
    assertion(Prog =@= prog([
        enum_decl(body, (page(view:view) ; redirect(to:text)))
    ], [])),
    assertion(Bindings == []),
    assertion(Findings == []).

test(expands_to_typed_variant_rels_and_tag_union) :-
    Sugared = prog([
        enum_decl(body, (page(view:view) ; redirect(to:text)))
    ], []),
    expand_enum_program(Sugared, Expanded),
    Expected = prog(
        [
            col_type(body_page/2, id, int),
            col_type(body_page/2, view, int),
            keyed(body_page/2, [2]),
            col_type(body_redirect/2, id, int),
            col_type(body_redirect/2, to, text),
            keyed(body_redirect/2, [2]),
            col_type(body_tag/2, id, int),
            col_type(body_tag/2, tag, text)
        ],
        [
            (body_tag(PageId, page) <- body_page(PageId, _PageView)),
            (body_tag(RedirectId, redirect) <- body_redirect(RedirectId, _RedirectTo))
        ]),
    assertion(Expanded =@= Expected).

test(refuses_variant_name_collision,
     [throws(unsupported_construct(enum_variant_name_collision(page)))]) :-
    Sugared = prog(
        [
            enum_decl(body, page(view:view)),
            col_type(page/1, id, int)
        ],
        []),
    expand_enum_program(Sugared, _).

:- end_tests(enum_decl_expansion).
