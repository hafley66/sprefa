:- initialization(main, main).

main :-
    once((member(X, [a,b,c,d]), X = d)),
    write_canonical(X), nl,
    halt.
