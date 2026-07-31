% 3_clock_history.pl : queryable replay table for the phase-5 gate.

:- module(clock_history,
          [ historical_bug_class/5,
            historical_bug_program/2,
            historical_bug_fixed_twin/2,
            historical_clock_receipt/3
          ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% historical_bug_class(Id, Witness, CaughtToday, ClockCatch, RequiredEvidence).
%
% ClockCatch is deliberately narrower than "the project has a regression
% test". Static ring/grade facts cannot prove storage-key SQL, partitioned
% retention, missing operational logging, or empty-group aggregate policy.
historical_bug_class(
    a2, a2_batch_sensitive_join,
    executable_semantics_only,
    not_provable,
    'A bare multi-trigger arm intentionally fires for every source and samples the others. Batch invariance would require a different existing-semantics ruling.').
historical_bug_class(
    a4, world_fed_keyed_arrival_replaces,
    oracle_emitter_golden,
    not_provable,
    'Compare keyed boundary deltas and final rows; ring B is identical on both implementations.').
historical_bug_class(
    a5, name_arity_collision,
    emitter_validation,
    not_provable,
    'Validate SQL and emitted target identifiers by Name/Arity before emission.').
historical_bug_class(
    a6, clock_rel_join_storms,
    oracle_emitter_golden,
    runtime_clock_crosscheck,
    'Compare inferred grade-zero placement with the emitted tick log and frozen mid-tick B state.').
historical_bug_class(
    a7, invalidation_log_poison,
    no_general_check,
    not_provable,
    'The program must declare whether invalidation is occurrence history or current membership; both are valid N and B programs.').
historical_bug_class(
    a8, retention_partition,
    documented_per_relation,
    not_provable,
    'Ring N does not encode a partition key; per-key retention requires a retention policy ruling.').
historical_bug_class(
    a9, transition_collapse_log,
    no_general_check,
    not_provable,
    'Operational transition logging must be tested at the boundary; dependency grades do not imply an audit row.').
historical_bug_class(
    a11, empty_count,
    oracle_emitter_golden,
    not_provable,
    'Empty-group count policy belongs to aggregate semantics; both zero rows and one count-zero row inhabit B.').

% ── the 2026-07-31 legs round ──────────────────────────────────────────────
%
% Three classes the checker had never been replayed against. Each is graded
% by what the checker SAYS, not by whether some suite is green: a12 and d1 by
% a label that appears on the wrong program and is absent from the right one,
% c2 by an inferred grade paired with the observed tick.
historical_bug_class(
    a12, marker_stops_backlog_replay,
    oracle_emitter_golden,
    labelled_ring_discriminates,
    'The unmarked twin reads its second atom in ring Z as a trigger, so a late arrival replays the whole banked backlog; latest() reads it in ring B as a sample. The dependency projection carries the difference and the multi-trigger boundary appears only on the unmarked twin.').
historical_bug_class(
    c2, same_tick_transition_collapse,
    oracle_emitter_golden,
    runtime_clock_crosscheck,
    'The update arm reads a departure at grade 1 and its keyed source in ring B. Ring B holds no per-tick multiplicity, so N-1 of N same-tick transitions is not a loss the ring can express; the checker owns the +1 placement and pairs it with the observed tick.').
historical_bug_class(
    d1, arm_absence_over_edge_headed_rel,
    no_general_check,
    labelled_boundary,
    'not(Atom) in an arm is a zero-test against a plane. Over a level-headed rel the plane is frozen before edges and the arm is order-independent; over an edge-headed rel a later occurrence in one batch tests what an earlier one wrote. The checker names the second case and stays silent on the first.').

historical_bug_program(
    a2,
    prog([ kind(left/1, log), keep(left/1, all),
           kind(right/1, log), keep(right/1, all),
           kind(answer/2, log), keep(answer/2, all) ],
         [ (answer(Left, Right) <+ left(Left), right(Right)) ])).
historical_bug_program(
    a4,
    prog([keyed(mode/2, [1])], [])).
historical_bug_program(
    a5,
    prog([], [ (same(X) <- input(X)),
               (same(X, Y) <- input(X), input(Y)) ])).
historical_bug_program(
    a6,
    prog([ kind(source/1, set),
           kind(seen/1, log), keep(seen/1, all) ],
         [ (visible(X) <- source(X)),
           (seen(X) <+ source(X), latest(visible(X))) ])).
historical_bug_program(
    a7,
    prog([ kind(invalidated/1, log), keep(invalidated/1, all) ],
         [])).
historical_bug_program(
    a8,
    prog([ kind(channel_item/2, log), keep(channel_item/2, count(1)) ],
         [])).
historical_bug_program(
    a9,
    prog([ kind(state_changed/3, log), keep(state_changed/3, all) ],
         [])).
historical_bug_program(
    a11,
    prog([], [ (total(count(N)) <- item(N)) ])).
historical_bug_program(
    a12,
    prog([ kind(demand/1, log), keep(demand/1, all),
           kind(config/1, log), keep(config/1, all),
           kind(answer/2, log), keep(answer/2, all) ],
         [ (answer(Demand, Config) <+ demand(Demand), config(Config)) ])).
historical_bug_program(
    c2,
    prog([ keyed(current/2, [1]),
           kind(changed/3, log), keep(changed/3, all) ],
         [ (changed(Key, Old, New) <+
              finalize(current(Key, Old)), current(Key, New)) ])).
historical_bug_program(
    d1,
    prog([ kind(req/1, log), keep(req/1, all),
           kind(out/1, log), keep(out/1, all) ],
         [ (out(Item) <+ req(Item), not(out(_))) ])).

% historical_bug_fixed_twin(Id, Program).
%
% The program a cold author writes AFTER learning the class. A label that
% fires on both twins proves nothing; these pin that the checker's output
% CHANGES when the bug is fixed.
historical_bug_fixed_twin(
    a12,
    prog([ kind(demand/1, log), keep(demand/1, all),
           kind(config/1, log), keep(config/1, all),
           kind(answer/2, log), keep(answer/2, all) ],
         [ (answer(Demand, Config) <+
              demand(Demand), latest(config(Config))) ])).
historical_bug_fixed_twin(
    d1,
    prog([ kind(req/1, log), keep(req/1, all),
           kind(out/1, log), keep(out/1, all) ],
         [ (out(Item) <+ req(Item), not(seen(Item))),
           (seen(Item) <- req(Item)) ])).

% historical_clock_receipt(Id, Status, Evidence).
%
% These are the exact claims the clock checker owns. A status of
% not_provable is a ruled boundary, not an unchecked test. The replay tests
% still pass each recorded program through the checker and pin its complete
% dependency projection. A6 remains a runtime crosscheck because its
% recorded program contains only grade-zero causal edges. The focused suite's
% pipe receipt separately pairs nonzero inferred grades with runtime ticks.
historical_clock_receipt(
    a2,
    not_provable,
    multi_trigger_batch_invariance(
      rule(1, edge, answer/2),
      [left/1, right/1])).
historical_clock_receipt(
    a4,
    not_provable,
    keyed_boundary_replacement_requires_runtime_deltas).
historical_clock_receipt(
    a5,
    not_provable,
    emitted_identifier_uniqueness_requires_target_validation).
historical_clock_receipt(
    a6,
    runtime_clock_crosscheck,
    grade_zero_offsets_match_observed_ticks(
      source/1,
      [seen/1-0, source/1-0, visible/1-0])).
historical_clock_receipt(
    a7,
    not_provable,
    occurrence_history_versus_membership_requires_relation_intent).
historical_clock_receipt(
    a8,
    not_provable,
    retention_partition_requires_policy_key).
historical_clock_receipt(
    a9,
    not_provable,
    operational_transition_rows_require_boundary_observation).
historical_clock_receipt(
    a11,
    not_provable,
    empty_group_policy_requires_aggregate_semantics).
historical_clock_receipt(
    a12,
    labelled_ring_discriminates,
    trigger_ring_z_versus_sample_ring_b(answer/2, config/1)).
historical_clock_receipt(
    c2,
    runtime_clock_crosscheck,
    departure_grade_matches_observed_tick(changed/3, clock(current/2, 1))).
historical_clock_receipt(
    d1,
    labelled_boundary,
    arm_absence_batch_invariance(rule(1, edge, out/1), out/1)).
