% 3_clock_check.test.pl : deterministic phase-5 checker receipts.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% The path walk is pinned off on the compile path (rulings.pl
% clock_path_check_pinned_off); this battery tests it, so it turns it on.
:- create_prolog_flag(dl6_clock_path_walk, false, [type(boolean), keep(true)]).
:- set_prolog_flag(dl6_clock_path_walk, true).
:- use_module(library(plunit)).
:- use_module('../../3_clock_check',
              [ clock_dependencies/2, clock_dependency/8, inferred_clock/4,
                clock_fact/5, clock_scc/3, clock_violation/2,
                clock_boundary/2, check_clock_program/1 ]).
:- use_module('../registry', [clock_role/4]).
:- use_module('../../compile', [program_plan/2]).
:- use_module('../../conformance/engine', [check_program/1, run_program/5]).
:- use_module('3_clock_history',
              [ historical_bug_class/5, historical_bug_program/2,
                historical_bug_fixed_twin/2, historical_clock_receipt/3 ]).

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

% ── the count rail ──────────────────────────────────────────────────────────
%
% Standing law: a formerly-quadratic path gets an operation-count assertion,
% never end-state equality alone. check_clock_program/1 over a dependent rule
% chain measured rules^4.864 in wall time while an all-pairs simple-path
% search sat under clock_scc/3, and cost the plan phase 255 s on
% flagship-flow.dl6 at 42 graph nodes.
%
% Inferences, not wall time, so the rail does not flake on a loaded machine.
% Two pins, because the two halves of the fix fail differently:
%
%   exponent  catches an asymptotic regression at any absolute scale
%   ceiling   catches a constant-factor regression that keeps the exponent
%
% MEASURED, all three on this machine, inference counts for chains of 20 and
% 58 rules. Exponent is log(count58/count20) / log(58/20).
%
%   | variant                                   | 20 rules | 58 rules | exp  |
%   |-------------------------------------------|---------:|---------:|-----:|
%   | shipped                                   |   16,670 |   54,331 | 1.11 |
%   | sabotage: delayed-node set recomputed per |          |          |      |
%   |   clock path (the hoist undone)           |   69,928 |  524,445 | 1.89 |
%   | sabotage: simple-path reachability back   |          |          |      |
%   |   under clock_components/3                |1,132,057 |66,564,729| 3.83 |
%
% Both sabotages go RED on both pins. The thresholds sit at 1.5 and 150,000,
% leaving the shipped numbers 35% and 2.8x of headroom.
test(clock_check_cost_stays_near_linear_in_chain_length) :-
    SmallLength = 20,
    LargeLength = 58,
    chain_program(SmallLength, SmallProgram),
    chain_program(LargeLength, LargeProgram),
    clock_check_inferences(SmallProgram, SmallCount),
    clock_check_inferences(LargeProgram, LargeCount),
    Exponent is log(LargeCount / SmallCount) / log(LargeLength / SmallLength),
    ( Exponent =< 1.5
    -> true
    ;  format(user_error,
              "clock check scales as rules^~3f: ~w then ~w inferences~n",
              [Exponent, SmallCount, LargeCount]),
       fail
    ),
    ( LargeCount =< 150000
    -> true
    ;  format(user_error,
              "clock check cost ~w inferences at ~w rules, ceiling 150000~n",
              [LargeCount, LargeLength]),
       fail
    ).

% Mirrors the generator in scripts/0_profile_compile_curve.sh: one dependent
% chain of level rules, which is the shape that measured the 4.864 exponent.
chain_program(RuleCount, prog([], Rules)) :-
    findall((Head <- Body),
            ( between(1, RuleCount, Index),
              Previous is Index - 1,
              atom_concat(chain_rel_, Index, HeadName),
              atom_concat(chain_rel_, Previous, BodyName),
              Head =.. [HeadName, Value],
              Body =.. [BodyName, Value] ),
            Rules).

clock_check_inferences(Program, Count) :-
    statistics(inferences, Before),
    ( catch(check_clock_program(Program), _, true) -> true ; true ),
    statistics(inferences, After),
    Count is After - Before.

% ── the second count rail: ROUTES, not length ───────────────────────────────
%
% The chain rail above stayed green through the whole path-enumeration era,
% because a chain has exactly one route and simple-path enumeration is linear
% on it. The cost that actually bit is a function of the number of PARALLEL
% mid-chain routes, and nothing measured that until a filesystem-fold program walked
% off the cliff in production (ARCH clock_check_path_blowup): its compile went
% 30 s to 9 m 40 s, then died with `Stack limit (1.0Gb) exceeded` inside
% clock_violation/2 at the served compiler's default stack.
%
% k diamonds in series is 2^k end-to-end routes over 4k rules. Every route
% weighs the same, so there is no violation to find and the checker must walk
% the whole thing. MEASURED on this machine:
%
%   | k  | shipped (offset propagation) | sabotage (simple-path enumeration) |
%   |----|-----------------------------:|-----------------------------------:|
%   |  4 |                       11,881 |                             14,876 |
%   | 12 |                       32,614 |                          2,888,482 |
%   | 16 |                       44,555 |                         60,464,484 |
%   | 20 |                       57,186 |                      1,201,719,858 |
%
% Exponent is log(count12/count4) / log(12/4): 0.92 shipped, 4.81 sabotaged.
% SABOTAGE RECEIPT, run: restore recurrence_free_clock/6 to clock_path/7 over
% the dependency list and this test goes RED on both pins, at diamonds^4.892
% against a 1.5 threshold and 2,888,482 against a 150,000 ceiling. The chain
% rail beside it stays GREEN through that same sabotage, which is the whole
% reason both exist.
test(clock_check_cost_stays_near_linear_in_parallel_routes) :-
    SmallRoutes = 4,
    LargeRoutes = 12,
    diamond_program(SmallRoutes, SmallProgram),
    diamond_program(LargeRoutes, LargeProgram),
    clock_check_inferences(SmallProgram, SmallCount),
    clock_check_inferences(LargeProgram, LargeCount),
    Exponent is log(LargeCount / SmallCount) / log(LargeRoutes / SmallRoutes),
    (   Exponent =< 1.5
    ->  true
    ;   format(user_error,
               "clock check scales as diamonds^~3f: ~w then ~w inferences~n",
               [Exponent, SmallCount, LargeCount]),
        fail
    ),
    (   LargeCount =< 150000
    ->  true
    ;   format(user_error,
               "clock check cost ~w inferences at ~w diamonds, ceiling 150000~n",
               [LargeCount, LargeRoutes]),
        fail
    ).

% hop0 -> {left_i, right_i} -> hop_i, i in 1..DiamondCount. Level rules only,
% so every edge is a grade-zero level_read and every one of the 2^k routes
% carries offset 0; the program is ACCEPTED, which is what makes this a cost
% measurement rather than an early-exit measurement.
diamond_program(DiamondCount, prog([], Rules)) :-
    findall(Rule,
            ( between(1, DiamondCount, Index),
              diamond_rules(Index, IndexRules),
              member(Rule, IndexRules) ),
            Rules).

diamond_rules(Index, [ (Left <- Previous), (Right <- Previous),
                       (Next <- Left),     (Next <- Right) ]) :-
    PreviousIndex is Index - 1,
    atom_concat(hop_, PreviousIndex, PreviousName),
    atom_concat(hop_, Index, NextName),
    atom_concat(left_, Index, LeftName),
    atom_concat(right_, Index, RightName),
    Previous =.. [PreviousName, Item],
    Next =.. [NextName, Item],
    Left =.. [LeftName, Item],
    Right =.. [RightName, Item].

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
    Ids == [a2, a4, a5, a6, a7, a8, a9, a11, a12, c2, d1],
    forall(member(Id, Ids), historical_bug_program(Id, _)),
    % A fixed twin is the discriminating half. Every class whose catch is a
    % LABEL must carry one, or the label proves nothing.
    forall(( historical_bug_class(Id, _, _, Catch, _),
             memberchk(Catch, [labelled_ring_discriminates, labelled_boundary])
           ),
           historical_bug_fixed_twin(Id, _)).

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
        a11-not_provable,
        a12-labelled_ring_discriminates,
        c2-runtime_clock_crosscheck,
        d1-labelled_boundary
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
        a11-not_provable,
        a12-labelled_ring_discriminates,
        c2-runtime_clock_crosscheck,
        d1-labelled_boundary
      ],
    % The two tables must agree class-by-class, or the ledger and the
    % receipts drift apart silently.
    forall(historical_bug_class(Id, _, _, Catch, _),
           historical_clock_receipt(Id, Catch, _)).

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

% a4, strengthened. The recorded a4 program is a bare keyed declaration with
% no rules, so its projection is empty BY CONSTRUCTION and the test above
% cannot tell a working checker from a broken one. This replays the same
% class with a consumer attached, which is what a world-fed keyed rel looks
% like in a real program, and states the boundary precisely:
%
%   the checker CAN say mode/2 is ring B, so at most one row per key survives
%   a boundary (TICK-MODEL section 1);
%   the checker CANNOT say which SQL primary key the emitter chose, and
%   PK-over-all-columns versus PK-over-key-columns was the entire A4 bug.
%
% Ring B is the specification the emitter violated. It is not a proof that
% the emitter implements it, which is why a4 stays not_provable.
test(a4_replay_with_a_consumer_labels_ring_b_and_stops_there) :-
    Program =
      prog([ keyed(mode/2, [1]),
             kind(mode_log/2, log), keep(mode_log/2, all) ],
           [ (mode_log(Key, Value) <+ mode(Key, Value)) ]),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, edge, mode_log/2), mode/2, mode_log/2,
                   z, n, positive, 0, trigger) ],
    % proof facts: the keyed source is ring B, its reader is ring N, both on
    % the same clock. Neither carries a storage key.
    once(clock_fact(Program, mode/2, b, clock(mode/2, 0), acyclic)),
    once(clock_fact(Program, mode_log/2, n, clock(mode/2, 0), acyclic)),
    historical_clock_receipt(
      a4, not_provable, keyed_boundary_replacement_requires_runtime_deltas).

% ── the 2026-07-31 legs round ──────────────────────────────────────────────
%
% Three classes replayed for the first time. Every claim below is a MEASURED
% number: the runtime legs run the recorded program through the same oracle
% the conformance corpus uses, and the inferred side is read out of the
% checker, never written down by hand.

% a12. Same rows, same schedule, one word of difference. The unmarked twin
% banks two demands and answers BOTH when config finally lands (the backlog
% replay); the marked twin samples config and answers nothing, because a
% sample is not a firing. The checker separates them BEFORE either runs:
% ring Z trigger versus ring B sample, and the multi-trigger boundary is
% present on one and absent on the other.
%
% RED-BEFORE: the discriminating leg is `\+ clock_boundary(Fixed, ...)`
% together with the ring in the dependency. Delete the `latest(...)` wrapper
% from the fixed twin and this test goes red on the absence check; make
% edge_sample carry ring z in registry.pl and it goes red on the projection.
test(a12_replay_ring_separates_backlog_replay_from_sampling) :-
    historical_bug_program(a12, Unmarked),
    historical_bug_fixed_twin(a12, Fixed),
    check_clock_program(Unmarked),
    check_clock_program(Fixed),
    clock_dependencies(Unmarked, UnmarkedDependencies),
    UnmarkedDependencies ==
      [ dependency(rule(1, edge, answer/2), config/1, answer/2,
                   z, n, positive, 0, trigger),
        dependency(rule(1, edge, answer/2), demand/1, answer/2,
                   z, n, positive, 0, trigger) ],
    clock_dependencies(Fixed, FixedDependencies),
    FixedDependencies ==
      [ dependency(rule(1, edge, answer/2), config/1, answer/2,
                   b, n, state, 0, edge_sample),
        dependency(rule(1, edge, answer/2), demand/1, answer/2,
                   z, n, positive, 0, trigger) ],
    once(clock_boundary(
           Unmarked,
           not_provable(multi_trigger_batch_invariance(
                          rule(1, edge, answer/2), [config/1, demand/1])))),
    \+ clock_boundary(
         Fixed, not_provable(multi_trigger_batch_invariance(_, _))),
    historical_clock_receipt(
      a12, labelled_ring_discriminates,
      trigger_ring_z_versus_sample_ring_b(answer/2, config/1)),
    % proof fact: the unmarked head carries a clock under BOTH origins, so a
    % config arrival is a firing of answer/2; the fixed head carries one.
    findall(Origin,
            clock_fact(Unmarked, answer/2, n, clock(Origin, 0), acyclic),
            UnmarkedOrigins),
    msort(UnmarkedOrigins, [config/1, demand/1]),
    findall(Origin,
            clock_fact(Fixed, answer/2, n, clock(Origin, 0), acyclic),
            FixedOrigins),
    FixedOrigins == [demand/1],
    a12_backlog_runtime_leg(Unmarked, Fixed).

% The runtime half, kept beside the static half so drift between them shows
% up here rather than in a distant fixture.
a12_backlog_runtime_leg(Unmarked, Fixed) :-
    Schedule = [[+demand(a)], [+demand(b)], [+config(one)]],
    once(run_program(Unmarked, [], Schedule, _, UnmarkedTicks)),
    UnmarkedTicks ==
      [ [+demand(a)],
        [+demand(b)],
        [+config(one), +answer(a, one), +answer(b, one)],
        [] ],
    once(run_program(Fixed, [], Schedule, _, FixedTicks)),
    FixedTicks == [ [+demand(a)], [+demand(b)], [+config(one)] ].

% c2. The same-tick transition "loss" is the B plane's definition, not a
% dropped row: current/2 is keyed, so it is ring B and holds no per-tick
% multiplicity to lose. What the checker owns is the PLACEMENT, and it is the
% update-arm verdict's +1 exactly: the departure at grade 1 is the only causal
% path, so changed/3 sits one tick after the replace it reports.
%
% RED-BEFORE: set edge_departure's grade to 0 in registry.pl and the inferred
% clock drops to 0 while the observed tick stays 1, which is the crosscheck
% this test exists to make.
test(c2_replay_pairs_departure_grade_with_observed_tick) :-
    historical_bug_program(c2, Program),
    check_clock_program(Program),
    clock_dependencies(Program, Dependencies),
    Dependencies ==
      [ dependency(rule(1, edge, changed/3), current/2, changed/3,
                   b, n, state, 0, edge_sample),
        dependency(rule(1, edge, changed/3), current/2, changed/3,
                   z, n, negative, 1, edge_departure) ],
    % proof facts: the keyed source is ring B at offset 0, the report is ring
    % N one tick later.
    once(clock_fact(Program, current/2, b, clock(current/2, 0), acyclic)),
    once(clock_fact(Program, changed/3, n, InferredClock, acyclic)),
    historical_clock_receipt(
      c2, runtime_clock_crosscheck,
      departure_grade_matches_observed_tick(changed/3, InferredClock)),
    InferredClock = clock(current/2, 1),
    % Three values for one key inside tick 2. B keeps the endpoint pair, and
    % the report lands at tick 3 = the replace tick plus the inferred grade.
    once(run_program(Program, [],
                     [ [+current(key, v1)],
                       [+current(key, v2), +current(key, v3)],
                       [] ],
                     Final, DeltaTicks)),
    DeltaTicks ==
      [ [+current(key, v1)],
        [-current(key, v1), +current(key, v3)],
        [+changed(key, v1, v3)],
        [] ],
    Final == [current(key, v3), changed(key, v1, v3)],
    nth1(ReplaceTick, DeltaTicks, ReplaceDeltas),
    memberchk(-current(key, v1), ReplaceDeltas),
    nth1(ReportTick, DeltaTicks, ReportDeltas),
    memberchk(+changed(key, v1, v3), ReportDeltas),
    ObservedGrade is ReportTick - ReplaceTick,
    ObservedGrade =:= 1,
    !.

% d1. not(Atom) in an arm reads a plane, and which plane is the whole story.
% Over the arm's own edge-headed head, two rows in ONE batch give two
% different answers depending on their order. Over a level-headed rel the
% plane is frozen before edges run and both orders agree. The checker names
% the first and stays silent on the second.
%
% RED-BEFORE: drop the `memberchk(Ref, EdgeHeaded)` guard from the new
% clock_boundary clause and the level-headed twin gets labelled too, which
% the absence leg below catches.
test(d1_replay_labels_arm_absence_only_over_the_edge_headed_plane) :-
    historical_bug_program(d1, EdgeHeaded),
    historical_bug_fixed_twin(d1, LevelHeaded),
    check_clock_program(EdgeHeaded),
    check_clock_program(LevelHeaded),
    historical_clock_receipt(d1, labelled_boundary, Evidence),
    once(clock_boundary(EdgeHeaded, not_provable(Evidence))),
    Evidence = arm_absence_batch_invariance(rule(1, edge, out/1), out/1),
    \+ clock_boundary(
         LevelHeaded, not_provable(arm_absence_batch_invariance(_, _))),
    % proof fact: both twins place out/1 on req/1's clock at offset 0. The
    % label is about batch order, not about tick placement.
    once(clock_fact(EdgeHeaded, out/1, n, clock(req/1, 0), acyclic)),
    once(clock_fact(LevelHeaded, out/1, n, clock(req/1, 0), acyclic)),
    d1_order_sensitivity_runtime_leg(EdgeHeaded, LevelHeaded).

% The measurement the label stands on. Same rows, one batch, two orders.
d1_order_sensitivity_runtime_leg(EdgeHeaded, LevelHeaded) :-
    once(run_program(EdgeHeaded, [], [[+req(a), +req(b)]], EdgeForward, _)),
    once(run_program(EdgeHeaded, [], [[+req(b), +req(a)]], EdgeReverse, _)),
    EdgeForward == [out(a), req(a), req(b)],
    EdgeReverse == [out(b), req(a), req(b)],
    once(run_program(LevelHeaded, [], [[+req(a), +req(b)]], LevelForward, _)),
    once(run_program(LevelHeaded, [], [[+req(b), +req(a)]], LevelReverse, _)),
    LevelForward == LevelReverse.

% The live corpus carries this shape. json_typed_capture_folds_into_a_keyed_
% int_total's first-write arm reads not(total(Repo, _)) over its own keyed
% edge-headed head; its schedule never puts two rows of one key in one batch,
% so the sensitivity is real and unexercised. This pins that the checker
% LABELS the ruled program rather than refusing it.
test(live_keyed_first_write_arm_is_labelled_not_refused) :-
    Program =
      prog([ keyed(total/2, [1]), kind(star/2, log), keep(star/2, all) ],
           [ (total(Repo, Stars) <+ star(Repo, Stars), not(total(Repo, _))) ]),
    check_clock_program(Program),
    once(clock_boundary(
           Program,
           not_provable(arm_absence_batch_invariance(
                          rule(1, edge, total/2), total/2)))).

% THE TWO-ARM VERSION OF THAT SHAPE IS THE CLOSEST THING TO `one` THE SURFACE
% HAS, and this pins why it is not `one`. Two arms, two triggers, one batch,
% each arm reading not(head) over the shared edge-headed head: exactly one row
% lands and nothing in the program says which. The checker labels both arms and
% refuses neither, so the winner is whoever gets there first -- here the ARRIVAL
% order, reversed below and reversing the answer with it. The emitted module
% referees the same race by SOURCE ARM order instead, so the two doors disagree
% whenever arm order and arrival order disagree; that half is measured in
% COMPOSE.md and is why conformance one_vs_any.pl grades only the order on
% which the two doors agree.
test(two_arm_negation_guard_is_a_race_not_one) :-
    Program =
      prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, all) ],
           [ (dispatch_first(AckId, acked) <+
                (dispatch_ack(AckId), not(dispatch_first(AckId, _AckTag)))),
             (dispatch_first(SealId, sealed) <+
                (dispatch_seal(SealId), not(dispatch_first(SealId, _SealTag)))) ]),
    check_clock_program(Program),
    findall(Boundary, clock_boundary(Program, Boundary), Boundaries),
    Boundaries ==
      [ not_provable(arm_absence_batch_invariance(
                       rule(1, edge, dispatch_first/2), dispatch_first/2)),
        not_provable(arm_absence_batch_invariance(
                       rule(2, edge, dispatch_first/2), dispatch_first/2)) ],
    once(run_program(Program, [],
                     [[ +dispatch_ack(1), +dispatch_seal(1) ]], AckFirst, _)),
    once(run_program(Program, [],
                     [[ +dispatch_seal(1), +dispatch_ack(1) ]], SealFirst, _)),
    AckFirst  == [dispatch_ack(1), dispatch_seal(1), dispatch_first(1, acked)],
    SealFirst == [dispatch_ack(1), dispatch_seal(1), dispatch_first(1, sealed)].

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
