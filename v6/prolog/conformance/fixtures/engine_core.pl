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

% A Log rel without a keep clause is a load error (q10: REQUIRED).
fixture(log_without_retention_rejected,
  prog([ kind(event/1, log) ], []),
  [],
  [ [ +event(one) ] ],
  [ throws(missing_retention(event/1)) ]).

% Log rels cannot be keyed (a keyed rel is a Set by construction).
fixture(keyed_log_rejected,
  prog([ kind(latest/2, log), keep(latest/2, all), keyed(latest/2, [1]) ], []),
  [],
  [],
  [ throws(keyed_log_rel(latest/2)) ]).

% An edge rule into an unkeyed Set rel is a type error (append into
% membership dedup is one of the two, pick with a declaration).
fixture(edge_into_unkeyed_set_rejected,
  prog([ kind(ping/1, log), keep(ping/1, all),
         kind(sink/1, set) ],
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
         (stage_two(Item) <+ only(stage_one(Item))) ]),
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
       [ (sent(Client, Item) <+ only(change_ev(Item)), subscriber(Client)) ]),
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
  prog([ kind(source_row/1, set) ],
       [ (mirror(Item) <- source_row(Item)) ]),
  [],
  [ [ +source_row(alpha), +source_row(beta) ],
    [ -source_row(alpha), -source_row(beta) ] ],
  [ deltas(mirror/1, [ [ +mirror(alpha), +mirror(beta) ],
                       [ -mirror(alpha), -mirror(beta) ] ]),
    final(mirror/1, []) ]).

% Set arrivals dedup (q2: identical content is the same thing) while Log
% arrivals stack: the same row delivered twice is one occurrence vs two.
fixture(set_dedups_log_stacks,
  prog([ kind(seen/1, set),
         kind(heard/1, log), keep(heard/1, all),
         kind(seen_count/1, log),  keep(seen_count/1, all),
         kind(heard_count/1, log), keep(heard_count/1, all) ],
       [ (seen_count(Item)  <+ seen(Item)),
         (heard_count(Item) <+ heard(Item)) ]),
  [],
  [ [ +seen(alpha), +seen(alpha), +heard(alpha), +heard(alpha) ] ],
  [ final(seen_count/1,  [ seen_count(alpha) ]),
    final(heard_count/1, [ heard_count(alpha), heard_count(alpha) ]) ]).
