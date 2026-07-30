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
:- use_module('../../0_refusal_messages',
              [ refusal_inventory/1, refusal_message_clause_count/1 ]).
:- use_module('../strat', [ stratum_groups/2 ]).
:- use_module('../lower',
              [ lower_program/2, compile_expr/4, compile_comparison/3,
                canonical_column_expr/2, level_support_sql/4 ]).
:- use_module('../analyze', [ check_supported_subset/1, literal_witness/1 ]).
:- use_module('../../0_enum_expand', [ expand_enum_program/2 ]).
:- use_module('../../0_match_expand', [ expand_match_program/2 ]).
:- use_module('../../1_expansion', [ expansion_phase/3, expand_program/3 ]).
:- use_module('../parse_dl', [ parse_dl/4 ]).
:- use_module('../print_dl', [ print_dl_program/3, print_term/5 ]).
:- use_module('../registry', [ surface/5, expression/5, host_execution/3 ]).
:- use_module('../../1_host_expand',
              [ prepare_program/5, compile_host_decl/2, compile_ts_query/2 ]).
:- use_module('../emit_ts',
              [ emit_program/5,
                % The emitter-mode seam (rank R8): which statement family a
                % plan compiles to, asserted by the incremental_mode unit.
                incremental_program_safe/4, reconcile_every_tick/2,
                derived_edge_carry_required/3, retraction_guard/2 ]).
:- use_module('../lower', [ boot_statements/5 ]).

% Body-walk characterization (rank R1) reaches the traversals on BOTH sides of
% the oracle/compiler split, because the review's central claim is that
% several of them are the same predicate written twice. Each of these was
% added to its module's export list for exactly this test rather than being
% called as a private qualified goal, which `just prolog-lint` refuses.
:- use_module('../analyze',
              [ body_ref_uses/2, conjunction_goals/2,
                level_body_latest_ref/2, level_body_pre_ref/2,
                listened_departure_refs/2,
                reserved_construct_in_body/2, body_forbidden_goal/2,
                rule_is_level/1, rule_is_edge/1 ]).
:- use_module('../../conformance/engine',
              [ trigger_items/2, body_finalize_ref/2,
                body_latest_ref/2, body_pre_ref/2,
                check_program/1 ]).
:- use_module('../../conformance/level_eval',
              [ goal_rel_refs/3, split_rules/4 ]).
:- use_module('../../conformance/body', [ body_atoms/2, comparison_goal/1 ]).
:- use_module('../../1_host_expand', [ body_goals/2 ]).
:- ensure_loaded('3_clock_check.test.pl').

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
    Lowered = lowered(_, _, _, [edgestmt(open_scope/2, route_change/2, HeadColumns, KeyColumns, ProjectSql, UpsertSql, DeltaProjectSql, arrival)], _, _, _, _),
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
            'INSERT INTO "world_mode" ("col1", "col2") VALUES (?, ?) ON CONFLICT ("col1") DO UPDATE SET "col2" = excluded."col2"',
            'DELETE FROM "world_mode" WHERE "col1" = ? AND "col2" = ?',
            'INSERT INTO "world_mode" ("col1", "col2") SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?) WHERE true ON CONFLICT ("col1") DO UPDATE SET "col2" = excluded."col2" RETURNING "col1", "col2"',
            'DELETE FROM "world_mode" WHERE ("col1", "col2") IN (SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?)) RETURNING "col1", "col2"'),
        ArrivalStatements).

test(switch_as_keyed_replace_frontier_ddl) :-
    lowered_for(switch_as_keyed_replace, Lowered),
    Lowered = lowered(_, Ddl, _, _, _, _, _, _),
    memberchk('CREATE TEMP TABLE "__frontier_route_change" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "session_id" TEXT NOT NULL, "route_id" TEXT NOT NULL)', Ddl),
    memberchk('CREATE INDEX "__frontier_route_change_phase" ON "__frontier_route_change" ("_phase")', Ddl),
    memberchk('CREATE TEMP TABLE "__next_frontier_open_scope" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "session_id" TEXT NOT NULL, "target" TEXT NOT NULL)', Ddl).

% FAIL-FIRST RECEIPT: pre/1 in an edge body needs a tick-local snapshot read
% plus ordered occurrence execution. Before pre_occurrence_loop this fixture
% stopped in analyze.pl with edge_body_needs_pre/1 and produced no lowered
% statement or snapshot table.
test(pre_edge_lowers_to_ordered_snapshot_read) :-
    lowered_for('merge_family.pl', batched_increments_both_count, Lowered),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    memberchk(
        'CREATE TEMP TABLE "__pre_counter" ("name" TEXT NOT NULL, "next" INTEGER NOT NULL, PRIMARY KEY ("name")) WITHOUT ROWID',
        Ddl),
    EdgeStatements =
        [edgestmt(counter/2, increment/2, [name, next], [name],
                  ProjectSql, _, _, ordered_arrival)],
    once(sub_atom(ProjectSql, _, _, _, 'FROM "__pre_counter" b0')).

% COUNT receipt for the formerly whole-state-per-occurrence refresh path.
% The relation snapshot appears once in the generated tick setup. Reducer
% writes thereafter mirror their one keyed row into __pre_counter.
test(ordered_pre_snapshots_once_then_mirrors_each_write) :-
    fixture_file('merge_family.pl', File),
    read_fixture_term(File, batched_increments_both_count, Term, Bindings),
    program_plan(Term-Bindings, Plan),
    lower_program(Plan, Lowered),
    Term = fixture(_, _, Initial, _, _),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, Initial, LevelStatements, Boot),
    emit_program(batched_increments_both_count, Plan, Lowered, Boot, Text),
    findall(At,
            sub_atom(Text, At, _, _, 'DELETE FROM "__pre_counter"'),
            SnapshotDeletes),
    length(SnapshotDeletes, 1),
    once(sub_atom(Text, _, _, _, 'function orderedPreWriteStatement')),
    \+ sub_atom(Text, _, _, _, 'refreshOrderedPre').

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
      'INSERT INTO "open_feed" ("session_id", "target") SELECT json_extract(value, \'$[0]\'), json_extract(value, \'$[1]\') FROM json_each(?) WHERE true ON CONFLICT ("session_id") DO UPDATE SET "target" = excluded."target" RETURNING "session_id", "target"'.

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
                 'SELECT d0."item" AS "item" FROM "__frontier_stage_one" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"',
                 arrival),
        EdgeStatements).

test(level_derived_trigger_reads_same_tick_frontier) :-
    lowered_for('occurrence_identity.pl', demand_view_fires_its_consumer_once,
                Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    memberchk(
        edgestmt(fetch_call/1, fetch_demand/1, [endpoint], [], _, _,
                 'SELECT d0."endpoint" AS "endpoint" FROM "__frontier_fetch_demand" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"',
                 arrival),
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
    canonical_column_expr(target, Expr),
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
            'SELECT b0."client" AS "client", d0."item" AS "item" FROM "__frontier_change_ev" d0, "subscriber" b0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"',
            arrival)
    ].

% The departure arm (TICK PHASE ALIGNMENT target 2). Three claims in one
% snapshot, all of them what makes the feature cheap:
%   1  the arm reads __departure_frontier_<rel>, the rel's OWN table, and
%      nothing else joins;
%   2  the SQL is the arrival arm's text with one table name swapped -- same
%      `_phase >= 0` filter, same `ORDER BY _phase, _sequence` -- so no new
%      statement shape enters the emitter and the existing EXPLAIN receipts
%      still describe it;
%   3  ONE arm, not one per body atom. keyed_replace_departs_the_old_row's
%      body is a bare finalize, and departed_fires_next_tick_on_retraction's
%      carries a now/1 beside it; neither raises a second occurrence source.
test(departure_arm_reads_the_departure_frontier) :-
    lowered_for('engine_core.pl', keyed_replace_departs_the_old_row, Lowered),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    memberchk(
        edgestmt(replaced_value/2, latest/2, [key, old_value], [], _, _,
                 'SELECT d0."key" AS "key", d0."value" AS "old_value" FROM "__departure_frontier_latest" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"',
                 departure),
        EdgeStatements),
    % The departure table is emitted for the LISTENED rel only.
    memberchk('CREATE TEMP TABLE "__departure_frontier_latest" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "key" TEXT NOT NULL, "value" TEXT NOT NULL)', Ddl),
    memberchk('CREATE INDEX "__departure_frontier_latest_phase" ON "__departure_frontier_latest" ("_phase")', Ddl),
    \+ ( member(OtherDdl, Ddl),
         sub_atom(OtherDdl, _, _, _, '__departure_frontier_'),
         \+ sub_atom(OtherDdl, _, _, _, '__departure_frontier_latest') ).

test(latest_keyed_sample_is_one_edge_arm_with_key_predicates) :-
    lowered_for('shell_stream.pl', identical_demand_dedups, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    findall(
        EdgeStatement,
        (member(EdgeStatement, EdgeStatements),
         EdgeStatement = edgestmt(_, fill/3, _, _, _, _, _, _)),
        SampledEdgeStatements),
    SampledEdgeStatements = [
        edgestmt(
            response/3,
            fill/3,
            [args, salt, payload],
            [],
            'SELECT ?1 AS "args", ?2 AS "salt", ?3 AS "payload" FROM "demand" b0 WHERE b0."args" = ?1 AND b0."salt" = ?2',
            _,
            'SELECT d0."args" AS "args", d0."salt" AS "salt", d0."payload" AS "payload" FROM "__frontier_fill" d0, "demand" b0 WHERE d0."_phase" >= 0 AND b0."args" = d0."args" AND b0."salt" = d0."salt" ORDER BY d0."_phase", d0."_sequence"',
            arrival)
    ].

:- end_tests(sql_text_snapshots).

:- begin_tests(incremental_mode).

test(positive_edge_level_program_is_incremental) :-
    load_plan(switch_as_keyed_replace, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, LevelStatements, _, _, _),
    incremental_program_safe(Plan, EdgeStatements, LevelStatements, true).

test(negative_level_body_uses_incremental_reconcile) :-
    load_plan(merge_policy, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, LevelStatements, _, _, _),
    incremental_program_safe(Plan, EdgeStatements, LevelStatements, true),
    reconcile_every_tick(Plan, true).

test(derived_edge_trigger_requires_incremental_carry_path) :-
    fixture_file('engine_core.pl', File),
    once(( read_fixture_term(File, edge_chain_hops_tick_per_stage, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan, Lowered) )),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    derived_edge_carry_required(Plan, EdgeStatements, true).

test(edb_edge_trigger_keeps_naive_referee_available) :-
    load_plan(switch_as_keyed_replace, Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, EdgeStatements, _, _, _, _),
    derived_edge_carry_required(Plan, EdgeStatements, false).

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
    level_support_sql(
        RelPlans, path/1, Rules,
        supportsql(_, SeedSql, _, _, _)),
    once(sub_atom(
        SeedSql, _, _, _,
        'WITH RECURSIVE "path" ("node") AS')),
    once(sub_atom(SeedSql, _, _, _, 'FROM "path" b0')),
    Plan = plan(test, prog([], Rules), RelPlans, [], Rules, []),
    retraction_guard(Plan, 'recursive-cte-reseed').

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

% FAIL-FIRST RECEIPT: not/1 in an edge body.
%
% This test read `throws(unsupported_construct(edge_body_needs_negation(_)))`
% until the edge-body negation lowering landed, and that refusal is what went
% RED first:
%   RED (before the lowering, this exact clause as an acceptance test):
%     [.../125] accepts_negated_atom_in_edge_body
%       unsupported_construct(edge_body_needs_negation((open(_),not(closed(_)))))
%   RED (after the lowering, the old refusal clause left in place):
%     ERROR: test supported_subset_gate:rejects_edge_body_with_extra_goal:
%            no_exception
%   GREEN: both directions below.
% engine.pl solves not(Goal) as \+ solve(Goal, Ctx) against the SAME Visible
% the positive atoms read, so NOT EXISTS over the negated rel's current table
% is the lowering; scopes.pl:exhaust_policy is the fixture.
test(accepts_negated_atom_in_edge_body) :-
    Prog = prog([keyed(scope/1, [1])], [ (scope(X) <+ (open(X), not(closed(X)))) ]),
    check_supported_subset(Prog).

% not/1 around anything but ONE plain relation atom stays refused:
% compile_negative_uses/4 renders rel atoms only, so a nested comparison
% would vanish from the emitted condition instead of being refused.
test(rejects_negated_conjunction_in_edge_body,
     [throws(unsupported_construct(edge_body_with_negation(_)))]) :-
    Prog = prog([keyed(scope/1, [1])],
                [ (scope(X) <+ (open(X), not((closed(X), budget(X))))) ]),
    check_supported_subset(Prog).

% Comparisons and `:=` binds in an edge body were their own named refusals
% (edge_body_needs_comparison / edge_body_needs_bind) until this arc; they now
% ride the same guard fold a level body uses.
test(accepts_comparison_and_bind_in_edge_body) :-
    Prog = prog([kind(hit/2, log), keep(hit/2, all),
                 kind(loud/2, log), keep(loud/2, all)],
                [ (loud(Name, Doubled) <+ hit(Name, Score), Score > 10,
                                          Doubled := Score * 2) ]),
    check_supported_subset(Prog).

test(accepts_plain_pre_atom_in_edge_body) :-
    Prog = prog(
        [kind(increment/1, log), keep(increment/1, all),
         keyed(counter/2, [1])],
        [ (counter(Name, Next) <+
              increment(Name), pre(counter(Name, Total)),
              Next := Total + 1) ]),
    check_supported_subset(Prog).

% FAIL-FIRST RECEIPT: now/1 in an edge body.
%
% RED (before the __tick lowering):
%   accepts_now_in_edge_body
%     unsupported_construct(edge_body_needs_now((ping(_),now(_))))
% GREEN: all four below.
% engine.pl step 8: "now(Tick) is a kernel read of the current tick (R3),
% never an arrival"; engine_core.pl:now_reads_the_tick is the fixture.
test(accepts_now_in_edge_body) :-
    Prog = prog([kind(ping/1, log), keep(ping/1, all),
                 kind(seen_at/2, log), keep(seen_at/2, all)],
                [ (seen_at(Name, Tick) <+ ping(Name), now(Tick)) ]),
    check_supported_subset(Prog).

% now/1 around anything but a variable is a MATCH against the tick, which
% engine.pl's solve/2 permits by unification and this lowering cannot express.
test(rejects_now_with_non_variable_argument,
     [throws(unsupported_construct(edge_body_with_now(_)))]) :-
    Prog = prog([kind(ping/1, log), keep(ping/1, all),
                 kind(seen_at/2, log), keep(seen_at/2, all)],
                [ (seen_at(Name, 7) <+ ping(Name), now(7)) ]),
    check_supported_subset(Prog).

% Compiler-only refusal (0_program_check.pl and engine.pl are deliberately
% untouched): a level body has no tick in its emitted DELETE/INSERT pair.
test(rejects_now_in_level_rule,
     [throws(unsupported_construct(now_in_level_rule(_, _)))]) :-
    Prog = prog([], [ (stamped(Name, Tick) <- ping(Name), now(Tick)) ]),
    check_supported_subset(Prog).

% FAIL-FIRST RECEIPT: an edge head's column type now comes from the body
% variable that feeds it, not from that head's own literal occurrences.
%
% RED (before the fixpoint took edge rules):
%   accepts_edge_head_column_typed_from_its_body
%     unsupported_construct(edge_head_column_type_mismatch(xref/2,1,int,text))
% GREEN below. pin_extracted's span id has integer literals in the schedule;
% xref/2 is edge-headed and has none of its own, so the old literal-witness-
% only rule stored an integer in a TEXT column and refused rather than
% resolving it.
test(accepts_edge_head_column_typed_from_its_body) :-
    Prog = prog([kind(pin_extracted/2, log), keep(pin_extracted/2, all),
                 keyed(xref/2, [1])],
                [ (xref(SpanId, Kind) <+ pin_extracted(SpanId, Kind)) ]),
    program_plan(fixture(edge_head_typing, Prog, [pin_extracted(20, doc_link)],
                         [], [])-[], Plan),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    memberchk('CREATE TABLE "xref" ("col1" INTEGER NOT NULL, "col2" TEXT NOT NULL, PRIMARY KEY ("col1")) WITHOUT ROWID', Ddl).

% A ref that ONLY an Initial row mentions still gets a table: engine.pl's
% seed_store/3 stores it, so it is part of the oracle's final state.
test(initial_only_ref_still_gets_a_table) :-
    Prog = prog([kind(ping/1, log), keep(ping/1, all)], []),
    program_plan(fixture(seeded, Prog, [known_repo(2)], [], [])-[], Plan),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    memberchk('CREATE TABLE "known_repo" ("col1" INTEGER NOT NULL, PRIMARY KEY ("col1")) WITHOUT ROWID', Ddl).

% The class the TICK PHASE ALIGNMENT arc opened: an edge arm joining a level
% rel an ARRIVAL can retract. It used to throw
% edge_body_joins_arrival_fed_level (a runtime-seam placeholder); now both
% pipelines freeze the mid-tick level plane where engine.pl freezes it, so it
% compiles. FAIL-FIRST RECEIPT for the runtime half, captured before the
% phase-order change with the refusal switched off, on the fixture this
% program is a reduction of (check_eventing.pl:clock_rel_join_storms, BOTH
% emitter modes, tick 3):
%   actual  "diag_seen":{"add":[["a_rs",3,..],["a_rs",5,..],["a_rs",7,..]]}
%   oracle  "diag_seen":{"add":[["a_rs",5,..]]}
% The two retracted diagnostics were still in the table when the tick_rel arm
% joined it. After the change both modes are byte-identical on tick log AND
% final state.
test(accepts_edge_join_against_an_arrival_fed_level_rel) :-
    Prog = prog([kind(tick_rel/1, log), keep(tick_rel/1, all),
                 kind(seen/2, log), keep(seen/2, all)],
                [ (diagnostic(Path, Code) <- file_line(Path, Code)),
                  (seen(Path, At) <+ diagnostic(Path, _), tick_rel(At)) ]),
    check_supported_subset(Prog).

% The emitted incremental tick must CARRY the freeze: applyLevelsBeforeEdges
% only grows the plane, so the retracting pass has to sit between it and
% applyEdges. Sabotage receipt: dropping
% emit_ts.pl:pre_edge_level_reconcile_lines/2 from the pipeline flips this red
% and takes clock_rel_join_storms back to the three-row tick above.
test(emitted_incremental_tick_freezes_the_level_plane_before_edges) :-
    Prog = prog([kind(tick_rel/1, log), keep(tick_rel/1, all),
                 kind(seen/2, log), keep(seen/2, all)],
                [ (diagnostic(Path, Code) <- file_line(Path, Code)),
                  (seen(Path, At) <+ diagnostic(Path, _), tick_rel(At)) ]),
    program_plan(fixture(freeze, Prog, [], [], [])-[], Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, [], LevelStatements, Boot),
    emit_program(freeze, Plan, Lowered, Boot, Text),
    once(sub_atom(Text, BeforeAt, _, _, 'IncrementalRuntime.applyLevelsBeforeEdges')),
    once(sub_atom(Text, ReconcileAt, _, _, 'IncrementalRuntime.recomputeLevelsBeforeEdges')),
    once(sub_atom(Text, EdgesAt, _, _, 'IncrementalRuntime.applyEdges')),
    BeforeAt < ReconcileAt, ReconcileAt < EdgesAt,
    % The naive referee's own freeze: recomputeLevels once before the edge
    % batch and once after (engine.pl's two level closures).
    findall(At, sub_atom(Text, At, _, _, 'concatMap((before) => recomputeLevels(seam)'), RecomputeAts),
    length(RecomputeAts, 2), !.

% The narrowing that keeps exhaust_policy compiled: a level rel whose own
% derivation reads only EDGE-WRITTEN rels cannot be moved by an arrival before
% the edges run, so joining it mid-tick is safe.
test(accepts_edge_join_against_an_edge_fed_level_rel) :-
    Prog = prog([kind(open_request/2, log), keep(open_request/2, all),
                 kind(closed/2, log), keep(closed/2, all),
                 keyed(open_tab/2, [1])],
                [ (open_tab(SessionId, TabId) <+ open_request(SessionId, TabId),
                                                 not(live_tab(SessionId, _))),
                  (closed(SessionId, TabId) <+ open_request(SessionId, TabId)),
                  (live_tab(SessionId, TabId) <- open_tab(SessionId, TabId),
                                                 not(closed(SessionId, TabId))) ]),
    check_supported_subset(Prog).

% The tick is an INTEGER witness. Without the seed in
% analyze.pl:seed_column_contribution/8 this column takes the C2 "no witness
% -> text" default and the emitted DDL says TEXT, which prints the tick
% quoted where the oracle prints it bare.
test(now_bound_head_column_is_integer_storage) :-
    Prog = prog([kind(ping/1, log), keep(ping/1, all),
                 kind(seen_at/2, log), keep(seen_at/2, all)],
                [ (seen_at(Name, Tick) <+ ping(Name), now(Tick)) ]),
    program_plan(fixture(now_typing, Prog, [], [], [])-[], Plan),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    % Column NAMES are col1/col2 here: surface names come from the fixture
    % file's variable bindings, and this program is built in Prolog with an
    % empty Bindings list. The TYPES are the point.
    memberchk('CREATE TABLE "seen_at" ("col1" TEXT NOT NULL, "col2" INTEGER NOT NULL)', Ddl),
    memberchk('CREATE TABLE "__tick" ("n" INTEGER NOT NULL)', Ddl).

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
                 'SELECT d0."id" AS "id", d0."tag" AS "tag" FROM "__frontier_door_tag" d0 WHERE d0."_phase" >= 0 ORDER BY d0."_phase", d0."_sequence"',
                 arrival),
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

test(key_position_zero_is_a_named_compile_refusal,
     [throws(unsupported_construct(
                 key_position_out_of_range(current/2, 0, 2)))]) :-
    check_supported_subset(prog([keyed(current/2, [0])], [])).

test(key_position_above_arity_is_a_named_compile_refusal,
     [throws(unsupported_construct(
                 key_position_out_of_range(current/2, 3, 2)))]) :-
    check_supported_subset(prog([keyed(current/2, [3])], [])).

test(duplicate_key_position_is_a_named_compile_refusal,
     [throws(unsupported_construct(
                 key_position_duplicate(current/2, 1)))]) :-
    check_supported_subset(prog([keyed(current/2, [1, 1])], [])).

test(keyed_edge_head_remains_supported) :-
    check_supported_subset(
        prog(
            [
                kind(source/2, log),
                keep(source/2, all),
                keyed(current/2, [1])
            ],
            [(current(Key, Value) <+ source(Key, Value))])).

test(match_surface_round_trips_with_prefix_semicolon_and_left_to_right_arms) :-
    string_codes(
        "match source(Key, Value) (\n  ; Value >= 10 |-> accepted(Key)\n  ; true |+> latest(Key, Value)\n).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    Program = prog(
        [],
        [match(
            source(Key, Value),
            ((accepted(Key) <- Value >= 10) ;
             (latest(Key, Value) <+ true)))]),
    print_dl_program(Program, Bindings, Text),
    assertion(
        atom_string(
            Text,
            "match source(Key, Value) (\n  ; Value >= 10 |-> accepted(Key)\n  ; true |+> latest(Key, Value)\n).\n")),
    atom_codes(Text, PrintedCodes),
    parse_dl(PrintedCodes, RoundTripped, _, []),
    Program =@= RoundTripped.

test(match_surface_allows_first_arm_without_prefix_semicolon) :-
    string_codes(
        "match source(Key, Value) (\n  Value >= 10 |-> accepted(Key)\n; true |+> latest(Key, Value)\n).\n",
        Codes),
    parse_dl(Codes, Program, Bindings, []),
    print_dl_program(Program, Bindings, Text),
    assertion(
        atom_string(
            Text,
            "match source(Key, Value) (\n  ; Value >= 10 |-> accepted(Key)\n  ; true |+> latest(Key, Value)\n).\n")).

test(match_surface_rejects_old_head_first_arm_spelling,
     [throws(dl_parse_error(statement, _))]) :-
    string_codes(
        "match source(Key, Value) (\n  accepted(Key) <- Value >= 10\n).\n",
        Codes),
    parse_dl(Codes, _, _, _).

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
      "sh fetch(ep: text, prev: text, bucket: int) -> (status: int) = `run {ep} $prev`.\nresult(Status) <- input(Ep, Prev, Bucket), fetch(Ep, Prev, Bucket, Status).\n? result(Status).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    Program = program(
                [sh_decl(fetch,
                         [col(ep, text), col(prev, text), col(bucket, int)],
                         [col(status, int)],
                         template("run {ep} $prev"))],
                [(_ <- (_, probe(fetch, [_, _], [_], [salt(bucket, _)])))],
                [query(result(_))]),
    print_dl_program(Program, Bindings, Printed),
    assertion(sub_atom(Printed, _, _, _,
                       "fetch(Ep, Prev, Bucket, Status)")),
    assertion(\+ sub_atom(Printed, _, _, _, "@ salt")),
    assertion(\+ sub_atom(Printed, _, _, _, "? fetch")),
    assertion(sub_atom(Printed, _, _, _, "? result(Status).")),
    atom_codes(Printed, PrintedCodes),
    parse_dl(PrintedCodes, Reparsed, _, []),
    Program =@= Reparsed.

test(rhs_probe_marker_is_rejected,
     [throws(dl_parse_error(statement, _))]) :-
    string_codes(
      "sh fetch(ep: text) -> (status: int) = `run {ep}`.\nresult(Status) <- ? fetch('repo', Status).\n",
      Codes),
    parse_dl(Codes, _, _, _).

test(rhs_postfix_probe_marker_is_rejected,
     [throws(dl_parse_error(statement, _))]) :-
    string_codes(
      "sh fetch(ep: text) -> (status: int) = `run {ep}`.\nresult(Status) <- fetch?('repo', Status).\n",
      Codes),
    parse_dl(Codes, _, _, _).

test(plain_host_resolution_is_declaration_order_independent) :-
    string_codes(
      "result(Status) <- fetch('repo', '', 3, Status).\nsh fetch(ep: text, prev: text, bucket: int) -> (status: int) = `run {ep} $prev`.\n",
      Codes),
    parse_dl(
      Codes,
      program(
        [sh_decl(fetch,
                 [col(ep, text), col(prev, text), col(bucket, int)],
                 [col(status, int)],
                 template("run {ep} $prev"))],
        [(result(Status) <-
            probe(fetch, [repo, ''], [Status], [salt(bucket, 3)]))],
        []),
      _,
      []).

test(removed_salt_surface_is_rejected,
     [throws(dl_parse_error(statement, _))]) :-
    string_codes(
      "result(Value) <- source(Value) @ salt(bucket: 3).\n",
      Codes),
    parse_dl(Codes, _, _, _).

test(plain_non_host_rhs_remains_relation_atom) :-
    string_codes(
      "result(Value) <- source(Value).\n",
      Codes),
    parse_dl(Codes, prog([], [(result(Value) <- source(Value))]), _, []).

test(plain_host_arity_mismatch_reaches_existing_named_refusal,
     [throws(probe_mismatch(probe(fetch, [repo], [], [])))]) :-
    string_codes(
      "sh fetch(ep: text) -> (status: int) = `run {ep}`.\nresult('missing') <- fetch('repo').\n",
      Codes),
    parse_dl(Codes, Program, _, []),
    prepare_program(Program, _, _, _, _).

test(removed_type_keyword_is_rejected,
     [throws(dl_parse_error(statement, _))]) :-
    string_codes("type span(start: int, end: int).", Codes),
    parse_dl(Codes, _, _, _).

test(referenced_rel_remains_queryable_and_marks_reference_edge) :-
    string_codes(
      "rel span(start: int, end: int).\nrel mark(at: span).\n",
      Codes),
    parse_dl(Codes, prog(Decls, []), _, []),
    memberchk(type_decl(span,
                        [col(start, int), col(end, int)]),
              Decls),
    memberchk(col_type(mark/1, at, span), Decls),
    memberchk(col_type(span/2, start, int), Decls),
    memberchk(col_type(span/2, end, int), Decls).

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
      sh_decl(local_fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int)],
              template("{ep}")),
      _).

test(host_freshness_input_may_be_absent_from_template) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text), col(prev, text), col(bucket, int)],
              [col(status, int)],
              template("{ep} $prev")),
      host_plan(fetch, _, _, _, _, _,
                input_roles([identity, identity, freshness]))).

test(host_contract_matches_name_columns_and_types_not_name_alone) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(ep, text), col(prev, text)],
              [col(status, int)],
              template("{ep} $prev")),
      host_plan(fetch, _, _, _, _, _,
                input_roles([identity, identity]))).

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

test(extract_host_uses_compiler_known_executor) :-
    compile_host_decl(
      sh_decl(extract,
              [col(path, text), col(digest, text)],
              [col(callee, text)],
              template("\"$DL_EXTRACT_BIN\" --family call {path}")),
      host_plan(extract, _, _, _, _, _,
                input_roles([identity, freshness]))),
    !.

test(named_extractor_projection_uses_template_selected_executor) :-
    Template = "\"$DL_EXTRACT_BIN\" --family cst,type,call,df {path}",
    host_execution(call_node, Template, sprefa_extract),
    compile_host_decl(
      sh_decl(call_node,
              [col(path, text), col(digest, text)],
              [col(record, text), col(kind, text), col(name, text)],
              template(Template)),
      host_plan(call_node, _, _, _, _, _,
                input_roles([identity, freshness]))),
    !.

test(extract_host_refuses_non_path_input,
     [throws(host_executor_mismatch(extract, sprefa_extract, [col(file, text)]))]) :-
    compile_host_decl(
      sh_decl(extract,
              [col(file, text)],
              [col(callee, text)],
              template("\"$DL_EXTRACT_BIN\" --family call {file}")),
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

% HOST-OUTPUT-SEAM FAIL-FIRST RECEIPT, acceptance direction:
% parse_dl/4 returned
%   [unsupported_surface(column_type_wrapper(scan_span,at,none))]
% and recorded the output as col(at,none). The term door already accepts the
% same declaration. The green contract covers the text parser, generated
% response-column lowering, and emitted host plan together.
test(host_declared_struct_output_parses_and_lowers_as_ref) :-
    string_codes(
      "rel span(end: int, start: int).\nrel source_path(path: text).\nrel host_span(path: text, at: span).\nsh scan_span(path: text) -> (at: span) = `scan {path}`.\nhost_span(Path, At) <- source_path(Path), scan_span(Path, At).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    program_plan(
      fixture(host_declared_struct_output_parses_and_lowers_as_ref,
              Program, [], [], [])-Bindings,
      Plan),
    Plan = plan(_, _, RelPlans, _, _, _),
    memberchk(
      relplan('__host_response_scan_span'/4, set,
              [witness_digest, ordinal, path, at],
              key([1, 2]), [text, int, text, ref(span)]),
      RelPlans),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), _, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, [], LevelStatements, Boot),
    emit_program(
      host_declared_struct_output_parses_and_lowers_as_ref,
      Plan, Lowered, Boot, Text),
    once(sub_atom(Text, _, _, _, '{ name: "at", type: "span" }')),
    once(sub_atom(
      Text, _, _, _,
      '"__host_response_scan_span": [null, null, null, "span"]')),
    !.

% HOST-OUTPUT-SEAM FAIL-FIRST RECEIPT, refusal direction:
% the former decl-B fallback erased `spann` to none and stopped at the generic
% column_type_wrapper finding. The parser now preserves the spelling so the
% shared program check names column_type_unknown(spann).
test(host_unknown_struct_output_refuses_by_type_name,
     [throws(unsupported_construct(column_type_unknown(spann)))]) :-
    string_codes(
      "rel span(end: int, start: int).\nrel source_path(path: text).\nrel host_span(path: text, at: span).\nsh scan_span(path: text) -> (at: spann) = `scan {path}`.\nhost_span(Path, At) <- source_path(Path), scan_span(Path, At).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    program_plan(
      fixture(host_unknown_struct_output_refuses_by_type_name,
              Program, [], [], [])-Bindings,
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
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, Initial, LevelStatements, Boot),
    emit_program(native_ts_query_term, Plan, Lowered, Boot, Text),
    once(sub_atom(Text, _, _, _, 'export const hostPlans')),
    % PHASE 2 (runtime bridge arc): the two named refusals are gone; both world
    % terms now carry the executor the served runtime dispatches on. The bind's
    % `literals` list is EMPTY for this fixture on purpose -- it declares
    % `bind interval(...)` and seeds an `interval(300, 1)` Initial row, but no
    % RULE reads a literal period, so no timer is owed.
    once(sub_atom(Text, _, _, _, 'execution: "shell"')),
    once(sub_atom(Text, _, _, _, 'literals: [], execution: "live_interval"')),
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
% THREE FORMER DRIFTS THESE GOLDENS PIN:
%
%   1. engine:trigger_items/2 now admits only plain-atom walk events as
%      arrivals, while finalize/1 remains the departure case. Registered
%      wrappers and comparisons are not relation atoms.
%   2. level_eval:goal_rel_refs/3 projects splice_bare registry rows through
%      their relation arguments, so next/1 and combine/variadic name their
%      contained refs.
%   3. body:body_atoms/2 now projects the same plain-atom walk events.
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
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1]-[]),
    body_atoms-[],
    reserved_constructs-[],
    forbidden_goals-[],
    host_body_goals-[next(a(1))]
  ]).

walk_golden(combine3,
  [ body_ref_uses-[use(a/1,[1],pos,trigger),use(b/1,[2],pos,trigger),use(c/1,[3],pos,trigger)],
    conjunction_goals-[a(1),b(2),c(3)],
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([a/1,b/1,c/1]-[]),
    body_atoms-[],
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
    % finalize/1 left the refused set with the departure frontier (TICK
    % PHASE ALIGNMENT target 2): it is a LIVE registry row now, so the
    % forbidden-goal scan no longer names it.
    forbidden_goals-[],
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
    trigger_items-[],
    engine_finalize_refs-[],
    engine_latest_refs-[],
    engine_pre_refs-[],
    analyze_latest_refs-[],
    analyze_pre_refs-[],
    goal_rel_refs-([unsubscribe/1]-[]),
    body_atoms-[],
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
    trigger_items-[arrival(a(1)),departure(g(10))],
    engine_finalize_refs-[g/1],
    engine_latest_refs-[c/1],
    engine_pre_refs-[h/1],
    analyze_latest_refs-[c/1],
    analyze_pre_refs-[h/1],
    goal_rel_refs-([a/1,d/1,e/1,f/1]-[b/1,c/1]),
    body_atoms-[a(1)],
    reserved_constructs-[],
    forbidden_goals-[pre(h(11))],
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

% The same claim one level up, over whole rule LISTS rather than one body:
% analyze:listened_departure_refs/2 is the compiler's copy of the predicate
% engine.pl:tick/7 gates DepartureCarry on, and it decides which rels get a
% departure frontier table. If the two ever disagreed, the emitted program
% would either stage departures nothing reads or miss the ones an arm needs.
% The oracle's own copy is private, so this rebuilds it from the exported
% body_finalize_ref/2 -- the same shape engine.pl's clause has.
test(listened_departure_refs_agree_across_doors) :-
    forall(departure_ref_case(Rules),
           ( findall(Ref,
                     ( member((_ <+ Body), Rules), body_finalize_ref(Body, Ref) ),
                     OracleRefs0),
             sort(OracleRefs0, OracleRefs),
             listened_departure_refs(Rules, CompilerRefs),
             OracleRefs == CompilerRefs )).

departure_ref_case([ (out(Item) <+ finalize(gone(Item))) ]).
departure_ref_case([ (out(Item) <+ src(Item)) ]).
% The update arm: one finalize plus a plain join. Only the finalize'd rel is
% listened to.
departure_ref_case([ (changed(Key, Old, New) <+ finalize(row(Key, Old)),
                                               row(Key, New)) ]).
% A LEVEL rule's finalize is not a departure listen (the oracle's clause reads
% <+ rules only, and both doors refuse the program anyway).
departure_ref_case([ (out(Item) <- finalize(gone(Item))) ]).
% Two rules listening to two rels, plus one that listens to neither.
departure_ref_case([ (a(Item) <+ finalize(one(Item))),
                     (b(Item) <+ finalize(two(Item))),
                     (c(Item) <+ plain(Item)) ]).
% A negated finalize is opaque to the walk on both sides.
departure_ref_case([ (out(Item) <+ src(Item), not(finalize(gone(Item)))) ]).

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

test(key_position_out_of_range_both_doors) :-
    Prog = prog([keyed(current/2, [3])], []),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == key_position_out_of_range(current/2, 3, 2),
    CompilerVerdict ==
        unsupported_construct(key_position_out_of_range(current/2, 3, 2)).

test(key_position_duplicate_both_doors) :-
    Prog = prog([keyed(current/2, [1, 1])], []),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == key_position_duplicate(current/2, 1),
    CompilerVerdict ==
        unsupported_construct(key_position_duplicate(current/2, 1)).

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

% ── the finalize diagnostic drift, now CLOSED ───────────────────────────────
% R2 recorded this as drift and preserved it: both doors refused a finalize/1
% in a level rule, and they named it differently, because the oracle had a
% dedicated check while the compiler reached it through the generic
% refused-goal path and reported the enclosing head (level_body_goal).
%
% TICK PHASE ALIGNMENT target 2 forced the repair rather than choosing it.
% finalize/1 became a LIVE registry row (it is the departure trigger in an edge
% body), which deleted the generic path this refusal was riding on -- measured,
% not assumed: with the row flipped and nothing else changed, this exact
% program compiled ACCEPTED. analyze.pl's shared_refusal list gained
% finalize_in_level_rule in the position engine.pl's own engine_check_order/1
% gives it, so the two doors now name the same class AND agree on which class a
% program violating several of them reports.
test(finalize_in_level_rule_agrees_across_doors) :-
    Prog = prog([], [ (out(Item) <- (src(Item), finalize(gone(Item)))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == finalize_in_level_rule(gone/1),
    CompilerVerdict == unsupported_construct(finalize_in_level_rule(gone/1)).

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

% finalize/1 is a departure occurrence and has no level-plane meaning at any
% negation depth. Both doors use the shared program check and name the same
% refusal.
test(nested_not_finalize_refused_by_both_doors) :-
    Prog = prog([], [ (out(Item) <- (src(Item), not(finalize(gone(Item))))) ]),
    door_verdict(oracle, Prog, OracleVerdict),
    door_verdict(compiler, Prog, CompilerVerdict),
    OracleVerdict == finalize_in_level_rule(gone/1),
    CompilerVerdict == unsupported_construct(finalize_in_level_rule(gone/1)).

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
% REFUSAL MESSAGE UMBRELLA

:- begin_tests(refusal_messages).

test(every_named_refusal_renders_one_line) :-
    refusal_message_clause_count(ClauseCount),
    ClauseCount =:= 1,
    refusal_inventory(Inventory),
    Inventory = [_ | _],
    forall(member(Name/_Arity-Example, Inventory),
           ( message_to_string(unsupported_construct(Example), Text),
             \+ sub_string(Text, _, _, _, "Unknown message"),
             atom_string(Name, NameText),
             sub_string(Text, _, _, _, NameText),
             split_string(Text, "\n", "", [_])
           )).

:- end_tests(refusal_messages).

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

% ═══════════════════════════════════════════════════════════════════════════
% EXPANSION ORDER (rank R3)
%
% The spreading verdict fixes the order as
% enum -> declaration spread -> row spread -> match, which puts ENUM BEFORE
% MATCH. The old single call ran match first, and moving the calls alone
% silently breaks match exhaustiveness:
%
%   expand_enum_program/2 removes every enum_decl/2 entry, and match coverage
%   used to read enum_decl/2 straight out of the declarations. Enum-first
%   therefore left the coverage check looking at declarations that no longer
%   mention any enum.
%
% RECEIPT taken against the unmodified expanders, before 1_expansion.pl
% existed. For a two-variant enum with a ONE-arm match:
%
%   expand_match_program/2                    threw
%     unsupported_construct(match_nonexhaustive(body, redirect))
%   expand_enum_program/2 then
%     expand_match_program/2                  SUCCEEDED
%
% and for the exhaustive twin the two orders produced IDENTICAL expanded
% terms. So no output diff and no tick log could have caught the loss; only
% the refusal disappears. That is what these tests hold onto.

:- begin_tests(expansion_order).

enum_program(Rules, prog([ enum_decl(body, (page(view:text)
                                            ; redirect(to:text))) ], Rules)).

exhaustive_match(Program) :-
    enum_program([ match(resp(Id),
                     ( (body_page(Id, v) <- true)
                     ; (body_redirect(Id, t) <- true) )) ], Program).

nonexhaustive_match(Program) :-
    enum_program([ match(resp(Id), (body_page(Id, v) <- true)) ], Program).

% The declared order is the spreading verdict's order followed by the
% relation-edge dependency expansion. The two spread phases remain
% placeholders until spreading is wired.
test(declared_phase_order) :-
    findall(Order-Name, expansion_phase(Order, Name, _), Unordered),
    msort(Unordered, Ordered),
    Ordered == [10-enum, 20-decl_spread, 30-row_spread, 40-match,
                50-relation_edge].

test(spread_phases_are_placeholders) :-
    expansion_phase(20, decl_spread, unwired),
    expansion_phase(30, row_spread, unwired).

test(level_relation_value_adds_target_membership) :-
    Program = prog(
        [ type_decl(user, [col(id, int), col(name, text)]),
          col_type(post/1, author, user) ],
        [ (post(user(Id, Name)) <- source(Id, Name)) ]),
    expand_program(Program, prog(_, [Expanded]), _),
    Expanded =@=
        (post(user(Id, Name)) <- (source(Id, Name), user(Id, Name))).

test(edge_relation_value_samples_target_membership) :-
    Program = prog(
        [ type_decl(user, [col(id, int), col(name, text)]),
          col_type(post/1, author, user) ],
        [ (post(user(Id, Name)) <+ source(Id, Name)) ]),
    expand_program(Program, prog(_, [Expanded]), _),
    Expanded =@=
        (post(user(Id, Name)) <+
            (source(Id, Name), latest(user(Id, Name)))).

test(existing_target_membership_is_not_duplicated) :-
    Program = prog(
        [ type_decl(user, [col(id, int), col(name, text)]),
          col_type(post/1, author, user) ],
        [ (post(user(Id, Name)) <-
              (source(Id, Name), user(Id, Name))) ]),
    expand_program(Program, prog(_, [Expanded]), _),
    Expanded =@=
        (post(user(Id, Name)) <-
            (source(Id, Name), user(Id, Name))).

% Enum-first through the driver produces exactly what match-first produced.
test(enum_first_preserves_expanded_terms) :-
    exhaustive_match(Program),
    expand_match_program(Program, MatchFirst),
    expand_program(Program, EnumFirst, _),
    EnumFirst =@= MatchFirst.

% The property the context exists to save.
test(enum_first_still_refuses_nonexhaustive_match) :-
    nonexhaustive_match(Program),
    catch(expand_program(Program, _, _), Thrown, true),
    Thrown == unsupported_construct(match_nonexhaustive(body, redirect)).

% The context is computed from the SURFACE declarations, and it names the
% generated variant relations rather than the surface variant terms, so a
% later phase can check coverage against what enum expansion actually made.
test(context_carries_generated_variant_refs) :-
    exhaustive_match(Program),
    expand_program(Program, _, Context),
    Context == [ body-[ body_page/2-page, body_redirect/2-redirect ] ].

% A program with no enum still expands, and its context is empty rather than
% failing.
test(program_without_enum_has_empty_context) :-
    Program = prog([], [ (out(Item) <- src(Item)) ]),
    expand_program(Program, Expanded, Context),
    Context == [],
    Expanded == Program.

% The driver is the whole pipeline: a program with neither sugar comes back
% unchanged, and one with both comes back fully desugared.
test(driver_expands_enum_and_match_together) :-
    exhaustive_match(Program),
    expand_program(Program, prog(Decls, Rules), _),
    \+ member(enum_decl(_, _), Decls),
    \+ member(match(_, _), Rules).

:- end_tests(expansion_order).

% ═══════════════════════════════════════════════════════════════════════════
% EXPRESSION OPERATOR INVENTORY (rank R5)
%
% The eleven arithmetic and comparison operators were listed in five places:
% body.pl's comparison_goal/1, lower.pl's arithmetic_expr/4 and
% comparison_operator_sql/5, print_dl.pl's arith_op/2, and two memberchk lists
% in analyze.pl. registry.pl's expression/5 is now the inventory, and these
% tests are its totality check: every row is reachable from every consumer
% that has an opinion about it, and no consumer knows an operator the table
% does not.

:- begin_tests(expression_inventory).

% The full inventory, written out rather than derived, so a row appearing or
% disappearing is a visible edit here.
expected_row('+'/2,    arithmetic,          1, infix('+'),            both_number).
expected_row('-'/2,    arithmetic,          1, infix('-'),            both_number).
expected_row('*'/2,    arithmetic,          2, infix('*'),            both_number).
expected_row('/'/2,    arithmetic,          2, numeric_division,      both_number).
expected_row(mod/2,    arithmetic,          2, sign_corrected_modulo, both_int).
expected_row('<'/2,    ordered_comparison,  0, infix('<'),            both_number).
expected_row('=<'/2,   ordered_comparison,  0, infix('<='),           both_number).
expected_row('>'/2,    ordered_comparison,  0, infix('>'),            both_number).
expected_row('>='/2,   ordered_comparison,  0, infix('>='),           both_number).
expected_row('=='/2,   identity_comparison, 0, infix('='),            same_type).
expected_row('\\=='/2, identity_comparison, 0, infix('<>'),           same_type).
expected_row(norm/1,    text_scalar,         3, ascii_alnum_lower,    text_only).

test(inventory_is_exactly_the_expected_rows) :-
    findall(Signature-Family-Precedence-Sql-Type,
            expression(Signature, Family, Precedence, Sql, Type), Actual),
    findall(Signature-Family-Precedence-Sql-Type,
            expected_row(Signature, Family, Precedence, Sql, Type), Expected),
    msort(Actual, SortedActual),
    msort(Expected, SortedExpected),
    SortedActual == SortedExpected.

% Every comparison row must ALSO be a registry surface row in the guard axis,
% and every surface guard row that is an infix operator must have an
% expression row. The two tables share these eleven functors and must not
% disagree about which ones exist.
test(expression_table_agrees_with_surface_rows) :-
    findall(Signature,
            ( expression(Signature, Family, _, _, _),
              memberchk(Family, [ordered_comparison, identity_comparison]) ),
            ComparisonRows),
    findall(Signature,
            surface(Signature, guard, no_refs, infix(_), _),
            SurfaceGuardRows),
    msort(ComparisonRows, SortedComparisons),
    msort(SurfaceGuardRows, SortedSurface),
    SortedComparisons == SortedSurface.

% The oracle's comparison_goal/1 recognizes exactly the comparison rows.
test(oracle_comparison_recognizer_is_total) :-
    forall(( expression(Name/2, Family, _, _, _),
             memberchk(Family, [ordered_comparison, identity_comparison]) ),
           ( Goal =.. [Name, 1, 2], comparison_goal(Goal) )),
    forall(( expression(Name/2, arithmetic, _, _, _) ),
           ( Goal =.. [Name, 1, 2], \+ comparison_goal(Goal) )).

% Every arithmetic row lowers to SQL, and mod's sign-corrected template is
% distinguishable from a plain infix rendering.
test(every_arithmetic_row_lowers_to_sql) :-
    forall(expression(Name/2, arithmetic, _, _, _),
           ( Expr =.. [Name, 1, 2],
             compile_expr(Expr, [], Sql, Type),
             Type == int,
             atom(Sql) )).

test(modulo_lowers_sign_corrected) :-
    compile_expr(mod(7, 3), [], Sql, _),
    Sql == '(((7 % 3) + 3) % 3)'.

test(norm_lowers_to_ascii_character_filter) :-
    compile_expr(norm('Route /V2: Café_42'), [], Sql, Type),
    Type == text,
    once(sub_atom(Sql, _, _, _, 'WITH RECURSIVE "__norm_chars"')),
    once(sub_atom(Sql, _, _, _, 'unicode("c") BETWEEN 48 AND 57')).

test(norm_refuses_integer_operand,
     [throws(unsupported_construct(text_operand_not_text(norm(7), 7, int)))]) :-
    compile_expr(norm(7), [], _, _).

% Every comparison row lowers to its declared SQL operator.
test(every_comparison_row_lowers_to_its_sql_operator) :-
    forall(( expression(Name/2, Family, _, infix(SqlOperator), _),
             memberchk(Family, [ordered_comparison, identity_comparison]) ),
           ( Goal =.. [Name, 1, 2],
             compile_comparison(Goal, [], Text),
             atomic_list_concat(['(1 ', SqlOperator, ' 2)'], Expected),
             Text == Expected )).

% The printer parenthesizes by the table's precedence: a tighter operator
% nested inside a looser one needs no parens, the reverse does.
test(printer_precedence_comes_from_the_table) :-
    print_term((1 + 2) * 3, [], 0, top, Tight),
    Tight == '(1 + 2) * 3',
    print_term(1 + 2 * 3, [], 0, top, Loose),
    Loose == '1 + 2 * 3'.

:- end_tests(expression_inventory).

:- begin_tests(phase5_value_plane).

test(parser_and_printer_round_trip_bool_and_float_without_surface_wrappers) :-
    string_codes(
      "rel sample(name: text, enabled: bool, score: float).\nselected(Name) <- sample(Name, true, Score), Score >= 0.25.\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    Program =@=
      prog(
        [ col_type(sample/3, name, text),
          col_type(sample/3, enabled, bool),
          col_type(sample/3, score, float) ],
        [ (selected(Name) <-
              sample(Name, bool_lit(true), Score),
              Score >= 0.25) ]),
    print_dl_program(Program, Bindings, Printed),
    assertion(sub_atom(Printed, _, _, _, "enabled: bool")),
    assertion(sub_atom(Printed, _, _, _, "score: float")),
    assertion(sub_atom(Printed, _, _, _, "sample(Name, true, Score)")),
    atom_codes(Printed, PrintedCodes),
    parse_dl(PrintedCodes, Reparsed, _, []),
    assertion(Program =@= Reparsed).

test(unbound_variable_is_not_a_bool_literal_witness, [fail]) :-
    literal_witness(_).

test(bool_and_float_storage_constraints_are_exact) :-
    lowered_for('5_value_plane.pl', bool_literals_round_trip, BoolLowered),
    BoolLowered = lowered(_, BoolDdl, _, _, _, _, _, _),
    memberchk(
      'CREATE TABLE "flag" ("name" TEXT NOT NULL, "enabled" INTEGER NOT NULL CHECK ("enabled" IN (0,1)), PRIMARY KEY ("name", "enabled")) WITHOUT ROWID',
      BoolDdl),
    lowered_for('5_value_plane.pl', float_arithmetic_is_binary64, FloatLowered),
    FloatLowered = lowered(_, FloatDdl, _, _, _, _, _, _),
    once(( member(ScoreDdl, FloatDdl),
           sub_atom(ScoreDdl, 0, _, _, 'CREATE TABLE "score"'),
           sub_atom(ScoreDdl, _, _, _,
                    '"value" REAL NOT NULL CHECK (typeof("value") = \'real\' AND "value" BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308)') )).

test(float_division_and_avg_lower_to_sqlite_real_operations) :-
    compile_expr(5 / 2, [], IntDivision, int),
    assertion(IntDivision == '(5 / 2)'),
    compile_expr(5.0 / 2, [], FloatDivision, float),
    assertion(FloatDivision == '(CAST(5.0 AS REAL) / 2)'),
    lowered_for('5_value_plane.pl', float_avg_is_grouped, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    memberchk(levelstmt(mean/2, _, [InsertSql], _, _, _), LevelStatements),
    assertion(sub_atom(InsertSql, _, _, _, 'avg(b0."value")')).

test(arithmetic_operator_constraint_keeps_unwitnessed_scan_state_numeric) :-
    Prog = prog(
      [ kind(increment/2, log),
        keep(increment/2, all),
        keyed(counter/2, [1]) ],
      [ (counter(Name, Next) <+
            increment(Name, _),
            pre(counter(Name, Total)),
            Next := Total + 1) ]),
    program_plan(fixture(scan_numeric_constraint, Prog, [], [], [])-[], Plan),
    Plan = plan(_, _, RelPlans, _, _, _),
    memberchk(relplan(counter/2, _, _, _, [text, int]), RelPlans).

:- end_tests(phase5_value_plane).

% ═══════════════════════════════════════════════════════════════════════════
% REGISTRY ROW TO ORACLE CLASSIFICATION (rank R4)
%
% level_eval.pl carried its own aggregate list, [count, sum, min, max,
% json_array] plus a json_object/2 clause, while analyze.pl already read the
% same set off registry.pl's aggregate rows. Adding an aggregate row updated
% the compiler and silently missed the oracle.
%
% THE CONSTRAINT THIS TABLE EXISTS TO PROTECT: the oracle is deliberately
% WIDER than the compiler. json_array/1 and json_object/2 carry
% head(refuse(aggregate)) in registry.pl and the compiler refuses them, but the
% reference engine EXECUTES both. So the oracle's classification must key off
% the aggregate AXIS and ignore the Status field entirely. A lookup that
% filtered on `live` would silently stop treating a json aggregate head as an
% aggregate, and it would stop QUIETLY: the rule would fall through to plain
% level evaluation and derive a row per body derivation instead of one grouped
% row.
%
% Tested through split_rules/4 rather than the classifier directly, so the
% assertion is about oracle BEHAVIOR and needs no new export.

:- begin_tests(oracle_aggregate_classification).

% One head term per registry aggregate row, arity taken from the row.
aggregate_head_term(Signature, Head) :-
    expression_free_aggregate(Signature, Name/Arity),
    length(Args, Arity),
    AggregateTerm =.. [Name | Args],
    Head =.. [total, AggregateTerm].

expression_free_aggregate(Signature, Signature) :-
    surface(Signature, aggregate, _, _, _).

% =@= and not ==: split_rules/4 collects through findall/3, which copies, so
% the returned rule is a VARIANT of the one passed in rather than the same
% term. == would fail here for a reason that has nothing to do with
% classification.
test(every_registry_aggregate_row_is_an_oracle_aggregate) :-
    forall(aggregate_head_term(_, Head),
           ( Rule = (Head <- src(1)),
             split_rules([Rule], AggregateRules, PlainLevel, _),
             AggregateRules =@= [Rule],
             PlainLevel == [] )).

% The registry has to actually carry the seven, or the test above is vacuous.
test(registry_carries_the_seven_aggregate_rows) :-
    findall(Signature, surface(Signature, aggregate, _, _, _), Rows),
    msort(Rows, Sorted),
    Sorted == [ avg/1, count/1, json_array/1, json_object/2, max/1, min/1, sum/1 ].

% The oracle stays WIDER than the compiler. Both json rows are refused by the
% compiler and both are still oracle aggregates.
test(refused_json_aggregates_stay_live_in_the_oracle) :-
    forall(member(Signature, [json_array/1, json_object/2]),
           surface(Signature, aggregate, _, head(refuse(aggregate)), refused)),
    forall(( member(Name/Arity, [json_array/1, json_object/2]),
             length(Args, Arity),
             AggregateTerm =.. [Name | Args],
             Head =.. [total, AggregateTerm] ),
           ( Rule = (Head <- src(1)),
             split_rules([Rule], AggregateRules, PlainLevel, EdgeRules),
             AggregateRules =@= [Rule],
             PlainLevel == [],
             EdgeRules == [] )).

% A head with no aggregate argument is a plain level rule, so the classifier
% has not become "any compound argument is an aggregate".
test(plain_head_is_not_an_aggregate) :-
    Rule = (total(1) <- src(1)),
    split_rules([Rule], [], [Rule], []).

% A compound head argument that is NOT a registry aggregate stays plain.
test(non_aggregate_compound_head_argument_stays_plain) :-
    Rule = (total(wrapped(1)) <- src(1)),
    split_rules([Rule], [], [Rule], []).

:- end_tests(oracle_aggregate_classification).

% ═══════════════════════════════════════════════════════════════════════════
% relation values at depth >= 2
% ═══════════════════════════════════════════════════════════════════════════
%
% The conformance corpus grades the ROWS (fixtures/6_relation_depth.pl) and
% tsv2/tests/relationDepth.test.ts grades the query PLAN. What is left for a
% unit test is the two compiler-only refusals: positions the dictionary
% rewrite deliberately does not enter. Neither has a fixture, because neither
% is a language refusal -- the reference engine executes both happily -- and a
% conformance fixture would assert the oracle's answer while saying nothing
% about the compiler's.

:- begin_tests(relation_depth_lowering).

depth_program(Rules, plan(depth, prog(Decls, Rules), RelPlans, [raw/4],
                          LevelRules, EdgeRules)) :-
    Decls = [ type_decl(repo,  [col(name, text)]),
              col_type(repo/1, name, text),
              type_decl(fpath, [col(name, text)]),
              col_type(fpath/1, name, text),
              type_decl(file,  [col(repo, repo), col(at, fpath)]),
              col_type(file/2, repo, repo), col_type(file/2, at, fpath),
              col_type(span/3, file, file),
              col_type(span/3, start, int), col_type(span/3, end, int),
              col_type(raw/4, repo_name, text), col_type(raw/4, path_name, text),
              col_type(raw/4, start, int), col_type(raw/4, end, int),
              col_type(seen/1, start, int) ],
    RelPlans = [ relplan(repo/1,  set, [name], none, [text]),
                 relplan(fpath/1, set, [name], none, [text]),
                 relplan(file/2,  set, [repo, at], none, [ref(repo), ref(fpath)]),
                 relplan(span/3,  set, [file, start, end], none,
                         [ref(file), int, int]),
                 relplan(raw/4,   set, [repo_name, path_name, start, end], none,
                         [text, text, int, int]),
                 relplan(seen/1,  set, [start], none, [int]) ],
    include(rule_is_level, Rules, LevelRules),
    include(rule_is_edge, Rules, EdgeRules).

% A depth-2 pattern under not/1. The NOT EXISTS subquery the negation lowers
% to has no room for the per-level joins, so the rewrite does not enter it and
% the leftover term is named rather than compiled back into the json_extract
% that used to answer nothing.
test(relation_pattern_under_negation_is_refused) :-
    depth_program([ (seen(Start) <-
                        raw(_, _, Start, _),
                        not(span(file(_, fpath('a.rs')), Start, _))) ],
                  Plan),
    catch((lower_program(Plan, _), fail), Thrown, true),
    Thrown = unsupported_construct(
                 relation_pattern_not_lowerable(span/3, file, file, _)).

% The same value in an EDGE rule. edge_statements_for_rule/3 compiles against
% RelPlans alone -- the dictionary plans are level-body-only by construction --
% so there is nowhere for the join to go. Refused with its own name, because
% the fix for it is a different piece of work than the negation case.
test(relation_value_in_edge_rule_is_refused) :-
    depth_program([ (span(file(repo(Name), fpath(Path)), Start, End) <+
                        raw(Name, Path, Start, End)) ],
                  Plan),
    catch((lower_program(Plan, _), fail), Thrown, true),
    Thrown = unsupported_construct(
                 relation_value_in_edge_rule(span/3, file, file, _)).

% The positive control for both: the same depth-2 construction as a LEVEL rule
% lowers, and it lowers to one dictionary atom per level rather than to a
% json_extract of the integer endpoint.
test(depth_two_level_construction_lowers_to_one_join_per_level) :-
    depth_program([ (span(file(repo(Name), fpath(Path)), Start, End) <-
                        raw(Name, Path, Start, End)) ],
                  Plan),
    lower_program(Plan, Lowered),
    arg(5, Lowered, LevelStatements),
    memberchk(levelstmt(span/3, _, InsertSqls, _, _, _), LevelStatements),
    atomic_list_concat(InsertSqls, ' ', Sql),
    forall(member(View, ['"__ref_repo"', '"__ref_fpath"', '"__ref_file"']),
           sub_atom(Sql, _, _, _, View)),
    \+ sub_atom(Sql, _, _, _, 'json_extract(b').

:- end_tests(relation_depth_lowering).
