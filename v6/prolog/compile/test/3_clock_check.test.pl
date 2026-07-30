% 3_clock_check.test.pl : deterministic phase-5 checker receipts.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module(library(plunit)).
:- use_module('../3_clock_check',
              [ clock_dependencies/2, clock_dependency/8, inferred_clock/4,
                clock_fact/5, clock_scc/3, clock_violation/2,
                check_clock_program/1 ]).
:- use_module('../registry', [clock_role/4]).
:- use_module('../compile', [program_plan/2]).
:- use_module('../../conformance/engine', [check_program/1]).
:- use_module('3_clock_history',
              [ historical_bug_class/5, historical_bug_program/2 ]).

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
          [changed/1-2, demand/1-0, fetch/1-0, folded/1-1]).

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

test(a2_counterexample_is_accepted_and_has_two_trigger_dependencies) :-
    historical_bug_program(a2, Program),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    include(is_trigger_dependency, Dependencies, Triggers),
    length(Triggers, 2).

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

is_trigger_dependency(dependency(_, _, _, _, _, _, _, trigger)).

:- end_tests(clock_checker).
