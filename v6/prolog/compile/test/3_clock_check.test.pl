% 3_clock_check.test.pl : deterministic phase-5 checker receipts.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module(library(plunit)).
:- use_module('../3_clock_check',
              [ clock_dependencies/2, clock_dependency/8, inferred_clock/4,
                clock_fact/5, clock_scc/3, clock_violation/2,
                clock_boundary/2, check_clock_program/1 ]).
:- use_module('../registry', [clock_role/4]).
:- use_module('../compile', [program_plan/2]).
:- use_module('../../conformance/engine', [check_program/1, run_program/5]).
:- use_module('3_clock_history',
              [ historical_bug_class/5, historical_bug_program/2,
                historical_clock_receipt/3 ]).

:- begin_tests(clock_checker).

test(registry_clock_roles_are_complete) :-
    findall(Role-Read-Sign-Grade, clock_role(Role, Read, Sign, Grade), Rows),
    Rows == [ level_read-b-positive-0,
              level_absence-b-negative-0,
              edge_trigger-z-positive-source_delay,
              edge_departure-z-negative-1,
              edge_sample-b-state-0,
              edge_pre-b-previous- -1,
              edge_absence-b-negative-0 ].

test(edge_chain_labels_and_offsets) :-
    Program =
      prog([ kind(source_ev/1, log), keep(source_ev/1, all),
             kind(stage_one/1, log), keep(stage_one/1, all),
             kind(stage_two/1, log), keep(stage_two/1, all) ],
           [ (stage_one(Item) <+ source_ev(Item)),
             (stage_two(Item) <+ stage_one(Item)) ]),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, edge, stage_one/1), source_ev/1, stage_one/1,
                   z, n, positive, 0, trigger),
        dependency(rule(2, edge, stage_two/1), stage_one/1, stage_two/1,
                   z, n, positive, 1, trigger) ],
    setof(Ref-Offset, inferred_clock(Program, Ref, source_ev/1, Offset),
          [source_ev/1-0, stage_one/1-0, stage_two/1-1]).

test(single_dependency_and_clock_fact_are_queryable) :-
    Program =
      prog([ kind(source/1, log), keep(source/1, all),
             kind(sink/1, log), keep(sink/1, all) ],
           [ (sink(X) <+ source(X)) ]),
    once(( clock_dependency(Program, rule(1, edge, sink/1), source/1, sink/1,
                             z, n, positive, 0),
           clock_fact(Program, sink/1, n, clock(source/1, 0), acyclic) )).

test(pipe_offsets_match_observed_ticks) :-
    Program =
      prog([ kind(fetch/1, log), keep(fetch/1, all),
             kind(demand/1, log), keep(demand/1, all),
             kind(folded/1, log), keep(folded/1, all),
             kind(changed/1, log), keep(changed/1, all) ],
           [ (demand(X) <+ fetch(X)),
             (folded(X) <+ demand(X)),
             (changed(X) <+ folded(X)) ]),
    setof(Ref-Offset, inferred_clock(Program, Ref, fetch/1, Offset),
          Inferred),
    Expected = [changed/1-2, demand/1-0, fetch/1-0, folded/1-1],
    Inferred == Expected,
    once(run_program(Program, [], [[+fetch(one)]], _Final, DeltaTicks)),
    DeltaTicks ==
      [ [+fetch(one), +demand(one)],
        [+folded(one)],
        [+changed(one)],
        [] ],
    observed_offsets(DeltaTicks, fetch/1, Observed),
    Observed == Expected,
    !.

test(equal_grade_diamond_passes) :-
    equal_diamond(Program),
    \+ clock_violation(Program, clock_path_conflict(_, _, _, _)),
    check_clock_program(Program).

test(unequal_grade_diamond_refuses,
     [throws(unsupported_construct(
         clock_path_conflict(source/1, joined/1, 0, 1)))]) :-
    unequal_diamond(Program),
    check_clock_program(Program).

test(positive_b_scc_is_constructive) :-
    Program = prog([], [ (path(X, Y) <- path(X, Z), edge(Z, Y)) ]),
    clock_scc(Program, [path/2], constructive_b).

test(positive_delay_scc_is_productive) :-
    Program =
      prog([kind(loop/1, log), keep(loop/1, all)],
           [(loop(X) <+ loop(X))]),
    clock_scc(Program, [loop/1], productive_delayed).

test(zero_grade_negative_scc_refuses) :-
    Program =
      prog([], [ (left(X) <- not(right(X))),
                 (right(X) <- left(X)) ]),
    once(clock_violation(
      Program,
      unconstructive_clock_cycle([left/1, right/1], nonpositive_cycle(0)))).

test(compiler_plan_runs_clock_checker,
     [throws(unsupported_construct(
         clock_path_conflict(source/1, joined/1, 0, 1)))]) :-
    unequal_diamond(Program),
    program_plan(fixture(clock_conflict, Program, [], [], [])-[], _).

test(oracle_runs_clock_checker,
     [throws(clock_path_conflict(source/1, joined/1, 0, 1))]) :-
    unequal_diamond(Program),
    check_program(Program).

test(five_cross_plane_classes_are_derived) :-
    Programs =
      [ prog([], [(out(X) <- finalize(input(X)))])
          -cross_plane(finalize_in_level_rule(input/1)),
        prog([], [(out(X) <- latest(input(X)))])
          -cross_plane(latest_in_level_rule(input/1)),
        prog([], [(out(X) <- pre(input(X)))])
          -cross_plane(pre_in_level_rule(input/1)),
        prog([kind(out/1, log), keep(out/1, all)],
             [(out(X) <- input(X))])
          -cross_plane(log_on_level_headed_rel(out/1)),
        prog([keyed(out/1, [1])], [(out(X) <- input(X))])
          -cross_plane(keyed_level_head(out/1))
      ],
    forall(member(Program-Expected, Programs),
           once(clock_violation(Program, Expected))).

test(historical_table_has_required_ids_and_programs) :-
    findall(Id, historical_bug_class(Id, _, _, _, _), Ids),
    Ids == [a2, a4, a5, a6, a7, a8, a9, a11],
    forall(member(Id, Ids), historical_bug_program(Id, _)).

test(historical_clock_receipt_status_partition_is_exact) :-
    findall(Id-Status,
            historical_clock_receipt(Id, Status, _),
            Rows),
    Rows ==
      [ a2-not_provable,
        a4-not_provable,
        a5-not_provable,
        a6-runtime_clock_crosscheck,
        a7-not_provable,
        a8-not_provable,
        a9-not_provable,
        a11-not_provable
      ].

test(historical_clock_catch_partition_is_exact) :-
    findall(Id-ClockCatch,
            historical_bug_class(Id, _, _, ClockCatch, _),
            Rows),
    Rows ==
      [ a2-not_provable,
        a4-not_provable,
        a5-not_provable,
        a6-runtime_clock_crosscheck,
        a7-not_provable,
        a8-not_provable,
        a9-not_provable,
        a11-not_provable
      ].

test(a2_replay_states_named_not_provable_boundary) :-
    historical_bug_program(a2, Program),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, edge, answer/2), left/1, answer/2,
                   z, n, positive, 0, trigger),
        dependency(rule(1, edge, answer/2), right/1, answer/2,
                   z, n, positive, 0, trigger) ],
    historical_clock_receipt(a2, not_provable, Evidence),
    once(clock_boundary(Program, not_provable(Evidence))).

test(single_trigger_has_no_multi_trigger_batch_boundary) :-
    Program =
      prog([ kind(source/1, log), keep(source/1, all),
             kind(sink/1, log), keep(sink/1, all) ],
           [ (sink(X) <+ source(X)) ]),
    \+ clock_boundary(
         Program,
         not_provable(multi_trigger_batch_invariance(_, _))).

test(a4_replay_has_no_rule_clock_claim) :-
    historical_bug_program(a4, Program),
    check_clock_program(Program),
    clock_dependencies(Program, []),
    historical_clock_receipt(
      a4, not_provable,
      keyed_boundary_replacement_requires_runtime_deltas).

test(a5_replay_keeps_name_arity_refs_distinct) :-
    historical_bug_program(a5, Program),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, level, same/1), input/1, same/1,
                   b, b, positive, 0, level_read),
        dependency(rule(2, level, same/2), input/1, same/2,
                   b, b, positive, 0, level_read) ],
    setof(Ref-Offset, inferred_clock(Program, Ref, input/1, Offset),
          [input/1-0, same/1-0, same/2-0]),
    historical_clock_receipt(
      a5, not_provable,
      emitted_identifier_uniqueness_requires_target_validation).

test(a6_grade_zero_offsets_are_runtime_crosschecked) :-
    historical_bug_program(a6, Program),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, level, visible/1), source/1, visible/1,
                   b, b, positive, 0, level_read),
        dependency(rule(2, edge, seen/1), source/1, seen/1,
                   z, n, positive, 0, trigger),
        dependency(rule(2, edge, seen/1), visible/1, seen/1,
                   b, n, state, 0, edge_sample) ],
    setof(Ref-Offset, inferred_clock(Program, Ref, source/1, Offset),
          Inferred),
    once(run_program(Program, [], [[+source(one)]], _Final, DeltaTicks)),
    observed_offsets(DeltaTicks, source/1, Observed),
    historical_clock_receipt(
      a6, runtime_clock_crosscheck,
      grade_zero_offsets_match_observed_ticks(source/1, Expected)),
    Inferred == Expected,
    Observed == Expected,
    !.

test(a7_replay_has_no_rule_clock_claim) :-
    historical_bug_program(a7, Program),
    check_clock_program(Program),
    clock_dependencies(Program, []),
    historical_clock_receipt(
      a7, not_provable,
      occurrence_history_versus_membership_requires_relation_intent).

test(a8_replay_labels_retained_relation_without_inventing_partition_clock) :-
    historical_bug_program(a8, Program),
    check_clock_program(Program),
    clock_dependencies(Program, []),
    historical_clock_receipt(
      a8, not_provable,
      retention_partition_requires_policy_key).

test(a9_replay_has_no_rule_clock_claim) :-
    historical_bug_program(a9, Program),
    check_clock_program(Program),
    clock_dependencies(Program, []),
    historical_clock_receipt(
      a9, not_provable,
      operational_transition_rows_require_boundary_observation).

test(a11_replay_labels_grade_zero_aggregate_dependency) :-
    historical_bug_program(a11, Program),
    check_clock_program(Program),
    clock_dependencies(
      Program,
      [ dependency(rule(1, level, total/1), item/1, total/1,
                   b, b, positive, 0, level_read) ]),
    setof(Ref-Offset, inferred_clock(Program, Ref, item/1, Offset),
          [item/1-0, total/1-0]),
    historical_clock_receipt(
      a11, not_provable,
      empty_group_policy_requires_aggregate_semantics).

equal_diamond(
  prog([ kind(source/1, log), keep(source/1, all),
         kind(left/1, log), keep(left/1, all),
         kind(right/1, log), keep(right/1, all),
         kind(joined/1, log), keep(joined/1, all) ],
       [ (left(X) <+ source(X)),
         (right(X) <+ source(X)),
         (joined(X) <+ left(X)),
         (joined(X) <+ right(X)) ])).

unequal_diamond(
  prog([ kind(source/1, log), keep(source/1, all),
         kind(slow/1, log), keep(slow/1, all),
         kind(joined/1, log), keep(joined/1, all) ],
       [ (joined(X) <+ source(X)),
         (slow(X) <+ source(X)),
         (joined(X) <+ slow(X)) ])).

observed_offsets(DeltaTicks, OriginRef, Offsets) :-
    first_add_tick(DeltaTicks, OriginRef, OriginTick),
    findall(Ref-Offset,
            ( first_add_tick(DeltaTicks, Ref, Tick),
              Offset is Tick - OriginTick
            ),
            Offsets0),
    sort(Offsets0, Offsets).

first_add_tick(DeltaTicks, Ref, Tick) :-
    nth1(Tick, DeltaTicks, Deltas),
    member(+Row, Deltas),
    functor(Row, Name, Arity),
    Ref = Name/Arity,
    \+ ( nth1(EarlierTick, DeltaTicks, EarlierDeltas),
         EarlierTick < Tick,
         member(+EarlierRow, EarlierDeltas),
         functor(EarlierRow, Name, Arity) ).

:- end_tests(clock_checker).
