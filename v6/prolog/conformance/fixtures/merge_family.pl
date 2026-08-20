% fixtures/merge_family.pl : merge_family lab checks promoted into the shared
% fixture format, RE-GRADED under the rulings (AGGREGATE.md 5c: pre-ruling
% expectations stay as comments, marked REJECTED READING).
%
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ merge: two edge rules, one head ════════════════════════════════════════
% `out` is a Log rel now (q3): edge heads that never retract ARE history.
% The trailing [] tick is the engine-owned drain (q5) consuming the carry.

fixture(merge_batches_per_tick,
  prog([ kind(event_a/1, log), keep(event_a/1, all),
         kind(event_b/1, log), keep(event_b/1, all),
         kind(out/1, log),     keep(out/1, all) ],
       [ (out(Item) <+ event_a(Item)),
         (out(Item) <+ event_b(Item)) ]),
  [],
  [ [ +event_a(alpha) ],
    [ +event_b(beta), +event_a(gamma) ],
    [ +event_b(delta) ] ],
  [ deltas(out/1, [ [ +out(alpha) ],
                    [ +out(beta), +out(gamma) ],
                    [ +out(delta) ],
                    [] ]),
    final(out/1, [ out(alpha), out(beta), out(delta), out(gamma) ]) ]).

fixture(merge_never_retracts,
  prog([ kind(event_a/1, log), keep(event_a/1, all),
         kind(event_b/1, log), keep(event_b/1, all),
         kind(out/1, log),     keep(out/1, all) ],
       [ (out(Item) <+ event_a(Item)),
         (out(Item) <+ event_b(Item)) ]),
  [],
  [ [ +event_a(alpha) ], [ +event_b(beta) ] ],
  [ deltas(out/1, [ [ +out(alpha) ], [ +out(beta) ], [] ]) ]).

% ═══ mergeByKey: keyed head, last writer wins ═══════════════════════════════

% One program, three keys, one keyed head fed by two rules. The keys are
% independent (latest/2 is keyed on column 1) so each key's tick range is the
% scenario the folded fixture used to run alone.
%   cli  ticks 1-3, last write wins across both rules.
%   hub  ticks 4-5, equal-row keyed write = no-op (r_equal_row_write): tick 5
%        is silent.
%   api  tick 6, two same-tick writes to one key from two rules.
%        REJECTED READING (merge lab, pre-ruling): this threw
%        keyed_conflict(latest/2, [api], [latest(api,v1), latest(api,v2)]).
%        Under q1 stamps the two arrivals are ORDERED occurrences; the later
%        write is a fold step and arrival order decides deterministically.
%        Prevention of true conflicts is the STATIC pairwise-disjointness
%        check, not a runtime throw.
% folded 2026-08-20 from key_last_write_wins, key_identical_write_is_silent,
% key_same_tick_ordered_not_conflict.
fixture(keyed_head_fold_across_two_rules,
  prog([ kind(from_poll/2, log), keep(from_poll/2, all),
         kind(from_push/2, log), keep(from_push/2, all),
         keyed(latest/2, [1]) ],
       [ (latest(Key, Value) <+ from_poll(Key, Value)),
         (latest(Key, Value) <+ from_push(Key, Value)) ]),
  [],
  [ [ +from_poll(cli, v1) ], [ +from_push(cli, v2) ], [ +from_poll(cli, v3) ],
    [ +from_poll(hub, v1) ], [ +from_push(hub, v1) ],
    [ +from_poll(api, v1), +from_push(api, v2) ] ],
  [ deltas(latest/2, [ [ +latest(cli, v1) ],
                       [ -latest(cli, v1), +latest(cli, v2) ],
                       [ -latest(cli, v2), +latest(cli, v3) ],
                       [ +latest(hub, v1) ],
                       [],
                       [ +latest(api, v2) ],
                       [] ]),
    final(latest/2, [ latest(api, v2), latest(cli, v3), latest(hub, v1) ]) ]).



% ═══ mergeByKeyScan: keyed head + pre, the fold ═════════════════════════════

fixture(counter_fold_matches_hand_computation,
  prog([ kind(increment/2, log), keep(increment/2, all),
         kind(decrement/2, log), keep(decrement/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next := Total + 1),
         (counter(Name, Next) <+ decrement(Name, _), pre(counter(Name, Total)), Next := Total - 1),
         (hot(Name) <- counter(Name, Total), Total >= 2) ]),
  [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1) ],
    [ +increment(clicks, ev2) ],
    [ +decrement(clicks, ev3) ],
    [ +increment(clicks, ev4) ] ],
  [ deltas(counter/2, [ [ -counter(clicks, 0), +counter(clicks, 1) ],
                        [ -counter(clicks, 1), +counter(clicks, 2) ],
                        [ -counter(clicks, 2), +counter(clicks, 1) ],
                        [ -counter(clicks, 1), +counter(clicks, 2) ],
                        [] ]),
    deltas(hot/1, [ [], [ +hot(clicks) ], [ -hot(clicks) ], [ +hot(clicks) ], [] ]),
    final(counter/2, [ counter(clicks, 2) ]) ]).

% seed arm + transition arm, mutually exclusive via negation.
fixture(seed_and_transition_are_disjoint,
  prog([ kind(increment/2, log), keep(increment/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, 1)    <+ increment(Name, _), not(pre(counter(Name, _)))),
         (counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next := Total + 1) ]),
  [],
  [ [ +increment(clicks, ev1) ], [ +increment(clicks, ev2) ] ],
  [ deltas(counter/2, [ [ +counter(clicks, 1) ],
                        [ -counter(clicks, 1), +counter(clicks, 2) ],
                        [] ]),
    final(counter/2, [ counter(clicks, 2) ]) ]).

% THE R1 FIX, the arc's central fixture. REJECTED READING (merge lab
% scan_undercounts_batched_events): two same-tick increments folded to +1
% (final counter(clicks,1)) because a tick was a set. Under q1 stamps the two
% arrivals are two ordered occurrences; pre chains within the tick (r1 rider);
% the boundary shows only start-vs-end (R2 rider): -0/+2, never the
% intermediate 1.
fixture(batched_increments_both_count,
  prog([ kind(increment/2, log), keep(increment/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next := Total + 1) ]),
  [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1), +increment(clicks, ev2) ] ],
  [ deltas(counter/2, [ [ -counter(clicks, 0), +counter(clicks, 2) ], [] ]),
     final(counter/2, [ counter(clicks, 2) ]) ]).

% pre/2 supplies the initial accumulator while preserving pre/1's evolving
% read after the first occurrence, so this is the one-arm spelling of the
% seed-and-transition pair above.
fixture(seeded_one_arm_fold_matches_two_arms,
  prog([ kind(increment/2, log), keep(increment/2, all),
         kind(decrement/2, log), keep(decrement/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name, _),
                                pre(counter(Name, Total), 0), Next := Total + 1),
         (counter(Name, Next) <+ decrement(Name, _),
                                pre(counter(Name, Total), 0), Next := Total - 1) ]),
  [],
  [ [ +increment(clicks, ev1) ],
    [ +increment(clicks, ev2) ],
    [ +decrement(clicks, ev3) ],
    [ +increment(clicks, ev4) ] ],
  [ deltas(counter/2, [ [ +counter(clicks, 1) ],
                        [ -counter(clicks, 1), +counter(clicks, 2) ],
                        [ -counter(clicks, 2), +counter(clicks, 1) ],
                        [ -counter(clicks, 1), +counter(clicks, 2) ], [] ]),
    final(counter/2, [ counter(clicks, 2) ]) ]).

fixture(seeded_pre_reads_a_body_bound_value,
  prog([ kind(increment/2, log), keep(increment/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name, Start),
                                pre(counter(Name, Total), Start), Next := Total + 1) ]),
  [],
  [ [ +increment(clicks, 4) ], [ +increment(clicks, 99) ] ],
  [ final(counter/2, [ counter(clicks, 6) ]) ]).

fixture(seeded_pre_multicolumn_fee_stats_fold,
  prog([ kind(fee/2, log), keep(fee/2, all),
         keyed(fee_stats/2, [1]) ],
       [ (fee_stats(Account, Next) <+ fee(Account, Amount),
                                      pre(fee_stats(Account, Total), 0),
                                      Next := Total + Amount) ]),
  [],
  [ [ +fee(merchant, 7), +fee(merchant, 5) ] ],
  [ final(fee_stats/2, [ fee_stats(merchant, 12) ]) ]).

fixture(seeded_pre_text_breadcrumb_fold,
  prog([ kind(step/2, log), keep(step/2, all),
         keyed(breadcrumb/2, [1]) ],
       [ (breadcrumb(Path, Next) <+ step(Path, Piece),
                                  pre(breadcrumb(Path, SoFar), ''),
                                  Next := concat([SoFar, Piece])) ]),
  [],
  [ [ +step(main, home), +step(main, '/'), +step(main, docs) ] ],
  [ final(breadcrumb/2, [ breadcrumb(main, 'home/docs') ]) ]).

% REJECTED READING (merge lab counter_same_tick_conflict): +increment and
% +decrement in one tick threw keyed_conflict. Ruled: ordered occurrences,
% 0 -> 1 -> 0, and the boundary diff is EMPTY (net-zero fold is silent).
fixture(increment_decrement_same_tick_nets_zero,
  prog([ kind(increment/2, log), keep(increment/2, all),
         kind(decrement/2, log), keep(decrement/2, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next := Total + 1),
         (counter(Name, Next) <+ decrement(Name, _), pre(counter(Name, Total)), Next := Total - 1) ]),
  [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1), +decrement(clicks, ev2) ] ],
  [ deltas(counter/2, [ [] ]),
    final(counter/2, [ counter(clicks, 0) ]) ]).

% The runtime conflict that REMAINS a conflict: one occurrence, two rules,
% two different rows for one key (bodies not disjoint on the same trigger).
fixture(one_occurrence_two_rows_still_conflicts,
  prog([ kind(ping/1, log), keep(ping/1, all),
         keyed(latest/2, [1]) ],
       [ (latest(cli, a) <+ ping(_)),
         (latest(cli, b) <+ ping(_)) ]),
  [],
  [ [ +ping(one) ] ],
  [ throws(keyed_conflict(latest/2, [cli], [ latest(cli, a), latest(cli, b) ])) ]).
