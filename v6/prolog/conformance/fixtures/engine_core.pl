% fixtures/engine_core.pl : engine-owned laws with no single source lab —
% retention (q10), rel-kind load checks (q3), edge-target typing, now() (R3),
% drain scheduling (q5). Owner: coordinator.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% keep(count(N)) prunes a Log rel to its newest N stamps at tick end.
fixture(retention_count_prunes_oldest,
  prog([ kind(event/1, log), keep(event/1, count(2)) ],
       []),
  [],
  [ [ +event(one) ], [ +event(two) ], [ +event(three) ] ],
  [ final(event/1, [ event(three), event(two) ]) ]).

% Retention reports the reclamation. The prune is an ordinary minus delta at
% the tick boundary, so the bound the program declared is graded rather than
% inferred from the final state.
%
% FAIL-FIRST: red on both doors before the retention-minus change
% (plans/2026-07-30-time-plane-unification-verdict.md recommendation 1). The
% oracle's boundary_deltas/6 diffed stamps in one direction only (new stamps
% became LogAdds, vanished stamps became nothing) and the emitter's
% boundaryDelta suppressed a log rel's negative weight behind a
% `kind === "set"` guard, so both doors dropped the prune symmetrically. The
% pre-change reading was tick 3 = [ +event(three) ] with no minus, which is
% the retention-grading gap this fixture closes.
%
% R7 is not weakened: the minus does not say the occurrence un-happened, it
% says the STORAGE row was reclaimed under a bound the program declared.
% Only keep(...) can emit it; retract_from_log/1 still throws.
fixture(retention_prune_is_a_visible_minus,
  prog([ kind(event/1, log), keep(event/1, count(2)) ],
       []),
  [],
  [ [ +event(one) ], [ +event(two) ], [ +event(three) ] ],
  [ deltas(event/1, [ [ +event(one) ],
                      [ +event(two) ],
                      [ -event(one), +event(three) ] ]) ]).

% A Log rel without a keep clause is a load error (q10: REQUIRED).
fixture(log_without_retention_rejected,
  prog([ kind(event/1, log) ], []),
  [],
  [ [ +event(one) ] ],
  [ throws(missing_retention(event/1)) ]).

% An aggregate in an edge head is a load error. Aggregates are a grouped
% recomputation over a bag of derivations; an edge rule fires once per
% occurrence and has no bag to aggregate.
%
% This law had no fixture before rank R2 of
% plans/2026-07-29-prolog-org-review.md, and the compiler had no matching
% check at all: check_supported_subset/1 ACCEPTED this program, so a compound
% aggregate argument reached generic head-expression lowering. Both doors
% refuse it now, the compiler naming the offending head as
% unsupported_construct(aggregate_in_edge_head(total/1)).
fixture(aggregate_in_edge_head_rejected,
  prog([ kind(hit/1, log), keep(hit/1, all) ],
       [ (total(count(Item)) <+ hit(Item)) ]),
  [],
  [ [ +hit(one) ] ],
  [ throws(aggregate_in_edge_head) ]).

% Retention is meaningful only on Log relations. A keep clause on a Set was
% previously accepted and had no effect.
fixture(keep_on_non_log_rel_rejected,
  prog([ keep(state/1, all) ], []),
  [],
  [],
  [ throws(keep_on_non_log_rel(state/1)) ]).

% Log rels cannot be keyed (a keyed rel is a Set by construction).
fixture(keyed_log_rejected,
  prog([ kind(latest/2, log), keep(latest/2, all), keyed(latest/2, [1]) ], []),
  [],
  [],
  [ throws(keyed_log_rel(latest/2)) ]).

% An edge rule into an unkeyed Set rel is a type error (append into
% membership dedup is one of the two, pick with a declaration).
fixture(edge_into_unkeyed_set_rejected,
  prog([ kind(ping/1, log), keep(ping/1, all) ],
       [ (sink(Item) <+ ping(Item)) ]),
  [],
  [ [ +ping(one) ] ],
  [ throws(edge_into_unkeyed_set(sink/1)) ]).

% Retracting from a Log rel throws: occurrences cannot un-happen.
fixture(log_retraction_rejected,
  prog([ kind(event/1, log), keep(event/1, all) ], []),
  [],
  [ [ +event(one) ], [ -event(one) ] ],
  [ throws(retract_from_log(event/1)) ]).

% A world-fed keyed Set replaces the existing row at the same key. The
% boundary reports the departed row before the arriving row.
fixture(world_fed_keyed_arrival_replaces,
  prog([ keyed(world_mode/2, [1]) ], []),
  [],
  [ [ +world_mode(1, a) ],
    [ +world_mode(1, b) ] ],
  [ deltas(world_mode/2,
           [ [ +world_mode(1, a) ],
             [ -world_mode(1, a), +world_mode(1, b) ] ]),
    final(world_mode/2, [ world_mode(1, b) ]) ]).

% A level-headed Log has no stamped store writes and therefore no Log delta
% channel. Refuse the declaration and rule combination at load time.
fixture(log_on_level_headed_rel_rejected,
  prog([ kind(derived_event/1, log), keep(derived_event/1, all) ],
       [ (derived_event(Item) <- source_item(Item)) ]),
  [],
  [ [ +source_item(alpha) ] ],
  [ throws(log_on_level_headed_rel(derived_event/1)) ]).

% latest/1 in a level body reads the same Visible rows as a bare atom. The
% marking has no level-rule meaning, so refuse it at load time.
fixture(latest_in_level_rule_rejected,
  prog([],
       [ (latest_copy(Item) <- source_item(Item), latest(source_item(Item))) ]),
  [],
  [ [ +source_item(alpha) ] ],
  [ throws(latest_in_level_rule(source_item/1)) ]).

% Level evaluation receives an empty PreState list, so pre/1 in a level body
% can never succeed. Refuse it at load time.
fixture(pre_in_level_rule_rejected,
  prog([],
       [ (previous_copy(Item) <- source_item(Item), pre(source_item(Item))) ]),
  [],
  [ [ +source_item(alpha) ] ],
  [ throws(pre_in_level_rule(source_item/1)) ]).

% now() reads the phantom tick (R3, kernel; never an arrival).
fixture(now_reads_the_tick,
  prog([ kind(ping/1, log),   keep(ping/1, all),
         kind(seen_at/2, log), keep(seen_at/2, all) ],
       [ (seen_at(Name, Tick) <+ ping(Name), now(Tick)) ]),
  [],
  [ [ +ping(alpha) ], [ +ping(beta) ] ],
  [ final(seen_at/2, [ seen_at(alpha, 1), seen_at(beta, 2) ]) ]).

% q4 next-tick + q5 engine drains: a two-stage chain fed ONE outside arrival
% moves one hop per tick; the engine schedules the drain ticks itself, and
% each hop lands in its own delta set (the self-diagnosis law's reading).
fixture(edge_chain_hops_tick_per_stage,
  prog([ kind(source_ev/1, log), keep(source_ev/1, all),
         kind(stage_one/1, log), keep(stage_one/1, all),
         kind(stage_two/1, log), keep(stage_two/1, all) ],
       [ (stage_one(Item) <+ source_ev(Item)),
         (stage_two(Item) <+ stage_one(Item)) ]),
  [],
  [ [ +source_ev(alpha) ] ],
  [ deltas(stage_one/1, [ [ +stage_one(alpha) ], [], [] ]),
    deltas(stage_two/1, [ [], [ +stage_two(alpha) ], [] ]),
    ticks(3) ]).

% q6: the marker narrows triggers. Without only/1 the second stage fires on
% ANY body atom's arrival, replaying backlog when a late subscriber joins;
% with the marker the subscription row alone cannot fire the rule.
fixture(marker_stops_backlog_replay,
  prog([ kind(change_ev/1, log),  keep(change_ev/1, all),
         kind(subscriber/1, log), keep(subscriber/1, all),
         kind(sent/2, log),       keep(sent/2, all) ],
       [ (sent(Client, Item) <+ change_ev(Item), latest(subscriber(Client))) ]),
  [],
  [ [ +subscriber(alice) ],
    [ +change_ev(one) ],
    [ +subscriber(bob) ] ],
  [ final(sent/2, [ sent(alice, one) ]) ]).

% The unmarked twin: bob's late subscription replays the backlog (any-atom
% is the DEFAULT, sometimes wanted: SSE catch-up).
fixture(unmarked_edge_replays_backlog,
  prog([ kind(change_ev/1, log),  keep(change_ev/1, all),
         kind(subscriber/1, log), keep(subscriber/1, all),
         kind(sent/2, log),       keep(sent/2, all) ],
       [ (sent(Client, Item) <+ change_ev(Item), subscriber(Client)) ]),
  [],
  [ [ +subscriber(alice) ],
    [ +change_ev(one) ],
    [ +subscriber(bob) ] ],
  [ final(sent/2, [ sent(alice, one), sent(bob, one) ]) ]).

% A tick of PURE retractions from a Set source rel: rows leave, the level
% view retracts, no edge fires. (Regression: the check_eventing promotion
% found the engine failing silently on any net-shrinking tick.)
fixture(retraction_only_tick_retracts_level_view,
  prog([],
       [ (mirror(Item) <- source_row(Item)) ]),
  [],
  [ [ +source_row(alpha), +source_row(beta) ],
    [ -source_row(alpha), -source_row(beta) ] ],
  [ deltas(mirror/1, [ [ +mirror(alpha), +mirror(beta) ],
                       [ -mirror(alpha), -mirror(beta) ] ]),
    final(mirror/1, []) ]).

% r4 (user-ruled): departed/1 binds a retraction. The -delta of a listened
% Set/level rel becomes a departure occurrence at T+1 (q4 next-tick), and
% the closed-at telemetry the eventing lab called inexpressible is now two
% rules. Tick trace: +mirror at 1, -mirror at 2, departure fires at 3
% (drain), closed_at's own write drains at 4.
fixture(departed_fires_next_tick_on_retraction,
  prog([
         kind(closed_at/2, log), keep(closed_at/2, all) ],
       [ (mirror(Item) <- source_row(Item)),
         (closed_at(Item, Tick) <+ finalize(mirror(Item)), now(Tick)) ]),
  [],
  [ [ +source_row(alpha) ],
    [ -source_row(alpha) ] ],
  [ deltas(mirror/1, [ [ +mirror(alpha) ], [ -mirror(alpha) ], [], [] ]),
    final(closed_at/2, [ closed_at(alpha, 3) ]),
    ticks(4) ]).

% r4 rider: a keyed REPLACE departs the old ROW (row-level reading; the key
% did not depart). Pinned so the lowering cannot choose the other reading
% silently.
fixture(keyed_replace_departs_the_old_row,
  prog([ kind(from_poll/2, log), keep(from_poll/2, all),
         keyed(latest/2, [1]),
         kind(replaced_value/2, log), keep(replaced_value/2, all) ],
       [ (latest(Key, Value) <+ from_poll(Key, Value)),
         (replaced_value(Key, OldValue) <+ finalize(latest(Key, OldValue))) ]),
  [],
  [ [ +from_poll(cli, v1) ], [ +from_poll(cli, v2) ] ],
  [ final(replaced_value/2, [ replaced_value(cli, v1) ]),
    final(latest/2, [ latest(cli, v2) ]) ]).

% Set arrivals dedup (q2: identical content is the same thing) while Log
% arrivals stack: the same row delivered twice is one occurrence vs two.
fixture(set_dedups_log_stacks,
  prog([
         kind(heard/1, log), keep(heard/1, all),
         kind(seen_count/1, log),  keep(seen_count/1, all),
         kind(heard_count/1, log), keep(heard_count/1, all) ],
       [ (seen_count(Item)  <+ seen(Item)),
         (heard_count(Item) <+ heard(Item)) ]),
  [],
  [ [ +seen(alpha), +seen(alpha), +heard(alpha), +heard(alpha) ] ],
  [ final(seen_count/1,  [ seen_count(alpha) ]),
    final(heard_count/1, [ heard_count(alpha), heard_count(alpha) ]) ]).
