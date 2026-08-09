% 0_receipts.pl : rel-as-stream lab, reference-engine half.
%
% Question (lane/rel-as-stream): can a rel BE a stream with the mechanics that
% already exist, without inventing a second kind of thing that fights
% locked(single_rel_type_system)?
%
% Every receipt below runs the SHIPPED reference interpreter
% (conformance/engine.pl) over a program written only in constructs the
% registry already lists as live. No production file is edited, no construct is
% added, and nothing here is a mock: run_program/5 is the same predicate the
% 139-fixture conformance corpus is graded by.
%
% The compiled half lives in receipts.sh, which puts the same .dl6 text through
% both doors and diffs the tick logs. Prolog cannot reach the emitter's SQL
% without a server, so the split is by capability, not by convenience.
%
% Run:
%   swipl -q -l v6/prolog/labs/rel_as_stream/0_receipts.pl -g go -g halt

:- module(rel_as_stream_receipts, [go/0]).

:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ programs ═══════════════════════════════════════════════════════════════

% R1/R2. The build: a log rel carrying a surface ordinal minted by an ordinary
% keyed cursor read through pre/1. `cursor` is the state model the user objects
% to; `stream` is what the state model was hiding.
ordinal_stream(
    prog([ kind(event/2, log), keep(event/2, all),
           keyed(cursor/2, [1]),
           kind(stream/3, log), keep(stream/3, all) ],
         [ (cursor(Name, 1) <+ event(Name, _), not(cursor(Name, _))),
           (cursor(Name, Next) <+ event(Name, _), pre(cursor(Name, At)),
                                                  Next := At + 1),
           (stream(Name, 1, Payload) <+ event(Name, Payload), not(cursor(Name, _))),
           (stream(Name, Next, Payload) <+ event(Name, Payload),
                                           pre(cursor(Name, At)), Next := At + 1) ])).

% R4. Two independent source rels folded into ONE totally ordered stream by
% sharing one cursor. This is rx `merge` over sources, not `mergeMap` over
% inner subscriptions; the flattening family belongs to lane/teardown-flatten.
merged_stream(
    prog([ kind(tick_in/1, log), keep(tick_in/1, all),
           kind(key_in/1, log),  keep(key_in/1, all),
           keyed(cursor/2, [1]),
           kind(merged/3, log), keep(merged/3, all) ],
         [ (cursor(global, 1) <+ tick_in(_), not(cursor(global, _))),
           (cursor(global, 1) <+ key_in(_),  not(cursor(global, _))),
           (cursor(global, Next) <+ tick_in(_), pre(cursor(global, At)), Next := At + 1),
           (cursor(global, Next) <+ key_in(_),  pre(cursor(global, At)), Next := At + 1),
           (merged(1, 'tick', Payload) <+ tick_in(Payload), not(cursor(global, _))),
           (merged(1, 'key',  Payload) <+ key_in(Payload),  not(cursor(global, _))),
           (merged(Next, 'tick', Payload) <+ tick_in(Payload),
                                             pre(cursor(global, At)), Next := At + 1),
           (merged(Next, 'key',  Payload) <+ key_in(Payload),
                                             pre(cursor(global, At)), Next := At + 1) ])).

% R5. N readers over one log at independent positions. The per-reader view is a
% LEVEL rel, never a log rel; R6 is the receipt for why it has to be.
n_readers(
    prog([ kind(stream/2, log), keep(stream/2, all),
           keyed(read_at/2, [1]) ],
         [ (pending(Reader, Ordinal, Payload) <-
                read_at(Reader, Cursor), stream(Ordinal, Payload), Ordinal > Cursor),
           (next_for(Reader, min(Ordinal)) <- pending(Reader, Ordinal, _)) ])).

% R6. The same projection declared as a log rel.
n_readers_as_log(
    prog([ kind(stream/2, log), keep(stream/2, all),
           keyed(read_at/2, [1]),
           kind(pending/3, log), keep(pending/3, all) ],
         [ (pending(Reader, Ordinal, Payload) <-
                read_at(Reader, Cursor), stream(Ordinal, Payload), Ordinal > Cursor) ])).

% R7. A program that tries to un-happen an occurrence.
retract_log(prog([ kind(ev/1, log), keep(ev/1, all) ], [])).

% R8. The watermark a reader-driven retention policy would need: computable as
% an ordinary derived rel, with no way to act on it.
watermark(
    prog([ kind(stream/2, log), keep(stream/2, all),
           keyed(read_at/2, [1]) ],
         [ (watermark(min(Cursor)) <- read_at(_, Cursor)),
           (retired(Ordinal) <- stream(Ordinal, _), watermark(Mark), Ordinal =< Mark) ])).

% R9. now/1 in an edge body.
tick_stamp(
    prog([ kind(ev/1, log), keep(ev/1, all),
           kind(stamped/2, log), keep(stamped/2, all) ],
         [ (stamped(Payload, Tick) <+ ev(Payload), now(Tick)) ])).

% R10. Identical rows into a log rel.
stacking(prog([ kind(ev/1, log), keep(ev/1, all) ], [])).

% R11. Arrival order inside one tick.
order_witness(
    prog([ kind(ev/1, log), keep(ev/1, all),
           keyed(cursor/2, [1]),
           kind(seq/2, log), keep(seq/2, all) ],
         [ (cursor(global, 1) <+ ev(_), not(cursor(global, _))),
           (cursor(global, Next) <+ ev(_), pre(cursor(global, At)), Next := At + 1),
           (seq(1, Payload) <+ ev(Payload), not(cursor(global, _))),
           (seq(Next, Payload) <+ ev(Payload), pre(cursor(global, At)), Next := At + 1) ])).

% R12. finalize/1 over a log rel whose rows retention removes.
finalize_over_log(
    prog([ kind(ev/2, log), keep(ev/2, count(2)),
           kind(evicted/2, log), keep(evicted/2, all) ],
         [ (evicted(Ordinal, Payload) <+ finalize(ev(Ordinal, Payload))) ])).

% ═══ helpers ════════════════════════════════════════════════════════════════

run_deltas(Program, Initial, Schedule, Ref, Deltas) :-
    run_program(Program, Initial, Schedule, _, DeltaTicks),
    rel_deltas(Ref, DeltaTicks, Deltas).

run_final(Program, Initial, Schedule, Ref, Rows) :-
    run_program(Program, Initial, Schedule, Final, _),
    rel_rows(Ref, Final, Rows).

refuses(Program, Initial, Schedule, Refusal) :-
    catch(( run_program(Program, Initial, Schedule, _, _), fail ), Refusal, true).

three_ticks([ [ +event(clicks, a) ],
              [ +event(clicks, b), +event(clicks, c) ],
              [ +event(clicks, d) ] ]).

% ═══ receipts ═══════════════════════════════════════════════════════════════

% R1. The sequence becomes data. Every occurrence gets its own ordinal,
% including the two that arrive inside ONE tick.
ordinal_is_total_across_and_within_ticks :-
    ordinal_stream(Program), three_ticks(Schedule),
    run_final(Program, [], Schedule, stream/3, Rows),
    Rows == [ stream(clicks, 1, a), stream(clicks, 2, b),
              stream(clicks, 3, c), stream(clicks, 4, d) ],
    format("PASS log rel plus a pre-minted ordinal is a total occurrence order~n").

% R2. THE POINT. Same program, same tick, two rels. The keyed state rel reports
% 1 -> 3 and the intermediate 2 is unobservable; the log rel reports 2 and 3 as
% separate rows. The user's objection to scan is that the intermediates vanish;
% publishing the sequence is what makes them stop vanishing, and it needs no
% construct that is not already live.
state_collapses_where_the_stream_does_not :-
    ordinal_stream(Program), three_ticks(Schedule),
    run_deltas(Program, [], Schedule, cursor/2, CursorDeltas),
    run_deltas(Program, [], Schedule, stream/3, StreamDeltas),
    nth1(2, CursorDeltas, CursorSecond),
    nth1(2, StreamDeltas, StreamSecond),
    CursorSecond == [ -cursor(clicks, 1), +cursor(clicks, 3) ],
    StreamSecond == [ +stream(clicks, 2, b), +stream(clicks, 3, c) ],
    format("PASS the collapsed intermediate is a row on the log rel and no row on the keyed rel~n").

% R3. The state model is NOT removed by this build. It is one keyed row, and it
% is still authored by hand. Recording this so the lab does not overclaim.
state_model_is_still_present :-
    ordinal_stream(prog(Decls, Rules)),
    memberchk(keyed(cursor/2, [1]), Decls),
    findall(Rule, ( member(Rule, Rules), Rule = (Head <+ _), functor(Head, cursor, 2) ),
            CursorRules),
    length(CursorRules, 2),
    format("PASS the ordinal costs one keyed rel and two rules; the state model moved, it did not leave~n").

% R4. rx `merge` over sources: one cursor, two producers, one total order.
merge_is_one_cursor_over_many_producers :-
    merged_stream(Program),
    run_final(Program, [],
              [ [ +tick_in(t1) ], [ +key_in(k1), +tick_in(t2) ], [ +key_in(k2) ] ],
              merged/3, Rows),
    Rows == [ merged(1, tick, t1), merged(2, key, k1),
              merged(3, tick, t2), merged(4, key, k2) ],
    format("PASS two source rels merge into one ordered stream under one shared cursor~n").

% R5. Channel with N readers: consumption-arms verdict section on log + keyed
% cursor + min, re-run here over an explicit ordinal instead of a stamp.
n_readers_hold_independent_positions :-
    n_readers(Program),
    run_final(Program,
              [ read_at(fast, 2), read_at(slow, 0),
                stream(1, a), stream(2, b), stream(3, c) ],
              [ [ +read_at(slow, 1) ] ],
              next_for/2, Rows),
    Rows == [ next_for(fast, 3), next_for(slow, 2) ],
    format("PASS one log serves N readers at independent cursors with min(ordinal)~n").

% R6. And the per-reader projection cannot itself be a stream: a level rule
% heading a log rel is a named unsupported construct (TICK-MODEL.md theorem four). So a
% consumer's view of a stream is a TABLE, always. This is the sharpest evidence
% that stream and table are not two kinds of thing.
a_readers_view_of_a_stream_is_a_table :-
    n_readers_as_log(Program),
    refuses(Program, [ read_at(fast, 0), stream(1, a) ], [ [] ], Refusal),
    Refusal == log_on_level_headed_rel(pending/3),
    format("PASS a derived view of a log rel must be a level rel: ~q~n", [Refusal]).

% R7. Occurrences cannot un-happen. The unsupported construct is correct and it is the reason
% the N plane carries no minus.
occurrences_cannot_un_happen :-
    retract_log(Program),
    refuses(Program, [], [ [ +ev(a) ], [ -ev(a) ] ], Refusal),
    Refusal == retract_from_log(ev/1),
    format("PASS a world retraction against a log rel is refused: ~q~n", [Refusal]).

% R8. The named gap from plans/2026-07-28-consumption-arms-verdict.md, now with
% a receipt on both halves: the retirement predicate is an ordinary derived rel
% and computes correctly; the log it names is untouched, because keep/2 takes a
% literal and no surface names a rel as the policy.
watermark_computes_and_cannot_act :-
    watermark(Program),
    Initial = [ read_at(fast, 2), read_at(slow, 1),
                stream(1, a), stream(2, b), stream(3, c) ],
    run_final(Program, Initial, [ [] ], retired/1, Retired),
    Retired == [ retired(1) ],
    run_final(Program, Initial, [ [] ], stream/2, Stream),
    Stream == [ stream(1, a), stream(2, b), stream(3, c) ],
    format("PASS a reader-driven watermark is derivable and inert: retired(1) computed, stream(1,a) still stored~n").

% R9. now/1 is the tick counter, so it is a PARTIAL order: two occurrences in
% one tick carry the same stamp. This is why the cursor exists at all.
now_is_the_tick_not_an_ordinal :-
    tick_stamp(Program),
    run_final(Program, [], [ [ +ev(a), +ev(b) ], [ +ev(c) ] ], stamped/2, Rows),
    Rows == [ stamped(a, 1), stamped(b, 1), stamped(c, 2) ],
    format("PASS now() ties within a tick, so it orders ticks and never occurrences~n").

% R10. A log rel is already the N plane: identical rows stack as separate
% occurrences. The ordinal buys ORDER, never distinctness.
identical_rows_stack_without_an_ordinal :-
    stacking(Program),
    run_final(Program, [], [ [ +ev(a), +ev(a) ], [ +ev(a) ] ], ev/1, Rows),
    Rows == [ ev(a), ev(a), ev(a) ],
    format("PASS identical occurrences already stack; the ordinal adds order, not identity~n").

% R11. The ordinal is a faithful record of arrival order, so the sequence a
% consumer reads is the sequence the world produced.
ordinal_follows_arrival_order :-
    order_witness(Program),
    run_final(Program, [], [ [ +ev(x), +ev(y) ] ], seq/2, Forward),
    run_final(Program, [], [ [ +ev(y), +ev(x) ] ], seq/2, Backward),
    Forward  == [ seq(1, x), seq(2, y) ],
    Backward == [ seq(1, y), seq(2, x) ],
    format("PASS the minted ordinal records arrival order inside one tick~n").

% R12. Retention removes rows and finalize/1 over the log never fires, because
% there is no minus on the N plane for it to bind. Two rows are gone from the
% store and `evicted` is empty across every tick. SLOT-LOG-FINALIZE-REFUSAL
% (update-arm verdict U5) called this silently dead; this is the receipt, and
% receipts.sh case (d) shows the one-line workaround.
finalize_over_a_log_never_fires :-
    finalize_over_log(Program),
    Schedule = [ [ +ev(1, a) ], [ +ev(2, b) ], [ +ev(3, c) ] ],
    run_final(Program, [], Schedule, ev/2, Kept),
    Kept == [ ev(2, b), ev(3, c) ],
    run_final(Program, [], Schedule, evicted/2, Evicted),
    Evicted == [],
    format("PASS retention dropped ev(1,a) and finalize over the log fired nothing~n").

% ═══ entry ══════════════════════════════════════════════════════════════════

receipt(ordinal_is_total_across_and_within_ticks).
receipt(state_collapses_where_the_stream_does_not).
receipt(state_model_is_still_present).
receipt(merge_is_one_cursor_over_many_producers).
receipt(n_readers_hold_independent_positions).
receipt(a_readers_view_of_a_stream_is_a_table).
receipt(occurrences_cannot_un_happen).
receipt(watermark_computes_and_cannot_act).
receipt(now_is_the_tick_not_an_ordinal).
receipt(identical_rows_stack_without_an_ordinal).
receipt(ordinal_follows_arrival_order).
receipt(finalize_over_a_log_never_fires).

go :-
    findall(Name, receipt(Name), Names),
    foldl(run_one, Names, 0-0, Passed-Failed),
    format("~w PASS ~w FAIL~n", [Passed, Failed]),
    ( Failed =:= 0 -> true ; halt(1) ).

run_one(Name, Passed0-Failed0, Passed-Failed) :-
    (   catch(call(Name), Error, (format("FAIL ~w threw ~q~n", [Name, Error]), fail))
    ->  Passed is Passed0 + 1, Failed = Failed0
    ;   format("FAIL ~w~n", [Name]),
        Passed = Passed0, Failed is Failed0 + 1 ).
