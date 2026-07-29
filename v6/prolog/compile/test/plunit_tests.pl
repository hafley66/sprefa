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

% Body-walk characterization (rank R1) reaches the traversals on BOTH sides of
% the oracle/compiler split, because the review's central claim is that
% several of them are the same predicate written twice. Each of these was
% added to its module's export list for exactly this test rather than being
% called as a private qualified goal, which `just prolog-lint` refuses.
:- use_module('../analyze',
              [ body_ref_uses/2, conjunction_goals/2,
                level_body_latest_ref/2, level_body_pre_ref/2,
                reserved_construct_in_body/2, body_forbidden_goal/2 ]).
:- use_module('../../conformance/engine',
              [ trigger_items/2, body_finalize_ref/2,
                body_latest_ref/2, body_pre_ref/2,
                check_program/1 ]).
:- use_module('../../conformance/level_eval', [ goal_rel_refs/3 ]).
:- use_module('../../conformance/body', [ body_atoms/2 ]).
:- use_module('../../1_host_expand', [ body_goals/2 ]).

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

test(latest_edge_sample_reads_base_table_in_both_sql_families) :-
    lowered_for('engine_core.pl', marker_stops_backlog_replay, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    EdgeStatements = [
        edgestmt(
            sent/2,
            change_ev/1,
            [client, item],
            [],
            'SELECT b0."client" AS "client", ?1 AS "item" FROM "subscriber" b0',
            _,
            'SELECT b0."client" AS "client", d0."item" AS "item" FROM "__frontier_change_ev" d0, "subscriber" b0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"')
    ].

test(latest_keyed_sample_is_one_edge_arm_with_key_predicates) :-
    lowered_for('shell_stream.pl', identical_demand_dedups, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    findall(
        EdgeStatement,
        (member(EdgeStatement, EdgeStatements),
         EdgeStatement = edgestmt(_, fill/3, _, _, _, _, _)),
        SampledEdgeStatements),
    SampledEdgeStatements = [
        edgestmt(
            response/3,
            fill/3,
            [args, salt, payload],
            [],
            'SELECT ?1 AS "args", ?2 AS "salt", ?3 AS "payload" FROM "demand" b0 WHERE b0."args" = ?1 AND b0."salt" = ?2',
            _,
            'SELECT d0."args" AS "args", d0."salt" AS "salt", d0."payload" AS "payload" FROM "__frontier_fill" d0, "demand" b0 WHERE d0."_phase" >= 0 AND b0."args" = d0."args" AND b0."salt" = d0."salt" ORDER BY d0."_phase", d0."_sequence"')
    ].

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

% FAIL-FIRST RECEIPT: latest/1 in an edge body.
%
% RED:
%   [21/73] latest_edge_sample_reads_base_table_in_both_sql_families
%     unsupported_construct(edge_body_with_latest(...))
%   [35/73] accepts_latest_plain_rel_sample_in_edge_body
%     unsupported_construct(edge_body_with_latest(...))
% GREEN:
%   73/73 passed before the statement-count assertion above was added.
test(accepts_latest_plain_rel_sample_in_edge_body) :-
    Prog = prog(
        [kind(change_ev/1, log), keep(change_ev/1, all),
         kind(subscriber/1, log), keep(subscriber/1, all),
         kind(sent/2, log), keep(sent/2, all)],
        [(sent(Client, Item) <+ change_ev(Item), latest(subscriber(Client)))]),
    check_supported_subset(Prog).

test(rejects_latest_wrapped_conjunction_in_edge_body,
     [throws(unsupported_construct(edge_body_with_latest(_)))]) :-
    Prog = prog(
        [kind(change_ev/1, log), keep(change_ev/1, all),
         kind(sent/2, log), keep(sent/2, all)],
        [(sent(Client, Item) <+
            change_ev(Item), latest((subscriber(Client), enabled(Client))))]),
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
    % PHASE 2 (runtime bridge arc): the two named refusals are gone; both world
    % terms now carry the executor the served runtime dispatches on. The bind's
    % `periods` list is EMPTY for this fixture on purpose -- it declares
    % `bind interval(...)` and seeds an `interval(300, 1)` Initial row, but no
    % RULE reads a literal period, so no timer is owed.
    once(sub_atom(Text, _, _, _, 'execution: "live_sh"')),
    once(sub_atom(Text, _, _, _, 'periods: [], execution: "live_interval"')),
    once(sub_atom(Text, _, _, _,
                  'export const unsupportedExecution: readonly string[] = [];')),
    once(sub_atom(Text, _, _, _,
                  'CREATE TABLE "__host_demand_tree_sitter"')),
    once(sub_atom(Text, _, _, _,
                  'CREATE TABLE "__host_response_tree_sitter"')),
    !.

:- end_tests(hosts_wiring).

% ═══════════════════════════════════════════════════════════════════════════
% BODY WALK CHARACTERIZATION (rank R1 of plans/2026-07-29-prolog-org-review.md)
%
% Written BEFORE the shared walker existed, against the fourteen independent
% body traversals the review inventoried, and committed green so the
% consolidation has an exact contract to preserve. Every golden below was
% computed from the pre-refactor implementations, not hand-written: a
% consolidation that changes ANY of these values changes observable compiler
% or oracle behavior, whatever the tick logs happen to say.
%
% The battery covers every shape the review named: comma association in both
% nestings, nested not/1, not over a conjunction, a not whose inner
% conjunction mixes signs, next/1, variadic combine, latest, finalize, pre, a
% reserved lifecycle wrapper, a := bind, a comparison, a plain relation atom,
% the true word, and one body carrying all of them at once.
%
% Bodies are GROUND on purpose. Every golden is then an exact literal and the
% comparison is ==, with no variable-identity slack to hide a reordering.
%
% THREE DRIFTS THESE GOLDENS PIN, all pre-existing and all preserved by the
% refactor rather than silently fixed (each is a registry row the hardcoded
% lists never learned about):
%
%   1. engine:trigger_items/2 makes an ARRIVAL out of next/1, combine, a
%      comparison, and a reserved lifecycle wrapper. See the `mixed` golden:
%      arrival(next(d(4))), arrival(combine(e(5),f(6))), arrival(8<9). These
%      are inert downstream because occurrence_trigger/4 unifies the item
%      against a real stored row and none of these shapes can match one, but
%      the classification is wrong at the source.
%   2. level_eval:goal_rel_refs/3 reports next/1 and combine/2 as POSITIVE
%      relation references, so stratification carries constraints naming
%      relations that cannot exist.
%   3. body:body_atoms/2 repeats drift 1 in its own hardcoded list.
%
% AND ONE ORDERING CONTRACT that is the reason goal_rel_refs/3 keeps a local
% not/1 recursion instead of projecting from the shared walker: see the
% `not_mixed` golden. For not((not(a(1)), b(2))) it answers [b/1,a/1], not
% source order, because its not/1 clause appends inner-positive before
% inner-negative. A source-ordered projection would answer [a/1,b/1] and
% change stratification constraint order.

:- begin_tests(body_walk_characterization).

walk_case(comma_right,   (a(1), (b(2), c(3)))).
walk_case(comma_left,    ((a(1), b(2)), c(3))).
walk_case(nested_not,    not(not(a(1)))).
walk_case(not_over_conj, not((a(1), latest(b(2))))).
walk_case(not_mixed,     not((not(a(1)), b(2)))).
walk_case(next_wrapper,  next(a(1))).
walk_case(combine3,      combine(a(1), b(2), c(3))).
walk_case(latest_only,   latest(a(1))).
walk_case(finalize_only, finalize(a(1))).
walk_case(pre_only,      pre(a(1))).
walk_case(lifecycle,     unsubscribe(a(1))).
walk_case(bind_goal,     (zz := 1)).
walk_case(comparison,    (1 < 9)).
walk_case(plain_atom,    a(1)).
walk_case(true_word,     true).
walk_case(mixed, ( a(1), not((b(2), latest(c(3)))), next(d(4)),
                   combine(e(5), f(6)), zz := 7, 8 < 9,
                   finalize(g(10)), pre(h(11)) )).

walk_golden(comma_right,
  [ body_ref_uses-[use(a/1,[1],pos,trigger),use(b/1,[2],pos,trigger),use(c/1,[3],pos,trigger)],
    conjunction_goals-[a(1),b(2),c(3)],
    trigger_items-[arrival(a(1)),arrival(b(2)),arrival(c(3))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1,b/1,c/1]-[]),
    body_atoms-[a(1),b(2),c(3)],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[a(1),b(2),c(3)]
  ]).

walk_golden(comma_left,
  [ body_ref_uses-[use(a/1,[1],pos,trigger),use(b/1,[2],pos,trigger),use(c/1,[3],pos,trigger)],
    conjunction_goals-[a(1),b(2),c(3)],
    trigger_items-[arrival(a(1)),arrival(b(2)),arrival(c(3))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1,b/1,c/1]-[]),
    body_atoms-[a(1),b(2),c(3)],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[a(1),b(2),c(3)]
  ]).

walk_golden(nested_not,
  [ body_ref_uses-[use(a/1,[1],neg,trigger)],
    conjunction_goals-[not(not(a(1)))],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[a/1]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[not(not(a(1)))]
  ]).

walk_golden(not_over_conj,
  [ body_ref_uses-[use(a/1,[1],neg,trigger),use(b/1,[2],neg,sampled)],
    conjunction_goals-[not((a(1),latest(b(2))))],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[b/1],
    engine_pre_refs-[],
    analyze_latest_refs-[b/1],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[a/1,b/1]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[not((a(1),latest(b(2))))]
  ]).

walk_golden(not_mixed,
  [ body_ref_uses-[use(a/1,[1],neg,trigger),use(b/1,[2],neg,trigger)],
    conjunction_goals-[not((not(a(1)),b(2)))],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[b/1,a/1]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[not((not(a(1)),b(2)))]
  ]).

walk_golden(next_wrapper,
  [ body_ref_uses-[use(a/1,[1],pos,trigger)],
    conjunction_goals-[a(1)],
    trigger_items-[arrival(next(a(1)))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([next/1]-[]),
    body_atoms-[next(a(1))],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[next(a(1))]
  ]).

walk_golden(combine3,
  [ body_ref_uses-[use(a/1,[1],pos,trigger),use(b/1,[2],pos,trigger),use(c/1,[3],pos,trigger)],
    conjunction_goals-[a(1),b(2),c(3)],
    trigger_items-[arrival(combine(a(1),b(2),c(3)))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([combine/3]-[]),
    body_atoms-[combine(a(1),b(2),c(3))],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[combine(a(1),b(2),c(3))]
  ]).

walk_golden(latest_only,
  [ body_ref_uses-[use(a/1,[1],pos,sampled)],
    conjunction_goals-[latest(a(1))],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[a/1],
    engine_pre_refs-[],
    analyze_latest_refs-[a/1],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[latest(a(1))]
  ]).

walk_golden(finalize_only,
  [ body_ref_uses-[use(a/1,[1],pos,trigger)],
    conjunction_goals-[finalize(a(1))],
    trigger_items-[departure(a(1))],
    engine_finalize_refs-[a/1],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[finalize(a(1))],
    host_body_goals-[finalize(a(1))]
  ]).

walk_golden(pre_only,
  [ body_ref_uses-[use(a/1,[1],pos,sampled)],
    conjunction_goals-[pre(a(1))],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[a/1],
    analyze_latest_refs-[],
    analyze_pre_refs-[a/1],
    goal_rel_refs-([]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[pre(a(1))],
    host_body_goals-[pre(a(1))]
  ]).

walk_golden(lifecycle,
  [ body_ref_uses-[use(a/1,[1],pos,trigger)],
    conjunction_goals-[unsubscribe(a(1))],
    trigger_items-[arrival(unsubscribe(a(1)))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([unsubscribe/1]-[]),
    body_atoms-[unsubscribe(a(1))],
    reserved_constructs-[lifecycle_arm(unsubscribe)],
    forbidden_goals-[],
    host_body_goals-[unsubscribe(a(1))]
  ]).

walk_golden(bind_goal,
  [ body_ref_uses-[],
    conjunction_goals-[zz:=1],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[zz:=1]
  ]).

walk_golden(comparison,
  [ body_ref_uses-[],
    conjunction_goals-[1<9],
    trigger_items-[arrival(1<9)],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[1<9]
  ]).

walk_golden(plain_atom,
  [ body_ref_uses-[use(a/1,[1],pos,trigger)],
    conjunction_goals-[a(1)],
    trigger_items-[arrival(a(1))],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1]-[]),
    body_atoms-[a(1)],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[a(1)]
  ]).

walk_golden(true_word,
  [ body_ref_uses-[],
    conjunction_goals-[true],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[true]
  ]).

walk_golden(mixed,
  [ body_ref_uses-[use(a/1,[1],pos,trigger),use(b/1,[2],neg,trigger),use(c/1,[3],neg,sampled),use(d/1,[4],pos,trigger),use(e/1,[5],pos,trigger),use(f/1,[6],pos,trigger),use(g/1,[10],pos,trigger),use(h/1,[11],pos,sampled)],
    conjunction_goals-[a(1),not((b(2),latest(c(3)))),d(4),e(5),f(6),zz:=7,8<9,finalize(g(10)),pre(h(11))],
    trigger_items-[arrival(a(1)),arrival(next(d(4))),arrival(combine(e(5),f(6))),arrival(8<9),departure(g(10))],
    engine_finalize_refs-[g/1],
    engine_latest_refs-[c/1],
    engine_pre_refs-[h/1],
    analyze_latest_refs-[c/1],
    analyze_pre_refs-[h/1],
    goal_rel_refs-([a/1,next/1,combine/2]-[b/1,c/1]),
    body_atoms-[a(1),next(d(4)),combine(e(5),f(6))],
    reserved_constructs-[],
    forbidden_goals-[finalize(g(10)),pre(h(11))],
    host_body_goals-[a(1),not((b(2),latest(c(3)))),next(d(4)),combine(e(5),f(6)),zz:=7,8<9,finalize(g(10)),pre(h(11))]
  ]).

% Actual value of one projection over one case, as the golden records it.
walk_actual(body_ref_uses,       Body, Uses)  :- body_ref_uses(Body, Uses).
walk_actual(conjunction_goals,   Body, Goals) :- conjunction_goals(Body, Goals).
walk_actual(trigger_items,       Body, Items) :- trigger_items(Body, Items).
walk_actual(engine_finalize_refs,  Body, Refs) :- findall(R, body_finalize_ref(Body, R), Refs).
walk_actual(engine_latest_refs,    Body, Refs) :- findall(R, body_latest_ref(Body, R), Refs).
walk_actual(engine_pre_refs,       Body, Refs) :- findall(R, body_pre_ref(Body, R), Refs).
walk_actual(analyze_latest_refs,   Body, Refs) :- findall(R, level_body_latest_ref(Body, R), Refs).
walk_actual(analyze_pre_refs,      Body, Refs) :- findall(R, level_body_pre_ref(Body, R), Refs).
walk_actual(goal_rel_refs,       Body, Pos-Neg) :- goal_rel_refs(Body, Pos, Neg).
walk_actual(body_atoms,          Body, Atoms) :- body_atoms(Body, Atoms).
walk_actual(reserved_constructs, Body, Found) :- findall(C, reserved_construct_in_body(Body, C), Found).
walk_actual(forbidden_goals,     Body, Found) :- findall(G, body_forbidden_goal(Body, G), Found).
walk_actual(host_body_goals,     Body, Goals) :- body_goals(Body, Goals).

% Every case, one projection, compared with ==. The failure message names the
% case and both values, so a consolidation regression says which shape broke.
check_projection(Projection) :-
    forall(( walk_case(Name, Body), walk_golden(Name, Rows),
             memberchk(Projection-Expected, Rows) ),
           ( walk_actual(Projection, Body, Actual),
             (   Actual == Expected
             ->  true
             ;   format("~n~w/~w~n  expected ~q~n  actual   ~q~n",
                        [Projection, Name, Expected, Actual]),
                 fail
             ) )).

test(body_ref_uses)        :- check_projection(body_ref_uses).
test(conjunction_goals)    :- check_projection(conjunction_goals).
test(trigger_items)        :- check_projection(trigger_items).
test(engine_finalize_refs) :- check_projection(engine_finalize_refs).
test(engine_latest_refs)   :- check_projection(engine_latest_refs).
test(engine_pre_refs)      :- check_projection(engine_pre_refs).
test(analyze_latest_refs)  :- check_projection(analyze_latest_refs).
test(analyze_pre_refs)     :- check_projection(analyze_pre_refs).
test(goal_rel_refs)        :- check_projection(goal_rel_refs).
test(body_atoms)           :- check_projection(body_atoms).
test(reserved_constructs)  :- check_projection(reserved_constructs).
test(forbidden_goals)      :- check_projection(forbidden_goals).
test(host_body_goals)      :- check_projection(host_body_goals).

% The engine and the compiler ship SEPARATE latest/1 and pre/1 body scans.
% The review's rank 1 claim is that they are the same predicate twice; this
% pins that equality directly, so a consolidation onto one implementation is
% checked against the claim rather than against one side's own golden.
test(engine_and_compiler_latest_scans_agree) :-
    forall(walk_case(_, Body),
           ( findall(R, body_latest_ref(Body, R), EngineRefs),
             findall(R, level_body_latest_ref(Body, R), CompilerRefs),
             EngineRefs == CompilerRefs )).

test(engine_and_compiler_pre_scans_agree) :-
    forall(walk_case(_, Body),
           ( findall(R, body_pre_ref(Body, R), EngineRefs),
             findall(R, level_body_pre_ref(Body, R), CompilerRefs),
             EngineRefs == CompilerRefs )).

:- end_tests(body_walk_characterization).

% ═══════════════════════════════════════════════════════════════════════════
% CROSS-PLANE PROGRAM CHECK PARITY (rank R2)
%
% The review found six invalid-program trigger classes implemented twice, once
% in the oracle's engine:check_program/1 and once in the compiler's
% analyze:check_supported_subset/1, plus two classes the ORACLE alone checks.
% These tests state, per class, what each door does with the same prog/2 term.
%
% Both doors keep their own exception vocabulary on purpose. The oracle throws
% a bare term; the compiler wraps in unsupported_construct/1 and, for keyed
% Log, carries the key positions the emitter would have needed. Those terms are
% fixture-visible data, so the shared trigger implementation must not
% normalize them.
%
% TWO OF THESE WERE WRITTEN RED. Before the shared check module existed the
% compiler ACCEPTED both programs the oracle rejects:
%
%   compiler_refuses_log_without_retention
%     was: check_supported_subset/1 succeeded, so the program compiled and
%     v6/prolog/compile/out/manifest.json carried
%     log_without_retention_rejected in bucket "compiled" with an empty
%     reason, against an oracle that throws missing_retention(event/1).
%   compiler_refuses_aggregate_in_edge_head
%     was: check_supported_subset/1 succeeded on (total(count(N)) <+ hit(N)),
%     so a compound aggregate argument reached generic head-expression
%     lowering, against an oracle that throws aggregate_in_edge_head.
%
% Both are green below and the manifest bucket moved with them.

:- begin_tests(cross_plane_check_parity).

% Throws Term, or the atom accepted if the door lets the program through.
door_verdict(oracle, Prog, Verdict) :-
    (   catch(check_program(Prog), Thrown, true)
    ->  ( var(Thrown) -> Verdict = accepted ; Verdict = Thrown )
    ;   Verdict = failed
    ).
door_verdict(compiler, Prog, Verdict) :-
    (   catch(check_supported_subset(Prog), Thrown, true)
    ->  ( var(Thrown) -> Verdict = accepted ; Verdict = Thrown )
    ;   Verdict = failed
    ).

% ── the six mirrored classes ─────────────────────────────────────────────────

test(keyed_level_head_both_doors) :-
    Prog = prog([ keyed(current/2, [1]) ], [ (current(Id, Tag) <- src(Id, Tag)) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == keyed_level_head(current/2),
    CompilerVerdict == unsupported_construct(keyed_level_head(current/2)).

% The compiler payload carries the key POSITIONS as well as the reference; the
% oracle's carries only the reference. Pinned so the shared trigger cannot
% quietly normalize one door's payload into the other's.
test(keyed_log_rel_payloads_differ_by_design) :-
    Prog = prog([ kind(latest/2, log), keep(latest/2, all),
                  keyed(latest/2, [1]) ], []),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == keyed_log_rel(latest/2),
    CompilerVerdict == unsupported_construct(keyed_log_rel(latest/2, [1])).

test(log_on_level_headed_rel_both_doors) :-
    Prog = prog([ kind(view/1, log), keep(view/1, all) ],
                [ (view(Item) <- src(Item)) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == log_on_level_headed_rel(view/1),
    CompilerVerdict == unsupported_construct(log_on_level_headed_rel(view/1)).

test(keep_on_non_log_rel_both_doors) :-
    Prog = prog([ keep(state/1, all) ], []),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == keep_on_non_log_rel(state/1),
    CompilerVerdict == unsupported_construct(keep_on_non_log_rel(state/1)).

test(latest_in_level_rule_both_doors) :-
    Prog = prog([], [ (out(Item) <- (src(Item), latest(cfg(Item)))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == latest_in_level_rule(cfg/1),
    CompilerVerdict == unsupported_construct(latest_in_level_rule(cfg/1)).

test(pre_in_level_rule_both_doors) :-
    Prog = prog([], [ (out(Item) <- (src(Item), pre(cfg(Item)))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == pre_in_level_rule(cfg/1),
    CompilerVerdict == unsupported_construct(pre_in_level_rule(cfg/1)).

% ── the finalize diagnostic drift, stated rather than repaired ───────────────
% Both doors refuse a finalize/1 in a level rule, and they name it differently:
% the oracle has a dedicated check, the compiler reaches it through the generic
% refused-goal path and reports the enclosing head. The review flagged this as
% diagnostic drift; it is fixture-visible on both sides, so R2 preserves it and
% records it here instead.
test(finalize_in_level_rule_diagnostics_drift) :-
    Prog = prog([], [ (out(Item) <- (src(Item), finalize(gone(Item)))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == finalize_in_level_rule(gone/1),
    % =@= over a term whose variable is SHARED between the head and the
    % finalize atom, because that is how the compiler reports it: the payload
    % keeps the rule's own variable rather than copying. Two anonymous holes
    % are not a variant of one repeated hole, so writing `out(_)` and
    % `gone(_)` here would fail for the wrong reason.
    Expected = unsupported_construct(
                 level_body_goal(out(Item), finalize(gone(Item)))),
    CompilerVerdict =@= Expected.

% ── nested not/1 parity ─────────────────────────────────────────────────────
% Both level-rule scans descend not/1, so a latest or pre buried under any
% depth of negation still refuses, and both doors agree on which reference is
% named. This is the shared-walker property (rank R1) read through the checks
% that consume it.
test(nested_not_latest_parity) :-
    Prog = prog([], [ (out(Item) <- (src(Item),
                                     not(not(latest(cfg(Item)))))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == latest_in_level_rule(cfg/1),
    CompilerVerdict == unsupported_construct(latest_in_level_rule(cfg/1)).

test(nested_not_pre_parity) :-
    Prog = prog([], [ (out(Item) <- (src(Item),
                                     not(not(pre(cfg(Item)))))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == pre_in_level_rule(cfg/1),
    CompilerVerdict == unsupported_construct(pre_in_level_rule(cfg/1)).

% A finalize under not/1 is opaque to BOTH doors, which is the asymmetry the
% review named: the finalize scan does not descend negation on either side.
% Pinned so closing one side alone becomes a visible change.
test(nested_not_finalize_is_opaque_to_both_doors) :-
    Prog = prog([], [ (out(Item) <- (src(Item), not(finalize(gone(Item))))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == accepted,
    CompilerVerdict == accepted.

% ── the two classes the oracle alone used to check ──────────────────────────

% FAIL-FIRST: CompilerVerdict was `accepted` before the shared check module.
test(compiler_refuses_log_without_retention) :-
    Prog = prog([ kind(event/1, log) ], []),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == missing_retention(event/1),
    CompilerVerdict == unsupported_construct(missing_retention(event/1)).

% FAIL-FIRST: CompilerVerdict was `accepted` before the shared check module.
test(compiler_refuses_aggregate_in_edge_head) :-
    Prog = prog([ kind(hit/1, log), keep(hit/1, all) ],
                [ (total(count(Item)) <+ hit(Item)) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == aggregate_in_edge_head,
    CompilerVerdict == unsupported_construct(aggregate_in_edge_head(total/1)).

% A plain edge head is untouched by the aggregate-edge closure.
test(plain_edge_head_still_accepted_by_both_doors) :-
    Prog = prog([ kind(hit/1, log), keep(hit/1, all),
                  keyed(total/1, [1]) ],
                [ (total(Item) <+ hit(Item)) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == accepted,
    CompilerVerdict == accepted.

:- end_tests(cross_plane_check_parity).

% ═══════════════════════════════════════════════════════════════════════════
% DECLARATION QUERY PARITY (rank R9)
%
% The oracle and the compiler each carried their own relation-kind resolver,
% clause for clause identical except that the oracle's took an extra Rules
% argument it never read. The fallback is the part that matters: an undeclared
% relation is a Set, and a keyed relation is a Set by construction, so clause
% ORDER decides what a relation declared both log and keyed resolves to.
%
% Written against the two separate implementations and green there, so the
% parity claim is pinned before one replaces both. The two door_ adapters
% below are the only lines that moved when the oracle dropped its unused
% argument; every assertion is unchanged.

:- begin_tests(declaration_query_parity).

oracle_relation_kind(Decls, Ref, Kind) :- engine:rel_kind(Decls, Ref, Kind).
compiler_relation_kind(Decls, Ref, Kind) :- analyze:rel_kind(Decls, Ref, Kind).
oracle_key(Decls, Ref, Positions) :- engine:decl_key(Decls, Ref, Positions).
compiler_key(Decls, Ref, Positions) :- analyze:decl_key(Decls, Ref, Positions).

% Decls, expected kind for r/1.
kind_case([],                                        set).
kind_case([kind(r/1, log), keep(r/1, all)],          log).
kind_case([kind(r/1, set)],                          set).
kind_case([keyed(r/1, [1])],                         set).
% Declared kind is consulted BEFORE the keyed fallback, so this is log.
kind_case([kind(r/1, log), keep(r/1, all), keyed(r/1, [1])], log).
kind_case([kind(r/1, set), keyed(r/1, [1])],         set).
% A declaration naming a DIFFERENT relation must not leak onto r/1.
kind_case([kind(other/1, log), keep(other/1, all)],  set).
kind_case([keyed(other/1, [1])],                     set).

test(relation_kind_agrees_across_doors) :-
    forall(kind_case(Decls, Expected),
           ( oracle_relation_kind(Decls, r/1, OracleKind),
             compiler_relation_kind(Decls, r/1, CompilerKind),
             OracleKind == Expected,
             CompilerKind == Expected )).

% Both doors read the same key positions out of the same declaration, and both
% FAIL rather than defaulting when the relation carries no key. The `none`
% below is this test's marker for that failure, never a value either door
% produces.
key_case([keyed(r/1, [1])],                    [1]).
key_case([keyed(r/2, [1, 2]), keyed(r/1, [1])], [1]).
key_case([kind(r/1, log), keep(r/1, all)],     none).
key_case([],                                   none).

test(decl_key_agrees_across_doors) :-
    forall(key_case(Decls, Expected),
           ( (   oracle_key(Decls, r/1, OraclePositions)
             ->  true
             ;   OraclePositions = none ),
             (   compiler_key(Decls, r/1, CompilerPositions)
             ->  true
             ;   CompilerPositions = none ),
             OraclePositions == Expected,
             CompilerPositions == Expected )).

:- end_tests(declaration_query_parity).
