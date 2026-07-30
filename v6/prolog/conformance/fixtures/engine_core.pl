% fixtures/engine_core.pl : engine-owned laws with no single source lab —
% retention (q10), rel-kind load checks (q3), edge-target typing, now() (R3),
% drain scheduling (q5). Owner: coordinator.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% keep(count(N)) prunes a Log rel to its newest N stamps at tick end.
%
% The deltas/2 leg was ADDED after the time-plane lab named this fixture's
% final/2-only expectation as the reason the retention hole survived three
% arcs: for the corpus's only keep(count(N)) fixture, grading the end state
% alone cannot see whether the prune was reported or silently dropped, and it
% was silently dropped. Retention is now graded on both legs.
fixture(retention_count_prunes_oldest,
  prog([ kind(event/1, log), keep(event/1, count(2)) ],
       []),
  [],
  [ [ +event(one) ], [ +event(two) ], [ +event(three) ] ],
  [ deltas(event/1, [ [ +event(one) ],
                      [ +event(two) ],
                      [ -event(one), +event(three) ] ]),
    final(event/1, [ event(three), event(two) ]) ]).

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

% finalize over a Log rel fires on the retention prune. THE NATURAL SPELLING
% WORKS, which SUPERSEDES the refusal three prior arcs proposed for it:
% plans/2026-07-30-rel-as-stream-lab.md card 4,
% plans/2026-07-29-update-arm-verdict.md SLOT-LOG-FINALIZE-REFUSAL, and
% plans/2026-07-28-consumption-arms-verdict.md assertion 17 all recommended
% refusing `finalize(logrel(...))` because it was statically dead -- retention
% pruned with no delta, so the arm had nothing to bind and failed silently.
% The time-plane verdict priced both directions and the fix won: a refusal
% needs two implementations plus a fail-first fixture, while making the
% spelling work needed the retention minus that was wanted anyway.
%
% Reading, per compile/TICK-MODEL.md 5.1: this binds the (dS)- of the RETAINED
% WINDOW, never of the occurrence. The firing already happened and every rule
% that was going to see it already saw it; what departs is the stored record,
% under the bound the program itself declared.
%
% Cost, named rather than discovered later: a pruning log rel with a finalize
% listener mints drain ticks for the departures, so this 4-tick schedule runs
% to 6. Programs that do not bind finalize on that rel pay nothing, because
% listened_departure_refs/2 gates the carry.
fixture(finalize_over_log_fires_on_retention_prune,
  prog([ kind(ev/2, log), keep(ev/2, count(2)),
         kind(gone/2, log), keep(gone/2, all) ],
       [ (gone(Ordinal, Payload) <+ finalize(ev(Ordinal, Payload))) ]),
  [],
  [ [ +ev(1, a) ], [ +ev(2, b) ], [ +ev(3, c) ], [ +ev(4, d) ] ],
  [ deltas(ev/2, [ [ +ev(1, a) ],
                   [ +ev(2, b) ],
                   [ -ev(1, a), +ev(3, c) ],
                   [ -ev(2, b), +ev(4, d) ],
                   [], [] ]),
    deltas(gone/2, [ [], [], [],
                     [ +gone(1, a) ],
                     [ +gone(2, b) ],
                     [] ]),
    final(ev/2, [ ev(3, c), ev(4, d) ]),
    final(gone/2, [ gone(1, a), gone(2, b) ]),
    ticks(6) ]).

% created_at and updated_at are ordinary columns two ordinary edge rules fill.
% Promoted from the time-plane lab (T15,
% plans/2026-07-30-time-plane-unification-verdict.md candidate 3), where it is
% the receipt that refutes the auto-metadata-plane hypothesis: the semantics
% the plane would add already ship, with zero new constructs, so an auto plane
% would cost 7.5 bytes/row on every rel to serve the rels that asked for it.
%
% now/1 supplies the tick, pre/1 carries the birth tick across a keyed
% replace, not/1 supplies the base case. The two rules are the two branches of
% one fold: in rx this is groupBy + scan, with pre/1 as the accumulator and
% not/1 as the seed.
%
% The graded value is thing(1, c, 1, 3): payload advanced to the third
% arrival, created pinned at tick 1, updated advanced to tick 3. The NAIVE
% one-rule spelling (drop the pre/1 rule) instead yields thing(1, c, 3, 3) --
% a column named created_at_tick holding updated_at semantics, silently. That
% trap is the honest argument for sugar later; this fixture is the oracle any
% such sugar has to match.
fixture(created_at_pinned_updated_at_advances,
  prog([ kind(arrive/2, log), keep(arrive/2, all),
         keyed(thing/4, [1]) ],
       [ (thing(Id, Payload, Born, Tick) <+
              arrive(Id, Payload), now(Tick),
              pre(thing(Id, _Old, Born, _Was))),
         (thing(Id, Payload, Tick, Tick) <+
              arrive(Id, Payload), now(Tick),
              not(thing(Id, _AnyPayload, _AnyBorn, _AnyUpdated))) ]),
  [],
  [ [ +arrive(1, a) ], [ +arrive(1, b) ], [ +arrive(1, c) ] ],
  [ final(thing/4, [ thing(1, c, 1, 3) ]) ]).

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

% An aggregate spelling neither door implements is a load error, not a value.
%
% group_concat is SQLite's, which is precisely why a cold author reaches for
% it, and this language has no such aggregate. With no registry row it was not
% a construct at all: the head argument fell through to generic compound
% rendering and stored ONE ROW PER INPUT holding the literal text of the call.
%
% FAIL-FIRST RECEIPT for this exact program, both doors, before the row and
% the aggregate_not_implemented class existed:
%
%   oracle    rows=[roster(group_concat(ada)), roster(group_concat(grace))]
%   compiler  COMPILED CLEAN, emitting
%             json_object('fn','group_concat','args',json_array(b0."col1"))
%
% Two rows of call text where the author asked for one joined row, and no
% error at either door. The refusal carries the aggregates that DO lower,
% read off the registry, because a refusal for a word the author reasonably
% expected has to say what to write instead. Both doors report the identical
% term; the compiler wraps it in unsupported_construct/1 as it wraps every
% refusal.
fixture(unimplemented_aggregate_head_rejected,
  prog([], [ (roster(group_concat(Name)) <- member_of(Name)) ]),
  [],
  [ [ +member_of(ada), +member_of(grace) ] ],
  [ throws(aggregate_not_implemented(roster/1, group_concat/1,
                                     [avg, count, max, min, sum])) ]).

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
