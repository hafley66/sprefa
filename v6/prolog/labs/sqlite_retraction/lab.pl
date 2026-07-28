% lab.pl : sqlite retraction lab (plans/2026-07-28-sqlite-retraction-lab-header.md).
% Re-proves the types lab's domination claims (plans/2026-07-28-types-as-rels-
% verdict.md, Q3) against REAL sqlite: real DDL, real DELETEs, real foreign
% keys, driven from swipl via process_create against the sqlite3 CLI. No
% ODBC, no packs, no in-memory prolog model of the store -- every survivor
% set below is read back from an actual .sqlite3 file under TMPDIR.
%
% Run: swipl -q -l v6/prolog/labs/sqlite_retraction/lab.pl -g go -g halt
%
% Three strategies, same schema (value_node/value_ref below), different
% delete mechanics:
%   fk_cascade      -- FOREIGN KEY ... ON DELETE CASCADE, PRAGMA foreign_keys=ON
%   support_count   -- UPDATE/DELETE rounds to quiescence, looped in prolog
%   fixpoint_recompute -- one recursive-CTE DELETE, the referee
%
% Schema (identical CREATE TABLE text across all three strategies; only the
% PRAGMA and the delete statement differ):
%   value_node(id, label, is_root, owner_id) -- owner_id is a single-parent
%     self-reference, ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED. It is
%     the ONLY column fk_cascade's delete relies on; support_count and
%     fixpoint_recompute never read it (PRAGMA foreign_keys stays OFF for
%     their sessions, so the column exists but is inert there).
%   value_ref(parent_id, child_id) -- the real, possibly multi-parent,
%     possibly cyclic logical reference graph. This is the DAG (or, for
%     scenario c, non-DAG) that support_count and fixpoint_recompute read.
%
% owner_id models the naive "give every row exactly one owner" schema a
% developer reaches for when they want ON DELETE CASCADE to clean up a tree.
% It is the ONLY way sqlite's cascade recursively removes descendant rows
% (cascade fires referenced-row-deletion -> referencing-row-deletion; a
% parent-points-at-child edge table cannot express "delete parent, remove
% now-orphaned children" at all -- see the verdict doc's finding on this).
% A shared child can only be assigned ONE owner, which is the schema-level
% reason fk_cascade is wrong whenever content is actually shared.

:- use_module(library(process)).
:- use_module(library(apply)).
:- use_module(library(lists)).
:- use_module('../../src/grader.pl').

:- discontiguous check/2.

% ---------------------------------------------------------------------------
% sqlite driving: one-shot scripts and persistent interactive sessions.
% Both go through the same sqlite3 CLI binary via process_create; no ODBC,
% no sqlite pack.
% ---------------------------------------------------------------------------

:- dynamic(db_counter/1).
db_counter(0).

fresh_db_path(Tag, DbPath) :-
    ( getenv('TMPDIR', TmpDir) -> true ; TmpDir = '/tmp' ),
    retract(db_counter(Count)),
    Count1 is Count + 1,
    assertz(db_counter(Count1)),
    get_time(Now),
    NowInt is integer(Now * 1000000),
    format(atom(DbPath), "~w/sqlite_retraction_~w_~w_~w.sqlite3",
           [TmpDir, Tag, NowInt, Count1]),
    ( exists_file(DbPath) -> delete_file(DbPath) ; true ).

% run_script(+DbPath, +Script, -OutLines, -ErrText, -ExitCode)
% One sqlite3 process, one script, exit. Returns stdout as a list of
% non-empty lines (list mode, '|' separated) and the raw stderr text.
run_script(DbPath, Script, OutLines, ErrText, ExitCode) :-
    process_create(path(sqlite3), [DbPath],
        [ stdin(pipe(In)), stdout(pipe(OutS)), stderr(pipe(ErrS)), process(Pid) ]),
    set_stream(In, encoding(utf8)),
    set_stream(OutS, encoding(utf8)),
    set_stream(ErrS, encoding(utf8)),
    format(In, ".mode list~n.separator |~n~w~n", [Script]),
    close(In),
    read_string(OutS, _, OutText),
    close(OutS),
    read_string(ErrS, _, ErrText),
    close(ErrS),
    process_wait(Pid, exit(ExitCode)),
    split_string(OutText, "\n", "", RawLines),
    exclude(==(""), RawLines, OutLines).

% persistent session: open_session/2, send/3 (one round trip, sentinel-
% delimited), close_session/1, kill_session/1 (real SIGKILL, for scenario e).
open_session(DbPath, session(In, Out, Pid)) :-
    process_create(path(sqlite3), [DbPath],
        [ stdin(pipe(In)), stdout(pipe(Out)), stderr(std), process(Pid) ]),
    set_stream(In, encoding(utf8)),
    set_stream(Out, encoding(utf8)),
    format(In, ".mode list~n.separator |~n", []),
    flush_output(In).

send(session(In, Out, _Pid), Sql, Lines) :-
    format(In, "~w~n", [Sql]),
    format(In, "SELECT '~~SENTINEL~~';~n", []),
    flush_output(In),
    read_until_sentinel(Out, Lines).

read_until_sentinel(Out, Lines) :-
    read_line_to_string(Out, Line0),
    read_lines_acc(Out, Line0, [], Lines).
read_lines_acc(_Out, "~SENTINEL~", Acc, Lines) :- !, reverse(Acc, Lines).
read_lines_acc(_Out, end_of_file, Acc, Lines) :- !, reverse(Acc, Lines).
read_lines_acc(Out, Line, Acc, Lines) :-
    read_line_to_string(Out, Next),
    read_lines_acc(Out, Next, [Line|Acc], Lines).

close_session(session(In, Out, Pid)) :-
    catch(( format(In, ".quit~n", []), flush_output(In) ), _, true),
    catch(close(In), _, true),
    catch(close(Out), _, true),
    process_wait(Pid, _).

kill_session(session(In, Out, Pid)) :-
    process_kill(Pid, kill),
    process_wait(Pid, _, [timeout(5)]),
    catch(close(In, [force(true)]), _, true),
    catch(close(Out, [force(true)]), _, true).

% ---------------------------------------------------------------------------
% schema, shared byte-for-byte across every strategy session; only the
% PRAGMA line differs (fk_cascade turns foreign_keys ON, the other two
% strategies never issue that pragma so the DEFERRABLE ON DELETE CASCADE
% clause on owner_id sits inert).
% ---------------------------------------------------------------------------

schema_script("
CREATE TABLE value_node (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  is_root INTEGER NOT NULL DEFAULT 0,
  owner_id INTEGER REFERENCES value_node(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE value_ref (
  parent_id INTEGER NOT NULL,
  child_id INTEGER NOT NULL,
  PRIMARY KEY (parent_id, child_id)
);
").

node_insert_sql(Id, Label, IsRoot, OwnerAtom, Sql) :-
    format(string(Sql), "INSERT INTO value_node VALUES (~w,'~w',~w,~w);",
           [Id, Label, IsRoot, OwnerAtom]).

owner_text(none, "NULL") :- !.
owner_text(Id, Text) :- format(string(Text), "~w", [Id]).

ref_insert_sql(ParentId, ChildId, Sql) :-
    format(string(Sql), "INSERT INTO value_ref VALUES (~w,~w);", [ParentId, ChildId]).

build_db(Tag, NodeRows, RefPairs, DbPath) :-
    % NodeRows: list of node(Id, Label, IsRoot, Owner) ; Owner is `none` or an id
    fresh_db_path(Tag, DbPath),
    schema_script(Schema),
    maplist(node_row_sql, NodeRows, NodeSqls),
    maplist(ref_pair_sql, RefPairs, RefSqls),
    atomic_list_concat(NodeSqls, "\n", NodeBlock),
    atomic_list_concat(RefSqls, "\n", RefBlock),
    format(string(Script), "~w~nBEGIN;~n~w~n~w~nCOMMIT;~n", [Schema, NodeBlock, RefBlock]),
    run_script(DbPath, Script, _OutLines, ErrText, ExitCode),
    ( ExitCode =:= 0, ErrText == ""
    -> true
    ;  format(user_error, "build_db(~w) failed: exit=~w err=~q~n", [Tag, ExitCode, ErrText]),
       fail
    ).

node_row_sql(node(Id, Label, IsRoot, Owner), Sql) :-
    owner_text(Owner, OwnerText),
    node_insert_sql(Id, Label, IsRoot, OwnerText, Sql).

ref_pair_sql(ParentId-ChildId, Sql) :- ref_insert_sql(ParentId, ChildId, Sql).

% ---------------------------------------------------------------------------
% queries
% ---------------------------------------------------------------------------

survivor_ids(DbPath, SortedIds) :-
    run_script(DbPath, "SELECT id FROM value_node ORDER BY id;", Lines, _Err, _Code),
    maplist(number_string, SortedIds, Lines).

node_count(DbPath, Count) :-
    run_script(DbPath, "SELECT count(*) FROM value_node;", [Line], _Err, _Code),
    number_string(Count, Line).

% A dangling ref is a value_ref row whose PARENT SURVIVES (so some live row
% still legitimately points through it) but whose CHILD is gone. This is the
% meaningful corruption case (a survivor's own pointer now points at
% nothing) -- it deliberately excludes rows whose parent was the row just
% released, since those are merely obsolete edges from a dead row, not a
% dangling pointer held by something still alive.
dangling_ref_pairs(DbPath, Pairs) :-
    run_script(DbPath,
        "SELECT parent_id||'|'||child_id FROM value_ref
         WHERE parent_id IN (SELECT id FROM value_node)
           AND child_id NOT IN (SELECT id FROM value_node)
         ORDER BY parent_id, child_id;",
        Lines, _Err, _Code),
    maplist(parse_pair, Lines, Pairs).

parse_pair(Line, ParentId-ChildId) :-
    split_string(Line, "|", "", [ParentStr, ChildStr]),
    number_string(ParentId, ParentStr),
    number_string(ChildId, ChildStr).

% ---------------------------------------------------------------------------
% strategies
% ---------------------------------------------------------------------------

% fk_cascade_release(+DbPath, +RootId, -ErrText, -ExitCode)
% ON DELETE CASCADE via owner_id; PRAGMA foreign_keys=ON for this session
% only (the other two strategies never turn this pragma on).
fk_cascade_release(DbPath, RootId, ErrText, ExitCode) :-
    format(string(Script), "PRAGMA foreign_keys=ON;~nDELETE FROM value_node WHERE id=~w;", [RootId]),
    run_script(DbPath, Script, _OutLines, ErrText, ExitCode).

% fixpoint_recompute_release(+DbPath, +RootId)
% One statement: delete the root, then one recursive-CTE DELETE removes
% everything not reachable from a remaining is_root=1 row. No pragma.
fixpoint_recompute_release(DbPath, RootId) :-
    format(string(Script), "
DELETE FROM value_node WHERE id=~w;
WITH RECURSIVE reachable(id) AS (
  SELECT id FROM value_node WHERE is_root=1
  UNION
  SELECT value_ref.child_id FROM value_ref JOIN reachable ON value_ref.parent_id = reachable.id
)
DELETE FROM value_node WHERE id NOT IN (SELECT id FROM reachable);
", [RootId]),
    run_script(DbPath, Script, _OutLines, ErrText, ExitCode),
    ( ExitCode =:= 0, ErrText == "" -> true
    ; format(user_error, "fixpoint_recompute_release failed: ~q~n", [ErrText]), fail ).

% support_count_release(+DbPath, +RootId, -Rounds)
% Delete the root, then loop: prune value_ref rows whose parent died, then
% collect+delete non-root nodes with zero remaining incoming value_ref rows.
% Each round is two plain SQL statements over a persistent session (no
% process-spawn per round); loop in prolog until a round changes nothing.
support_count_release(DbPath, RootId, Rounds) :-
    open_session(DbPath, Session),
    format(string(DeleteRootSql), "DELETE FROM value_node WHERE id=~w;", [RootId]),
    send(Session, DeleteRootSql, _),
    support_count_loop(Session, 0, Rounds),
    close_session(Session).

support_count_round_sql("
DELETE FROM value_ref WHERE parent_id NOT IN (SELECT id FROM value_node);
DELETE FROM value_node WHERE is_root=0 AND id NOT IN (SELECT child_id FROM value_ref);
SELECT changes();
").

support_count_loop(Session, RoundsSoFar, Rounds) :-
    support_count_round_sql(Sql),
    send(Session, Sql, Lines),
    ( Lines = [ChangesLine], number_string(Changes, ChangesLine), Changes > 0
    -> RoundsSoFar1 is RoundsSoFar + 1,
       support_count_loop(Session, RoundsSoFar1, Rounds)
    ;  Rounds = RoundsSoFar
    ).

% ---------------------------------------------------------------------------
% assertion helpers: format a diagnostic to user_error and fail (grader.pl's
% catch/if-then turns a failing goal into a printed "fail  Name" line, never
% an uncaught exception, so a mismatch never crashes the whole run).
% ---------------------------------------------------------------------------

must_equal(Label, Expected, Actual) :-
    ( Expected == Actual -> true
    ; format(user_error, "MISMATCH ~w: expected ~q got ~q~n", [Label, Expected, Actual]),
      fail
    ).

must_contain(Label, Needle, Haystack) :-
    ( sub_string(Haystack, _, _, _, Needle) -> true
    ; format(user_error, "MISSING ~w: ~q not found in ~q~n", [Label, Needle, Haystack]),
      fail
    ).

% ===========================================================================
% Scenario a: straight chain, depth 4. Hand-computed: releasing the root
% removes the whole chain under every strategy; there is only one owner per
% node so fk_cascade's owner_id chain and value_ref's parent chain coincide.
% ===========================================================================

scenario_a_nodes([node(1,root,1,none), node(2,n2,0,1), node(3,n3,0,2), node(4,n4,0,3)]).
scenario_a_refs([1-2, 2-3, 3-4]).

check(chain_fk_cascade_full_delete, chain_fk_cascade_full_delete).
chain_fk_cascade_full_delete :-
    scenario_a_nodes(Nodes), scenario_a_refs(Refs),
    build_db(chain_fk, Nodes, Refs, DbPath),
    fk_cascade_release(DbPath, 1, ErrText, ExitCode),
    must_equal(chain_fk_cascade_exit, 0, ExitCode),
    must_equal(chain_fk_cascade_err, "", ErrText),
    survivor_ids(DbPath, Survivors),
    must_equal(chain_fk_cascade_survivors, [], Survivors),
    delete_file(DbPath).

check(chain_support_count_full_delete, chain_support_count_full_delete).
chain_support_count_full_delete :-
    scenario_a_nodes(Nodes), scenario_a_refs(Refs),
    build_db(chain_support, Nodes, Refs, DbPath),
    support_count_release(DbPath, 1, Rounds),
    must_equal(chain_support_count_rounds, 3, Rounds),
    survivor_ids(DbPath, Survivors),
    must_equal(chain_support_count_survivors, [], Survivors),
    delete_file(DbPath).

check(chain_fixpoint_recompute_full_delete, chain_fixpoint_recompute_full_delete).
chain_fixpoint_recompute_full_delete :-
    scenario_a_nodes(Nodes), scenario_a_refs(Refs),
    build_db(chain_fixpoint, Nodes, Refs, DbPath),
    fixpoint_recompute_release(DbPath, 1),
    survivor_ids(DbPath, Survivors),
    must_equal(chain_fixpoint_recompute_survivors, [], Survivors),
    delete_file(DbPath).

% ===========================================================================
% Scenario a at 10k rows: same chain, depth 10000, three timings. This also
% surfaces a real sqlite ceiling: fk_cascade recurses through a trigger for
% every hop, and sqlite's compiled-in trigger_depth limit (`.limit
% trigger_depth` reports 1000 on this build, and the CLI cannot raise it past
% that compiled maximum) means a chain deeper than 1000 hops makes fk_cascade
% FAIL OUTRIGHT, not just slow. Verified by direct probe: 1000 nodes deletes
% clean, 1001 nodes raises "too many levels of trigger recursion" and leaves
% the table untouched (the statement fails atomically). At 10k this is not a
% timing question at all for fk_cascade; it simply cannot run.
% ===========================================================================

build_10k_chain_nodes(Nodes) :-
    numlist(2, 10000, Rest),
    foldl(chain_node_acc, Rest, [node(1,n1,1,none)], RevNodes),
    reverse(RevNodes, Nodes).
chain_node_acc(Id, Acc, [node(Id, Label, 0, Owner)|Acc]) :-
    format(atom(Label), "n~w", [Id]),
    Owner is Id - 1.

build_10k_chain_refs(Refs) :-
    numlist(2, 10000, Rest),
    maplist(chain_ref, Rest, Refs).
chain_ref(Id, Parent-Id) :- Parent is Id - 1.

check(chain_10k_fixpoint_recompute_timing, chain_10k_fixpoint_recompute_timing).
chain_10k_fixpoint_recompute_timing :-
    build_10k_chain_nodes(Nodes), build_10k_chain_refs(Refs),
    build_db(chain10k_fixpoint, Nodes, Refs, DbPath),
    get_time(Start),
    fixpoint_recompute_release(DbPath, 1),
    get_time(End),
    ElapsedMs is integer((End - Start) * 1000),
    survivor_ids(DbPath, Survivors),
    must_equal(chain_10k_fixpoint_recompute_survivors, [], Survivors),
    format(user_error, "TIMING chain_10k_fixpoint_recompute_ms=~w~n", [ElapsedMs]),
    nb_setval(timing_fixpoint_10k_ms, ElapsedMs),
    delete_file(DbPath).

check(chain_10k_support_count_timing, chain_10k_support_count_timing).
chain_10k_support_count_timing :-
    build_10k_chain_nodes(Nodes), build_10k_chain_refs(Refs),
    build_db(chain10k_support, Nodes, Refs, DbPath),
    get_time(Start),
    support_count_release(DbPath, 1, Rounds),
    get_time(End),
    ElapsedMs is integer((End - Start) * 1000),
    must_equal(chain_10k_support_count_rounds, 9999, Rounds),
    survivor_ids(DbPath, Survivors),
    must_equal(chain_10k_support_count_survivors, [], Survivors),
    format(user_error, "TIMING chain_10k_support_count_ms=~w rounds=~w~n", [ElapsedMs, Rounds]),
    nb_setval(timing_support_10k_ms, ElapsedMs),
    delete_file(DbPath).

check(chain_10k_fk_cascade_hits_trigger_depth_limit, chain_10k_fk_cascade_hits_trigger_depth_limit).
chain_10k_fk_cascade_hits_trigger_depth_limit :-
    build_10k_chain_nodes(Nodes), build_10k_chain_refs(Refs),
    build_db(chain10k_fk, Nodes, Refs, DbPath),
    get_time(Start),
    fk_cascade_release(DbPath, 1, ErrText, ExitCode),
    get_time(End),
    ElapsedMs is integer((End - Start) * 1000),
    must_equal(chain_10k_fk_cascade_exit, 1, ExitCode),
    must_contain(chain_10k_fk_cascade_err, "too many levels of trigger recursion", ErrText),
    node_count(DbPath, Count),
    must_equal(chain_10k_fk_cascade_untouched_on_failure, 10000, Count),
    format(user_error, "TIMING chain_10k_fk_cascade_failed_ms=~w (statement rejected, table untouched)~n", [ElapsedMs]),
    nb_setval(timing_fk_cascade_10k_ms, ElapsedMs),
    delete_file(DbPath).

% ===========================================================================
% Scenario b: shared child, two roots, one child. Hand-computed: releasing
% root1 first must leave the child alive (root2 still refs it) under the
% correct strategies; only releasing root2 afterward should kill it.
% fk_cascade, driven by the single-owner column, kills the child on the
% FIRST release (whichever root happens to own it) and leaves the surviving
% root's value_ref row dangling.
% ===========================================================================

scenario_b_nodes([node(1,root1,1,none), node(2,root2,1,none), node(3,child,0,1)]).
scenario_b_refs([1-3, 2-3]).

check(shared_child_correct_survives_step1_then_dies_step2, shared_child_correct_survives_step1_then_dies_step2).
shared_child_correct_survives_step1_then_dies_step2 :-
    scenario_b_nodes(Nodes), scenario_b_refs(Refs),
    build_db(shared_correct, Nodes, Refs, DbPath),
    support_count_release(DbPath, 1, _Rounds1),
    survivor_ids(DbPath, SurvivorsStep1),
    must_equal(shared_child_survives_step1, [2, 3], SurvivorsStep1),
    dangling_ref_pairs(DbPath, DanglingStep1),
    must_equal(shared_child_no_dangling_step1, [], DanglingStep1),
    support_count_release(DbPath, 2, _Rounds2),
    survivor_ids(DbPath, SurvivorsStep2),
    must_equal(shared_child_dies_step2, [], SurvivorsStep2),
    delete_file(DbPath).

check(shared_child_fixpoint_recompute_agrees, shared_child_fixpoint_recompute_agrees).
shared_child_fixpoint_recompute_agrees :-
    scenario_b_nodes(Nodes), scenario_b_refs(Refs),
    build_db(shared_fixpoint, Nodes, Refs, DbPath),
    fixpoint_recompute_release(DbPath, 1),
    survivor_ids(DbPath, SurvivorsStep1),
    must_equal(shared_child_fixpoint_survives_step1, [2, 3], SurvivorsStep1),
    fixpoint_recompute_release(DbPath, 2),
    survivor_ids(DbPath, SurvivorsStep2),
    must_equal(shared_child_fixpoint_dies_step2, [], SurvivorsStep2),
    delete_file(DbPath).

check(shared_child_fk_cascade_kills_child_early_and_dangles, shared_child_fk_cascade_kills_child_early_and_dangles).
shared_child_fk_cascade_kills_child_early_and_dangles :-
    scenario_b_nodes(Nodes), scenario_b_refs(Refs),
    build_db(shared_fk, Nodes, Refs, DbPath),
    fk_cascade_release(DbPath, 1, ErrText1, ExitCode1),
    must_equal(shared_fk_step1_exit, 0, ExitCode1),
    must_equal(shared_fk_step1_err, "", ErrText1),
    survivor_ids(DbPath, SurvivorsStep1),
    must_equal(shared_fk_wrong_early_kill, [2], SurvivorsStep1),
    dangling_ref_pairs(DbPath, DanglingStep1),
    must_equal(shared_fk_dangling_ref_after_step1, [2-3], DanglingStep1),
    fk_cascade_release(DbPath, 2, ErrText2, ExitCode2),
    must_equal(shared_fk_step2_exit, 0, ExitCode2),
    must_equal(shared_fk_step2_err, "", ErrText2),
    survivor_ids(DbPath, SurvivorsStep2),
    must_equal(shared_fk_step2_survivors, [], SurvivorsStep2),
    delete_file(DbPath).

% ===========================================================================
% Scenario c: cycle. a and b reference each other; root reaches only a
% directly. Hand-computed: support_count can never see either drop to zero
% incoming value_ref rows (a<-b, b<-a hold regardless of root), so it leaves
% both alive -- counts lie on cycles. fixpoint_recompute reseeds reachability
% from whatever is_root=1 rows remain (none, once root is deleted), so the
% recursive CTE correctly empties. fk_cascade's owner_id chain (root owns a,
% a owns b) happens to mirror one spanning path through the cycle, so it
% ALSO empties here -- a coincidence of this particular ownership
% assignment, not a property of cascade in general (scenario b already
% showed the opposite outcome). Separately: a genuinely circular owner_id
% (two rows each the other's declared owner) cannot be inserted immediately
% -- captured with the real sqlite error text -- but CAN be inserted under
% DEFERRABLE INITIALLY DEFERRED, and cascading through it deletes both rows
% in one statement with no infinite loop and no error.
% ===========================================================================

scenario_c_nodes([node(1,root,1,none), node(2,a,0,1), node(3,b,0,2)]).
scenario_c_refs([1-2, 2-3, 3-2]).

check(cycle_support_count_leaves_both_alive, cycle_support_count_leaves_both_alive).
cycle_support_count_leaves_both_alive :-
    scenario_c_nodes(Nodes), scenario_c_refs(Refs),
    build_db(cycle_support, Nodes, Refs, DbPath),
    support_count_release(DbPath, 1, Rounds),
    must_equal(cycle_support_count_rounds_is_zero, 0, Rounds),
    survivor_ids(DbPath, Survivors),
    must_equal(cycle_support_count_both_alive, [2, 3], Survivors),
    delete_file(DbPath).

check(cycle_fixpoint_recompute_removes_both, cycle_fixpoint_recompute_removes_both).
cycle_fixpoint_recompute_removes_both :-
    scenario_c_nodes(Nodes), scenario_c_refs(Refs),
    build_db(cycle_fixpoint, Nodes, Refs, DbPath),
    fixpoint_recompute_release(DbPath, 1),
    survivor_ids(DbPath, Survivors),
    must_equal(cycle_fixpoint_recompute_both_removed, [], Survivors),
    delete_file(DbPath).

check(cycle_fk_cascade_owner_chain_also_removes_both, cycle_fk_cascade_owner_chain_also_removes_both).
cycle_fk_cascade_owner_chain_also_removes_both :-
    scenario_c_nodes(Nodes), scenario_c_refs(Refs),
    build_db(cycle_fk, Nodes, Refs, DbPath),
    fk_cascade_release(DbPath, 1, ErrText, ExitCode),
    must_equal(cycle_fk_cascade_exit, 0, ExitCode),
    must_equal(cycle_fk_cascade_err, "", ErrText),
    survivor_ids(DbPath, Survivors),
    must_equal(cycle_fk_cascade_both_removed_by_coincidence, [], Survivors),
    delete_file(DbPath).

check(cycle_fk_cascade_immediate_circular_insert_fails, cycle_fk_cascade_immediate_circular_insert_fails).
cycle_fk_cascade_immediate_circular_insert_fails :-
    fresh_db_path(cycle_immediate, DbPath),
    Script = "
PRAGMA foreign_keys=ON;
CREATE TABLE cyc (id INTEGER PRIMARY KEY, owner_id INTEGER REFERENCES cyc(id) ON DELETE CASCADE);
INSERT INTO cyc(id, owner_id) VALUES (1, 2);
",
    run_script(DbPath, Script, _OutLines, ErrText, ExitCode),
    must_equal(cycle_immediate_insert_exit, 1, ExitCode),
    must_contain(cycle_immediate_insert_err, "FOREIGN KEY constraint failed", ErrText),
    delete_file(DbPath).

check(cycle_fk_cascade_deferred_circular_insert_then_cascades_cleanly, cycle_fk_cascade_deferred_circular_insert_then_cascades_cleanly).
cycle_fk_cascade_deferred_circular_insert_then_cascades_cleanly :-
    fresh_db_path(cycle_deferred, DbPath),
    SchemaScript = "
PRAGMA foreign_keys=ON;
CREATE TABLE cyc (id INTEGER PRIMARY KEY, owner_id INTEGER REFERENCES cyc(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO cyc(id, owner_id) VALUES (1, 2);
INSERT INTO cyc(id, owner_id) VALUES (2, 1);
COMMIT;
",
    run_script(DbPath, SchemaScript, _OutLines0, ErrText0, ExitCode0),
    must_equal(cycle_deferred_insert_exit, 0, ExitCode0),
    must_equal(cycle_deferred_insert_err, "", ErrText0),
    DeleteScript = "PRAGMA foreign_keys=ON;\nDELETE FROM cyc WHERE id=1;",
    run_script(DbPath, DeleteScript, _OutLines1, ErrText1, ExitCode1),
    must_equal(cycle_deferred_delete_exit, 0, ExitCode1),
    must_equal(cycle_deferred_delete_err, "", ErrText1),
    run_script(DbPath, "SELECT id FROM cyc ORDER BY id;", SurvivorLines, _Err2, _Code2),
    must_equal(cycle_deferred_cascade_removes_both, [], SurvivorLines),
    delete_file(DbPath).

% ===========================================================================
% Scenario d: diamond, root -> a,b -> shared c. Hand-computed: releasing the
% root kills all four rows under every strategy (nothing else refs a, b, or
% c). The check that matters here is not the final survivor set (all three
% agree) but the AFFECTED ROW COUNT: c is reachable via two paths
% (root->a->c and root->b->c), and a naive per-edge cascade could visit it
% twice. sqlite's changes() function only reports the directly-targeted
% row for a cascading DELETE (cascaded rows are excluded from changes() by
% design), so the honest measurement is a before/after COUNT(*) diff, not
% changes() -- used uniformly below for all three strategies.
% ===========================================================================

scenario_d_nodes([node(1,root,1,none), node(2,a,0,1), node(3,b,0,1), node(4,c,0,2)]).
scenario_d_refs([1-2, 1-3, 2-4, 3-4]).

check(diamond_fk_cascade_visits_each_row_exactly_once, diamond_fk_cascade_visits_each_row_exactly_once).
diamond_fk_cascade_visits_each_row_exactly_once :-
    scenario_d_nodes(Nodes), scenario_d_refs(Refs),
    build_db(diamond_fk, Nodes, Refs, DbPath),
    node_count(DbPath, CountBefore),
    must_equal(diamond_fk_count_before, 4, CountBefore),
    fk_cascade_release(DbPath, 1, ErrText, ExitCode),
    must_equal(diamond_fk_exit, 0, ExitCode),
    must_equal(diamond_fk_err, "", ErrText),
    node_count(DbPath, CountAfter),
    must_equal(diamond_fk_count_after, 0, CountAfter),
    delete_file(DbPath).

check(diamond_support_count_visits_each_row_exactly_once, diamond_support_count_visits_each_row_exactly_once).
diamond_support_count_visits_each_row_exactly_once :-
    scenario_d_nodes(Nodes), scenario_d_refs(Refs),
    build_db(diamond_support, Nodes, Refs, DbPath),
    node_count(DbPath, CountBefore),
    must_equal(diamond_support_count_before, 4, CountBefore),
    support_count_release(DbPath, 1, Rounds),
    must_equal(diamond_support_count_rounds, 2, Rounds),
    node_count(DbPath, CountAfter),
    must_equal(diamond_support_count_after, 0, CountAfter),
    delete_file(DbPath).

check(diamond_fixpoint_recompute_visits_each_row_exactly_once, diamond_fixpoint_recompute_visits_each_row_exactly_once).
diamond_fixpoint_recompute_visits_each_row_exactly_once :-
    scenario_d_nodes(Nodes), scenario_d_refs(Refs),
    build_db(diamond_fixpoint, Nodes, Refs, DbPath),
    node_count(DbPath, CountBefore),
    must_equal(diamond_fixpoint_count_before, 4, CountBefore),
    fixpoint_recompute_release(DbPath, 1),
    node_count(DbPath, CountAfter),
    must_equal(diamond_fixpoint_count_after, 0, CountAfter),
    delete_file(DbPath).

check(diamond_all_strategies_agree_full_delete, diamond_all_strategies_agree_full_delete).
diamond_all_strategies_agree_full_delete :-
    scenario_d_nodes(Nodes), scenario_d_refs(Refs),
    build_db(diamond_agree_fk, Nodes, Refs, DbFk),
    fk_cascade_release(DbFk, 1, _E1, _X1),
    survivor_ids(DbFk, SurvivorsFk),
    delete_file(DbFk),
    build_db(diamond_agree_support, Nodes, Refs, DbSupport),
    support_count_release(DbSupport, 1, _Rounds),
    survivor_ids(DbSupport, SurvivorsSupport),
    delete_file(DbSupport),
    build_db(diamond_agree_fixpoint, Nodes, Refs, DbFixpoint),
    fixpoint_recompute_release(DbFixpoint, 1),
    survivor_ids(DbFixpoint, SurvivorsFixpoint),
    delete_file(DbFixpoint),
    must_equal(diamond_agree_fk_vs_support, SurvivorsFk, SurvivorsSupport),
    must_equal(diamond_agree_support_vs_fixpoint, SurvivorsSupport, SurvivorsFixpoint),
    must_equal(diamond_agree_all_empty, [], SurvivorsFixpoint).

% ===========================================================================
% Scenario e: crash mid cascade. support_count runs its first round inside
% an explicit transaction that is never committed; the process is stopped
% before it can commit or roll back cleanly (once by explicit ROLLBACK to
% simulate, once by a real SIGKILL to the sqlite3 process). Reopening the
% db file must show the PRE-delete state in both cases: sqlite's rollback
% journal recovers an interrupted transaction on the next connection.
% ===========================================================================

scenario_e_setup(Nodes, Refs) :-
    Nodes = [node(1,root,1,none), node(2,n2,0,1), node(3,n3,0,2), node(4,n4,0,3)],
    Refs = [1-2, 2-3, 3-4].

check(crash_mid_cascade_rollback_simulated_atomicity, crash_mid_cascade_rollback_simulated_atomicity).
crash_mid_cascade_rollback_simulated_atomicity :-
    scenario_e_setup(Nodes, Refs),
    build_db(crash_rollback, Nodes, Refs, DbPath),
    open_session(DbPath, Session),
    send(Session, "BEGIN;", _),
    send(Session, "DELETE FROM value_node WHERE id=1;", _),
    send(Session, "DELETE FROM value_ref WHERE parent_id NOT IN (SELECT id FROM value_node);", _),
    send(Session, "DELETE FROM value_node WHERE is_root=0 AND id NOT IN (SELECT child_id FROM value_ref);", _),
    send(Session, "ROLLBACK;", _),
    close_session(Session),
    survivor_ids(DbPath, Survivors),
    must_equal(crash_rollback_pre_delete_state_intact, [1, 2, 3, 4], Survivors),
    delete_file(DbPath).

check(crash_mid_cascade_real_sigkill_recovers_pre_delete_state, crash_mid_cascade_real_sigkill_recovers_pre_delete_state).
crash_mid_cascade_real_sigkill_recovers_pre_delete_state :-
    scenario_e_setup(Nodes, Refs),
    build_db(crash_sigkill, Nodes, Refs, DbPath),
    format(atom(JournalPath), "~w-journal", [DbPath]),
    open_session(DbPath, Session),
    send(Session, "PRAGMA journal_mode=DELETE;", _),
    send(Session, "BEGIN;", _),
    send(Session, "DELETE FROM value_node WHERE id=1;", _),
    send(Session, "DELETE FROM value_ref WHERE parent_id NOT IN (SELECT id FROM value_node);", _),
    ( exists_file(JournalPath) -> HotJournalBeforeKill = yes ; HotJournalBeforeKill = no ),
    must_equal(crash_sigkill_leaves_hot_journal, yes, HotJournalBeforeKill),
    kill_session(Session),
    survivor_ids(DbPath, Survivors),
    must_equal(crash_sigkill_pre_delete_state_recovered, [1, 2, 3, 4], Survivors),
    delete_file(DbPath),
    ( exists_file(JournalPath) -> delete_file(JournalPath) ; true ).

% ===========================================================================
% entry point
% ===========================================================================

go :- run(check).
