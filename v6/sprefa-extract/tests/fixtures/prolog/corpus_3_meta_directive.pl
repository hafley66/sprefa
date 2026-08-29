% Expected: the file's own :- meta_predicate directive drives the closure
% slots. apply_twice/3's first argument is declared `2`, so the bare atom
% double2 in go/1 mints a double2/2 site and a closure reference. The setof
% caret form unwraps Template^Goal so parent/2 is a goal reference.
:- module(meta_directive, [apply_twice/3, go/1]).
:- meta_predicate apply_twice(2, ?, ?).

apply_twice(Goal, X, Y) :- call(Goal, X, Y).

go(Y) :- apply_twice(double2, 1, Y).

double2(X, Y) :- Y is X * 2.

parents(Ps) :- setof(P^parent(P, C), C, Ps).

parent(a, b).
