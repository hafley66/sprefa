% channel.pl : THREAD 3, channel = log + consumed(reader, ordinal) + watermark.
%
% N readers, M writers, modelled on the oracle. The three questions:
%   1. does the per-reader min-ordinal read compose out of shipped pieces
%   2. what does the low watermark buy
%   3. what EXACTLY can static keep(count(N)) not express
%
% .dl surface:
%
%   rel publish(payload: text) log keep(all)
%   rel chan(ordinal: int, payload: text) log keep(all)
%   rel wcount(channel: text, next: int) key(channel)
%   rel cursor(reader: text, last: int) key(reader)
%   rel active(reader: text)
%   rel delivered(reader: text, ordinal: int, payload: text) log keep(all)
%
%   wcount(channel, next) <+ publish(_), pre(wcount(channel, so_far)),
%                             next := so_far + 1.
%   chan(next, payload)   <+ publish(payload), pre(wcount(channel, so_far)),
%                             next := so_far + 1.
%   next_for(reader, ordinal, payload) <- cursor(reader, last), active(reader),
%                                          ordinal := last + 1,
%                                          chan(ordinal, payload).
%   cursor(reader, ordinal)   <+ next_for(reader, ordinal, _).
%   delivered(reader, ordinal, payload) <+ next_for(reader, ordinal, payload).
%   watermark(channel, min(last)) <- cursor(_, last).
%
% rx lowering:
%   const chan$ = publish$.pipe(
%     scan((seq, payload) => ({ ordinal: seq.ordinal + 1, payload }),
%          { ordinal: 0, payload: null }),
%     shareReplay({ refCount: false }));
%   const readerOf = (reader, from) =>
%     chan$.pipe(filter(row => row.ordinal > from), concatMap(deliver(reader)));
%   const watermark$ = combineLatest(cursors).pipe(map(all => Math.min(...all)));
%   -- shareReplay IS the log with keep(all); its buffer bound is the
%   retention question, and rxjs offers exactly the same two answers the
%   language does today: a static bufferSize, or unbounded. Nothing in rxjs
%   prunes a replay buffer against a consumer cursor either.

:- module(ca_channel, [ channel_scenario/2, retention_option/4, retention_slot/1 ]).

:- use_module(library(lists)).
:- use_module(oracle).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous channel_scenario/2.

channel_prog(Bound,
  prog([ kind(publish/1, log), keep(publish/1, all),
         kind(chan/2, log), keep(chan/2, Bound),
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
         (watermark(c, min(Last)) <- cursor(_, Last)) ])).

two_readers_one_late([ wcount(c, 0), cursor(reader_one, 0), cursor(reader_two, 0),
                       active(reader_one) ]).

% two writers publish in ONE tick, a third publishes in the next; reader_two
% wakes at tick 4.
late_reader_schedule([ [ +publish(m1), +publish(m2) ],
                       [ +publish(m3) ],
                       [],
                       [ +active(reader_two) ],
                       [], [], [] ]).

% ═══ ROUND 1 : the channel composes out of shipped pieces ══════════════════

% M writers in ONE tick get distinct ordinals, same pre/1 chaining as the
% queue. No writer coordination construct exists or is needed.
channel_scenario(r1_two_writers_in_one_tick_get_distinct_ordinals, Goal) :-
    channel_prog(all, Prog), two_readers_one_late(Initial),
    Goal = ( oracle_log_final(Prog, Initial, [[ +publish(m1), +publish(m2) ]], Final, _),
             final_has(Final, chan(1, m1)), final_has(Final, chan(2, m2)) ).

% N readers advance independently, one ordinal per tick each, and the low
% watermark is an ordinary min aggregate over the cursor rel.
channel_scenario(r1_two_readers_advance_independently_and_the_watermark_follows, Goal) :-
    channel_prog(all, Prog), two_readers_one_late(Initial),
    late_reader_schedule(Schedule),
    Goal = ( oracle_log_final(Prog, Initial, Schedule, Final, Log),
             length(Log, 7),
             watermark_track(Log, Track), Track == [0, 1, 2, 3],
             final_has(Final, cursor(reader_one, 3)),
             final_has(Final, cursor(reader_two, 3)),
             final_has(Final, watermark(c, 3)) ).

% The watermark exists at boot (min over the seeded cursors) and only its
% CHANGES appear as deltas, so the track is read as boot value plus every
% +watermark row in order.
watermark_track(Log, [0 | Rest]) :-
    findall(Value, ( member(Deltas, Log), member(+watermark(c, Value), Deltas) ), Rest).

% A reader that wakes up late catches up from its own cursor, not from the
% newest row. Ordinal 1 is still there because keep(all) kept it.
channel_scenario(r1_a_late_reader_catches_up_from_its_own_cursor, Goal) :-
    channel_prog(all, Prog), two_readers_one_late(Initial),
    late_reader_schedule(Schedule),
    Goal = ( oracle_log_final(Prog, Initial, Schedule, Final, _),
             final_has(Final, delivered(reader_two, 1, m1)),
             final_has(Final, delivered(reader_two, 2, m2)),
             final_has(Final, delivered(reader_two, 3, m3)) ).

% ═══ ROUND 2 : what static keep(count(N)) cannot express ═══════════════════
% keep(count(N)) is a function of the LOG ALONE: newest N stamps, evaluated
% at tick end. The retention a channel needs is a function of a JOIN against
% the reader cursors. No static bound expresses a join, and the failure is
% not graceful.

% Same program, same schedule, keep(all) vs keep(count(2)): the LOGS ARE
% IDENTICAL up to the tick where the lagging reader would have read, and the
% prune emits no delta of any kind (match-frontier C4, reproduced here in a
% channel setting rather than a bare Log rel).
channel_scenario(r2_the_prune_is_invisible_in_the_tick_log, Goal) :-
    channel_prog(all, KeepAll), channel_prog(count(2), KeepTwo),
    Initial = [ wcount(c, 0), cursor(reader_one, 0), cursor(reader_two, 0),
                active(reader_one) ],
    Goal = ( oracle_log_final(KeepAll, Initial,
                              [ [ +publish(m1), +publish(m2) ], [ +publish(m3) ],
                                [], [], [] ], FinalAll, LogAll),
             oracle_log_final(KeepTwo, Initial,
                              [ [ +publish(m1), +publish(m2) ], [ +publish(m3) ],
                                [], [], [] ], FinalTwo, LogTwo),
             LogAll == LogTwo,
             final_has(FinalAll, chan(1, m1)),
             final_lacks(FinalTwo, chan(1, m1)) ).

% ROUND 2, the sharp half. With the same keep(count(2)) the late reader is
% permanently stalled: its cursor never moves, the watermark never moves, and
% the last three ticks of the run are empty. The system is quiescent and
% wrong, and the watermark that would have prevented the prune is sitting in
% the final state saying ordinal 1 is unread.
channel_scenario(r2_a_static_keep_count_permanently_stalls_the_lagging_reader, Goal) :-
    channel_prog(count(2), Prog), two_readers_one_late(Initial),
    late_reader_schedule(Schedule),
    Goal = ( oracle_log_final(Prog, Initial, Schedule, Final, Log),
             length(Log, 7),
             append(_, [[], [], []], Log),
             final_has(Final, cursor(reader_two, 0)),
             final_has(Final, watermark(c, 0)),
             final_has(Final, active(reader_two)),
             final_lacks(Final, delivered(reader_two, 1, m1)) ).

% The same run under keep(all) delivers every row to the late reader. The
% ONLY difference between the two programs is the retention bound.
channel_scenario(r2_the_same_program_under_keep_all_loses_nothing, Goal) :-
    channel_prog(all, Prog), two_readers_one_late(Initial),
    late_reader_schedule(Schedule),
    Goal = ( oracle_log_final(Prog, Initial, Schedule, Final, _),
             final_has(Final, cursor(reader_two, 3)),
             final_has(Final, watermark(c, 3)) ).

% ═══ ROUND 3 : the retention slot ══════════════════════════════════════════
% retention_option(Option, Spelling, Buys, Costs). No fiat: the four options
% are priced and the smallest honest one is named, not decreed.

retention_slot('SLOT-RETENTION-SPELLING').

retention_option(
    s1_retention_as_an_ordinary_rule,
    'finalize head over the log rel: chan(Ordinal, _) leaves when Ordinal =< watermark',
    [ 'zero new decl words',
      'the prune becomes a visible -delta, which closes the silent-prune crack in the same change',
      'the bound is an ordinary derived value, so any join expresses it',
      'tick-log-only grading can see retention for the first time' ],
    [ 'requires lifting engine.pl:196 retract_from_log, which today throws',
      'a retracting head is a new head kind for edge rules (none exists)',
      'stratification obligation: the retention read must not feed the log it prunes' ]).

retention_option(
    s2_decl_word_referencing_a_derived_column,
    'keep(until(watermark, last)) in the rel decl',
    [ 'no change to the log-is-append-only law',
      'the bound stays a decl, which is where every other retention bound lives' ],
    [ 'one new decl word',
      'the prune stays invisible, so the silent-prune crack survives',
      'a decl now names a rel, which makes decls order-dependent on rules' ]).

retention_option(
    s3_aggregate_expression_in_the_decl,
    'keep(min(cursor.last))',
    [ 'most general; any aggregate over any rel' ],
    [ 'puts an expression language inside decls, which nothing else needs',
      'same invisibility cost as s2',
      'the aggregate has to be recomputed at every tick end, off the rule plane' ]).

retention_option(
    s4_no_construct_program_owns_it,
    'store the channel as a keyed Set rel and delete rows with an ordinary rule',
    [ 'zero language change' ],
    [ 'loses stamps, so it loses duplicate occurrences and arrival order',
      'a Set rel cannot hold two identical payloads, which a channel must',
      'the program pays the whole bookkeeping the log already does' ]).

channel_scenario(r3_every_retention_option_is_priced_both_ways, Goal) :-
    Goal = forall(retention_option(Name, Spelling, Buys, Costs),
                  ( atom(Name), atom(Spelling),
                    length(Buys, BuyCount), BuyCount >= 1,
                    length(Costs, CostCount), CostCount >= 1 )).

% The receipt behind s1's headline claim: the log's append-only law is
% ALREADY violated by retention, just invisibly. keep(count(2)) removes a row
% the program can never see leave.
channel_scenario(r3_retention_already_removes_log_rows_without_a_delta, Goal) :-
    channel_prog(count(2), Prog),
    Goal = ( oracle_log_final(Prog, [ wcount(c, 0), cursor(reader_one, 0) ],
                              [ [ +publish(m1) ], [ +publish(m2) ], [ +publish(m3) ] ],
                              Final, Log),
             findall(Row, ( member(Deltas, Log), member(-Row, Deltas), Row = chan(_, _) ),
                     Removals),
             Removals == [],
             final_lacks(Final, chan(1, m1)) ).

% And the counter-receipt: an explicit retraction of a Log row throws today,
% which is exactly the law s1 has to lift.
channel_scenario(r3_explicit_log_retraction_throws_today, Goal) :-
    Goal = oracle_throws(
               prog([ kind(chan/2, log), keep(chan/2, all) ], []),
               [], [[ -chan(1, m1) ]], retract_from_log(chan/2)).
