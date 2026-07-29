% grade_fixtures.pl -- run the lab's fixture/5 CANDIDATES through the SAME
% oracle engine conformance/go.pl runs, without adding them to the promoted
% corpus. `engine:fixture_expectations_hold/2` is the identical entry point; a
% green run here means these terms are promotable as written.
%
% Run: swipl -q -l v6/prolog/labs/comment_node/grade_fixtures.pl -g go -g halt

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module('../../src/grader.pl').
:- use_module('../../conformance/engine').

:- ensure_loaded('fixtures.pl').

check(Name, engine:fixture_expectations_hold(Name, Expectations)) :-
    fixture(Name, _, _, _, Expectations).

go :- run(check).
