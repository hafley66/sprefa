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
:- use_module('../../0_match_expand', [ expand_match_program/2 ]).
:- use_module('../parse_dl', [ parse_dl/4 ]).
:- use_module('../print_dl', [ print_dl_program/3 ]).
:- use_module('../../1_host_expand',
              [ prepare_program/5, compile_host_decl/2, compile_ts_query/2 ]).
:- use_module('../emit_ts', [ emit_program/5 ]).
:- use_module('../lower', [ boot_statements/4 ]).

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

fixture_file(Base, File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/', Base], File).

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

lowered_for(Base, Name, Lowered) :-
    once(( fixture_file(Base, File),
           read_fixture_term(File, Name, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )).

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

% FAIL-FIRST RECEIPT: world-fed keyed arrival replacement.
%
% RED:
%   [10/70] sql_text_snapshots:
%     world_fed_keyed_arrival_uses_key_constraint_and_replace **FAILED
%   test sql_text_snapshots:
%     world_fed_keyed_arrival_uses_key_constraint_and_replace: failed
% GREEN:
%   [10/70] sql_text_snapshots:
%     world_fed_keyed_arrival_uses_key_constraint_and_replace passed
% EMITTER RED, both modes:
%   WRONG world_fed_keyed_arrival_replaces first diff at line 2:
%     actual={"tick":2,"deltas":{"world_mode":{
%       "add":[[1,"b"]],"del":[]}}}
%     oracle={"tick":2,"deltas":{"world_mode":{
%       "add":[[1,"b"]],"del":[[1,"a"]]}}}
%   FINAL_WRONG world_fed_keyed_arrival_replaces
%     actual={"final":{"world_mode":[[1,"a"],[1,"b"]]}}
%     oracle={"final":{"world_mode":[[1,"b"]]}}
% EMITTER GREEN, both modes:
%   RUN total=70 identical=67 wrong=0 run_error=2 no_oracle_log=1
%   FINAL total=70 final_identical=67 final_wrong=2 no_oracle_final=1
test(world_fed_keyed_arrival_uses_key_constraint_and_replace) :-
    lowered_for('engine_core.pl', world_fed_keyed_arrival_replaces, Lowered),
    Lowered = lowered(_, Ddl, ArrivalStatements, _, _, _, _, _),
    include(ddl_for_table(world_mode), Ddl, [WorldModeDdl]),
    once(sub_atom(WorldModeDdl, _, _, _, 'PRIMARY KEY ("col1")')),
    \+ sub_atom(WorldModeDdl, _, _, _, 'PRIMARY KEY ("col1", "col2")'),
    memberchk(
        arrivalstmt(
            world_mode/2,
            set,
            'INSERT OR REPLACE INTO "world_mode" ("col1", "col2") VALUES (?, ?)',
            'DELETE FROM "world_mode" WHERE "col1" = ? AND "col2" = ?',
            'INSERT OR REPLACE INTO "world_mode" ("col1", "col2") SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?) RETURNING "col1", "col2"',
            'DELETE FROM "world_mode" WHERE ("col1", "col2") IN (SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?)) RETURNING "col1", "col2"'),
        ArrivalStatements).

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
    LevelStatements = [levelstmt(demanded/2, DemandedDelete, [DemandedInsert], _, _, none), levelstmt(route_view/2, RouteViewDelete, [RouteViewInsert], _, _, none)],
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
      'INSERT OR REPLACE INTO "open_feed" ("session_id", "target") SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?) RETURNING "session_id", "target"'.

test(demand_laziness_level_sql) :-
    lowered_for(demand_laziness_effect_rows, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    LevelStatements = [levelstmt(demanded/2, _, [DemandedInsert], DemandedDeltaInsert, _, none), levelstmt(effect_call/1, _, [EffectCallInsert], EffectCallDeltaInsert, _, none)],
    DemandedInsert == 'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT b0."target", b0."session_id" FROM "open_feed" b0',
    EffectCallInsert == 'INSERT OR IGNORE INTO "effect_call" ("target") SELECT b0."target" FROM "demanded" b0',
    DemandedDeltaInsert ==
      'INSERT OR IGNORE INTO "demanded" ("target", "session_id") SELECT DISTINCT d0."target", d0."session_id" FROM "__frontier_open_feed" d0 WHERE d0."_phase" >= 0 RETURNING "target", "session_id"',
    EffectCallDeltaInsert ==
      'INSERT OR IGNORE INTO "effect_call" ("target") SELECT DISTINCT d0."target" FROM "__frontier_demanded" d0 WHERE d0."_phase" >= 0 RETURNING "target"'.

test(edge_derived_trigger_reads_promoted_frontier) :-
    lowered_for('engine_core.pl', edge_chain_hops_tick_per_stage, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    memberchk(
        edgestmt(stage_two/1, stage_one/1, [item], [], _, _,
                 'SELECT d0."item" AS "item" FROM "__frontier_stage_one" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"'),
        EdgeStatements).

test(level_derived_trigger_reads_same_tick_frontier) :-
    lowered_for('occurrence_identity.pl', demand_view_fires_its_consumer_once,
                Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    memberchk(
        edgestmt(fetch_call/1, fetch_demand/1, [endpoint], [], _, _,
                 'SELECT d0."endpoint" AS "endpoint" FROM "__frontier_fetch_demand" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"'),
        EdgeStatements).

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

test(derived_edge_trigger_requires_incremental_carry_path) :-
    fixture_file('engine_core.pl', File),
    once(( read_fixture_term(File, edge_chain_hops_tick_per_stage, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    emit_ts:derived_edge_carry_required(Plan, EdgeStatements, true).

test(edb_edge_trigger_keeps_naive_referee_available) :-
    load_plan(switch_as_keyed_replace, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    emit_ts:derived_edge_carry_required(Plan, EdgeStatements, false).

test(acyclic_support_count_statements_are_emitted) :-
    lowered_for(shared_demand_refcount, Lowered),
    Lowered = lowered(_, Ddl, _, _, LevelStatements, _, _, _),
    memberchk('CREATE TEMP TABLE "__support_next_effect_call" ("target" TEXT NOT NULL, "__support_count" INTEGER NOT NULL, PRIMARY KEY ("target")) WITHOUT ROWID', Ddl),
    memberchk(levelstmt(effect_call/1, _, _, _,
                        supportsql(ClearSql, SeedSql, UpdateSql,
                                   CollectZeroSql, InsertNewSql),
                        none),
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

% EXPRESSION + AGGREGATE LIFT: count/sum/min/max are LOWERED now, so the
% blanket aggregate refusal is gone and the gate must accept them.
test(accepts_count_aggregate_head) :-
    Prog = prog([], [ (total(count(X)) <- item(X)) ]),
    check_supported_subset(Prog).

% json_array/json_object stay refused, and the reason is NOT "not implemented
% yet": a Prolog list value renders through the shared tick-log encoder
% (ticklog.pl term_text/2) as right-nested cons text -- [|](4,[|](4,[|](9,[])))
% -- and json_object as obj([|](-(k,v),[])). Neither is what
% json_group_array/json_group_object produce, so no ORDER BY pinning makes
% them byte-identical. Same encoding gap braces_in_head_position already
% fails on in the final-state leg, which predates this arc.
test(rejects_json_array_aggregate_head,
     [throws(unsupported_construct(aggregate_head(_)))]) :-
    Prog = prog([], [ (bag(json_array(X)) <- item(X)) ]),
    check_supported_subset(Prog).

test(rejects_json_object_aggregate_head,
     [throws(unsupported_construct(aggregate_head(_)))]) :-
    Prog = prog([], [ (doc(json_object(Key, Value)) <- pair(Key, Value)) ]),
    check_supported_subset(Prog).

% An aggregate whose body reads its own head: engine.pl forces Gap=1 for
% every body ref of an aggregate head (level_eval.pl rule_body_constraint/4),
% so the oracle throws not_stratified. Refused by a precise name here.
test(rejects_self_reading_aggregate_head,
     [throws(unsupported_construct(aggregate_head_reads_itself(total/1)))]) :-
    Prog = prog([], [ (total(count(X)) <- total(X)) ]),
    check_supported_subset(Prog).

% A comparison under not/1 would be silently dropped: compile_negative_uses/4
% renders a negated atom as a bare NOT EXISTS over rel columns and never sees
% the conjunction's other goals.
test(rejects_guard_under_negation,
     [throws(unsupported_construct(negated_guard_goal(_, _)))]) :-
    Prog = prog([], [ (flagged(Name) <- item(Name, Size), not((budget(Name, Cap), Size > Cap))) ]),
    check_supported_subset(Prog).

% PHASE C2 RULING 2 renamed this refusal from the blanket edge_body_shape to
% the precise edge_body_needs_negation (analyze.pl:edge_trigger_shape/2):
% a marked-single trigger with any extra body goal is the OUT-OF-SCOPE
% "marked + extra guard" bucket (SCOREBOARD.md's 9-fixture tally), distinct
% from the unmarked-conjunction shape this ruling widened.
test(rejects_edge_body_with_extra_goal, [throws(unsupported_construct(edge_body_needs_negation(_)))]) :-
    Prog = prog([keyed(scope/1, [1])], [ (scope(X) <+ (open(X), not(closed(X)))) ]),
    check_supported_subset(Prog).

% FAIL-FIRST RECEIPTS: silent-inert level-rule forms.
%
% RED:
%   rejects_log_on_level_headed_rel: no_exception
%   rejects_latest_in_level_rule: no_exception
%   rejects_pre_in_level_rule: wrong error
%     Expected: unsupported_construct(pre_in_level_rule(item/1))
%     Got: unsupported_construct(
%       level_body_goal(snapshot(A),pre(item(A))))
% GREEN:
%   [34/70] rejects_log_on_level_headed_rel passed
%   [35/70] rejects_latest_in_level_rule passed
%   [36/70] rejects_pre_in_level_rule passed
% ENGINE RED:
%   132 PASS / 3 fail
%   fail log_on_level_headed_rel_rejected
%   fail latest_in_level_rule_rejected
%   fail pre_in_level_rule_rejected
% ENGINE GREEN:
%   135 PASS / 0 fail
% COMPILER GREEN:
%   log_on_level_headed_rel_rejected:
%     unsupported log_on_level_headed_rel(derived_event/1)
%   latest_in_level_rule_rejected:
%     unsupported latest_in_level_rule(source_item/1)
%   pre_in_level_rule_rejected:
%     unsupported pre_in_level_rule(source_item/1)
test(rejects_log_on_level_headed_rel,
     [throws(unsupported_construct(log_on_level_headed_rel(derived_event/1)))]) :-
    Prog = prog(
        [kind(derived_event/1, log), keep(derived_event/1, all)],
        [ (derived_event(X) <- item(X)) ]),
    check_supported_subset(Prog).

test(rejects_latest_in_level_rule,
     [throws(unsupported_construct(latest_in_level_rule(item/1)))]) :-
    Prog = prog([], [ (snapshot(X) <- item(X), latest(item(X))) ]),
    check_supported_subset(Prog).

test(rejects_pre_in_level_rule,
     [throws(unsupported_construct(pre_in_level_rule(item/1)))]) :-
    Prog = prog([], [ (snapshot(X) <- item(X), pre(item(X))) ]),
    check_supported_subset(Prog).

test(rejects_keep_on_non_log_rel,
     [throws(unsupported_construct(keep_on_non_log_rel(state/1)))]) :-
    check_supported_subset(prog([keep(state/1, all)], [])).

test(accepts_level_derived_edge_trigger) :-
    Prog = prog(
        [kind(source/1, log), keep(source/1, all),
         kind(sink/1, log), keep(sink/1, all)],
        [(view(X) <- source(X)), (sink(X) <+ view(X))]),
    check_supported_subset(Prog).

test(accepts_edge_derived_edge_trigger) :-
    Prog = prog(
        [kind(source/1, log), keep(source/1, all),
         kind(stage_one/1, log), keep(stage_one/1, all),
         kind(stage_two/1, log), keep(stage_two/1, all)],
        [(stage_one(X) <+ source(X)), (stage_two(X) <+ stage_one(X))]),
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

% ═══ Q4 reconciliation (plans/2026-07-29-sqlite-udf-graft-verdict.md) ═══════
% The assertions from the sqlite_udf verdict's expression-lift set that bind
% on THIS arc. The UDF-specific ones (P1.5 NULL behavior, P1.7 registration on
% every connection, P3.3/P3.4 sprf_sym staging) do not apply: no UDF is
% grafted here, and LIBSQL_UDF_API is still an unresolved slot in that verdict.

% Q4 P1.1 / P2.1: typed columns carry explicit INTEGER or TEXT affinity, and
% the __delta_ / __frontier_ / __next_frontier_ TEMP tables repeat the SAME
% types rather than defaulting to no affinity -- a delta row that lost its
% affinity would compare differently from the base row it mirrors.
test(delta_and_frontier_tables_repeat_column_affinity) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, head_expression_evaluates_derived_column, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )),
    Lowered = lowered(_, Ddl, _, _, _, _, _, _),
    forall(member(Prefix, ['', '__delta_', '__frontier_', '__next_frontier_']),
           ( atomic_list_concat(['CREATE TEMP TABLE "', Prefix, 'callee_set_size"'], TempHead),
             atomic_list_concat(['CREATE TABLE "', Prefix, 'callee_set_size"'], BaseHead),
             once(( member(Sql, Ddl),
                    ( sub_atom(Sql, 0, _, _, TempHead) ; sub_atom(Sql, 0, _, _, BaseHead) ),
                    sub_atom(Sql, _, _, _, '"left" TEXT NOT NULL'),
                    sub_atom(Sql, _, _, _, '"left_size" INTEGER NOT NULL') )) )).

% Q4 P1.8: a comparison compiles to typed SQLite values, never to rendered
% text, and a cross-type comparison is REFUSED rather than answered under
% affinity conversion. Prolog ==/2 is term identity, so '1' never equals 1.
test(cross_type_comparison_is_refused,
     [throws(unsupported_construct(comparison_type_mismatch(_, _, _)))]) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, text_one_and_numeric_one_are_not_equal, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, _) )).

% Q4 P1.2: text "1" and numeric 1 stay distinct. The same boundary in JOIN
% position: engine.pl joins by unification, SQLite joins under affinity
% conversion and would answer the opposite (measured, see
% lower.pl:join_column_types_agree/4).
test(cross_type_join_is_refused,
     [throws(unsupported_construct(join_column_type_mismatch(_, _, _, _)))]) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, text_one_and_numeric_one_never_join, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, _) )).

% Q4 P1.4: every expression carries a declared result type that reaches the
% destination column. head_arithmetic_column_is_int_not_text_collapse above
% asserts the type; this asserts it survives into the DDL that stores it,
% since column_def/3 is the only reader.
test(expression_result_type_reaches_the_ddl) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, head_expression_evaluates_derived_column, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )),
    Lowered = lowered(_, Ddl, _, _, _, _, _, _),
    once(( member(Sql, Ddl),
           sub_atom(Sql, 0, _, _, 'CREATE TABLE "union_size"'),
           sub_atom(Sql, _, _, _, '"col3" INTEGER NOT NULL') )).

% engine.pl's `mod` is FLOORED (sign of the divisor); SQLite's `%` is C's
% (sign of the dividend). The emitted text must be the floored correction, not
% a bare `%`, or division_truncates_toward_zero_mod_follows_divisor_sign gets
% two of its four rows wrong.
test(mod_lowers_to_the_floored_correction) :-
    expressions_fixture_file(File),
    once(( read_fixture_term(File, division_truncates_toward_zero_mod_follows_divisor_sign,
                             Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    memberchk(levelstmt(probe/3, _, [InsertSql], _, _, _), LevelStatements),
    once(sub_atom(InsertSql, _, _, _, '% b0."denominator") + b0."denominator") % b0."denominator")')).

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

test(enum_tag_view_can_trigger_keyed_edge_head) :-
    string_codes(
        "rel door(closed(note: text) ; open(note: text)).\nrel current(id: int, tag: text) key(1).\ncurrent(Id, Tag) <+ door_tag(Id, Tag).\n",
        Codes),
    parse_dl(Codes, Prog, Bindings, Findings),
    assertion(Findings == []),
    Schedule = [
        [+door_closed(1, "boot")],
        [+door_open(1, "ready")]
    ],
    once(program_plan(
        fixture(door_enum_edge_acceptance, Prog, [], Schedule, [])-Bindings,
        Plan)),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    memberchk(
        edgestmt(current/2, door_tag/2, [id, tag], [id], _, _,
                 'SELECT d0."id" AS "id", d0."tag" AS "tag" FROM "__frontier_door_tag" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"'),
        EdgeStatements).

:- end_tests(enum_decl_expansion).

:- begin_tests(match_block).

test(shared_expansion_produces_one_ordinary_rule_per_arm) :-
    Sugared = prog(
        [],
        [
            match(
                source(Key, Value),
                ((accepted(Key) <- Value >= 10) ;
                 (latest(Key, Value) <+ true)))
        ]),
    expand_match_program(Sugared, prog([], ExpandedRules)),
    ExpandedRules =@=
        [
            (accepted(Key) <- source(Key, Value), Value >= 10),
            (latest(Key, Value) <+ source(Key, Value))
        ].

test(enum_match_requires_every_variant,
     [throws(unsupported_construct(match_nonexhaustive(body, redirect)))]) :-
    expand_match_program(
        prog(
            [enum_decl(body, (page(view:text) ; redirect(to:text)))],
            [
                match(
                    decoded(Id, Tag, Value),
                    (body_page(Id, Value) <+ Tag == page))
            ]),
        _).

test(keyed_level_head_is_a_named_compile_refusal,
     [throws(unsupported_construct(keyed_level_head(current/2)))]) :-
    check_supported_subset(
        prog(
            [keyed(current/2, [1])],
            [(current(Key, Value) <- source(Key, Value))])).

test(keyed_edge_head_remains_supported) :-
    check_supported_subset(
        prog(
            [
                kind(source/2, log),
                keep(source/2, all),
                keyed(current/2, [1])
            ],
            [(current(Key, Value) <+ source(Key, Value))])).

test(match_surface_round_trips_with_semicolon_arms) :-
    string_codes(
        "match source(Key, Value) (\n    accepted(Key) <- Value >= 10\n  ; latest(Key, Value) <+ true\n).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    print_dl_program(Program, Bindings, Text),
    atom_codes(Text, PrintedCodes),
    parse_dl(PrintedCodes, RoundTripped, _, []),
    Program =@= RoundTripped.

test(sugar_and_hand_written_desugar_lower_to_identical_sql) :-
    lowered_for('1_match_block.pl', match_classify_response, SugaredLowered),
    lowered_for('1_match_block.pl', match_classify_response_desugared,
                DesugaredLowered),
    SugaredLowered =.. [lowered, _SugaredName | SugaredFields],
    DesugaredLowered =.. [lowered, _DesugaredName | DesugaredFields],
    SugaredFields =@= DesugaredFields.

test(retention_count_is_one_set_based_delete_statement) :-
    lowered_for('engine_core.pl', retention_count_prunes_oldest, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    memberchk(
        retentionstmt(
            event/1,
            2,
            'DELETE FROM "event" WHERE rowid NOT IN (SELECT rowid FROM "event" ORDER BY rowid DESC LIMIT 2) RETURNING "col1"'),
        LevelStatements).

:- end_tests(match_block).

:- begin_tests(hosts_wiring).

test(selected_surface_round_trips) :-
    string_codes(
      "sh fetch(ep: text) -> (status: int) = `run {ep}`.\nresult(Status) <- input(Ep), ? fetch(Ep, Status) @ salt(bucket: 3).\n? result(Status).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    Program = program(
                [sh_decl(fetch, [col(ep, text)], [col(status, int)],
                         template("run {ep}"))],
                [(_ <- (_, probe(fetch, [_], [_], [salt(bucket, 3)])))],
                [query(result(_))]),
    print_dl_program(Program, Bindings, Printed),
    atom_codes(Printed, PrintedCodes),
    parse_dl(PrintedCodes, Reparsed, _, []),
    Program =@= Reparsed.

test(named_body_omissions_are_fresh) :-
    string_codes(
      "rel source(a: text, b: text, c: text).\nout(X) <- source(a: X).\n",
      Codes),
    parse_dl(Codes, prog(_, [(_ <- source(X, OmittedB, OmittedC))]), _, []),
    var(X),
    var(OmittedB),
    var(OmittedC),
    OmittedB \== OmittedC,
    X \== OmittedB,
    X \== OmittedC.

test(named_partial_head_is_refused) :-
    string_codes(
      "rel source(a: text, b: text, c: text).\nsource(a: X) <- seed(X).\n",
      Codes),
    parse_dl(Codes, _, _,
             [unsupported_surface(partial_head(source/3))]).

test(host_unreferenced_input_refusal,
     [throws(template_mismatch(unreferenced_input(prev)))]) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int)],
              template("{ep}")),
      _).

test(host_output_reference_refusal,
     [throws(template_mismatch(output_used_as_input(status)))]) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text)],
              [col(status, int)],
              template("{ep} $status")),
      _).

test(host_unknown_column_refusal,
     [throws(template_mismatch(unknown_column(missing)))]) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text)],
              [col(status, int)],
              template("{ep} {missing}")),
      _).

test(host_shell_local_dollar_name_is_not_a_column) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int)],
              template("R={ep}; P=$prev; printf '%s' \"$R\"")),
      _).

test(host_overlap_refusal,
     [throws(column_mismatch(input_output_overlap(ep)))]) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text)],
              [col(ep, text)],
              template("{ep}")),
      _).

test(host_duplicate_column_refusal,
     [throws(column_mismatch(input, duplicate(ep)))]) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text), col(ep, text)],
              [col(status, int)],
              template("{ep}")),
      _).

test(probe_arity_refusal,
     [throws(probe_mismatch(probe(fetch, [repo], [], [])))]) :-
    prepare_program(
      program(
        [sh_decl(fetch, [col(ep, text)], [col(status, int)],
                 template("{ep}"))],
        [(result(_Status) <- probe(fetch, [repo], [], []))],
        []),
      _, _, _, _).

test(bind_and_rule_head_refusal,
     [throws(bind_and_rule_head(interval))]) :-
    prepare_program(
      program(
        [bind_decl(interval, [col(period, int), col(bucket, int)])],
        [(interval(Period, Bucket) <- seed(Period, Bucket))],
        []),
      _, _, _, _).

test(native_ts_query_exact_text) :-
    compile_ts_query(
      ts_query(
        [ group(
            node(call_expression,
                 [field(function,
                        capture(callee, node(identifier, [])))]),
            [predicate(eq, capture_ref(callee), string("fetch"))]),
          quant(optional, node(comment, [])),
          quant(zero_or_more, wildcard)
        ]),
      Text),
    Text ==
      "((call_expression function: (identifier) @callee) (#eq? @callee \"fetch\"))\n(comment)?\n_*".

test(emitter_carries_world_plans_and_demand_sql) :-
    fixture_file('2_hosts_wiring.pl', File),
    read_fixture_term(File, native_ts_query_term, Term, Bindings),
    program_plan(Term-Bindings, Plan),
    lower_program(Plan, Lowered),
    Term = fixture(_, _, Initial, _, _),
    Plan = plan(_, _, RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(RelPlans, Initial, LevelStatements, Boot),
    emit_program(native_ts_query_term, Plan, Lowered, Boot, Text),
    once(sub_atom(Text, _, _, _, 'export const hostPlans')),
    once(sub_atom(Text, _, _, _,
                  'unsupported_host_execution_phase_2(tree_sitter)')),
    once(sub_atom(Text, _, _, _,
                  'unsupported_bind_execution_phase_2(interval)')),
    once(sub_atom(Text, _, _, _,
                  'CREATE TABLE "__host_demand_tree_sitter"')),
    once(sub_atom(Text, _, _, _,
                  'CREATE TABLE "__host_response_tree_sitter"')),
    !.

:- end_tests(hosts_wiring).
