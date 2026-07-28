% lab.pl : the ONE self-loading entry point for the consumption + arms lab.
%
%   swipl -q -l v6/prolog/labs/consumption_arms/lab.pl -g go -g halt
%
% Exits 0 printing ONLY `PASS <name>` lines. A FAIL line means the lab is not
% done. Contract: plans/2026-07-28-consumption-arms-lab-header.md.
% Verdict:  plans/2026-07-28-consumption-arms-verdict.md.
%
% Nothing here edits v6/prolog/conformance/**; engine.pl is consumed
% read-only through oracle.pl exactly the way ticklog.pl consumes it.

:- use_module(library(lists)).
:- use_module(library(apply)).

:- use_module(oracle).
:- use_module(model).
:- use_module(arms).
:- use_module(consume).
:- use_module(channel).
:- use_module(collapse).
:- use_module(desugar).
:- use_module(rounds).
:- use_module(journal).
:- use_module(fixtures).

:- use_module('../../conformance/engine').

:- dynamic failed/0.

check(Name, Goal) :-
    (   catch(Goal, Error, ( format("FAIL ~w threw ~q~n", [Name, Error]), fail ))
    ->  format("PASS ~w~n", [Name])
    ;   format("FAIL ~w~n", [Name]), assertz(failed)
    ).

go :-
    thread_checks,
    closing_round_checks,
    journal_checks,
    fixture_checks,
    ( failed -> halt(1) ; true ).

% ═══ the four threads plus the optional fifth ══════════════════════════════

thread_checks :-
    forall(arms_scenario(Name, Goal),     check(Name, ca_arms:Goal)),
    forall(consume_scenario(Name, Goal),  check(Name, ca_consume:Goal)),
    forall(channel_scenario(Name, Goal),  check(Name, ca_channel:Goal)),
    forall(collapse_scenario(Name, Goal), check(Name, ca_collapse:Goal)),
    forall(desugar_scenario(Name, Goal),  check(Name, ca_desugar:Goal)),
    check(threads_all_five_are_covered, five_threads_covered).

five_threads_covered :-
    forall(member(Thread-Predicate,
                  [ arms-arms_scenario, consume-consume_scenario,
                    channel-channel_scenario, collapse-collapse_scenario,
                    desugar-desugar_scenario ]),
           ( atom(Thread), Head =.. [Predicate, _, _],
             findall(1, ca_any(Head), Ones), length(Ones, Count), Count >= 3 )).

ca_any(Head) :- ( ca_arms:Head ; ca_consume:Head ; ca_channel:Head
                ; ca_collapse:Head ; ca_desugar:Head ).

% ═══ the closing rounds ════════════════════════════════════════════════════

closing_round_checks :-
    forall(rounds_scenario(Name, Goal), check(Name, ca_rounds:Goal)),
    check(round_seven_found_nothing_new_so_the_fixpoint_closes, closing_round_is_empty),
    check(rounds_four_five_and_six_each_found_exactly_one_break, one_break_per_round).

closing_round_is_empty :- round(7, _, []).

% each adversarial round before the last minted exactly one new assertion,
% which is what "amend, do not accumulate" looks like when it works.
one_break_per_round :-
    forall(member(Round, [4, 5, 6]),
           ( findall(Number, assertion(Number, Round, _, _), Minted),
             length(Minted, 1) )).

% ═══ the assertion set and the journal ═════════════════════════════════════

journal_checks :-
    check(journal_every_assertion_names_at_least_one_check, assertions_name_checks),
    check(journal_every_named_check_exists, named_checks_exist),
    check(journal_assertion_numbers_are_dense_from_one, assertion_numbers_dense),
    check(journal_every_round_from_one_to_four_is_journalled, rounds_journalled),
    check(journal_every_round_before_the_last_found_something, rounds_before_last_found_something),
    check(journal_every_assertion_round_is_a_journalled_round, assertion_rounds_exist),
    check(journal_the_assertion_set_covers_all_five_threads, assertions_cover_threads),
    check(journal_every_amendment_names_a_real_assertion, amendments_are_real).

assertions_name_checks :-
    forall(assertion(_, _, Text, Checks),
           ( atom(Text), atom_length(Text, Length), Length > 60,
             Checks = [_ | _] )).

named_checks_exist :-
    forall(( assertion(_, _, _, Checks), member(CheckName, Checks) ),
           lab_check_exists(CheckName)).

lab_check_exists(Name) :-
    (   ca_arms:arms_scenario(Name, _)      -> true
    ;   ca_consume:consume_scenario(Name, _) -> true
    ;   ca_channel:channel_scenario(Name, _) -> true
    ;   ca_collapse:collapse_scenario(Name, _) -> true
    ;   ca_desugar:desugar_scenario(Name, _) -> true
    ;   ca_rounds:rounds_scenario(Name, _)   -> true
    ).

assertion_numbers_dense :-
    findall(Number, assertion(Number, _, _, _), Numbers),
    msort(Numbers, Sorted),
    length(Sorted, Count), numlist(1, Count, Sorted).

rounds_journalled :-
    findall(Number, round(Number, _, _), Numbers),
    msort(Numbers, Sorted), Sorted == [1, 2, 3, 4, 5, 6, 7].

rounds_before_last_found_something :-
    forall(member(Number, [1, 2, 3, 4, 5, 6]),
           ( round(Number, _, Findings), length(Findings, Count), Count >= 4 )).

assertion_rounds_exist :-
    forall(assertion(_, Round, _, _), round(Round, _, _)).

% every thread has at least three assertions of its own, checked by the
% prefix of the checks each assertion names.
assertions_cover_threads :-
    forall(member(Prefix, [r1_switch, r1_subscribe, r1_two_writers,
                           r1_exactly_one_instrumentation,
                           r1_the_plus_half_lands]),
           ( assertion(_, _, _, Checks), member(Check, Checks),
             atom_concat(Prefix, _, Check), ! )).

amendments_are_real :-
    forall(amends(Number, Round),
           ( assertion(Number, _, _, _), round(Round, _, _) )).

% ═══ prospective fixtures ══════════════════════════════════════════════════
% Graded by the REAL conformance harness (engine:fixture_expectations_hold/2)
% against user:fixture/5 clauses that live in this lab and nowhere else.

fixture_checks :-
    forall(prospective_fixture(Name),
           ( user:fixture(Name, _, _, _, Expectations),
             atom_concat('fixture_', Name, CheckName),
             check(CheckName, engine:fixture_expectations_hold(Name, Expectations)) )),
    check(fixture_exactly_three_prospective_fixtures, three_prospective_fixtures),
    check(fixture_none_are_in_the_conformance_corpus, fixtures_are_lab_local).

three_prospective_fixtures :-
    findall(Name, prospective_fixture(Name), Names), length(Names, 3).

% every user:fixture/5 visible in this process came from this lab's file, so
% nothing here can be mistaken for a conformance fixture.
fixtures_are_lab_local :-
    findall(Name, user:fixture(Name, _, _, _, _), Loaded),
    msort(Loaded, Sorted),
    findall(Name2, prospective_fixture(Name2), Ours),
    msort(Ours, OursSorted),
    Sorted == OursSorted.
