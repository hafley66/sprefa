:- module(main, [run/3]).

:- use_module(util).

run(Input, Extra, Output) :-
    tool(Input, Mid),
    helper(Mid, Extra, Output).
