% Minimal SWI-Prolog saved-state entry point for the binary-packaging lab.
% 6_BUILD.lisp does not load this file. Build with qsave_program/2 as shown
% in 1_SOURCES.md; the output path is outside the repository.

:- table path/2.

edge(a, b).
edge(b, c).
edge(c, a).
edge(c, d).

path(X, Y) :- edge(X, Y).
path(X, Y) :- edge(X, Z), path(Z, Y).

main :-
    call_with_time_limit(1,
                         ( setof(Y, path(a, Y), Answers),
                           format('SWI-SAVED PATH ~q~n', [Answers]) )),
    halt.
