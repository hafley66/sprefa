% arms.pl : THREAD 1, the lifecycle arms under the RULED rx Observer
% vocabulary (rulings.pl lifecycle_arm_vocabulary: next / finalize /
% unsubscribe / complete / subscribe / error).
%
% Per arm the table answers four questions and every answer that can be run
% is run on the oracle:
%   granularity  row or rel
%   subject      WHICH rel the arm actually fires on
%   binds        what the body sees
%   fires_at     same tick as the causing delta, or the Ti drain after it
%   kernel       the shipped kernel form it grounds out to
%
% The error arm is graded, not assumed, against the failure-is-a-value
% envelope: an error arm must not become a second failure channel.

:- module(ca_arms, [ arm/6, arm_refusal/3, arm_slot/2, arms_scenario/2 ]).

:- use_module(library(lists)).
:- use_module(oracle).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous arms_scenario/2.

% ═══ the arm table ═════════════════════════════════════════════════════════
% arm(Name, Granularity, Subject, Binds, FiresAt, Kernel)

arm(next, row, the_named_rel, the_arrived_row, same_tick_as_the_plus_delta,
    bare_trigger_atom).
arm(finalize, row, the_named_rel, the_departed_row, drain_tick_after_the_minus_delta,
    finalize_departure_trigger).
arm(subscribe, row, the_demand_rel, the_demand_row, same_tick_as_the_plus_delta,
    bare_trigger_atom_on_the_demand_rel).
arm(unsubscribe, row, the_demand_rel, the_departing_demand_row,
    drain_tick_after_the_minus_delta, finalize_on_the_demand_rel).
arm(complete, row, the_scope_rel, the_departing_scope_row,
    drain_tick_after_the_minus_delta, finalize_on_the_live_scope_rel).
arm(error, row, the_named_rel, the_error_variant_row, same_tick_as_the_plus_delta,
    bare_trigger_atom_plus_variant_destructure).

% ═══ refusals ══════════════════════════════════════════════════════════════
% arm_refusal(Name, Term, Why)

arm_refusal(error, second_failure_channel(error),
            'routing a thrown exception to an error arm is a second failure channel, which the envelope ruling bans; an exception is also not a row, so it cannot appear in the tick log and item-9 grading could never see it').
arm_refusal(error, arm_variant_absent(error, chan/2),
            'error over a rel whose decl declares no error variant is statically dead, the same class as finalize over a Log rel').
arm_refusal(complete, arm_has_no_scope_row(complete, chan/2),
            'complete over a rel with no scope row has nothing to fire on; a rel is a table and a table never completes').
arm_refusal(unsubscribe, arm_has_no_demand_rel(unsubscribe, chan/2),
            'unsubscribe over a rel nothing demands has nothing to fire on').
arm_refusal(finalize, finalize_over_log_rel(chan/2),
            'a Log rel emits no minus delta, so a finalize arm over one is statically dead (match-frontier C4, still true here)').

% ═══ slots ═════════════════════════════════════════════════════════════════

arm_slot('SLOT-ARM-ARGUMENT',
         'subscribe, unsubscribe and complete read as if they take the data rel and in fact fire on a different rel (the demand row, the scope row). Either the one-argument form is refused and the program writes finalize(demand(...)) itself, or the one-argument form is allowed only where the block statically determines the scope or demand rel').
arm_slot('SLOT-ERROR-VARIANT-NAME',
         'under the enum ruling a variant is named by the program. Either error is a reserved variant name, or the arm word is dropped and the program writes the variant arm by its own name').
arm_slot('SLOT-ERROR-TERMINALITY',
         'rx error terminates the subscription; an error variant row terminates nothing. Either the word is kept and the difference is written down loudly, or the arm additionally retracts the demand row, which would make error the only arm with a side effect').

% ═══ ROUND 1 : each arm grounds out, and on WHICH rel ══════════════════════

% next and finalize are the shipped pair. finalize fires one tick after the
% minus delta; that asymmetry is the arm family's, not this lab's.
arms_scenario(r1_next_fires_in_the_arrival_tick_and_finalize_one_drain_later, Goal) :-
    demand_prog(Prog),
    Goal = ( oracle_log(Prog, [], [[ +demand(a) ], [ -demand(a) ]], Log),
             Log == [ [ +demand(a), +started(a) ],
                      [ -demand(a) ],
                      [ +stopped(a) ],
                      [] ] ).

% subscribe / unsubscribe written out: the demand rel IS the subscription.
%
%   rel demand(key: text)
%   rel started(key: text) log keep(all)
%   rel stopped(key: text) log keep(all)
%   started(key) <+ demand(key).
%   stopped(key) <+ finalize(demand(key)).
%
% rx lowering:
%   demand$.pipe(
%     groupBy(row => row.key),
%     mergeMap(group => group.pipe(
%       take(1), tap(onSubscribe), finalize(onUnsubscribe))))
%   -- the group's own lifetime is the subscription; rx finalize on the inner
%   is the unsubscribe arm, which is why the two words are one mechanism.
demand_prog(prog([ kind(demand/1, set),
                   kind(started/1, log), keep(started/1, all),
                   kind(stopped/1, log), keep(stopped/1, all) ],
                 [ (started(Key) <+ demand(Key)),
                   (stopped(Key) <+ finalize(demand(Key))) ])).

% subscribe and unsubscribe are next and finalize ON THE DEMAND REL. The same
% program above IS the subscribe/unsubscribe pair; nothing else is needed.
arms_scenario(r1_subscribe_and_unsubscribe_are_next_and_finalize_on_the_demand_rel, Goal) :-
    Goal = ( arm(subscribe, _, the_demand_rel, _, same_tick_as_the_plus_delta,
                 bare_trigger_atom_on_the_demand_rel),
             arm(unsubscribe, _, the_demand_rel, _, drain_tick_after_the_minus_delta,
                 finalize_on_the_demand_rel),
             arm(next, _, _, _, same_tick_as_the_plus_delta, bare_trigger_atom),
             arm(finalize, _, _, _, drain_tick_after_the_minus_delta,
                 finalize_departure_trigger) ).

% complete is finalize ON THE LIVE SCOPE REL, and the scope rel is an
% ordinary level rule (open minus closed). The scope departs at tick 3, the
% complete arm fires at tick 4.
arms_scenario(r1_complete_is_finalize_on_the_live_scope_rel, Goal) :-
    Goal = ( oracle_log(
                 prog([ kind(open_request/1, log), keep(open_request/1, all),
                        kind(close_request/1, log), keep(close_request/1, all),
                        kind(completed/1, log), keep(completed/1, all),
                        keyed(open_scope/1, [1]), keyed(closed/1, [1]) ],
                      [ (open_scope(Scope) <+ open_request(Scope)),
                        (closed(Scope) <+ close_request(Scope)),
                        (live_scope(Scope) <- open_scope(Scope), not(closed(Scope))),
                        (completed(Scope) <+ finalize(live_scope(Scope))) ]),
                 [], [[ +open_request(s1) ], [], [ +close_request(s1) ]], Log),
             Log == [ [ +live_scope(s1), +open_scope(s1), +open_request(s1) ],
                      [],
                      [ -live_scope(s1), +closed(s1), +close_request(s1) ],
                      [ +completed(s1) ],
                      [] ] ).

% ═══ ROUND 1 : the error arm, graded ═══════════════════════════════════════
% Reading (A): error is an enum-variant destructure over the ordinary
% envelope rel. Zero constructs, zero new channels, and it runs today.
arms_scenario(r1_the_error_arm_is_an_ordinary_variant_destructure, Goal) :-
    envelope_prog(Prog),
    Goal = ( oracle_log(Prog, [], [[ +resp(a, error(boom)) ]], Log),
             Log == [ [ +resp(a, error(boom)), +handled(a, boom) ], [] ] ).

envelope_prog(prog([ kind(resp/2, log), keep(resp/2, all),
                     kind(served/2, log), keep(served/2, all),
                     kind(handled/2, log), keep(handled/2, all) ],
                   [ (served(Key, Body) <+ resp(Key, ok(Body))),
                     (handled(Key, Message) <+ resp(Key, error(Message))) ])).

% ═══ ROUND 2 : the error arm is NOT terminal ═══════════════════════════════
% This breaks the round-1 assertion "error is the rx word". In rx, error is
% the last notification a subscription ever receives. Here the rel keeps
% producing next rows after the error arm has fired, in the very next tick,
% with nothing anywhere marking the subscription dead.
arms_scenario(r2_the_rel_keeps_producing_after_the_error_arm_fires, Goal) :-
    envelope_prog(Prog),
    Goal = ( oracle_log_final(Prog, [],
                              [[ +resp(a, ok(one)) ], [ +resp(a, error(boom)) ],
                               [ +resp(a, ok(two)) ]], Final, Log),
             Log == [ [ +resp(a, ok(one)), +served(a, one) ],
                      [ +resp(a, error(boom)), +handled(a, boom) ],
                      [ +resp(a, ok(two)), +served(a, two) ],
                      [] ],
             final_has(Final, served(a, two)) ).

% ROUND 2, the reconciliation the contract asked for. The second-channel
% reading is refused on THREE independent grounds, each of which is a
% standing law rather than a lab opinion.
arms_scenario(r2_the_second_channel_reading_is_refused_on_three_grounds, Goal) :-
    Goal = ( arm_refusal(error, second_failure_channel(error), Why),
             sub_atom(Why, _, _, _, 'envelope ruling'),
             sub_atom(Why, _, _, _, 'not a row'),
             sub_atom(Why, _, _, _, 'tick log') ).

% ROUND 2: an error arm over a rel with no error variant desugars cleanly and
% is statically dead. Same shape as finalize over a Log rel, and just as
% silent. The receipt: the rule loads, runs, and never fires.
arms_scenario(r2_an_error_arm_with_no_matching_variant_is_silently_dead, Goal) :-
    Goal = ( oracle_log_final(
                 prog([ kind(resp/2, log), keep(resp/2, all),
                        kind(handled/2, log), keep(handled/2, all) ],
                      [ (handled(Key, Message) <+ resp(Key, error(Message))) ]),
                 [], [[ +resp(a, ok(one)) ], [ +resp(a, ok(two)) ]], Final, Log),
             Log == [ [ +resp(a, ok(one)) ], [ +resp(a, ok(two)) ] ],
             final_lacks(Final, handled(a, _)) ).

% ═══ ROUND 3 : the arm family is not timing symmetric ══════════════════════
% Three of six arms fire in the tick of their delta and three fire one drain
% tick later. The split is not next-versus-the-rest: it is plus-side versus
% minus-side, because a minus delta only becomes an occurrence through the
% departure carry (engine.pl:307-311).
arms_scenario(r3_plus_side_arms_fire_at_t_and_minus_side_arms_at_t_plus_one, Goal) :-
    Goal = ( findall(Name, arm(Name, _, _, _, same_tick_as_the_plus_delta, _), Plus),
             msort(Plus, PlusSorted), PlusSorted == [error, next, subscribe],
             findall(Name2, arm(Name2, _, _, _, drain_tick_after_the_minus_delta, _), Minus),
             msort(Minus, MinusSorted), MinusSorted == [complete, finalize, unsubscribe] ).

% ROUND 3: every arm is ROW granularity once the right rel is named. There is
% no rel-level arm in the vocabulary, which is the finding that makes the
% whole family one construct instead of two.
arms_scenario(r3_every_arm_is_row_granularity_on_some_rel, Goal) :-
    Goal = forall(arm(_, Granularity, Subject, _, _, _),
                  ( Granularity == row,
                    memberchk(Subject, [the_named_rel, the_demand_rel, the_scope_rel]) )).

% ROUND 3: three of the six arms fire on a rel the arm does not name. That is
% the whole content of SLOT-ARM-ARGUMENT.
arms_scenario(r3_three_arms_fire_on_a_rel_they_do_not_name, Goal) :-
    Goal = ( findall(Name, ( arm(Name, _, Subject, _, _, _), Subject \== the_named_rel ),
                     Indirect),
             msort(Indirect, Sorted),
             Sorted == [complete, subscribe, unsubscribe],
             arm_slot('SLOT-ARM-ARGUMENT', _) ).

% ROUND 3: every refusal names a term, and no refusal is a bare opinion.
arms_scenario(r3_every_refusal_names_a_term_and_a_reason, Goal) :-
    Goal = forall(arm_refusal(Name, Term, Why),
                  ( arm(Name, _, _, _, _, _), compound(Term), atom(Why),
                    atom_length(Why, Length), Length > 40 )).
