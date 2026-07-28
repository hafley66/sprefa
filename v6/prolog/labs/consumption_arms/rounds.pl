% rounds.pl : the CLOSING ROUNDS, 4 through 7.
%
% Rounds 1 to 3 live inside the thread files (arms.pl, consume.pl,
% channel.pl, collapse.pl, desugar.pl) because their scenarios ARE the thread
% content. Rounds 4 onward are pure adversarial passes: every scenario here
% was written to break a numbered assertion, and each is named after what it
% attacks.
%
% The fixpoint closes when a round finds nothing. Rounds 4, 5 and 6 each
% found one thing. Round 7 found nothing.

:- module(ca_rounds, [ rounds_scenario/2 ]).

:- use_module(library(lists)).
:- use_module(oracle).
:- use_module(model).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous rounds_scenario/2.

queue_pacing_b(prog([ kind(req/1, log), keep(req/1, all),
                      kind(out/1, log), keep(out/1, all),
                      keyed(counter/2, [1]),
                      keyed(slot/3, [1, 2]),
                      keyed(done/2, [1, 2]) ],
                    [ (counter(q, Next) <+ req(_), pre(counter(q, SoFar)), Next := SoFar + 1),
                      (slot(q, Next, Value) <+ req(Value), pre(counter(q, SoFar)),
                         Next := SoFar + 1),
                      (head(q, min(Ordinal)) <- slot(q, Ordinal, _), not(done(q, Ordinal))),
                      (head_value(q, Ordinal, Value) <- head(q, Ordinal),
                         slot(q, Ordinal, Value)),
                      (done(q, Ordinal) <+ head_value(q, Ordinal, _)),
                      (out(Value) <+ head_value(q, _, Value)) ])).

% ═══ ROUND 4 ═══════════════════════════════════════════════════════════════
% Aim: empty and partial queues, a self-feeding channel, and a collapse on a
% drain tick rather than an arrival tick.
% Found: ONE thing, the trailing quiescence tick (assertion 28, amends 24).

% attacks assertion 3 at N equals zero: no spurious head row from a min
% aggregate over an empty undrained set, no error, one tick.
rounds_scenario(r4_an_empty_queue_settles_in_one_tick_with_no_head_row, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)], [[]], Final, Log),
             Log == [[]],
             \+ memberchk(head(q, _), Final),
             \+ memberchk(out(_), Final) ).

% attacks assertion 2: a queue that fully drains and is pushed again resumes
% at the next ordinal instead of replaying, because the counter is durable
% and the done rel keeps drained slots out of the head.
rounds_scenario(r4_a_drained_queue_resumes_at_the_next_ordinal, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog, [counter(q, 0)],
                              [[ +req(a) ], [], [], [ +req(b) ]], Final, Log),
             length(Log, 6),
             final_has(Final, slot(q, 1, a)), final_has(Final, slot(q, 2, b)),
             final_has(Final, out(a)), final_has(Final, out(b)),
             final_has(Final, counter(q, 2)) ).

% attacks assertion 6 in its sharpest form: a PARTIALLY drained queue. If
% restart resumed for partial queues the assertion would be wrong.
rounds_scenario(r4_a_partially_drained_queue_also_stalls_on_restart, Goal) :-
    queue_pacing_b(Prog),
    Goal = ( oracle_log_final(Prog,
                              [ counter(q, 3), slot(q, 1, a), slot(q, 2, b), slot(q, 3, c),
                                done(q, 1), out(a) ],
                              [], Final, Log),
             Log == [],
             final_has(Final, head(q, 2)),
             final_lacks(Final, out(b)) ).

% attacks assertion 16: a reader that consumes and republishes onto its own
% channel. The cursor arithmetic and the min aggregate both survive a channel
% that grows while it is being read, and the run is bounded.
rounds_scenario(r4_a_reader_that_republishes_onto_its_own_channel_terminates, Goal) :-
    echo_prog(Prog),
    Goal = ( oracle_log_final(Prog,
                              [ wcount(c, 0), cursor(reader_one, 0), active(reader_one) ],
                              [[ +publish(m1) ]], Final, Log),
             length(Log, Ticks), Ticks =< 8,
             final_has(Final, chan(1, m1)),
             final_has(Final, chan(2, echo(m1))),
             final_lacks(Final, chan(3, echo(echo(m1)))) ).

echo_prog(prog([ kind(publish/1, log), keep(publish/1, all),
                 kind(chan/2, log), keep(chan/2, all),
                 kind(active/1, set),
                 keyed(wcount/2, [1]),
                 keyed(cursor/2, [1]) ],
               [ (wcount(c, Next) <+ publish(_), pre(wcount(c, SoFar)), Next := SoFar + 1),
                 (chan(Next, Payload) <+ publish(Payload), pre(wcount(c, SoFar)),
                    Next := SoFar + 1),
                 (next_for(Reader, Ordinal, Payload) <-
                      cursor(Reader, Last), active(Reader), Ordinal := Last + 1,
                      chan(Ordinal, Payload)),
                 (cursor(Reader, Ordinal) <+ next_for(Reader, Ordinal, _)),
                 (wcount(c, Next) <+ next_for(_, 1, Payload), pre(wcount(c, SoFar)),
                    Next := SoFar + 1, Payload = m1),
                 (chan(Next, echo(Payload)) <+ next_for(_, 1, Payload),
                    pre(wcount(c, SoFar)), Next := SoFar + 1) ])).

% attacks assertions 20 and 21: a collapse on a DRAIN tick. Two carry
% occurrences write one key. Same site, one event, count 2.
rounds_scenario(r4_a_collapse_inside_a_drain_tick_mints_one_event_from_the_same_site, Goal) :-
    Goal = ( crun(cprog([ kind(src/1, log), keyed(mid/2, [1]), keyed(sink/2, [1]) ],
                        [ crule(arr(src(Value)), [], mid(Value, Value)),
                          crule(arr(mid(Value, _)), [], sink(k, Value)) ]),
                  [], [[ +src(a), +src(b) ]], 100, Log, Collapses),
             Collapses == [ collapse(2, sink/2, [k], 2, true) ],
             Log = [ line(1, _), line(2, _) | _ ] ).

% ROUND 4 FINDING. Attacks assertion 24 and breaks it. The level form and the
% edge form agree on every delta, and they do NOT agree on tick count: the
% edge write carries itself into one trailing quiescence tick that the level
% form never mints.
rounds_scenario(r4_the_edge_form_mints_one_extra_quiescence_tick, Goal) :-
    Goal = ( oracle_log(prog([ kind(src/1, set) ], [ (out(Item) <- src(Item)) ]),
                        [], [[ +src(a) ]], LevelLog),
             oracle_log(prog([ kind(src/1, set), keyed(out/1, [1]) ],
                             [ (out(Item) <+ src(Item)) ]),
                        [], [[ +src(a) ]], EdgeLog),
             LevelLog == [ [ +out(a), +src(a) ] ],
             EdgeLog  == [ [ +out(a), +src(a) ], [] ] ).

% and the follow-up that locates it: the extra tick comes from the edge WRITE
% carrying itself, not from the head kind. A level rel feeding an edge rule
% mints the same trailing tick.
rounds_scenario(r4_the_quiescence_tick_comes_from_the_edge_write_not_the_head_kind, Goal) :-
    Goal = ( oracle_log(prog([ kind(src/1, set), keyed(sink/1, [1]) ],
                             [ (out(Item) <- src(Item)), (sink(Item) <+ out(Item)) ]),
                        [], [[ +src(a) ]], Log),
             Log == [ [ +out(a), +sink(a), +src(a) ], [] ] ).

% ═══ ROUND 5 ═══════════════════════════════════════════════════════════════
% Aim: attack the ordinal minting with duplicates, the collapse count with a
% conflict, and retention with a bound smaller than one tick's batch.
% Found: ONE thing, the same-tick prune (assertion 26, amends 17).

% attacks assertion 2: two IDENTICAL Log arrivals in one tick. A Log rel mints
% an occurrence per arrival even for a duplicate row (engine.pl:186-189), so
% both get their own ordinal. The queue does not deduplicate, which is the
% behaviour a queue has to have.
rounds_scenario(r5_duplicate_log_pushes_in_one_tick_get_distinct_ordinals, Goal) :-
    Goal = ( oracle_log_final(prog([ kind(req/1, log), keep(req/1, all),
                                     keyed(counter/2, [1]), keyed(slot/3, [1, 2]) ],
                                   [ (counter(q, Next) <+ req(_), pre(counter(q, SoFar)),
                                        Next := SoFar + 1),
                                     (slot(q, Next, Value) <+ req(Value),
                                        pre(counter(q, SoFar)), Next := SoFar + 1) ]),
                              [counter(q, 0)], [[ +req(a), +req(a) ]], Final, _),
             final_has(Final, slot(q, 1, a)), final_has(Final, slot(q, 2, a)),
             final_has(Final, counter(q, 2)) ).

% attacks assertion 21: two DIFFERENT rules writing one key inside ONE
% occurrence is not a collapse, it is a refusal. The collapse count therefore
% only ever counts writes from distinct occurrences.
rounds_scenario(r5_two_rules_writing_one_key_in_one_occurrence_throw_keyed_conflict, Goal) :-
    Goal = oracle_throws(
               prog([ kind(poll/1, log), keep(poll/1, all), keyed(latest/2, [1]) ],
                    [ (latest(k, Value) <+ poll(Value)),
                      (latest(k, other) <+ poll(_)) ]),
               [], [[ +poll(v1) ]],
               keyed_conflict(latest/2, [k], [latest(k, other), latest(k, v1)])).

% ROUND 5 FINDING. Attacks assertion 17 and sharpens it past breaking. A Log
% row appended and pruned inside ONE tick carries no delta of ANY sign
% anywhere in the run: chan(1, m1) is written, retained out, and never
% appears in the tick log at all. Retention is not merely invisible, it can
% erase a row from the grading record entirely.
rounds_scenario(r5_a_row_appended_and_pruned_in_one_tick_has_no_delta_of_any_sign, Goal) :-
    prune_prog(count(1), Prog),
    Goal = ( oracle_log_final(Prog, [wcount(c, 0)], [[ +publish(m1), +publish(m2) ]],
                              Final, Log),
             Log == [ [ -wcount(c, 0), +wcount(c, 2),
                        +publish(m1), +publish(m2), +chan(2, m2) ],
                      [] ],
             final_lacks(Final, chan(1, m1)) ).

prune_prog(Bound, prog([ kind(publish/1, log), keep(publish/1, all),
                         kind(chan/2, log), keep(chan/2, Bound),
                         keyed(wcount/2, [1]) ],
                       [ (wcount(c, Next) <+ publish(_), pre(wcount(c, SoFar)),
                            Next := SoFar + 1),
                         (chan(Next, Payload) <+ publish(Payload),
                            pre(wcount(c, SoFar)), Next := SoFar + 1) ])).

% and the follow-up that locates it in RETENTION rather than in Log rels: the
% identical program under keep(all) shows both rows.
rounds_scenario(r5_keep_all_shows_the_row_that_keep_count_one_erased, Goal) :-
    prune_prog(all, Prog),
    Goal = ( oracle_log(Prog, [wcount(c, 0)], [[ +publish(m1), +publish(m2) ]], Log),
             Log == [ [ -wcount(c, 0), +wcount(c, 2),
                        +publish(m1), +publish(m2), +chan(1, m1), +chan(2, m2) ],
                      [] ] ).

% ═══ ROUND 6 ═══════════════════════════════════════════════════════════════
% Aim: put the error arm on a KEYED envelope, which is the shape every SWR
% cache uses, and see whether the round-2 error findings survive.
% Found: ONE thing, the swallowed error (assertion 27, amends 12 and 13).

keyed_envelope(prog([ kind(resp/2, log), keep(resp/2, all),
                      keyed(latest_resp/2, [1]),
                      kind(served/2, log), keep(served/2, all),
                      kind(handled/2, log), keep(handled/2, all) ],
                    [ (latest_resp(Key, Value) <+ resp(Key, Value)),
                      (served(Key, Body) <+ latest_resp(Key, ok(Body))),
                      (handled(Key, Message) <+ latest_resp(Key, error(Message))) ])).

log_envelope(prog([ kind(resp/2, log), keep(resp/2, all),
                    kind(served/2, log), keep(served/2, all),
                    kind(handled/2, log), keep(handled/2, all) ],
                  [ (served(Key, Body) <+ resp(Key, ok(Body))),
                    (handled(Key, Message) <+ resp(Key, error(Message))) ])).

% ROUND 6 FINDING. An error row and an ok row for one key in ONE tick: the
% keyed replace drops the error row before any arm sees it, so the error arm
% never fires and the final state carries no trace that a failure happened.
rounds_scenario(r6_a_keyed_envelope_swallows_an_error_delivered_in_the_same_tick, Goal) :-
    keyed_envelope(Prog),
    Goal = ( oracle_log_final(Prog, [],
                              [[ +resp(a, error(boom)), +resp(a, ok(two)) ]], Final, Log),
             Log == [ [ +latest_resp(a, ok(two)),
                        +resp(a, error(boom)), +resp(a, ok(two)) ],
                      [ +served(a, two) ],
                      [] ],
             final_lacks(Final, handled(a, boom)) ).

% the SAME two rows one tick apart DO fire the error arm. Whether a failure is
% observed at all is a function of how the scheduler batched the arrivals.
rounds_scenario(r6_the_same_two_rows_one_tick_apart_do_fire_the_error_arm, Goal) :-
    keyed_envelope(Prog),
    Goal = ( oracle_log_final(Prog, [],
                              [[ +resp(a, error(boom)) ], [ +resp(a, ok(two)) ]],
                              Final, _),
             final_has(Final, handled(a, boom)),
             final_has(Final, served(a, two)) ).

% and the ruled collapse event is exactly what reports the drop. This is the
% first place in the lab where the trace obligation earns its keep: without
% it the swallowed error leaves no record anywhere.
rounds_scenario(r6_the_collapse_event_is_what_reports_the_swallowed_error, Goal) :-
    Goal = ( crun(cprog([ kind(resp/2, log), keyed(latest_resp/2, [1]) ],
                        [ crule(arr(resp(Key, Value)), [], latest_resp(Key, Value)) ]),
                  [], [[ +resp(a, error(boom)), +resp(a, ok(two)) ]], 100, _, Collapses),
             Collapses == [ collapse(1, latest_resp/2, [a], 2, true) ] ).

% the escape is a DECL choice, not a construct: the same two rows through a
% Log envelope fire both arms in the same tick. Assertion 1 again.
rounds_scenario(r6_a_log_envelope_never_swallows_an_error, Goal) :-
    log_envelope(Prog),
    Goal = ( oracle_log_final(Prog, [],
                              [[ +resp(a, error(boom)), +resp(a, ok(two)) ]], Final, _),
             final_has(Final, handled(a, boom)),
             final_has(Final, served(a, two)) ).

% ═══ ROUND 7 : the closing round, no findings ══════════════════════════════

% attacks assertion 21 from the other side: one write per key per tick mints
% no event at all, so the collapse log is silent on a well-paced queue.
rounds_scenario(r7_one_write_per_tick_mints_no_collapse_event, Goal) :-
    Goal = ( crun(cprog([ kind(item/2, log), keyed(sink/2, [1]) ],
                        [ crule(arr(item(_, Value)), [], sink(k, Value)) ]),
                  [], [[ +item(1, a) ], [ +item(2, b) ]], 100, _, Collapses),
             Collapses == [] ).

% attacks assertions 17 and the finalize refusal together: retention removes
% event(a) and the finalize arm over that Log rel still never fires. Two
% independent silences stacked on one row.
rounds_scenario(r7_finalize_over_a_log_rel_stays_dead_even_when_retention_removes_the_row, Goal) :-
    Goal = ( oracle_log_final(prog([ kind(event/1, log), keep(event/1, count(1)),
                                     kind(gone/1, log), keep(gone/1, all) ],
                                   [ (gone(Item) <+ finalize(event(Item))) ]),
                              [], [[ +event(a) ], [ +event(b) ]], Final, Log),
             Log == [ [ +event(a) ], [ +event(b) ] ],
             Final == [ event(b) ] ).

% attacks assertion 4 from the collapse side: the pacing (b) drain writes one
% row per tick into its keyed consumer, so the collapse log stays empty where
% pacing (a) would fill it.
rounds_scenario(r7_the_pacing_b_drain_never_collapses_at_its_keyed_consumer, Goal) :-
    Goal = ( crun(cprog([ kind(head_value/2, log), keyed(seen/2, [1]) ],
                        [ crule(arr(head_value(_, Value)), [], seen(q, Value)) ]),
                  [], [[ +head_value(1, a) ], [ +head_value(2, b) ],
                       [ +head_value(3, c) ]], 100, _, Collapses),
             Collapses == [] ).

% the replay obligation: every amendment the run produced is journalled, and
% the amendments are exactly the four the rounds recorded.
rounds_scenario(r7_every_amendment_across_the_run_is_journalled, Goal) :-
    Goal = ( findall(Number-Round, ca_journal:amends(Number, Round), Amendments),
             msort(Amendments, Sorted),
             Sorted == [4-3, 12-6, 13-6, 17-5, 24-4] ).
