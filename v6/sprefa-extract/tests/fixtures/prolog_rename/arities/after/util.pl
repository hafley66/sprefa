:- module(util, [tool/2, helper/3]).

tool(Input, Output) :-
    Output is Input + 1.

helper(Input, Extra, Output) :-
    Output is Input + Extra.
