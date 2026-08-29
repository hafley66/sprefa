% Expected: a call edge from go/1 to double/2 through the maplist/3 closure,
% through call/3, and through the forall/2, findall/3 goal arguments.
% Observed 2026-08-28 corpus run: `double` under maplist and call yields no
% reference or site at all; goals under forall/findall stay `term_arg`.
:- module(corpus_1_meta_closures, [go/1]).

double(Input, Output) :- Output is Input * 2.

go(Doubled) :-
    maplist(double, [1, 2], Doubled),
    call(double, 1, _),
    forall(member(Element, Doubled), double(Element, _)),
    findall(Twice, double(3, Twice), _).
