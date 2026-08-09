:- module(refs, [foo/1, bar/1, baz/1, nested/1]).

foo(relplan(1, 2, 3, 4, 5)).
bar(relplan(6, 7, 8, 9, 0)).

baz(X) :-
    side_effect,
    call(relplan(a, b, c, d, e)),
    process(relplan(f, g, h, i, j)),
    list([relplan(u, v, w, x, y)]).

nested(X) :-
    wrap(outer(inner(w))).
