:- module(util, [tool/2]).

:- dynamic tool/2.

tool(Input, Output) :-
    Output is Input + 1.

tool(Input, Output) :-
    Input < 0,
    Output = 0.
