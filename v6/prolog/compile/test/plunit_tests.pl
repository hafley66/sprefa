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

fixture_file('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1aa8144f466d366d/v6/prolog/conformance/fixtures/scopes.pl').

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

:- end_tests(stratum_order).

:- begin_tests(column_naming).

% analyze.pl:rel_columns/4 mines column names from the fixture's OWN surface
% variable names (via read_fixture_term/4's variable_names preservation),
% not from any hardcoded per-fixture table. relplan(Ref, Kind, Columns, Key).

test(switch_as_keyed_replace_columns) :-
    load_plan(switch_as_keyed_replace, plan(_, _, RelPlans, _, _, _)),
    memberchk(relplan(open_scope/2, set, [session_id, target], key([1])), RelPlans),
    memberchk(relplan(demanded/2, set, [target, session_id], none), RelPlans),
    memberchk(relplan(route_view/2, set, [route_id, body], none), RelPlans),
    memberchk(relplan(route_change/2, log, [session_id, route_id], none), RelPlans),
    memberchk(relplan(route_row/2, set, [route_id, body], none), RelPlans).

test(demand_laziness_columns) :-
    load_plan(demand_laziness_effect_rows, plan(_, _, RelPlans, _, _, _)),
    memberchk(relplan(open_feed/2, set, [session_id, target], key([1])), RelPlans),
    memberchk(relplan(demanded/2, set, [target, session_id], none), RelPlans),
    memberchk(relplan(effect_call/1, set, [target], none), RelPlans).

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
    Lowered = lowered(_, _, _, [edgestmt(open_scope/2, route_change/2, HeadColumns, KeyColumns, ProjectSql, UpsertSql)], _, _, _, _),
    HeadColumns == [session_id, target],
    KeyColumns == [session_id],
    ProjectSql ==
      'SELECT ?1 AS "session_id", json_object(\'fn\', \'route_data\', \'args\', json_array(?2)) AS "target"',
    UpsertSql ==
      'INSERT INTO "open_scope" ("session_id", "target") VALUES (?, ?) ON CONFLICT("session_id") DO UPDATE SET "target" = excluded."target"'.

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
    LevelStatements = [levelstmt(demanded/2, DemandedDelete, [DemandedInsert]), levelstmt(route_view/2, RouteViewDelete, [RouteViewInsert])],
    DemandedDelete == 'DELETE FROM "demanded"',
    DemandedInsert == 'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT b0."target", b0."session_id" FROM "open_scope" b0',
    RouteViewDelete == 'DELETE FROM "route_view"',
    RouteViewInsert ==
      'INSERT OR IGNORE INTO "route_view" ("route_id", "body") SELECT json_extract(b0."target", \'$.args[0]\'), b1."body" FROM "demanded" b0, "route_row" b1 WHERE json_extract(b0."target", \'$.fn\') = \'route_data\' AND b1."route_id" = json_extract(b0."target", \'$.args[0]\')'.

test(demand_laziness_no_edge_rules) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, _, [], _, _, _, _).

test(demand_laziness_level_sql) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    LevelStatements = [levelstmt(demanded/2, _, [DemandedInsert]), levelstmt(effect_call/1, _, [EffectCallInsert])],
    DemandedInsert == 'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT b0."target", b0."session_id" FROM "open_feed" b0',
    EffectCallInsert == 'INSERT OR IGNORE INTO "effect_call" ("target") SELECT b0."target" FROM "demanded" b0'.

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
    memberchk(deltastmt(open_scope/2, SelectSql), DeltaStatements),
    once(sub_atom(SelectSql, _, _, _, 'FROM "open_scope"')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("target")')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("session_id")')),
    once(sub_atom(SelectSql, _, _, _, 'AS "session_id"')),
    once(sub_atom(SelectSql, _, _, _, 'AS "target"')).

test(switch_as_keyed_replace_delta_sql_route_change_log) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, _, _, _, _, DeltaStatements, _, _),
    memberchk(deltastmt(route_change/2, SelectSql), DeltaStatements),
    once(sub_atom(SelectSql, _, _, _, 'FROM "route_change"')),
    once(sub_atom(SelectSql, _, _, _, 'json_valid("route_id")')),
    once(sub_atom(SelectSql, _, _, _, 'AS "route_id"')).

:- end_tests(sql_text_snapshots).

:- begin_tests(supported_subset_gate).

% analyze.pl:check_supported_subset/1 refuses constructs lower.pl cannot
% lower yet, with a specific term rather than a generic failure -- verify the
% guard itself fires rather than silently passing through.

test(rejects_aggregate_head, [throws(unsupported_construct(aggregate_head(_)))]) :-
    Prog = prog([], [ (total(count(X)) <- item(X)) ]),
    check_supported_subset(Prog).

test(rejects_edge_body_with_extra_goal, [throws(unsupported_construct(edge_body_shape(_, _)))]) :-
    Prog = prog([keyed(scope/1, [1])], [ (scope(X) <+ (only(open(X)), not(closed(X)))) ]),
    check_supported_subset(Prog).

test(rejects_pre_in_level_body, [throws(unsupported_construct(level_body_goal(_, pre(_))))]) :-
    Prog = prog([], [ (snapshot(X) <- pre(item(X))) ]),
    check_supported_subset(Prog).

:- end_tests(supported_subset_gate).
