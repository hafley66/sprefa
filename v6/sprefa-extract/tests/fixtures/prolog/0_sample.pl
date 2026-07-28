:- module(sample, [path/2, greeting//0]).

edge(a, b).
edge(b, c).

path(From, To) :-
    edge(From, Mid),
    path(Mid, To).
path(From, To) :-
    edge(From, To).

greeting --> token(hello), token(world).

qualified(X) :-
    lists:member(X, [a, b]).
