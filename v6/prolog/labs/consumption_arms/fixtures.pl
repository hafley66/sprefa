% fixtures.pl : the three PROSPECTIVE fixture/5 terms (arms, queue, channel).
%
% They are user:fixture/5 clauses so the real conformance harness
% (engine:fixture_expectations_hold/2) grades them, and they live HERE, not
% in v6/prolog/conformance/fixtures/**. Promoting them is the coordinator's
% call, not this lab's.
%
% Every expected log was hand-computed from engine.pl before it was run.

:- module(ca_fixtures, [ prospective_fixture/1 ]).

:- use_module(library(lists)).
:- use_module('../../conformance/engine').

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- multifile user:fixture/5.
:- discontiguous user:fixture/5.

prospective_fixture(lifecycle_arms_on_demand_and_scope_rels).
prospective_fixture(queue_min_ordinal_drains_one_item_per_tick).
prospective_fixture(channel_two_readers_one_log_and_a_watermark).

% ═══ 1. ARMS ═══════════════════════════════════════════════════════════════
% subscribe / unsubscribe / complete written in the shipped kernel words, so
% the fixture grades the CLAIM that the three arms need no construct:
%   subscribe   = a bare trigger on the demand rel
%   unsubscribe = finalize on the demand rel
%   complete    = finalize on the live scope rel (open minus closed)
% The plus-side arm fires in the arrival tick; both minus-side arms fire one
% drain tick after their minus delta, which is the timing asymmetry the arm
% family carries.
user:fixture(lifecycle_arms_on_demand_and_scope_rels,
  prog([ kind(open_request/1, log), keep(open_request/1, all),
         kind(close_request/1, log), keep(close_request/1, all),
         kind(completed/1, log), keep(completed/1, all),
         kind(started/1, log), keep(started/1, all),
         kind(stopped/1, log), keep(stopped/1, all),
         kind(demand/1, set),
         keyed(open_scope/1, [1]), keyed(closed/1, [1]) ],
       [ (open_scope(Scope) <+ open_request(Scope)),
         (closed(Scope) <+ close_request(Scope)),
         (live_scope(Scope) <- open_scope(Scope), not(closed(Scope))),
         (started(Key) <+ demand(Key)),
         (stopped(Key) <+ finalize(demand(Key))),
         (completed(Scope) <+ finalize(live_scope(Scope))) ]),
  [],
  [ [ +open_request(s1), +demand(s1) ],
    [],
    [ +close_request(s1), -demand(s1) ] ],
  [ deltas(started/1,   [ [ +started(s1) ], [], [], [], [] ]),
    deltas(stopped/1,   [ [], [], [], [ +stopped(s1) ], [] ]),
    deltas(completed/1, [ [], [], [], [ +completed(s1) ], [] ]),
    deltas(live_scope/1, [ [ +live_scope(s1) ], [], [ -live_scope(s1) ], [], [] ]),
    final(completed/1, [ completed(s1) ]),
    final(stopped/1, [ stopped(s1) ]),
    ticks(5) ]).

% ═══ 2. QUEUE ══════════════════════════════════════════════════════════════
% Three requests arrive in ONE tick and drain ONE PER TICK. The ordinal is
% minted in the language by a keyed counter read through pre/1, which chains
% across occurrences inside the tick, so the three slots get 1, 2, 3.
user:fixture(queue_min_ordinal_drains_one_item_per_tick,
  prog([ kind(req/1, log), keep(req/1, all),
         kind(out/1, log), keep(out/1, all),
         keyed(counter/2, [1]),
         keyed(slot/3, [1, 2]),
         keyed(done/2, [1, 2]) ],
       [ (counter(q, Next) <+ req(_), pre(counter(q, SoFar)), Next := SoFar + 1),
         (slot(q, Next, Value) <+ req(Value), pre(counter(q, SoFar)), Next := SoFar + 1),
         (head(q, min(Ordinal)) <- slot(q, Ordinal, _), not(done(q, Ordinal))),
         (head_value(q, Ordinal, Value) <- head(q, Ordinal), slot(q, Ordinal, Value)),
         (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
         (out(Value) <+ head_value(q, _, Value)) ]),
  [ counter(q, 0) ],
  [ [ +req(a), +req(b), +req(c) ] ],
  [ deltas(out/1, [ [], [ +out(a) ], [ +out(b) ], [ +out(c) ], [] ]),
    deltas(head/2, [ [ +head(q, 1) ],
                     [ -head(q, 1), +head(q, 2) ],
                     [ -head(q, 2), +head(q, 3) ],
                     [ -head(q, 3) ],
                     [] ]),
    final(slot/3, [ slot(q, 1, a), slot(q, 2, b), slot(q, 3, c) ]),
    final(done/2, [ done(q, 1), done(q, 2), done(q, 3) ]),
    final(counter/2, [ counter(q, 3) ]),
    ticks(5) ]).

% ═══ 3. CHANNEL ════════════════════════════════════════════════════════════
% Two writers publish in one tick, a third publishes in the next, two readers
% share the log, and the second reader wakes at tick 4 and catches up from
% ITS OWN cursor rather than from the newest row. The low watermark is a min
% aggregate over the cursor rel and it climbs only as the slowest reader does.
user:fixture(channel_two_readers_one_log_and_a_watermark,
  prog([ kind(publish/1, log), keep(publish/1, all),
         kind(chan/2, log), keep(chan/2, all),
         kind(delivered/3, log), keep(delivered/3, all),
         kind(active/1, set),
         keyed(wcount/2, [1]),
         keyed(cursor/2, [1]) ],
       [ (wcount(c, Next) <+ publish(_), pre(wcount(c, SoFar)), Next := SoFar + 1),
         (chan(Next, Payload) <+ publish(Payload), pre(wcount(c, SoFar)), Next := SoFar + 1),
         (next_for(Reader, Ordinal, Payload) <-
              cursor(Reader, Last), active(Reader), Ordinal := Last + 1,
              chan(Ordinal, Payload)),
         (cursor(Reader, Ordinal) <+ next_for(Reader, Ordinal, _)),
         (delivered(Reader, Ordinal, Payload) <+ next_for(Reader, Ordinal, Payload)),
         (watermark(c, min(Last)) <- cursor(_, Last)) ]),
  [ wcount(c, 0), cursor(reader_one, 0), cursor(reader_two, 0), active(reader_one) ],
  [ [ +publish(m1), +publish(m2) ],
    [ +publish(m3) ],
    [],
    [ +active(reader_two) ],
    [], [], [] ],
  [ deltas(delivered/3,
      [ [],
        [ +delivered(reader_one, 1, m1) ],
        [ +delivered(reader_one, 2, m2) ],
        [ +delivered(reader_one, 3, m3), +delivered(reader_two, 1, m1) ],
        [ +delivered(reader_two, 2, m2) ],
        [ +delivered(reader_two, 3, m3) ],
        [] ]),
    deltas(watermark/2,
      [ [], [], [],
        [ -watermark(c, 0), +watermark(c, 1) ],
        [ -watermark(c, 1), +watermark(c, 2) ],
        [ -watermark(c, 2), +watermark(c, 3) ],
        [] ]),
    final(chan/2, [ chan(1, m1), chan(2, m2), chan(3, m3) ]),
    final(cursor/2, [ cursor(reader_one, 3), cursor(reader_two, 3) ]),
    final(watermark/2, [ watermark(c, 3) ]),
    ticks(7) ]).
