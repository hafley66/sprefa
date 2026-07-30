% 3_clock_history.pl : queryable replay table for the phase-5 gate.

:- module(clock_history,
          [ historical_bug_class/5,
            historical_bug_program/2
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
