:- module(util, [helper/2]).

:- dynamic helper/2.

helper(Input, Output) :-
    Output is Input + 1.

helper(Input, Output) :-
    Input < 0,
    Output = 0.
