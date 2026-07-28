% consume.pl : THREAD 2, the consumption axis.
%
%   switch = boundary read plus collapse logging (rulings.pl
%            transition_rule_semantics).
%   queue  = durable pending rel plus min-ordinal consume, the shape already
%            shipped in fixtures/scopes.pl:146 (concat_program_queue).
%
% The PACING sub-choice is graded BOTH ways with real tick logs:
%   (a) every queued item fires in ONE tick, ordered by ordinal
%   (b) one item per drain tick
%
% Every log below was hand-computed from engine.pl before it was run.
% Where the run surprised the author the scenario comment says so.

:- module(ca_consume, [ consume_scenario/2, pacing_log/2, pacing_note/3,
                        consume_slot/2 ]).

:- use_module(library(lists)).
:- use_module(oracle).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous consume_scenario/2.

% ═══ the two spellings ══════════════════════════════════════════════════════
%
% .dl surface, switch:
%
%   rel poll(key: text, value: text) log keep(all)
%   rel latest(key: text, value: text) key(key)
%   latest(key, value) <+ poll(key, value).
%
% rx lowering (switch):
%   poll$.pipe(
%     groupBy(row => row.key),
%     mergeMap(group => group.pipe(switchMap(row => of(row)))),
%     distinctUntilChanged((left, right) => sameRow(left, right)))
%   -- switchMap on the inner group IS the keyed replace: the previous inner
%   value is dropped the instant a newer one for that key arrives.
%
% .dl surface, queue:
%
%   rel req(value: text) log keep(all)
%   rel counter(queue: text, next: int) key(queue)
%   rel slot(queue: text, ordinal: int, value: text) key(queue, ordinal)
%   rel done(queue: text, ordinal: int) key(queue, ordinal)
%   counter(queue, next) <+ req(value), pre(counter(queue, so_far)),
%                            next := so_far + 1.
%   slot(queue, next, value) <+ req(value), pre(counter(queue, so_far)),
%                                next := so_far + 1.
%   head(queue, min(ordinal)) <- slot(queue, ordinal, _),
%                                not(done(queue, ordinal)).
%   head_value(queue, ordinal, value) <- head(queue, ordinal),
%                                        slot(queue, ordinal, value).
%   done(queue, ordinal) <+ head_value(queue, ordinal, _).
%   out(value) <+ head_value(queue, _, value).
%
% rx lowering (queue, pacing b):
%   req$.pipe(
%     scan((ordinal, value) => ({ ordinal: ordinal.ordinal + 1, value }),
%          { ordinal: 0, value: null }),
%     concatMap(slot => consumeOne(slot)))
%   -- concatMap IS the min-ordinal drain: it subscribes to the next inner
%   only after the previous inner completes, which is exactly one item per
%   settled boundary. Pacing (a) is the same pipeline with mergeMap.

queue_decls([ kind(req/1, log), keep(req/1, all),
              kind(out/1, log), keep(out/1, all),
              keyed(counter/2, [1]),
              keyed(slot/3, [1, 2]),
              keyed(done/2, [1, 2]) ]).

% the two rules that mint the ordinal. pre/1 reads the EVOLVING pre-state
% (engine.pl tick note 4), so a second occurrence in the same tick sees the
% first occurrence's counter write and mints a DISTINCT ordinal.
enqueue_rules([ (counter(q, Next) <+ req(_), pre(counter(q, SoFar)), Next := SoFar + 1),
                (slot(q, Next, Value) <+ req(Value), pre(counter(q, SoFar)),
                   Next := SoFar + 1) ]).

% pacing (b): the min-ordinal head, one item per drain tick.
queue_pacing_b(prog(Decls, Rules)) :-
    queue_decls(Decls), enqueue_rules(Enqueue),
    append(Enqueue,
           [ (head(q, min(Ordinal)) <- slot(q, Ordinal, _), not(done(q, Ordinal))),
             (head_value(q, Ordinal, Value) <- head(q, Ordinal), slot(q, Ordinal, Value)),
             (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
             (out(Value) <+ head_value(q, _, Value)) ],
           Rules).

% pacing (a): every undrained slot is ready at once.
queue_pacing_a(prog(Decls, Rules)) :-
    queue_decls(Decls), enqueue_rules(Enqueue),
    append(Enqueue,
           [ (head_value(q, Ordinal, Value) <- slot(q, Ordinal, Value), not(done(q, Ordinal))),
             (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
             (out(Value) <+ head_value(q, _, Value)) ],
           Rules).

% both pacings again, this time with a KEYED downstream consumer (seen/2) in
% place of the Log rel out/1. This is where the two pacings stop agreeing.
queue_pacing_b_keyed(prog(Decls, Rules)) :-
    queue_decls(Decls0), select(kind(out/1, log), Decls0, Decls1),
    select(keep(out/1, all), Decls1, Decls2), Decls = [keyed(seen/2, [1]) | Decls2],
    enqueue_rules(Enqueue),
    append(Enqueue,
           [ (head(q, min(Ordinal)) <- slot(q, Ordinal, _), not(done(q, Ordinal))),
             (head_value(q, Ordinal, Value) <- head(q, Ordinal), slot(q, Ordinal, Value)),
             (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
             (seen(q, Value) <+ head_value(q, _, Value)) ],
           Rules).

queue_pacing_a_keyed(prog(Decls, Rules)) :-
    queue_decls(Decls0), select(kind(out/1, log), Decls0, Decls1),
    select(keep(out/1, all), Decls1, Decls2), Decls = [keyed(seen/2, [1]) | Decls2],
    enqueue_rules(Enqueue),
    append(Enqueue,
           [ (head_value(q, Ordinal, Value) <- slot(q, Ordinal, Value), not(done(q, Ordinal))),
             (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
             (seen(q, Value) <+ head_value(q, _, Value)) ],
           Rules).

% pacing (a) with the PAYLOAD column ahead of the ordinal column in the ready
% view. Same program shape, same data, one column swap.
queue_pacing_a_payload_first(prog(Decls, Rules)) :-
    queue_decls(Decls0), select(kind(out/1, log), Decls0, Decls1),
    select(keep(out/1, all), Decls1, Decls2), Decls = [keyed(seen/2, [1]) | Decls2],
    enqueue_rules(Enqueue),
    append(Enqueue,
           [ (ready(q, Value, Ordinal) <- slot(q, Ordinal, Value), not(done(q, Ordinal))),
             (done(q, Ordinal) <+ ready(q, _, Ordinal)),
             (seen(q, Value) <+ ready(q, Value, _)) ],
           Rules).

three_reqs([[ +req(a), +req(b), +req(c) ]]).

% ═══ ROUND 1 : the axis is a DECL choice, not a construct ═══════════════════

% Switch: two polls for one key in one tick leave ONE row. The consumption
% policy is spelled by key(key) and nothing else.
consume_scenario(r1_switch_is_the_key_declaration, Goal) :-
    Goal = ( oracle_log_final(
                 prog([ kind(poll/2, log), keep(poll/2, all), keyed(latest/2, [1]) ],
                      [ (latest(Key, Value) <+ poll(Key, Value)) ]),
                 [latest(cli, v0)], [[ +poll(cli, v1), +poll(cli, v2) ]], Final, Log),
             Log == [ [ -latest(cli, v0), +latest(cli, v2), +poll(cli, v1), +poll(cli, v2) ],
                      [] ],
             final_has(Final, latest(cli, v2)),
             final_lacks(Final, latest(cli, v1)) ).

% Queue: the same two arrivals under key(queue, ordinal) leave BOTH rows,
% and the ordinal is minted in-language by a keyed counter read through
% pre/1. No new construct on either side of the axis.
consume_scenario(r1_queue_is_the_ordinal_key_declaration, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)], [[ +req(a), +req(b) ]], Final, _),
             final_has(Final, slot(q, 1, a)),
             final_has(Final, slot(q, 2, b)),
             final_has(Final, counter(q, 2)) ).

% The ordinal question, graded on its own: TWO pushes inside ONE tick get
% DISTINCT ordinals. pre/1 chains across occurrences, so no stamp exposure
% and no engine-minted sequence column is needed.
consume_scenario(r1_ordinal_mints_in_language_across_one_tick, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)],
                              [[ +req(a), +req(b), +req(c) ]], Final, _),
             findall(Ordinal, member(slot(q, Ordinal, _), Final), Ordinals),
             msort(Ordinals, Sorted), Sorted == [1, 2, 3] ).

% ═══ ROUND 1 : the two pacing logs ══════════════════════════════════════════

pacing_log(pacing_a, Log) :- queue_pacing_a(Prog), three_reqs(Schedule),
    oracle_log(Prog, [counter(q, 0)], Schedule, Log).
pacing_log(pacing_b, Log) :- queue_pacing_b(Prog), three_reqs(Schedule),
    oracle_log(Prog, [counter(q, 0)], Schedule, Log).
pacing_log(pacing_a_keyed, Log) :- queue_pacing_a_keyed(Prog), three_reqs(Schedule),
    oracle_log(Prog, [counter(q, 0)], Schedule, Log).
pacing_log(pacing_b_keyed, Log) :- queue_pacing_b_keyed(Prog), three_reqs(Schedule),
    oracle_log(Prog, [counter(q, 0)], Schedule, Log).

pacing_note(pacing_a, ticks, 3).
pacing_note(pacing_b, ticks, 5).
pacing_note(pacing_a, items_visible_at_a_keyed_consumer, 1).
pacing_note(pacing_b, items_visible_at_a_keyed_consumer, 3).

consume_scenario(r1_pacing_a_lands_every_item_in_one_tick, Goal) :-
    Goal = ( pacing_log(pacing_a, Log),
             Log == [ [ -counter(q, 0), +counter(q, 3),
                        +head_value(q, 1, a), +head_value(q, 2, b), +head_value(q, 3, c),
                        +slot(q, 1, a), +slot(q, 2, b), +slot(q, 3, c),
                        +req(a), +req(b), +req(c) ],
                      [ -head_value(q, 1, a), -head_value(q, 2, b), -head_value(q, 3, c),
                        +done(q, 1), +done(q, 2), +done(q, 3),
                        +out(a), +out(b), +out(c) ],
                      [] ] ).

consume_scenario(r1_pacing_b_lands_one_item_per_drain_tick, Goal) :-
    Goal = ( pacing_log(pacing_b, Log),
             Log == [ [ -counter(q, 0), +counter(q, 3), +head(q, 1), +head_value(q, 1, a),
                        +slot(q, 1, a), +slot(q, 2, b), +slot(q, 3, c),
                        +req(a), +req(b), +req(c) ],
                      [ -head(q, 1), -head_value(q, 1, a), +done(q, 1),
                        +head(q, 2), +head_value(q, 2, b), +out(a) ],
                      [ -head(q, 2), -head_value(q, 2, b), +done(q, 2),
                        +head(q, 3), +head_value(q, 3, c), +out(b) ],
                      [ -head(q, 3), -head_value(q, 3, c), +done(q, 3), +out(c) ],
                      [] ] ).

% Into a LOG consumer the two pacings deliver the same three rows. The whole
% difference lives in the tick INDEX, which is why the pacing question cannot
% be settled by looking at out/1 alone.
consume_scenario(r1_both_pacings_deliver_the_same_rows_into_a_log_consumer, Goal) :-
    Goal = ( pacing_log(pacing_a, LogA), pacing_log(pacing_b, LogB),
             out_rows(LogA, RowsA), out_rows(LogB, RowsB),
             RowsA == [out(a), out(b), out(c)], RowsA == RowsB ).

out_rows(Log, Rows) :-
    findall(Row, ( member(Deltas, Log), member(+Row, Deltas), Row = out(_) ), Rows0),
    msort(Rows0, Rows).

% ═══ ROUND 2 : pacing (a) loses items at a keyed consumer ═══════════════════
% This is the round-2 break of the round-1 assertion "the two pacings differ
% only in the tick index". They differ in the DATA the moment the consumer
% has a key, because three writes to one key inside one tick fold to the last
% one and the boundary shows only that.

consume_scenario(r2_pacing_a_loses_two_of_three_items_at_a_keyed_consumer, Goal) :-
    queue_pacing_a_keyed(Prog), three_reqs(Schedule),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)], Schedule, Final, Log),
             seen_rows(Log, Seen), Seen == [seen(q, c)],
             final_has(Final, seen(q, c)),
             final_lacks(Final, seen(q, a)), final_lacks(Final, seen(q, b)) ).

consume_scenario(r2_pacing_b_keeps_all_three_items_at_a_keyed_consumer, Goal) :-
    queue_pacing_b_keyed(Prog), three_reqs(Schedule),
    Goal = ( oracle_log(Prog, [counter(q, 0)], Schedule, Log),
             seen_rows(Log, Seen), Seen == [seen(q, a), seen(q, b), seen(q, c)] ).

seen_rows(Log, Rows) :-
    findall(Row, ( member(Deltas, Log), member(+Row, Deltas), Row = seen(_, _) ), Rows0),
    msort(Rows0, Rows).

% ROUND 2, the sharper half. Under pacing (a) the survivor is not even the
% LAST queued item: the within-tick fold order is the standard order of the
% ready view's TERMS, so moving the payload column ahead of the ordinal
% column changes which item wins. zulu is ordinal 1 and it beats mike and
% alpha purely on alphabetical order. Pacing (a) does not preserve FIFO at
% all; it preserves nothing the program stated.
consume_scenario(r2_pacing_a_survivor_is_decided_by_column_order_not_by_ordinal, Goal) :-
    queue_pacing_a_payload_first(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)],
                              [[ +req(zulu), +req(alpha), +req(mike) ]], Final, _),
             final_has(Final, seen(q, zulu)),
             final_has(Final, slot(q, 1, zulu)),
             final_lacks(Final, seen(q, mike)) ).

% ═══ ROUND 3 : the C7-sidestep claim, graded ═══════════════════════════════
% The claim under test: a durable pending rel sidesteps C7 (the Ti carry set
% is not durable, so a crash between drain ticks loses pending firings).
%
% It does not, on its own. The ROW survives the crash; the FIRING does not.

% Restart from the exact store the pacing-b run reached at the end of tick 1,
% with an empty schedule: the queue has three undrained slots and the run
% produces ZERO ticks. head/2 and head_value/3 are recomputed and present in
% the final state, so the demand is visible; no occurrence is ever minted for
% it, because run_program seeds PrevLevel from the boot level closure
% (engine.pl:348-351), which makes every boot-true level row already-seen.
consume_scenario(r3_crash_restart_stalls_with_the_queue_intact, Goal) :-
    queue_pacing_b(Prog), mid_run_store(Store),
    Goal = ( oracle_log_final(Prog, Store, [], Final, Log),
             Log == [],
             final_has(Final, slot(q, 1, a)),
             final_has(Final, head(q, 1)),
             final_has(Final, head_value(q, 1, a)),
             final_lacks(Final, out(a)) ).

mid_run_store([ counter(q, 3), slot(q, 1, a), slot(q, 2, b), slot(q, 3, c),
                req(a), req(b), req(c) ]).

% Re-delivering the durable rows as arrivals does NOT restart it either: a
% Set arrival already present in the store is not an occurrence
% (engine.pl:192-195). One empty tick, nothing consumed.
consume_scenario(r3_replaying_the_durable_rows_as_arrivals_also_stalls, Goal) :-
    queue_pacing_b(Prog), mid_run_store(Store),
    Goal = ( oracle_log_final(Prog, Store,
                              [[ +slot(q, 1, a), +slot(q, 2, b), +slot(q, 3, c) ]],
                              Final, Log),
             Log == [[]],
             final_lacks(Final, out(a)) ).

% The queue DOES resume when the rows genuinely arrive for the first time,
% which is the receipt that nothing is wrong with the queue itself: the gap
% is the boot occurrence, not the rel.
consume_scenario(r3_a_genuinely_fresh_arrival_drains_the_whole_queue, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 3), req(a), req(b), req(c)],
                              [[ +slot(q, 1, a), +slot(q, 2, b), +slot(q, 3, c) ]],
                              Final, Log),
             length(Log, 4),
             final_has(Final, out(a)), final_has(Final, out(b)), final_has(Final, out(c)) ).

% ═══ ROUND 3 : pacing (b) turns the drain cap into a queue length cap ══════
% engine.pl:79 caps consecutive drain ticks at 100, and SLOT-SPILL is ruled
% error-at-cap-never-spill. Under pacing (b) each queued item costs one drain
% tick, so the cap becomes a data-dependent limit on queue depth. 99 items
% pass; 100 throws. Under pacing (a) the whole queue costs one drain tick and
% the cap is never approached.

consume_scenario(r3_pacing_b_drain_count_is_queue_length_plus_two, Goal) :-
    Goal = forall(member(Length-Ticks, [1-3, 2-4, 3-5, 4-6, 5-7]),
                  ( queue_run_length(pacing_b, Length, Actual), Actual == Ticks )).

consume_scenario(r3_pacing_a_drain_count_is_flat_in_queue_length, Goal) :-
    Goal = forall(member(Length, [1, 2, 3, 4, 5]),
                  ( queue_run_length(pacing_a, Length, Actual), Actual == 3 )).

consume_scenario(r3_pacing_b_of_ninety_nine_items_survives_the_cap, Goal) :-
    Goal = ( queue_run_length(pacing_b, 99, Ticks), Ticks == 101 ).

consume_scenario(r3_pacing_b_of_one_hundred_items_throws_drain_overflow, Goal) :-
    queue_pacing_b(Prog), req_batch(100, Batch),
    Goal = oracle_throws(Prog, [counter(q, 0)], [Batch], drain_overflow(100)).

consume_scenario(r3_pacing_a_of_one_hundred_items_does_not_throw, Goal) :-
    Goal = ( queue_run_length(pacing_a, 100, Ticks), Ticks == 3 ).

queue_run_length(Pacing, Length, Ticks) :-
    ( Pacing == pacing_b -> queue_pacing_b(Prog) ; queue_pacing_a(Prog) ),
    req_batch(Length, Batch),
    oracle_log(Prog, [counter(q, 0)], [Batch], Log),
    length(Log, Ticks).

req_batch(Length, Batch) :-
    numlist(1, Length, Values),
    findall(+req(Value), member(Value, Values), Batch).

% ═══ the two slots this thread opens ═══════════════════════════════════════

consume_slot('SLOT-QUEUE-PACING',
             'both pacings are expressible with zero new constructs, so the choice is not an expressiveness question. Pacing (a) reintroduces the collapse the queue existed to avoid at the first keyed consumer and picks a survivor by term order; pacing (b) preserves per-item observability and hard-fails at 100 queued items under the error-at-cap ruling. Picking (b) carries a live cost with an owner: the drain cap has to stop being a queue-length cap').

consume_slot('SLOT-BOOT-OCCURRENCE',
             'a durable queue does not resume after a crash because run_program seeds PrevLevel from the boot level closure, so no boot-true level row is ever an occurrence. Seeding it empty would resume every queue and re-fire every boot-true level row, which under content-addressed salts is a cache lookup rather than duplicate work, and which collides with the stated endurance goal of no boot replay of unanswered demand').

consume_scenario(r3_the_pacing_and_boot_occurrence_slots_are_both_named, Goal) :-
    Goal = ( consume_slot('SLOT-QUEUE-PACING', PacingWhy),
             sub_atom(PacingWhy, _, _, _, 'zero new constructs'),
             consume_slot('SLOT-BOOT-OCCURRENCE', BootWhy),
             sub_atom(BootWhy, _, _, _, 'PrevLevel'),
             findall(Slot, consume_slot(Slot, _), Slots),
             length(Slots, 2) ).
