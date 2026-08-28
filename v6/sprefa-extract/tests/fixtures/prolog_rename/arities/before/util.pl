:- module(util, [helper/2, helper/3]).

helper(Input, Output) :-
    Output is Input + 1.

helper(Input, Extra, Output) :-
    Output is Input + Extra.
