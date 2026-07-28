% fixtures/occurrence_identity.pl : the occurrence_identity lab promoted into
% the shared fixture format, under ruling q1 = hybrid_stamps_plus_support_count.
%
% The lab COMPARED five mechanism settings (a, a_naive, b, b_naive, hybrid)
% through one switch. The conformance engine implements ONE of them: q1 hybrid,
% which review_occurrence_identity.md:118-135 restates as "A's semantics plus
% the store keeping its IVM support count as engine bookkeeping". Every lab
% check about A's semantics promotes as a plain fixture below. Every lab check
% that measured a REJECTED mechanism is listed in `not promoted:` with the
% mechanism it measured; those are receipts for why the ruling went the way it
% did, not laws the engine can be graded against.
%
% Support count is engine-internal under the ruling (stamps carry
% multiplicity), so no fixture reads it. Stamps themselves are internal too;
% what they buy is observable as the ORDER and MULTIPLICITY of Log deltas at
% the boundary (r7 multiset_on_log_set_on_set), and that is what these
% fixtures grade.
%
% not promoted:
%   b_count_blind_fold_undercounts
%     measured mechanism B (Z-set counts, no order): the count-blind fold
%     reaching 1 instead of 2. B was not ruled; the engine has no such mode.
%   b_count_aware_fold_reaches_two
%     measured mechanism B's count_of(Atom, Count) body form. AGGREGATE calls
%     that surface DEAD unless pure B is ruled; it was not.
%   count_aware_fold_correct_under_both
%     same count_of surface, graded across a/b/hybrid. Dead surface plus a
%     mechanism sweep; neither is expressible here.
%   b_counter_fold_is_order_independent
%     measured mechanism B's admissible-order set (both orders agree when the
%     step functions commute). B has no admissible-order set in this engine:
%     arrival order is the order.
%   b_lww_has_two_admissible_answers
%     measured mechanism B returning both latest(cli,v2) and latest(cli,v1)
%     for one arrival sequence. Rejected mechanism; the ruled engine has one
%     answer, graded by lww_fold_follows_arrival_order below.
%   b_concat_has_two_admissible_answers
%     same, for the non-commuting concat fold under mechanism B.
%   occurrence_firing_breaks_demand_dedup
%     measured the a_naive and b_naive settings (occurrence firing on demand
%     rels) double-firing an effect. Both rejected; q2 scopes occurrence
%     identity to Log rels by declaration, so the shape cannot be written.
%   storage_cost_stamps_vs_counts
%     a storage-cost table across a/b/hybrid (extra columns per rel). Measures
%     rejected mechanisms' physical cost; fixtures grade semantics only.
%   hybrid_settles_undercount_order_and_dedup
%     ran the hybrid setting against A's and B's failing cases via the
%     mechanism switch. The review's finding is that hybrid IS A semantically,
%     so its semantic content is exactly the fixtures below; the remaining half
%     (support count readable underneath the stamps) is engine bookkeeping with
%     no boundary-observable form.
%   delta_shape_measured_per_mechanism
%     a per-mechanism delta-volume table (a-3, a_naive-3, b-2, b_naive-3,
%     hybrid-3). The ruled row is promoted as
%     identical_increments_stack_as_log_deltas and
%     level_view_reads_set_projection_not_occurrences; the rest measure
%     rejected mechanisms.
%   level_view_over_folded_state_still_retracts
%     ruled semantics, but already graded: merge_family.pl's
%     counter_fold_matches_hand_computation grades hot/1 deltas
%     [[], [+hot], [-hot], [+hot], []] over the same fold. Not duplicated.
%   a_ordered_fold_reaches_two
%     ruled semantics, already graded by merge_family.pl's
%     batched_increments_both_count. The half that is NOT covered there (the
%     driver rel carrying no event-id column) is promoted below as
%     log_driver_fold_needs_no_id_column.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ the fold driver needs no id column ═════════════════════════════════════
% Promotes occurrence_identity_removes_the_id_column (the A half).
% merge_family had to write increment(Name, EventId) for two clicks to be two
% rows at all (its ambiguity 3). Under q1 the engine stamps the Log rel, so
% increment/1 with no id column still delivers two occurrences and the fold
% chains across both.

fixture(log_driver_fold_needs_no_id_column,
  prog([ kind(increment/1, log), keep(increment/1, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name), pre(counter(Name, Total)),
                                Next := Total + 1) ]),
  [ counter(clicks, 0) ],
  [ [ +increment(clicks), +increment(clicks) ] ],
  [ final(counter/2, [ counter(clicks, 2) ]),
    final(increment/1, [ increment(clicks), increment(clicks) ]),
    deltas(counter/2, [ [ -counter(clicks, 0), +counter(clicks, 2) ], [] ]) ]).

% ═══ r7 on a fold driver: byte-identical rows are separate occurrences ══════
% Promotes the ruled row of delta_shape_measured_per_mechanism. engine_core's
% set_dedups_log_stacks covers the minimal case (a Log arrival stacking against
% a Set arrival deduping); this is the fold-driver case, where the SAME tick
% produces three +Row deltas on the Log rel and exactly one -old/+new pair on
% the keyed head it drives. The intermediate 1 and 2 never cross the boundary.

fixture(identical_increments_stack_as_log_deltas,
  prog([ kind(increment/1, log), keep(increment/1, all),
         keyed(counter/2, [1]) ],
       [ (counter(Name, Next) <+ increment(Name), pre(counter(Name, Total)),
                                Next := Total + 1) ]),
  [ counter(clicks, 0) ],
  [ [ +increment(clicks), +increment(clicks), +increment(clicks) ] ],
  [ deltas(increment/1, [ [ +increment(clicks), +increment(clicks),
                            +increment(clicks) ], [] ]),
    deltas(counter/2, [ [ -counter(clicks, 0), +counter(clicks, 3) ], [] ]),
    final(counter/2, [ counter(clicks, 3) ]),
    ticks(2) ]).

% ═══ order-dependent fold 1: last write wins inside one tick ════════════════
% Promotes a_lww_follows_arrival_order. v2 arrives BEFORE v1, so arrival order
% and term order disagree; the ruled answer is the arrival-last one.

fixture(lww_fold_follows_arrival_order,
  prog([ kind(set_value/2, log), keep(set_value/2, all),
         keyed(latest/2, [1]) ],
       [ (latest(Key, Value) <+ set_value(Key, Value),
                                pre(latest(Key, _PreviousValue))) ]),
  [ latest(cli, v0) ],
  [ [ +set_value(cli, v2), +set_value(cli, v1) ] ],
  [ final(latest/2, [ latest(cli, v1) ]),
    deltas(latest/2, [ [ -latest(cli, v0), +latest(cli, v1) ], [] ]) ]).

% ═══ order-dependent fold 2: concat, where nothing commutes ═════════════════
% This pair is the INVERSION of the lab's b_state_collides_on_distinct_arrival
% _orders. That check proved mechanism B produced a byte-identical store for
% two distinct arrival orders, which was B's failure. Under the ruled stamps
% the same two orders are two different programs' worth of outcome: ab and ba.
% The pair below states that positively, one fixture per order.
%
% concat_fold_follows_arrival_order also closes the gap
% review_occurrence_identity.md:108-110 flagged: the lab never graded the
% concat_ba DEFAULT run, only the collision. Here [b, a] in one tick is
% scheduled and the fold result is asserted in arrival order.

fixture(concat_fold_follows_arrival_order,
  prog([ kind(append_line/2, log), keep(append_line/2, all),
         keyed(log_text/2, [1]) ],
       [ (log_text(Channel, Next) <+ append_line(Channel, Piece),
                                     pre(log_text(Channel, SoFar)),
                                     Next := concat([SoFar, Piece])) ]),
  [ log_text(main, '') ],
  [ [ +append_line(main, a), +append_line(main, b) ] ],
  [ final(log_text/2, [ log_text(main, ab) ]),
    deltas(log_text/2, [ [ -log_text(main, ''), +log_text(main, ab) ], [] ]) ]).

fixture(concat_fold_reversed_arrival_reverses_result,
  prog([ kind(append_line/2, log), keep(append_line/2, all),
         keyed(log_text/2, [1]) ],
       [ (log_text(Channel, Next) <+ append_line(Channel, Piece),
                                     pre(log_text(Channel, SoFar)),
                                     Next := concat([SoFar, Piece])) ]),
  [ log_text(main, '') ],
  [ [ +append_line(main, b), +append_line(main, a) ] ],
  [ final(log_text/2, [ log_text(main, ba) ]),
    deltas(log_text/2, [ [ -log_text(main, ''), +log_text(main, ba) ], [] ]) ]).

% ═══ arrival order is observable in the Log delta sequence ══════════════════
% Promotes a_preserves_jsonl_arrival_order. The lab read stamps out of the
% store; stamps are engine-internal here, but r7 emits one +Row per new stamp
% IN STAMP ORDER, so the delta list IS the arrival order. The level view over
% the same rel reads the set projection and comes out sorted either way, which
% is the contrast the next fixture pair carries.
%
% shuffled_arrival_reorders_log_deltas is the INVERSION of the lab's
% b_loses_jsonl_arrival_order: that check proved mechanism B's store was the
% same term for both arrival orders. Under the ruling the two orders give two
% different delta sequences, while seen/1 stays identical.

fixture(log_deltas_follow_arrival_order,
  prog([ kind(line/3, log), keep(line/3, all) ],
       [ (seen(Path) <- line(_StreamId, Path, _Name)) ]),
  [],
  [ [ +line(1, 'a.ts', alpha), +line(1, 'a.ts', beta), +line(1, 'b.ts', gamma) ] ],
  [ deltas(line/3, [ [ +line(1, 'a.ts', alpha),
                       +line(1, 'a.ts', beta),
                       +line(1, 'b.ts', gamma) ] ]),
    deltas(seen/1, [ [ +seen('a.ts'), +seen('b.ts') ] ]),
    ticks(1) ]).

fixture(shuffled_arrival_reorders_log_deltas,
  prog([ kind(line/3, log), keep(line/3, all) ],
       [ (seen(Path) <- line(_StreamId, Path, _Name)) ]),
  [],
  [ [ +line(1, 'b.ts', gamma), +line(1, 'a.ts', beta), +line(1, 'a.ts', alpha) ] ],
  [ deltas(line/3, [ [ +line(1, 'b.ts', gamma),
                       +line(1, 'a.ts', beta),
                       +line(1, 'a.ts', alpha) ] ]),
    deltas(seen/1, [ [ +seen('a.ts'), +seen('b.ts') ] ]),
    ticks(1) ]).

% ═══ level views read the set projection, not the occurrences ══════════════
% Promotes level_view_sees_set_projection_not_occurrences (the ruled run) and
% the across-ticks half of the delta-shape table's ruled row: the SAME row
% arriving in two consecutive ticks replays on the Log rel and is silent on the
% level view. A level view that saw stamps would emit -seen/+seen on every
% repeat and mint bogus history, which is the failure r7 names.

fixture(level_view_reads_set_projection_not_occurrences,
  prog([ kind(line/3, log), keep(line/3, all) ],
       [ (seen(Path) <- line(_StreamId, Path, _Name)) ]),
  [],
  [ [ +line(1, 'a.ts', alpha) ],
    [ +line(1, 'a.ts', alpha) ] ],
  [ deltas(seen/1, [ [ +seen('a.ts') ], [] ]),
    deltas(line/3, [ [ +line(1, 'a.ts', alpha) ], [ +line(1, 'a.ts', alpha) ] ]),
    final(line/3, [ line(1, 'a.ts', alpha), line(1, 'a.ts', alpha) ]),
    ticks(2) ]).

% ═══ membership firing keeps demand dedup ═══════════════════════════════════
% Promotes membership_firing_keeps_demand_dedup, positively. The lab's negative
% twin (occurrence_firing_breaks_demand_dedup) measured a_naive and b_naive,
% both rejected, so only this direction promotes.
%
% engine_core's set_dedups_log_stacks already grades one stage. This goes the
% step further the arc needs: an actual demand -> fill shape. stale/1 is a Log
% rel and stacks (two occurrences, two deltas); fetch_demand/1 is the Set-rel
% demand view over it, so the second identical arrival makes it no newer; and
% the edge consumer downstream of the demand row fires ONCE across both ticks.
% Two fires would be gh-cache.dl's 720-vs-12-calls-per-hour defect returning.

fixture(demand_view_fires_its_consumer_once,
  prog([ kind(stale/1, log), keep(stale/1, all),
         kind(fetch_demand/1, set),
         kind(fetch_call/1, log), keep(fetch_call/1, all) ],
       [ (fetch_demand(Endpoint) <- stale(Endpoint)),
         (fetch_call(Endpoint) <+ only(fetch_demand(Endpoint))) ]),
  [],
  [ [ +stale('repos/cli/cli') ],
    [ +stale('repos/cli/cli') ] ],
  [ deltas(fetch_demand/1, [ [ +fetch_demand('repos/cli/cli') ], [] ]),
    deltas(fetch_call/1, [ [ +fetch_call('repos/cli/cli') ], [] ]),
    deltas(stale/1, [ [ +stale('repos/cli/cli') ], [ +stale('repos/cli/cli') ] ]),
    final(fetch_call/1, [ fetch_call('repos/cli/cli') ]),
    final(stale/1, [ stale('repos/cli/cli'), stale('repos/cli/cli') ]),
    ticks(2) ]).

% ═══ the same row value stacks WITHIN one tick AND ACROSS ticks ═════════════
% Not a lab promotion (owner: 2026-07-28 cleanup audit, finding F8): the tsv2
% Phase A hand-carved oracle corpus (tickLoop.test.ts's two hand-carved
% programs, demand_laziness_effect_rows/switch_as_keyed_replace, both from
% scopes.pl) never puts a byte-identical row on a Log rel twice, so a
% multiset-diff regression there (the multiplicity loop collapsing to a
% single push, turning Log-append into Set-dedup) leaves every one of those
% oracle tests green; only tsv2/tests/diff.test.ts's one unit case on
% runtime/diff.ts's multisetDiff caught it directly.
%
% This fixture combines the two occurrence shapes this file otherwise grades
% separately: identical_increments_stack_as_log_deltas covers WITHIN one tick
% (3 identical rows, one batch); level_view_reads_set_projection_not_occurrences
% covers ACROSS ticks (1 identical row, 2 ticks). Here `heard/1` gets the SAME
% row twice in tick 1's own batch AND a third time in tick 2 -- both shapes at
% once, on a rel a downstream unmarked single-atom edge trigger also reads
% (`heard_count/1`, the set_dedups_log_stacks idiom from engine_core.pl), so
% two independently-diffed Log rels carry the occurrence count across the
% tsv2 sweep's byte-for-byte tick-log grade, not just one.

fixture(log_stacks_within_tick_and_across_ticks,
  prog([ kind(heard/1, log), keep(heard/1, all),
         kind(heard_count/1, log), keep(heard_count/1, all) ],
       [ (heard_count(Item) <+ heard(Item)) ]),
  [],
  [ [ +heard(alpha), +heard(alpha) ],
    [ +heard(alpha) ] ],
  [ deltas(heard/1, [ [ +heard(alpha), +heard(alpha) ], [ +heard(alpha) ], [] ]),
    deltas(heard_count/1, [ [ +heard_count(alpha), +heard_count(alpha) ],
                            [ +heard_count(alpha) ], [] ]),
    final(heard/1, [ heard(alpha), heard(alpha), heard(alpha) ]),
    final(heard_count/1, [ heard_count(alpha), heard_count(alpha), heard_count(alpha) ]),
    ticks(3) ]).
