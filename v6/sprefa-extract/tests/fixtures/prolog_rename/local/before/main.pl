:- module(main, [run/2, qualified/2, widen/3, label/2]).

:- use_module(util).

run(Input, Output) :-
    helper(Input, Output).

qualified(Input, Output) :-
    util:helper(Input, Output).

widen(Input, Extra, Output) :-
    helper(Input, Extra, Output).

helper(Input, Extra, Output) :-
    Output is Input + Extra.

label(Input, Text) :-
    format(atom(Text), "helper ~w", [Input]).
