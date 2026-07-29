% lab.pl : UPDATE-ARM lab (contract: plans/2026-07-29-update-arm-lab-header.md).
%
% Grades the zero-construct hypothesis: is the SQL "AFTER UPDATE with OLD and
% NEW" arm already expressible as finalize(old row) joined against the current
% table, with no new construct?
%
% Every check runs the REAL oracle (conformance/engine.pl run_program/5). No
% model, no re-implementation, no network, no daemon.
%
% Run: cd v6/prolog && swipl -q -l labs/update_arm/lab.pl -g go -g halt
%
% SABOTAGE RECEIPTS (probes run against this file, all red as required, so no
% check here passes vacuously):
%   u4 expecting the phantom pair (v1,v2)  -> got [changed_value(cli,v1,v3)]
%   u4 expecting two rows (v1,v2)+(v2,v3)  -> got [changed_value(cli,v1,v3)]
%   u1 expecting the arm at the replace tick
%       -> got [[],[],[+changed_value(cli,v1,v2)],[]] against
%          want [[],[+changed_value(cli,v1,v2)],[]]
%   s3 expecting the shared-scope single row
%       -> got [echoed(api,api),echoed(api,worker)] against want [echoed(api,api)]
%   u3 expecting the pure delete to yield a row -> got []
%   u5 expecting the Log-rel arm to yield a row -> got []

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module('../../src/grader.pl').
:- use_module('../../conformance/engine.pl').

:- use_module(library(lists)).

% ═══ observation helpers ════════════════════════════════════════════════════
% Same expectation vocabulary as the conformance harness (final/2, deltas/2,
% ticks/1), re-stated here because engine.pl keeps expectation_holds/3 private.

expect_run(Program, InitialRows, Schedule, Expectations) :-
    run_program(Program, InitialRows, Schedule, FinalAll, DeltaTicks),
    forall(member(Expectation, Expectations),
           expectation_holds(Expectation, FinalAll, DeltaTicks)).

expectation_holds(final(Ref, Expected), FinalAll, _) :-
    rel_rows(Ref, FinalAll, Actual),
    (   Actual == Expected
    ->  true
    ;   format("    MISMATCH final ~w~n      got  ~q~n      want ~q~n",
               [Ref, Actual, Expected]),
        fail
    ).
expectation_holds(deltas(Ref, Expected), _, DeltaTicks) :-
    rel_deltas(Ref, DeltaTicks, Actual),
    (   Actual == Expected
    ->  true
    ;   format("    MISMATCH deltas ~w~n      got  ~q~n      want ~q~n",
               [Ref, Actual, Expected]),
        fail
    ).
expectation_holds(ticks(Expected), _, DeltaTicks) :-
    length(DeltaTicks, Actual),
    (   Actual == Expected
    ->  true
    ;   format("    MISMATCH ticks got ~w want ~w~n", [Actual, Expected]),
        fail
    ).

expect_throw(Program, InitialRows, Schedule, Expected) :-
    catch(( run_program(Program, InitialRows, Schedule, _, _), fail ),
          Thrown, true),
    (   Thrown == Expected
    ->  true
    ;   format("    MISMATCH throw~n      got  ~q~n      want ~q~n",
               [Thrown, Expected]),
        fail
    ).

% ═══ the programs under test ════════════════════════════════════════════════

% The hypothesis, in the ONE spelling the engine accepts: an EDGE rule whose
% trigger is the departure and whose join is the current table.
update_arm_program(
  prog([ kind(poll_value/2, log), keep(poll_value/2, all),
         keyed(current_value/2, [1]),
         kind(changed_value/3, log), keep(changed_value/3, all) ],
       [ (current_value(Key, Value) <+ poll_value(Key, Value)),
         (changed_value(Key, OldValue, NewValue) <+
              finalize(current_value(Key, OldValue)),
              current_value(Key, NewValue)) ])).

% Same program plus the pure-delete arm: the current-side join negated instead
% of bound. Tests whether replace and delete are separable without a construct.
update_and_delete_arm_program(
  prog([ kind(poll_value/2, log), keep(poll_value/2, all),
         keyed(current_value/2, [1]),
         kind(changed_value/3, log), keep(changed_value/3, all),
         kind(deleted_value/2, log), keep(deleted_value/2, all) ],
       [ (current_value(Key, Value) <+ poll_value(Key, Value)),
         (changed_value(Key, OldValue, NewValue) <+
              finalize(current_value(Key, OldValue)),
              current_value(Key, NewValue)),
         (deleted_value(Key, OldValue) <+
              finalize(current_value(Key, OldValue)),
              not(current_value(Key, _))) ])).

% U6: the same arm written as a match block over the current table.
match_update_arm_program(
  prog([ kind(poll_value/2, log), keep(poll_value/2, all),
         keyed(current_value/2, [1]),
         kind(changed_value/3, log), keep(changed_value/3, all) ],
       [ (current_value(Key, Value) <+ poll_value(Key, Value)),
         match(current_value(Key, NewValue),
               (changed_value(Key, OldValue, NewValue) <+
                    finalize(current_value(Key, OldValue)))) ])).

% ═══ U0 : the header's LITERAL spelling ═════════════════════════════════════
% `changed(Key, Old, New) <- finalize(r(Key, Old)), r(Key, New).`
% engine.pl:113-114 refuses finalize in any level rule, at load time.

u0_literal_level_rule_spelling_is_refused :-
    expect_throw(
      prog([ keyed(current_value/2, [1]) ],
           [ (changed_value(Key, OldValue, NewValue) <-
                  finalize(current_value(Key, OldValue)),
                  current_value(Key, NewValue)) ]),
      [], [],
      finalize_in_level_rule(current_value/2)).

% The edge spelling of the SAME rule loads and runs.
u0_edge_rule_spelling_loads :-
    update_arm_program(Program),
    run_program(Program, [], [], _, _).

% ═══ U1 : keyed replace yields exactly one (Key, Old, New) row ══════════════

u1_keyed_replace_yields_one_pair :-
    update_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1) ], [ +poll_value(cli, v2) ] ],
      [ final(changed_value/3, [ changed_value(cli, v1, v2) ]),
        final(current_value/2, [ current_value(cli, v2) ]) ]).

% Tick placement, read off the oracle's own delta log: the replace lands at
% tick 2, the arm fires at tick 3. Departure is a NEXT-TICK occurrence
% (engine.pl:308-312), so the arm is replace-tick PLUS ONE, never same-tick.
u1_arm_fires_the_tick_after_the_replace :-
    update_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1) ], [ +poll_value(cli, v2) ] ],
      [ deltas(current_value/2,
               [ [ +current_value(cli, v1) ],
                 [ -current_value(cli, v1), +current_value(cli, v2) ],
                 [],
                 [] ]),
        deltas(changed_value/3,
               [ [], [], [ +changed_value(cli, v1, v2) ], [] ]),
        ticks(4) ]).

% U1 rider: the arm does NOT need the keyed decl. Any rel that emits a minus
% delta and holds the successor at T+1 works, including a DERIVED level view
% whose source row was swapped. keyed/2 is one way to produce the minus delta,
% not a precondition of the arm.
u1c_the_arm_works_over_a_derived_level_view :-
    expect_run(
      prog([ kind(changed_value/3, log), keep(changed_value/3, all) ],
           [ (mirror_value(Key, Value) <- source_value(Key, Value)),
             (changed_value(Key, OldValue, NewValue) <+
                  finalize(mirror_value(Key, OldValue)),
                  mirror_value(Key, NewValue)) ]),
      [],
      [ [ +source_value(cli, v1) ],
        [ -source_value(cli, v1), +source_value(cli, v2) ] ],
      [ final(changed_value/3, [ changed_value(cli, v1, v2) ]),
        deltas(changed_value/3,
               [ [], [], [ +changed_value(cli, v1, v2) ], [] ]),
        ticks(4) ]).

% ═══ U2 : plain insert, no prior row ════════════════════════════════════════

u2_plain_insert_leaves_the_arm_silent :-
    update_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1) ] ],
      [ final(changed_value/3, []),
        final(current_value/2, [ current_value(cli, v1) ]),
        deltas(changed_value/3, [ [], [] ]),
        ticks(2) ]).

% ═══ U3 : plain delete, no successor ════════════════════════════════════════
% The departure occurrence DOES fire; the current-side join finds nothing, so
% the rule produces no row. Silent, with no diagnostic anywhere.

u3_plain_delete_leaves_the_arm_silent :-
    update_arm_program(Program),
    expect_run(Program, [ current_value(cli, v1) ],
      [ [ -current_value(cli, v1) ] ],
      [ final(changed_value/3, []),
        final(current_value/2, []),
        deltas(current_value/2, [ [ -current_value(cli, v1) ], [] ]),
        ticks(2) ]).

% U3 rider: delete IS separable from replace, with no new construct, by
% negating the current side. Same tick, same departure, disjoint arms.
u3b_delete_is_separable_by_negating_the_current_side :-
    update_and_delete_arm_program(Program),
    expect_run(Program, [ current_value(cli, v1) ],
      [ [ -current_value(cli, v1) ] ],
      [ final(deleted_value/2, [ deleted_value(cli, v1) ]),
        final(changed_value/3, []) ]).

u3b_replace_does_not_trip_the_delete_arm :-
    update_and_delete_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1) ], [ +poll_value(cli, v2) ] ],
      [ final(changed_value/3, [ changed_value(cli, v1, v2) ]),
        final(deleted_value/2, []) ]).

% ═══ U4 : same-tick double replace, v1 -> v2 -> v3 ══════════════════════════
% The ruled collapse (R2 rider, engine.pl:299-304) means the intermediate v2 is
% not boundary-observable. The arm sees the HONEST ENDPOINT PAIR (v1, v3),
% exactly one row. No phantom (v1, v2) or (v2, v3), and never two rows.

u4_same_tick_double_replace_yields_the_endpoint_pair :-
    update_arm_program(Program),
    expect_run(Program, [ current_value(cli, v1) ],
      [ [ +poll_value(cli, v2), +poll_value(cli, v3) ] ],
      [ final(changed_value/3, [ changed_value(cli, v1, v3) ]),
        final(current_value/2, [ current_value(cli, v3) ]),
        deltas(current_value/2,
               [ [ -current_value(cli, v1), +current_value(cli, v3) ],
                 [],
                 [] ]),
        deltas(changed_value/3,
               [ [], [ +changed_value(cli, v1, v3) ], [] ]),
        ticks(3) ]).

% U4 rider: the same two writes with the rel EMPTY at tick start produce NO
% departure at all, so the arm never fires and v1 is never observable as an
% old value. Firing count is a function of the tick-start state, not the data.
u4b_same_tick_replaces_from_empty_yield_nothing :-
    update_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1), +poll_value(cli, v2) ] ],
      [ final(changed_value/3, []),
        final(current_value/2, [ current_value(cli, v2) ]),
        deltas(current_value/2, [ [ +current_value(cli, v2) ], [] ]),
        ticks(2) ]).

% ═══ U5 : the arm over a Log rel ════════════════════════════════════════════
% Log rels emit only plus deltas (engine.pl:322-337), so no departure
% occurrence is ever minted. Statically dead. No refusal, no warning.

log_arm_program(Retention,
  prog([ kind(event_log/2, log), keep(event_log/2, Retention),
         kind(changed_event/3, log), keep(changed_event/3, all) ],
       [ (changed_event(Key, OldValue, NewValue) <+
              finalize(event_log(Key, OldValue)),
              event_log(Key, NewValue)) ])).

u5_log_rel_arm_is_silently_dead :-
    log_arm_program(all, Program),
    expect_run(Program, [],
      [ [ +event_log(api, v1) ], [ +event_log(api, v2) ] ],
      [ final(changed_event/3, []),
        final(event_log/2, [ event_log(api, v1), event_log(api, v2) ]),
        deltas(changed_event/3, [ [], [] ]),
        ticks(2) ]).

% U5 rider: retention prunes the old row out of the store and emits NO delta of
% any kind, so even the one case where a Log row genuinely leaves is invisible
% to the arm.
u5b_retention_prune_emits_no_departure :-
    log_arm_program(count(1), Program),
    expect_run(Program, [],
      [ [ +event_log(api, v1) ], [ +event_log(api, v2) ] ],
      [ final(changed_event/3, []),
        final(event_log/2, [ event_log(api, v2) ]),
        deltas(event_log/2, [ [ +event_log(api, v1) ],
                              [ +event_log(api, v2) ] ]) ]).

% ═══ U6 : finalize inside a match arm body ══════════════════════════════════
% expand_match_program conjoins the source atom in front of the arm guards, so
% the block form becomes `head <+ current_value(Key,New), finalize(...)`. The
% source atom degrades to the join and the finalize stays the trigger.

u6_match_block_composes_into_the_update_arm :-
    match_update_arm_program(Program),
    expect_run(Program, [],
      [ [ +poll_value(cli, v1) ], [ +poll_value(cli, v2) ] ],
      [ final(changed_value/3, [ changed_value(cli, v1, v2) ]),
        deltas(changed_value/3,
               [ [], [], [ +changed_value(cli, v1, v2) ], [] ]),
        ticks(4) ]).

% Byte-identical tick logs, block form vs hand-desugared form, same schedule.
u6_match_block_tick_log_matches_the_hand_written_rule :-
    update_arm_program(HandWritten),
    match_update_arm_program(Block),
    Schedule = [ [ +poll_value(cli, v1) ], [ +poll_value(cli, v2) ] ],
    run_program(HandWritten, [], Schedule, HandFinal, HandDeltas),
    run_program(Block, [], Schedule, BlockFinal, BlockDeltas),
    (   HandDeltas == BlockDeltas, HandFinal == BlockFinal
    ->  true
    ;   format("    MISMATCH block vs hand-written~n      hand  ~q~n      block ~q~n",
               [HandDeltas, BlockDeltas]),
        fail
    ).

% U6 rider: the SAME arm written with a level arrow is refused after expansion,
% so the block sugar cannot smuggle finalize into a level rule.
u6b_finalize_in_a_level_match_arm_is_refused :-
    expect_throw(
      prog([ keyed(current_value/2, [1]) ],
           [ match(current_value(Key, NewValue),
                   (changed_value(Key, OldValue, NewValue) <-
                        finalize(current_value(Key, OldValue)))) ]),
      [], [],
      finalize_in_level_rule(current_value/2)).

% ═══ SUGAR-SCOPE rider ══════════════════════════════════════════════════════
% What does an arm see: the trigger atom's bindings only, or sibling arms too?

% S1: an arm reading a trigger column works (the landed shape).
s1_arm_reads_a_trigger_column :-
    expect_run(
      prog([ kind(resp/2, log), keep(resp/2, all) ],
           [ match(resp(Endpoint, Status),
                   ( (ok_endpoint(Endpoint) <- Status == 200)
                   ; (bad_endpoint(Endpoint) <- Status >= 400) )) ]),
      [],
      [ [ +resp(api, 200), +resp(worker, 503) ] ],
      [ final(ok_endpoint/1, [ ok_endpoint(api) ]),
        final(bad_endpoint/1, [ bad_endpoint(worker) ]) ]).

% S2: an arm naming a variable only its SIBLING binds sees an unbound
% variable, not the sibling's value. In a head or expression position that is
% a loud throw at head evaluation.
s2_sibling_binding_in_a_head_column_throws :-
    expect_throw(
      prog([ kind(resp/2, log), keep(resp/2, all) ],
           [ match(resp(Endpoint, Status),
                   ( (ok_endpoint(Endpoint, Doubled) <-
                          Status == 200, Doubled := Status * 2)
                   ; (echo_endpoint(Endpoint, Doubled) <- Status == 304) )) ]),
      [],
      [ [ +resp(api, 304) ] ],
      unbound_in_expression).

% S3: the same variable in a BODY rel-atom column is silently a fresh
% wildcard. A shared-scope reading would bind Tag to hot and yield ONE row;
% the engine yields TWO. No error, no warning: the join just widens.
s3_sibling_binding_in_a_body_atom_silently_widens :-
    expect_run(
      prog([ kind(resp/2, log), keep(resp/2, all) ],
           [ match(resp(Endpoint, Status),
                   ( (tagged(Endpoint, Tag) <-
                          Status == 200, label(Endpoint, Tag))
                   ; (echoed(Endpoint, Other) <-
                          Status == 304, label(Other, Tag)) )) ]),
      [ label(api, hot), label(worker, cold) ],
      [ [ +resp(api, 200), +resp(api, 304) ] ],
      [ final(tagged/2, [ tagged(api, hot) ]),
        final(echoed/2, [ echoed(api, api), echoed(api, worker) ]) ]).

% ═══ checks ═════════════════════════════════════════════════════════════════

check(u0_literal_level_rule_spelling_is_refused,
      u0_literal_level_rule_spelling_is_refused).
check(u0_edge_rule_spelling_loads,
      u0_edge_rule_spelling_loads).
check(u1_keyed_replace_yields_one_pair,
      u1_keyed_replace_yields_one_pair).
check(u1_arm_fires_the_tick_after_the_replace,
      u1_arm_fires_the_tick_after_the_replace).
check(u1c_the_arm_works_over_a_derived_level_view,
      u1c_the_arm_works_over_a_derived_level_view).
check(u2_plain_insert_leaves_the_arm_silent,
      u2_plain_insert_leaves_the_arm_silent).
check(u3_plain_delete_leaves_the_arm_silent,
      u3_plain_delete_leaves_the_arm_silent).
check(u3b_delete_is_separable_by_negating_the_current_side,
      u3b_delete_is_separable_by_negating_the_current_side).
check(u3b_replace_does_not_trip_the_delete_arm,
      u3b_replace_does_not_trip_the_delete_arm).
check(u4_same_tick_double_replace_yields_the_endpoint_pair,
      u4_same_tick_double_replace_yields_the_endpoint_pair).
check(u4b_same_tick_replaces_from_empty_yield_nothing,
      u4b_same_tick_replaces_from_empty_yield_nothing).
check(u5_log_rel_arm_is_silently_dead,
      u5_log_rel_arm_is_silently_dead).
check(u5b_retention_prune_emits_no_departure,
      u5b_retention_prune_emits_no_departure).
check(u6_match_block_composes_into_the_update_arm,
      u6_match_block_composes_into_the_update_arm).
check(u6_match_block_tick_log_matches_the_hand_written_rule,
      u6_match_block_tick_log_matches_the_hand_written_rule).
check(u6b_finalize_in_a_level_match_arm_is_refused,
      u6b_finalize_in_a_level_match_arm_is_refused).
check(s1_arm_reads_a_trigger_column,
      s1_arm_reads_a_trigger_column).
check(s2_sibling_binding_in_a_head_column_throws,
      s2_sibling_binding_in_a_head_column_throws).
check(s3_sibling_binding_in_a_body_atom_silently_widens,
      s3_sibling_binding_in_a_body_atom_silently_widens).

go :- run(check).
