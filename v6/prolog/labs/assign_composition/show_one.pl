% show_one.pl : print the written program, the expanded program, and both
% tick logs for a single fixture. The debugging companion to grade_expansion.
% Run: swipl -q -l show_one.pl -g "show(counter_fold_matches_hand_computation)" -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('0_assign_expand', [expand_assign_program/2]).

show(Name) :-
    fixture(Name, Program, Initial, Schedule, _),
    copy_term(Program, ProgramToExpand),
    expand_assign_program(ProgramToExpand, Expanded),
    format("--- WRITTEN PROGRAM ---~n~q~n", [Program]),
    format("--- EXPANDED PROGRAM ---~n~q~n", [Expanded]),
    format("--- WRITTEN LOG ---~n"),
    print_ticklog(Program, Initial, Schedule),
    format("--- EXPANDED LOG ---~n"),
    print_ticklog(Expanded, Initial, Schedule).
